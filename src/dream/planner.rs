use crate::dream::{DreamConfig, MemoryOperation, OperationKind};
use std::collections::HashSet;

pub struct DreamPlanner;

impl DreamPlanner {
    pub fn plan(
        mut operations: Vec<MemoryOperation>,
        config: &DreamConfig,
    ) -> Vec<MemoryOperation> {
        Self::deduplicate(&mut operations);
        operations.sort_by_key(|op| Self::priority(&op.kind));
        let validated: Vec<_> = operations
            .into_iter()
            .filter(Self::validate)
            .take(config.max_entries_per_cycle)
            .collect();
        tracing::info!("Dream plan: {} operations", validated.len());
        validated
    }

    fn priority(kind: &OperationKind) -> u32 {
        match kind {
            OperationKind::Conflict { .. } => 0,
            OperationKind::ConsolidateTheme { .. } => 1,
            OperationKind::RefineInstructions { .. } => 2,
            OperationKind::Prune { .. } => 3,
            OperationKind::Merge { .. } => 4,
            OperationKind::Update { .. } => 5,
            OperationKind::Confirm { .. } => 6,
            OperationKind::Create => 7,
            OperationKind::HealAgent { .. } => 8,
        }
    }

    fn deduplicate(operations: &mut Vec<MemoryOperation>) {
        let mut seen = HashSet::new();
        operations.retain(|op| {
            let key = format!(
                "{}/{}/{:?}",
                op.topic,
                op.subtopic,
                std::mem::discriminant(&op.kind)
            );
            seen.insert(key)
        });
    }

    /// Returns true when `value` is a single, safe path segment sequence.
    ///
    /// The analyzer parses JSON produced by an LLM, so `existing_path`,
    /// `source_entries` and `l2_paths` are attacker-influenced when the model is
    /// prompt-injected. They must not contain `..`, absolute roots, or be empty.
    pub fn is_safe_memory_path(value: &str) -> bool {
        if value.trim().is_empty() || value.len() > 512 {
            return false;
        }
        let path = std::path::Path::new(value);
        if path.is_absolute() {
            return false;
        }
        path.components().all(|component| {
            matches!(component, std::path::Component::Normal(_))
                || matches!(component, std::path::Component::CurDir)
        })
    }

    fn validate(op: &MemoryOperation) -> bool {
        if op.topic.is_empty() || op.subtopic.is_empty() {
            return false;
        }
        if !Self::is_safe_memory_path(&format!("{}/{}.md", op.topic, op.subtopic)) {
            return false;
        }
        let paths_ok = match &op.kind {
            OperationKind::Update { existing_path } => Self::is_safe_memory_path(existing_path),
            OperationKind::Merge { source_entries } => {
                !source_entries.is_empty()
                    && source_entries.iter().all(|p| Self::is_safe_memory_path(p))
            }
            OperationKind::ConsolidateTheme { l2_paths } => {
                l2_paths.iter().all(|p| Self::is_safe_memory_path(p))
            }
            _ => true,
        };
        if !paths_ok {
            tracing::warn!(
                "Dream plan: dropping operation with unsafe path (topic '{}')",
                op.topic
            );
            return false;
        }

        match &op.kind {
            OperationKind::Create
            | OperationKind::Update { .. }
            | OperationKind::Merge { .. }
            | OperationKind::ConsolidateTheme { .. }
            | OperationKind::RefineInstructions { .. }
            | OperationKind::Confirm { .. } => !op.content.is_empty() && op.content.len() <= 5000,
            OperationKind::Prune { .. } => true,
            OperationKind::Conflict {
                existing_data,
                new_data,
            } => !existing_data.is_empty() && !new_data.is_empty(),
            OperationKind::HealAgent { .. } => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(kind: OperationKind) -> MemoryOperation {
        MemoryOperation {
            kind,
            topic: "rust".into(),
            subtopic: "borrow-checker".into(),
            content: "Ownership rules apply.".into(),
            reasoning: "test".into(),
            confidence: 0.9,
        }
    }

    #[test]
    fn rejects_unsafe_memory_paths() {
        assert!(!DreamPlanner::is_safe_memory_path("../../.ssh/authorized_keys"));
        assert!(!DreamPlanner::is_safe_memory_path("/etc/passwd"));
        assert!(!DreamPlanner::is_safe_memory_path(""));
        assert!(DreamPlanner::is_safe_memory_path("rust/borrow.md"));
    }

    #[test]
    fn traversal_through_operation_fields_is_dropped() {
        let ops = vec![
            operation(OperationKind::Update {
                existing_path: "../../.ssh/authorized_keys".into(),
            }),
            operation(OperationKind::Merge {
                source_entries: vec!["../../../etc/passwd".into()],
            }),
        ];
        assert!(DreamPlanner::plan(ops, &DreamConfig::default()).is_empty());
    }

    #[test]
    fn safe_operations_survive_validation() {
        let ops = vec![
            operation(OperationKind::Create),
            operation(OperationKind::Update {
                existing_path: "rust/borrow.md".into(),
            }),
            operation(OperationKind::Merge {
                source_entries: vec!["rust/borrow.md".into()],
            }),
        ];
        let planned = DreamPlanner::plan(ops, &DreamConfig::default());
        assert_eq!(planned.len(), 3);
    }

    #[test]
    fn topic_traversal_is_rejected() {
        let mut op = operation(OperationKind::Create);
        op.topic = "../../etc".into();
        assert!(DreamPlanner::plan(vec![op], &DreamConfig::default()).is_empty());
    }
}
