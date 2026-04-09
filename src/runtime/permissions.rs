use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Moderate,
    Dangerous,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Allow,
    Deny(String),
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    Default,           // Ask for dangerous operations
    AcceptEdits,       // Auto-approve file edits, ask for bash
    BypassPermissions, // Full auto (CI/container mode)
    ReadOnly,          // No writes, no bash
    Plan,              // Propose only, never execute
}

impl std::str::FromStr for AgentMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "accept-edits" | "acceptedits" => Ok(AgentMode::AcceptEdits),
            "bypass" | "yolo" | "dangerously-skip-permissions" => Ok(AgentMode::BypassPermissions),
            "readonly" | "read-only" => Ok(AgentMode::ReadOnly),
            "plan" => Ok(AgentMode::Plan),
            _ => Ok(AgentMode::Default),
        }
    }
}

impl std::fmt::Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentMode::Default => write!(f, "default"),
            AgentMode::AcceptEdits => write!(f, "accept-edits"),
            AgentMode::BypassPermissions => write!(f, "bypass-permissions"),
            AgentMode::ReadOnly => write!(f, "read-only"),
            AgentMode::Plan => write!(f, "plan"),
        }
    }
}

pub struct PermissionGate {
    pub mode: AgentMode,
    pub allow_patterns: Vec<GlobPattern>,
    pub deny_patterns: Vec<GlobPattern>,
}

#[derive(Debug, Clone)]
pub struct GlobPattern {
    pub tool_name: String,
    pub pattern: Option<Regex>,
}

impl GlobPattern {
    pub fn parse(s: &str) -> Option<Self> {
        // Format: "ToolName(pattern)" or "ToolName"
        let s = s.trim();
        if let Some(open) = s.find('(') {
            if let Some(close) = s.find(')') {
                let tool_name = s[..open].trim().to_string();
                let pattern_str = &s[open + 1..close];
                let regex_str = pattern_str.replace('*', ".*").replace('?', ".");
                return Some(Self {
                    tool_name,
                    pattern: Regex::new(&format!("^{}$", regex_str)).ok(),
                });
            }
        }
        Some(Self {
            tool_name: s.to_string(),
            pattern: None,
        })
    }

    pub fn matches(&self, tool_name: &str, argument: &str) -> bool {
        if self.tool_name != tool_name && self.tool_name != "*" {
            return false;
        }
        match &self.pattern {
            Some(re) => re.is_match(argument),
            None => true, // No pattern = match all invocations
        }
    }
}

impl PermissionGate {
    pub fn new(mode: AgentMode, allow: &[String], deny: &[String]) -> Self {
        let allow_patterns = allow.iter().filter_map(|s| GlobPattern::parse(s)).collect();
        let deny_patterns = deny.iter().filter_map(|s| GlobPattern::parse(s)).collect();
        Self {
            mode,
            allow_patterns,
            deny_patterns,
        }
    }

    pub fn check(&self, tool_name: &str, risk: RiskLevel, signature: &str) -> Permission {
        // LAYER 1: Mode check
        match self.mode {
            AgentMode::BypassPermissions => return Permission::Allow,
            AgentMode::ReadOnly => {
                if risk != RiskLevel::Safe {
                    return Permission::Deny("Read-only mode: operations blocked".into());
                }
            }
            AgentMode::Plan => {
                return Permission::Deny("Plan mode: execution disabled".into());
            }
            _ => {}
        }

        // LAYER 2: Deny list (always checked, even in bypass if we wanted, but here it's after bypass)
        for pattern in &self.deny_patterns {
            if pattern.matches(tool_name, signature) {
                return Permission::Deny(format!("Blocked by deny rule: {}", signature));
            }
        }

        // LAYER 3: Allow list
        for pattern in &self.allow_patterns {
            if pattern.matches(tool_name, signature) {
                return Permission::Allow;
            }
        }

        // LAYER 4: Risk-based fallback
        match (&self.mode, risk) {
            (_, RiskLevel::Safe) => Permission::Allow,
            (AgentMode::AcceptEdits, RiskLevel::Moderate) => {
                if tool_name == "FileWrite" {
                    Permission::Allow
                } else {
                    Permission::Ask
                }
            }
            _ => Permission::Ask,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deny_list_blocks() {
        let gate = PermissionGate::new(AgentMode::Default, &[], &["Bash(rm*)".to_string()]);
        let result = gate.check("Bash", RiskLevel::Dangerous, "rm -rf /");
        assert!(matches!(result, Permission::Deny(_)));
    }

    #[test]
    fn test_allow_list_permits() {
        let gate = PermissionGate::new(AgentMode::Default, &["Bash(git*)".to_string()], &[]);
        let result = gate.check("Bash", RiskLevel::Dangerous, "git status");
        assert_eq!(result, Permission::Allow);
    }

    #[test]
    fn test_readonly_mode() {
        let gate = PermissionGate::new(AgentMode::ReadOnly, &[], &[]);
        assert_eq!(
            gate.check("FileRead", RiskLevel::Safe, "test.txt"),
            Permission::Allow
        );
        assert!(matches!(
            gate.check("FileWrite", RiskLevel::Moderate, "test.txt"),
            Permission::Deny(_)
        ));
    }
}
