//! Telemetry Hub: Real-time event broadcasting and persistence coordinator.
use crate::db::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub category: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

pub struct TelemetryHub {
    db: Arc<RwLock<Database>>,
    broadcast_tx: broadcast::Sender<TelemetryEvent>,
}

impl TelemetryHub {
    pub fn new(db: Arc<RwLock<Database>>) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            db,
            broadcast_tx: tx,
        }
    }

    pub async fn log_event(&self, category: &str, event_type: &str, payload: serde_json::Value) {
        let event = TelemetryEvent {
            category: category.to_string(),
            event_type: event_type.to_string(),
            payload: payload.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        // 1. Persist to DB
        let db = self.db.read().await;
        if let Err(e) = db.log_telemetry_event(category, event_type, &payload) {
            tracing::error!("Failed to persist telemetry event: {}", e);
        }

        // 2. Broadcast to real-time subscribers
        let _ = self.broadcast_tx.send(event);
    }

    /// Broadcasts an event without persisting to the database.
    /// Used for high-frequency stress testing (War Room).
    pub async fn broadcast_event(
        &self,
        category: &str,
        event_type: &str,
        payload: serde_json::Value,
    ) {
        let event = TelemetryEvent {
            category: category.to_string(),
            event_type: event_type.to_string(),
            payload,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let _ = self.broadcast_tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryEvent> {
        self.broadcast_tx.subscribe()
    }

    pub async fn get_history(
        &self,
        category: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let db = self.db.read().await;
        db.get_telemetry_history(category, limit)
    }
}
