use crate::api::telemetry::TelemetryHub;
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub struct WarRoomStressTester {
    telemetry: Arc<TelemetryHub>,
}

impl WarRoomStressTester {
    pub fn new(telemetry: Arc<TelemetryHub>) -> Self {
        Self { telemetry }
    }

    /// Floods the telemetry stream with events at High Frequency.
    /// Skip persistence is used to avoid DB bloat.
    pub async fn flood_telemetry(&self, rate_hz: u32, duration_secs: u64) {
        let mut interval = interval(Duration::from_micros(1_000_000 / rate_hz as u64));
        let start = std::time::Instant::now();
        let end = start + Duration::from_secs(duration_secs);

        tracing::info!("War Room: Initiating 100Hz Telemetry Flood Stress Test...");

        while std::time::Instant::now() < end {
            interval.tick().await;

            // Broadcast only (skip persistence)
            let _ = self
                .telemetry
                .broadcast_event(
                    "stress_test",
                    "flood_signal",
                    serde_json::json!({
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "seq": 0, // In practice we could increment this
                        "load": "HIGH",
                        "entropy": rand::random::<u32>()
                    }),
                )
                .await;
        }

        tracing::info!("War Room: Telemetry Flood Stress Test Complete.");
    }

    /// Simulates a cascading failure in the hive.
    /// This should be persisted for historical analysis of the dashboard alerts.
    pub async fn simulate_cascading_failure(&self) -> anyhow::Result<()> {
        tracing::warn!("War Room: Simulating Cascading Hive Failure...");

        // 1. Initial Error
        self.telemetry
            .log_event(
                "system",
                "error",
                serde_json::json!({
                    "component": "KAIROS",
                    "error": "UNEXPECTED_HEARTBEAT_HALT",
                    "urgency": "CRITICAL"
                }),
            )
            .await;

        tokio::time::sleep(Duration::from_millis(500)).await;

        // 2. Healing Attempt
        self.telemetry
            .log_event(
                "healing",
                "attempt",
                serde_json::json!({
                    "strategy": "WORKTREE_RECOVERY",
                    "target": "src/daemon/heartbeat.rs"
                }),
            )
            .await;

        tokio::time::sleep(Duration::from_millis(800)).await;

        // 3. Healing Failure
        self.telemetry
            .log_event(
                "healing",
                "failure",
                serde_json::json!({
                    "error": "GIT_WORKTREE_LOCK_CONTENTION",
                    "retries_exhausted": true
                }),
            )
            .await;

        tokio::time::sleep(Duration::from_millis(300)).await;

        // 4. Secondary System Failure (Adversarial)
        self.telemetry
            .log_event(
                "swarm",
                "adversarial_alert",
                serde_json::json!({
                    "origin": "RedSwarm",
                    "vulnerability_type": "REMOTE_CODE_INJECTION_SIMULATED",
                    "status": "CONTAINMENT_FAIL"
                }),
            )
            .await;

        tracing::error!("War Room: Cascading Failure Simulation peak reached.");
        Ok(())
    }
}
