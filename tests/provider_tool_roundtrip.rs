//! Provider requests are inspected at a local HTTP server; no external API is used.
use axum::{extract::State, routing::post, Json, Router};
use dreamswarm::db::Database;
use dreamswarm::query::engine::QueryEngine;
use dreamswarm::runtime::{agent_loop::AgentRuntime, config::AppConfig, session::Session};
use dreamswarm::tools::{file_read::FileReadTool, ToolRegistry};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
struct ServerState {
    bodies: Arc<Mutex<Vec<Value>>>,
    openai: bool,
    paths: [String; 2],
}

async fn complete(State(state): State<ServerState>, Json(body): Json<Value>) -> Json<Value> {
    let mut bodies = state.bodies.lock().await;
    bodies.push(body);
    if bodies.len() == 1 {
        if state.openai {
            return Json(json!({"choices":[{"message":{"tool_calls":[
                {"id":"call-1","type":"function","function":{"name":"FileRead","arguments":json!({"path":state.paths[0]}).to_string()}},
                {"id":"call-2","type":"function","function":{"name":"FileRead","arguments":json!({"path":state.paths[1]}).to_string()}}
            ]},"finish_reason":"tool_calls"}], "usage":{"prompt_tokens":1,"completion_tokens":1}}));
        }
        return Json(json!({"content":[
            {"type":"text","text":"Reading both."},
            {"type":"tool_use","id":"call-1","name":"FileRead","input":{"path":state.paths[0]}},
            {"type":"tool_use","id":"call-2","name":"FileRead","input":{"path":state.paths[1]}}
        ],"usage":{"input_tokens":1,"output_tokens":1},"stop_reason":"tool_use"}));
    }
    if state.openai {
        Json(
            json!({"choices":[{"message":{"content":"Done"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}),
        )
    } else {
        Json(
            json!({"content":[{"type":"text","text":"Done"}],"usage":{"input_tokens":1,"output_tokens":1},"stop_reason":"end_turn"}),
        )
    }
}

#[tokio::test]
async fn anthropic_and_openai_receive_grouped_tool_calls_and_results() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.txt");
    let b = tmp.path().join("b.txt");
    std::fs::write(&a, "first file").unwrap();
    std::fs::write(&b, "second file").unwrap();
    for provider in ["anthropic", "openai"] {
        let state = ServerState {
            bodies: Arc::new(Mutex::new(vec![])),
            openai: provider == "openai",
            paths: [
                a.to_string_lossy().into_owned(),
                b.to_string_lossy().into_owned(),
            ],
        };
        let app = Router::new()
            .route("/v1/messages", post(complete))
            .route("/v1/chat/completions", post(complete))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        std::env::set_var("ANTHROPIC_API_KEY", "test");
        std::env::set_var("ANTHROPIC_BASE_URL", &base);
        std::env::set_var("OPENAI_API_KEY", "test");
        std::env::set_var("OPENAI_BASE_URL", format!("{base}/v1"));
        let mut config = AppConfig::default();
        config.state_dir = tmp.path().join(provider);
        let db = Database::new(&config.state_dir).unwrap();
        db.migrate().unwrap();
        let engine = QueryEngine::new(provider, &config.model, &config).unwrap();
        let mut tools = ToolRegistry::new();
        tools.register(Box::new(FileReadTool));
        let mut runtime = AgentRuntime::new(Session::new(), engine, tools, config, db, None);
        let result = runtime
            .run_turn("Read both files", |_, _| async { false })
            .await
            .unwrap();
        assert_eq!(result.final_text, "Done");
        let bodies = state.bodies.lock().await;
        assert_eq!(bodies.len(), 2);
        let messages = bodies[1]["messages"].as_array().unwrap();
        if provider == "anthropic" {
            assert_eq!(messages[1]["role"], "assistant");
            assert_eq!(messages[1]["content"].as_array().unwrap().len(), 3);
            assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
            assert_eq!(messages[2]["content"][1]["tool_use_id"], "call-2");
        } else {
            let assistant = messages
                .iter()
                .find(|m| m["tool_calls"].is_array())
                .unwrap();
            assert_eq!(assistant["tool_calls"].as_array().unwrap().len(), 2);
            let results: Vec<_> = messages.iter().filter(|m| m["role"] == "tool").collect();
            assert_eq!(results.len(), 2);
            assert_eq!(results[0]["tool_call_id"], "call-1");
            assert_eq!(results[1]["content"], "second file");
        }
        server.abort();
    }
}
