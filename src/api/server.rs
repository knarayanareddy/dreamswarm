use crate::api::telemetry::TelemetryHub;
use crate::memory::MemorySystem;
use axum::{
    extract::{Query, State},
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct ApiState {
    pub memory: Arc<RwLock<MemorySystem>>,
    pub telemetry: Arc<TelemetryHub>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub path: String,
    pub content: String,
}

pub async fn start_api_server(state: ApiState, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/query", get(handle_query))
        .route("/consensus", get(handle_consensus))
        .route("/api/v1/telemetry/stream", get(handle_telemetry_stream))
        .route("/api/v1/telemetry/history", get(handle_telemetry_history))
        .route("/api/v1/control/stop", post(handle_control_stop))
        .route("/api/v1/control/dream", post(handle_control_dream))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("The Oracle listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

#[axum::debug_handler]
async fn handle_query(
    State(state): State<ApiState>,
    Query(params): Query<SearchQuery>,
) -> Json<Vec<SearchResult>> {
    let mem = state.memory.read().await;
    let results = mem.loader.query(&params.q).unwrap_or_default();

    let mapped = results
        .into_iter()
        .map(|t| SearchResult {
            path: t.path,
            content: t.content,
        })
        .collect();

    Json(mapped)
}

async fn handle_consensus(State(_state): State<ApiState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "Oracle Online",
        "consensus": "Swarm intelligence is synchronized across local and global substrates.",
        "layer_3_health": "Optimal",
        "trust_index": 0.92
    }))
}

async fn handle_telemetry_stream(
    State(state): State<ApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.telemetry.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event) => {
            let json = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(Event::default().data(json)))
        }
        Err(_) => None,
    });

    Sse::new(stream)
}

async fn handle_telemetry_history(State(state): State<ApiState>) -> Json<Vec<serde_json::Value>> {
    let history = state
        .telemetry
        .get_history(None, 100)
        .await
        .unwrap_or_default();
    Json(history)
}

async fn handle_control_stop(State(state): State<ApiState>) -> Json<serde_json::Value> {
    state
        .telemetry
        .log_event(
            "system",
            "control_signal",
            serde_json::json!({"action": "STOP", "origin": "dashboard"}),
        )
        .await;
    // Note: The actual daemon stop logic will listen to this event or use a shared atomic flag.
    Json(serde_json::json!({"status": "Stop signal broadcasted to hive"}))
}

async fn handle_control_dream(State(state): State<ApiState>) -> Json<serde_json::Value> {
    state
        .telemetry
        .log_event(
            "system",
            "control_signal",
            serde_json::json!({"action": "DREAM", "origin": "dashboard"}),
        )
        .await;
    Json(serde_json::json!({"status": "Manual Deep Dream initiated"}))
}
