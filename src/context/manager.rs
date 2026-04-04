use crate::context::auto_compact::AutoCompact;
use crate::context::full_compact::FullCompact;
use crate::context::micro_compact::MicroCompact;
use crate::context::token_counter::TokenCounter;
use crate::query::engine::LLMProvider;
use serde_json::Value;
use tracing::{info, warn};

pub struct ContextManager {
    max_context_tokens: usize,
    auto_compact_threshold: f64,
    full_compact_threshold: f64,
    micro_compact: MicroCompact,
    auto_compact: AutoCompact,
    full_compact: FullCompact,
    pub compaction_events: u32,
    pub total_tokens_saved: usize,
}

#[derive(Debug)]
pub struct CompactionReport {
    pub stage_fired: CompactionStage,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub tokens_saved: usize,
}

#[derive(Debug, Clone)]
pub enum CompactionStage {
    None,
    MicroCompact,
    AutoCompact,
    FullCompact,
    ReactiveCompact,
}

impl ContextManager {
    pub fn new(max_context_tokens: usize, auto_compact_threshold: f64) -> Self {
        Self {
            max_context_tokens,
            auto_compact_threshold,
            full_compact_threshold: 0.95,
            micro_compact: MicroCompact::new(),
            auto_compact: AutoCompact::new(),
            full_compact: FullCompact::new(),
            compaction_events: 0,
            total_tokens_saved: 0,
        }
    }

    pub async fn check_and_compact(
        &mut self,
        messages: &mut Vec<Value>,
        current_turn: u64,
        provider: &dyn LLMProvider,
    ) -> anyhow::Result<Option<CompactionReport>> {
        let current_tokens = TokenCounter::estimate_messages(messages);
        let ratio = current_tokens as f64 / self.max_context_tokens as f64;

        info!("Context: {}/{} tokens ({:.1}%)", current_tokens, self.max_context_tokens, ratio * 100.0);

        if ratio < self.auto_compact_threshold {
            return Ok(None);
        }

        let tokens_before = current_tokens;

        // STAGE 1: MicroCompact
        info!(" Stage 1: MicroCompact");
        let micro_saved = self.micro_compact.compact(messages, current_turn);
        let after_micro = TokenCounter::estimate_messages(messages);
        let micro_ratio = after_micro as f64 / self.max_context_tokens as f64;

        if micro_saved > 0 {
            info!(" MicroCompact saved ~{} tokens ({:.1}% -> {:.1}%)", micro_saved, ratio * 100.0, micro_ratio * 100.0);
        }

        if micro_ratio < self.auto_compact_threshold {
            self.record_compaction(tokens_before - after_micro);
            return Ok(Some(CompactionReport {
                stage_fired: CompactionStage::MicroCompact,
                tokens_before,
                tokens_after: after_micro,
                tokens_saved: tokens_before - after_micro,
            }));
        }

        // STAGE 2: AutoCompact
        if !self.auto_compact.is_disabled() {
            info!(" Stage 2: AutoCompact");
            match self.auto_compact.compact(messages, provider).await {
                Ok(summary) => {
                    let total_len = messages.len();
                    let preserved_messages: Vec<Value> = messages[total_len.saturating_sub(summary.turns_preserved)..].to_vec();

                    messages.clear();
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": format!("[CONVERSATION SUMMARY - {} turns compressed]\n\n{}", summary.turns_compressed, summary.summary_text)
                    }));
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": "I've reviewed the conversation summary and I'm ready to continue."
                    }));
                    messages.extend(preserved_messages);

                    let after_auto = TokenCounter::estimate_messages(messages);
                    info!(" AutoCompact: {} turns compressed, {:.1}% -> {:.1}%", 
                        summary.turns_compressed, micro_ratio * 100.0, after_auto as f64 / self.max_context_tokens as f64 * 100.0);

                    if (after_auto as f64 / self.max_context_tokens as f64) < self.full_compact_threshold {
                        self.record_compaction(tokens_before - after_auto);
                        return Ok(Some(CompactionReport {
                            stage_fired: CompactionStage::AutoCompact,
                            tokens_before,
                            tokens_after: after_auto,
                            tokens_saved: tokens_before - after_auto,
                        }));
                    }
                }
                Err(e) => {
                    warn!("AutoCompact failed: {}", e);
                }
            }
        }

        // STAGE 3: FullCompact
        info!(" Stage 3: FullCompact (emergency)");
        let result = self.full_compact.compact(messages, provider).await?;
        *messages = result.compacted_messages;
        let after_full = TokenCounter::estimate_messages(messages);
        
        info!(" FullCompact: {} -> {} tokens, {} files re-injected", tokens_before, after_full, result.files_reinjected.len());
        self.record_compaction(tokens_before - after_full);
        
        Ok(Some(CompactionReport {
            stage_fired: CompactionStage::FullCompact,
            tokens_before,
            tokens_after: after_full,
            tokens_saved: tokens_before - after_full,
        }))
    }

    pub async fn handle_413(
        &mut self,
        messages: &mut Vec<Value>,
        provider: &dyn LLMProvider,
    ) -> anyhow::Result<CompactionReport> {
        warn!(" Reactive Compact triggered (413 from API)");
        let tokens_before = TokenCounter::estimate_messages(messages);
        let result = self.full_compact.compact(messages, provider).await?;
        *messages = result.compacted_messages;
        let tokens_after = TokenCounter::estimate_messages(messages);
        self.record_compaction(tokens_before - tokens_after);

        Ok(CompactionReport {
            stage_fired: CompactionStage::ReactiveCompact,
            tokens_before,
            tokens_after,
            tokens_saved: tokens_before - tokens_after,
        })
    }

    fn record_compaction(&mut self, saved: usize) {
        self.compaction_events += 1;
        self.total_tokens_saved += saved;
    }
}
