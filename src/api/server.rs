use crate::api::telemetry::TelemetryHub;
use crate::memory::MemorySystem;
use crate::runtime::config::{AppConfig, OllamaConfig, RoutingPolicy};
use crate::swarm::{TeamConfig, WorkerInfo};
use axum::{
    extract::{Query, State},
    response::{
        sse::{Event, Sse},
        Html,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

// ── Shared State ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiState {
    pub memory: Arc<RwLock<MemorySystem>>,
    pub telemetry: Arc<TelemetryHub>,
    pub config: Arc<RwLock<AppConfig>>,
    pub workers: Arc<RwLock<Vec<WorkerInfo>>>,
}

// ── Request / Response DTOs ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub path: String,
    pub content: String,
}

/// Partial config patch — all fields optional so the UI can send only what changed.
#[derive(Deserialize)]
pub struct ConfigPatch {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub permission_mode: Option<String>,
    pub routing_policy: Option<RoutingPolicy>,
    pub allow_patterns: Option<Vec<String>>,
    pub deny_patterns: Option<Vec<String>>,
    pub ollama_endpoint: Option<String>,
    pub ollama_model: Option<String>,
}

/// Request body for swarm launch from the dashboard.
#[derive(Deserialize)]
pub struct SwarmLaunchRequest {
    pub mission: String,
    pub team_config: Option<TeamConfig>,
}

// ── Server Bootstrap ──────────────────────────────────────────────────────────

pub async fn start_api_server(state: ApiState, port: u16) -> anyhow::Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Dashboard UI
        .route("/", get(serve_dashboard))
        .route("/dashboard", get(serve_dashboard))
        // Existing endpoints
        .route("/query", get(handle_query))
        .route("/consensus", get(handle_consensus))
        .route("/api/v1/telemetry/stream", get(handle_telemetry_stream))
        .route("/api/v1/telemetry/history", get(handle_telemetry_history))
        .route("/api/v1/control/stop", post(handle_control_stop))
        .route("/api/v1/control/dream", post(handle_control_dream))
        .route("/api/v1/control/war-room", post(handle_control_war_room))
        // New: Config
        .route(
            "/api/v1/config",
            get(handle_get_config).post(handle_update_config),
        )
        // New: Swarm management
        .route("/api/v1/swarm/workers", get(handle_get_workers))
        .route("/api/v1/swarm/launch", post(handle_swarm_launch))
        // Static files fallback
        .fallback_service(ServeDir::new("static"))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("The Oracle listening on http://{}", listener.local_addr()?);
    tracing::info!("Dashboard available at http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

// ── Existing Handlers ─────────────────────────────────────────────────────────

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

async fn handle_control_war_room(State(state): State<ApiState>) -> Json<serde_json::Value> {
    state
        .telemetry
        .log_event(
            "system",
            "control_signal",
            serde_json::json!({"action": "WAR_ROOM", "origin": "dashboard"}),
        )
        .await;
    Json(serde_json::json!({"status": "War Room stress cycle broadcasted"}))
}

// ── New: Config Handlers ──────────────────────────────────────────────────────

async fn handle_get_config(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let cfg = state.config.read().await;
    // Return as JSON; serde handles PathBuf → string automatically
    match serde_json::to_value(&*cfg) {
        Ok(v) => Json(v),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn handle_update_config(
    State(state): State<ApiState>,
    Json(patch): Json<ConfigPatch>,
) -> Json<serde_json::Value> {
    let mut cfg = state.config.write().await;

    if let Some(model) = patch.model {
        cfg.model = model;
    }
    if let Some(provider) = patch.provider {
        cfg.provider = provider;
    }
    if let Some(mode) = patch.permission_mode {
        cfg.permission_mode = mode.parse().unwrap_or(cfg.permission_mode);
    }
    if let Some(policy) = patch.routing_policy {
        cfg.routing_policy = policy;
    }
    if let Some(allow) = patch.allow_patterns {
        cfg.allow_patterns = allow;
    }
    if let Some(deny) = patch.deny_patterns {
        cfg.deny_patterns = deny;
    }

    // Ollama sub-config
    if patch.ollama_endpoint.is_some() || patch.ollama_model.is_some() {
        let existing = cfg.ollama_config.get_or_insert(OllamaConfig {
            endpoint: "http://localhost:11434".to_string(),
            model: "llama3.1:8b".to_string(),
        });
        if let Some(ep) = patch.ollama_endpoint {
            existing.endpoint = ep;
        }
        if let Some(m) = patch.ollama_model {
            existing.model = m;
        }
    }

    // Persist to ~/.dreamswarm/config.toml
    match cfg.save_to_toml() {
        Ok(_) => Json(
            serde_json::json!({"status": "Config saved", "path": AppConfig::config_file_path(&cfg.state_dir)}),
        ),
        Err(e) => Json(
            serde_json::json!({"status": "Config updated in memory (disk write failed)", "error": e.to_string()}),
        ),
    }
}

// ── New: Swarm Handlers ───────────────────────────────────────────────────────

async fn handle_get_workers(State(state): State<ApiState>) -> Json<Vec<serde_json::Value>> {
    let workers = state.workers.read().await;
    let serialized = workers
        .iter()
        .filter_map(|w| serde_json::to_value(w).ok())
        .collect();
    Json(serialized)
}

async fn handle_swarm_launch(
    State(state): State<ApiState>,
    Json(req): Json<SwarmLaunchRequest>,
) -> Json<serde_json::Value> {
    if req.mission.trim().is_empty() {
        return Json(serde_json::json!({"error": "Mission cannot be empty"}));
    }

    let team_cfg = req.team_config.unwrap_or_default();

    state
        .telemetry
        .log_event(
            "swarm",
            "launch_requested",
            serde_json::json!({
                "mission": req.mission,
                "team_name": team_cfg.team_name,
                "max_workers": team_cfg.max_workers,
                "spawn_strategy": format!("{:?}", team_cfg.spawn_strategy),
                "linked_repositories": team_cfg.linked_repositories,
                "origin": "dashboard"
            }),
        )
        .await;

    Json(serde_json::json!({
        "status": "Swarm launch queued",
        "mission": req.mission,
        "team": team_cfg.team_name
    }))
}
