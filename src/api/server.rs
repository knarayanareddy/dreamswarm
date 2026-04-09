use crate::memory::MemorySystem;
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct ApiState {
    pub memory: Arc<RwLock<MemorySystem>>,
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("The Oracle listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_query(
    State(state): State<ApiState>,
    Query(params): Query<SearchQuery>,
) -> Json<Vec<SearchResult>> {
    let mem = state.memory.read().await;
    // We use the MemoryLoader to perform a context-aware query
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
    // Phase 6 placeholder: This will eventually return results synthesized from
    // multiple agent audits in the L3 memory layer.
    Json(serde_json::json!({
        "status": "Oracle Online",
        "consensus": "Swarm intelligence is synchronized across local and global substrates.",
        "layer_3_health": "Optimal",
        "trust_index": 0.92
    }))
}
