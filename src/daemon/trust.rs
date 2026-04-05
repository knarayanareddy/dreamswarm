use crate::daemon::Urgency;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustSystem {
    pub current_level: f64,
    pub consecutive_denials: u32,
    pub consecutive_approvals: u32,
    pub total_denials: u32,
    pub total_approvals: u32,
    pub last_decision: Option<TrustDecision>,
    pub degradation_rate: f64,
    pub recovery_rate: f64,
    pub min_level: f64,
    pub history: Vec<TrustDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustDecision {
    pub timestamp: DateTime<Utc>,
    pub action_description: String,
    pub approved: bool,
    pub trust_before: f64,
    pub trust_after: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrustPermission {
    AutoAct,
    AskFirst,
    ObserveOnly,
    Paused,
}

impl Default for TrustSystem {
    fn default() -> Self {
        Self {
            current_level: 1.0,
            consecutive_denials: 0,
            consecutive_approvals: 0,
            total_denials: 0,
            total_approvals: 0,
            last_decision: None,
            degradation_rate: 0.15,
            recovery_rate: 0.05,
            min_level: 0.1,
            history: Vec::new(),
        }
    }
}

impl TrustSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_approval(&mut self, action_description: &str) {
        let before = self.current_level;
        self.consecutive_approvals += 1;
        self.consecutive_denials = 0;
        self.total_approvals += 1;
        self.current_level = (self.current_level + self.recovery_rate).min(1.0);
        if self.consecutive_approvals >= 3 {
            self.current_level = (self.current_level + self.recovery_rate).min(1.0);
        }
        let decision = TrustDecision {
            timestamp: Utc::now(),
            action_description: action_description.to_string(),
            approved: true,
            trust_before: before,
            trust_after: self.current_level,
        };
        self.last_decision = Some(decision.clone());
        self.history.push(decision);
        self.trim_history();
        tracing::info!(
            "Trust: {:.2} -> {:.2} (approval: {})",
            before,
            self.current_level,
            action_description
        );
    }

    pub fn record_denial(&mut self, action_description: &str) {
        let before = self.current_level;
        self.consecutive_denials += 1;
        self.consecutive_approvals = 0;
        self.total_denials += 1;
        self.current_level = (self.current_level - self.degradation_rate).max(self.min_level);
        if self.consecutive_denials >= 3 {
            self.current_level =
                (self.current_level - self.degradation_rate * 0.5).max(self.min_level);
            tracing::warn!(
                "{} consecutive denials - accelerated trust degradation",
                self.consecutive_denials
            );
        }
        let decision = TrustDecision {
            timestamp: Utc::now(),
            action_description: action_description.to_string(),
            approved: false,
            trust_before: before,
            trust_after: self.current_level,
        };
        self.last_decision = Some(decision.clone());
        self.history.push(decision);
        self.trim_history();
        tracing::info!(
            "Trust: {:.2} -> {:.2} (denial: {})",
            before,
            self.current_level,
            action_description
        );
    }

    pub fn permission_for(&self, urgency: &Urgency) -> TrustPermission {
        if self.current_level < 0.2 {
            return TrustPermission::Paused;
        }
        if self.current_level < 0.3 {
            return TrustPermission::ObserveOnly;
        }
        match (self.current_level, urgency) {
            (t, _) if t >= 0.8 => match urgency {
                Urgency::Critical => TrustPermission::AskFirst,
                _ => TrustPermission::AutoAct,
            },
            (t, _) if t >= 0.5 => match urgency {
                Urgency::Low | Urgency::Medium => TrustPermission::AutoAct,
                _ => TrustPermission::AskFirst,
            },
            _ => TrustPermission::AskFirst,
        }
    }

    pub fn should_auto_act(&self, urgency: &Urgency) -> bool {
        self.permission_for(urgency) == TrustPermission::AutoAct
    }

    pub fn is_paused(&self) -> bool {
        self.current_level < 0.2
    }

    pub fn reset(&mut self) {
        self.current_level = 1.0;
        self.consecutive_denials = 0;
        self.consecutive_approvals = 0;
        tracing::info!("Trust reset to 1.0");
    }

    fn trim_history(&mut self) {
        if self.history.len() > 100 {
            self.history.drain(0..self.history.len() - 100);
        }
    }
}
