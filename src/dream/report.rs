use crate::dream::DreamReport;

pub struct DreamReporter;

impl DreamReporter {
    pub fn format(report: &DreamReport) -> String {
        let mut output = String::new();
        output.push_str("╔══════════════════════════════════════════╗\n");
        output.push_str("║ autoDream Report                         ║\n");
        output.push_str("╚══════════════════════════════════════════╝\n\n");
        output.push_str(&format!("Duration: {}s\nTokens: {} | Cost: ${:.4}\n\n", report.duration_secs, report.tokens_consumed, report.cost_usd));
        output.push_str(&format!("Observations: {}\nOperations Applied: {}\n\n", report.observations_collected, report.operations_applied));
        output.push_str(&format!("🔀 Merged: {}\n➕ Created: {}\n🗑 Pruned: {}\n✅ Confirmed: {}\n", report.entries_merged, report.entries_created, report.entries_pruned, report.entries_confirmed));
        if !report.errors.is_empty() {
            output.push_str(&format!("\n⚠ Errors ({}):\n", report.errors.len()));
            for err in &report.errors { output.push_str(&format!(" - {}\n", err)); }
        }
        output.push_str(&format!("\nMemory: {} → {}\n", &report.memory_before_hash[..8], &report.memory_after_hash[..8]));
        output
    }

    pub fn format_brief(report: &DreamReport) -> String {
        format!("autoDream: {}s, {} obs → {} ops ({} merged, {} created, {} pruned, {} confirmed, {} contradictions), {} tokens, ${:.4}",
            report.duration_secs, report.observations_collected, report.operations_applied,
            report.entries_merged, report.entries_created, report.entries_pruned, report.entries_confirmed, report.contradictions_resolved, report.tokens_consumed, report.cost_usd)
    }
}
