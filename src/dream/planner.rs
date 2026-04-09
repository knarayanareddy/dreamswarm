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
            OperationKind::Prune { .. } => 1,
            OperationKind::Merge { .. } => 2,
            OperationKind::Update { .. } => 3,
            OperationKind::Confirm { .. } => 4,
            OperationKind::Create => 5,
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

    fn validate(op: &MemoryOperation) -> bool {
        if op.topic.is_empty() || op.subtopic.is_empty() {
            return false;
        }
        match &op.kind {
            OperationKind::Create
            | OperationKind::Update { .. }
            | OperationKind::Merge { .. }
            | OperationKind::Confirm { .. } => !op.content.is_empty() && op.content.len() <= 2000,
            OperationKind::Prune { .. } => true,
            OperationKind::Conflict {
                existing_data,
                new_data,
            } => !existing_data.is_empty() && !new_data.is_empty(),
        }
    }
}
