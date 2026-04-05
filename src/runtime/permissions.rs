use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Moderate,
    Dangerous,
    Critical,
}

pub struct PermissionGate;

impl Default for PermissionGate {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionGate {
    pub fn new() -> Self {
        Self
    }

    pub fn check(&self, _tool_name: &str, _risk: RiskLevel, _signature: &str) -> bool {
        // Will implement 5-layer checking logic later
        true
    }
}
