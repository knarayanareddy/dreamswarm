use crate::api::auth::{require_auth, ApiSecurity};
use crate::api::telemetry::TelemetryHub;
use crate::memory::MemorySystem;
use crate::runtime::config::{AppConfig, OllamaConfig, RoutingPolicy};
use crate::swarm::coordinator::SwarmCoordinator;
use crate::swarm::{TeamConfig, WorkerInfo};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{header, HeaderName, HeaderValue, Method},
    middleware,
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
use tokio::sync::{Mutex, RwLock};
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

// ── Shared State ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiState {
    pub memory: Arc<RwLock<MemorySystem>>,
    pub telemetry: Arc<TelemetryHub>,
    pub config: Arc<RwLock<AppConfig>>,
    pub workers: Arc<RwLock<Vec<WorkerInfo>>>,
    /// The active SwarmCoordinator (None before first launch).
    pub coordinator: Arc<Mutex<Option<SwarmCoordinator>>>,
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

/// Starts the Oracle API.
///
/// The server binds to loopback unless `DREAMSWARM_API_BIND` explicitly names
/// another interface, and every `/api/v1` route requires a bearer token. The
/// dashboard endpoints are unauthenticated but expose no data.
pub async fn start_api_server(state: ApiState, port: u16) -> anyhow::Result<()> {
    let bind_host =
        std::env::var("DREAMSWARM_API_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    if !matches!(bind_host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        tracing::warn!(
            "The Oracle is binding to {} instead of loopback; confirm this is intended",
            bind_host
        );
    }

    let (security, generated_token) = ApiSecurity::bootstrap(&bind_host, port)?;
    if let Some(token) = generated_token {
        println!("🔑 Oracle API token (this session): {}", token);
        println!(
            "   Send it as the '{}' header.",
            crate::api::auth::TOKEN_HEADER
        );
        println!("   Set DREAMSWARM_API_TOKEN to pin your own token.");
    }
    let dashboard = serve_dashboard;

    // Only the dashboard itself may be embedded/inspected cross-origin, and
    // only with the methods the browser UI actually needs.
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static(crate::api::auth::TOKEN_HEADER),
        ])
        .allow_origin(
            std::env::var("DREAMSWARM_API_ORIGIN")
                .ok()
                .and_then(|o| o.parse::<HeaderValue>().ok())
                .unwrap_or_else(|| "http://localhost".parse().unwrap()),
        );

    let protected: Router<ApiState> = Router::new()
        .route("/query", get(handle_query))
        .route("/api/v1/telemetry/stream", get(handle_telemetry_stream))
        .route("/api/v1/telemetry/history", get(handle_telemetry_history))
        .route("/api/v1/control/stop", post(handle_control_stop))
        .route("/api/v1/control/dream", post(handle_control_dream))
        .route("/api/v1/control/war-room", post(handle_control_war_room))
        .route(
            "/api/v1/config",
            get(handle_get_config).post(handle_update_config),
        )
        .route("/api/v1/swarm/workers", get(handle_get_workers))
        .route("/api/v1/swarm/launch", post(handle_swarm_launch))
        .route("/api/v1/swarm/stop", post(handle_swarm_stop))
        // `Extension` must run before the auth middleware, so it is layered
        // last (axum applies layers outside-in).
        .layer(middleware::from_fn(require_auth))
        .layer(axum::Extension(security.clone()))
        .layer(DefaultBodyLimit::max(256 * 1024));

    let public = Router::new()
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
        // Unauthenticated but data-free: used by the dashboard to show a
        // liveness badge without an API token.
        .route("/consensus", get(handle_consensus_public))
        // Static assets are public; they contain no session data.
        .fallback_service(ServeDir::new("static"));

    let app = public.merge(protected).layer(cors).with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", bind_host, port)).await?;
    tracing::info!("The Oracle listening on http://{}", listener.local_addr()?);
    tracing::info!(
        "Dashboard available at http://localhost:{} (API token required)",
        port
    );
    axum::serve(listener, app).await?;
    Ok(())
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

/// Serves the dashboard shell.
///
/// Deliberately public: it contains no data. Every data endpoint requires the
/// API token, so this response cannot leak configuration to a web page that
/// probes loopback.
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

/// Liveness probe. Exposes no configuration or worker data, so it is safe to
/// serve without the API token.
async fn handle_consensus_public() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "Oracle Online" }))
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

/// Serializes the parts of the configuration the dashboard needs.
///
/// The previous handler serialized `AppConfig` wholesale, which included the
/// DeepSeek API key in plaintext. Secrets are now reported only as `present`.
fn redacted_config(cfg: &AppConfig) -> serde_json::Value {
    serde_json::json!({
        "model": cfg.model,
        "provider": cfg.provider,
        "permission_mode": cfg.permission_mode,
        "routing_policy": cfg.routing_policy,
        "allow_patterns": cfg.allow_patterns,
        "deny_patterns": cfg.deny_patterns,
        "working_dir": cfg.working_dir,
        "state_dir": cfg.state_dir,
        "ollama_config": cfg.ollama_config.as_ref().map(|o| serde_json::json!({
            "endpoint": o.endpoint,
            "model": o.model,
        })),
        "deepseek_configured": cfg.deepseek_config.is_some(),
        "s3_relay_configured": cfg.s3_relay_config.is_some(),
    })
}

async fn handle_get_config(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let cfg = state.config.read().await;
    Json(redacted_config(&cfg))
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

// ── Role decomposition ─────────────────────────────────────────────────────────
// Splits a mission into (worker_name, role, instructions) per agent slot.
fn derive_worker_roles(mission: &str, max_workers: usize) -> Vec<(String, String, String)> {
    let candidates = vec![
        (
            "architect".to_string(),
            "architect".to_string(),
            format!("Plan the design and file structure for the following task. Do NOT write code yet — produce a concise implementation plan only.\n\nTask: {}", mission),
        ),
        (
            "coder".to_string(),
            "coder".to_string(),
            format!("Implement the following task. Write all necessary code, following the project's existing patterns. Be thorough and complete.\n\nTask: {}", mission),
        ),
        (
            "tester".to_string(),
            "tester".to_string(),
            format!("Write comprehensive unit tests and integration tests for the following task. Ensure all edge cases are covered and tests pass.\n\nTask: {}", mission),
        ),
        (
            "reviewer".to_string(),
            "reviewer".to_string(),
            format!("Review the implementation for correctness, performance, and style. Suggest and apply improvements.\n\nTask: {}", mission),
        ),
    ];
    candidates.into_iter().take(max_workers).collect()
}

fn validate_mission_limits(mission: &str) -> Result<(), String> {
    if mission.trim().is_empty() {
        return Err("Mission cannot be empty".to_string());
    }
    if mission.chars().count() > 8_000 {
        return Err("Mission is too long (max 8000 characters)".to_string());
    }
    Ok(())
}

async fn handle_swarm_launch(
    State(state): State<ApiState>,
    Json(req): Json<SwarmLaunchRequest>,
) -> Json<serde_json::Value> {
    if let Err(message) = validate_mission_limits(&req.mission) {
        return Json(serde_json::json!({ "error": message }));
    }

    let cfg = state.config.read().await;
    let working_dir = cfg.working_dir.to_string_lossy().to_string();
    let state_dir = cfg.state_dir.clone();
    drop(cfg);

    let team_cfg = req.team_config.unwrap_or_default();
    if !crate::swarm::executors::command::is_safe_identifier(&team_cfg.team_name) {
        return Json(serde_json::json!({
            "error": "team_name may only contain letters, digits, '-', '_', '.', ':' and '/'"
        }));
    }
    for host in &team_cfg.remote_hosts {
        if crate::swarm::executors::command::validate_remote_host(host).is_err() {
            return Json(serde_json::json!({ "error": format!("invalid remote host '{}'", host) }));
        }
    }
    let max_workers = team_cfg.max_workers.clamp(1, 4);
    let team_name = team_cfg.team_name.clone();
    let mission = req.mission.clone();

    // Build-and-spawn inside a blocking mutex guard so the coordinator stays alive
    let mut coordinator_guard = state.coordinator.lock().await;

    // Create a fresh coordinator for this launch
    let mut coordinator = match SwarmCoordinator::new(team_cfg, &working_dir, state_dir) {
        Ok(c) => c,
        Err(e) => {
            return Json(
                serde_json::json!({"error": format!("Failed to create coordinator: {}", e)}),
            )
        }
    };

    let roles = derive_worker_roles(&mission, max_workers);
    let mut spawned = Vec::new();

    for (name, role, instructions) in roles {
        match coordinator.spawn_worker(&name, &role, &instructions).await {
            Ok(worker) => {
                state
                    .telemetry
                    .log_event(
                        "swarm",
                        "worker_spawned",
                        serde_json::json!({
                            "worker_id": worker.id,
                            "name": worker.name,
                            "role": worker.role,
                            "team": team_name,
                            "status": "Active",
                        }),
                    )
                    .await;
                spawned.push(worker.clone());
                state.workers.write().await.push(worker);
            }
            Err(e) => tracing::warn!("Worker '{}' spawn failed: {}", name, e),
        }
    }

    // Broadcast a summary event
    state
        .telemetry
        .log_event(
            "swarm",
            "launch_complete",
            serde_json::json!({
                "mission": mission,
                "team": team_name,
                "workers_spawned": spawned.len(),
                "origin": "dashboard",
            }),
        )
        .await;

    *coordinator_guard = Some(coordinator);

    Json(serde_json::json!({
        "status": "Swarm launched",
        "team": team_name,
        "workers_spawned": spawned.len(),
        "workers": spawned.iter().map(|w| serde_json::json!({
            "id": w.id,
            "name": w.name,
            "role": w.role,
            "status": "Active",
        })).collect::<Vec<_>>(),
    }))
}

async fn handle_swarm_stop(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let mut coordinator_guard = state.coordinator.lock().await;
    if let Some(ref mut coordinator) = *coordinator_guard {
        match coordinator.shutdown_team().await {
            Ok(_) => {
                // Mark all workers as ShuttingDown in the shared list
                let mut workers = state.workers.write().await;
                for w in workers.iter_mut() {
                    w.status = crate::swarm::WorkerStatus::ShuttingDown;
                }
                state.telemetry
                    .log_event(
                        "swarm",
                        "shutdown",
                        serde_json::json!({"origin": "dashboard", "workers_stopped": workers.len()}),
                    )
                    .await;
                *coordinator_guard = None;
                Json(serde_json::json!({"status": "All workers shut down"}))
            }
            Err(e) => Json(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        Json(serde_json::json!({"status": "No active swarm to stop"}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_secret() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.deepseek_config = Some(crate::runtime::config::DeepSeekConfig {
            api_key: "sk-super-secret".to_string(),
            model: "deepseek-chat".to_string(),
        });
        cfg
    }

    #[test]
    fn config_response_redacts_api_keys() {
        let rendered = serde_json::to_string(&redacted_config(&config_with_secret())).unwrap();
        assert!(
            !rendered.contains("sk-super-secret"),
            "the API key must never be serialized to the dashboard"
        );
        assert!(rendered.contains("\"deepseek_configured\":true"));
        assert!(rendered.contains("\"model\""));
    }

    #[test]
    fn mission_validation_rejects_empty_and_oversized_input() {
        assert!(validate_mission_limits("").is_err());
        assert!(validate_mission_limits("   ").is_err());
        assert!(validate_mission_limits("build a parser").is_ok());
        assert!(validate_mission_limits(&"x".repeat(8_001)).is_err());
    }

    #[test]
    fn worker_roles_are_capped_by_max_workers() {
        let roles = derive_worker_roles("do work", 2);
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[1].0, "coder");
    }
}
