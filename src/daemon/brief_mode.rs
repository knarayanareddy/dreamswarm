use crate::daemon::Urgency;

pub struct BriefFormatter;

impl BriefFormatter {
    pub fn format_action(action_type: &str, description: &str, result: &str) -> String {
        let now = chrono::Utc::now().format("%H:%M");
        format!(
            "[{}] {} {} -> {}",
            now,
            Self::action_emoji(action_type),
            description,
            Self::truncate(result, 120)
        )
    }

    pub fn format_observation(description: &str) -> String {
        let now = chrono::Utc::now().format("%H:%M");
        format!("[{}] {}", now, Self::truncate(description, 150))
    }

    pub fn format_notification(message: &str, urgency: &Urgency) -> String {
        let icon = match urgency {
            Urgency::Low => "i",
            Urgency::Medium => "!",
            Urgency::High => "!!",
            Urgency::Critical => "🚨",
        };
        format!("{} {}", icon, message)
    }

    pub fn format_status_bar(
        trust_pct: f64,
        actions_today: u64,
        cost_today: f64,
        idle_mins: u64,
    ) -> String {
        let trust_bar = Self::progress_bar(trust_pct, 10);
        format!(
            "🌙 KAIROS | Trust: {} {:.0}% | Actions: {} | Cost: ${:.3} | Idle: {}m",
            trust_bar,
            trust_pct * 100.0,
            actions_today,
            cost_today,
            idle_mins
        )
    }

    pub fn format_approval_request(action: &str, reasoning: &str) -> String {
        format!(
            "🌙 KAIROS wants to act:\nAction: {}\nReason: {}\n[y/N] > ",
            Self::truncate(action, 100),
            Self::truncate(reasoning, 200)
        )
    }

    fn action_emoji(action_type: &str) -> &'static str {
        match action_type {
            "RunTests" | "test" => "🧪",
            "FixBuildError" | "fix" => "🔧",
            "RespondToPR" | "pr" => "📋",
            "UpdateDocs" | "docs" => "📝",
            "SendNotification" | "notify" => "🔔",
            _ => "⚡",
        }
    }

    fn truncate(s: &str, max: usize) -> String {
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}...", &s[..max - 3])
        }
    }

    fn progress_bar(fraction: f64, width: usize) -> String {
        let filled = (fraction * width as f64).round() as usize;
        let empty = width.saturating_sub(filled);
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
    }
}
