use crate::memory::index::MemoryIndex;
use crate::memory::topics::TopicStore;
use std::path::PathBuf;

pub struct MemoryContext {
    pub index_content: String,
    pub index_tokens: usize,
    pub topics: Vec<TopicInjection>,
    pub topics_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug)]
pub struct TopicInjection {
    pub path: String,
    pub content: String,
    pub tokens: usize,
}

pub struct MemoryLoader {
    index: MemoryIndex,
    topics: TopicStore,
    max_topic_tokens: usize,
    max_topics_per_turn: usize,
}

impl MemoryLoader {
    pub fn new(memory_dir: PathBuf) -> Self {
        Self {
            index: MemoryIndex::new(memory_dir.join("MEMORY.md")),
            topics: TopicStore::new(memory_dir.join("topics")),
            max_topic_tokens: 5000,
            max_topics_per_turn: 3,
        }
    }

    pub fn load_for_turn(&self, user_query: Option<&str>) -> anyhow::Result<MemoryContext> {
        let index_content = self.index.load_raw()?;
        let index_tokens = index_content.len() / 4;

        let mut topics = Vec::new();
        let mut topics_tokens = 0;

        if let Some(query) = user_query {
            let relevant = self.index.find_relevant(query)?;
            for entry in relevant.iter().take(self.max_topics_per_turn) {
                if topics_tokens >= self.max_topic_tokens * self.max_topics_per_turn {
                    break;
                }
                if let Some(content) = self.topics.read(&entry.file_path)? {
                    let tokens = content.len() / 4;
                    let (final_content, final_tokens) = if tokens > self.max_topic_tokens {
                        let (truncated, t) = self.truncate_to_tokens(&content, self.max_topic_tokens);
                        (truncated, t)
                    } else {
                        (content, tokens)
                    };

                    topics.push(TopicInjection {
                        path: entry.file_path.clone(),
                        content: final_content,
                        tokens: final_tokens,
                    });
                    topics_tokens += final_tokens;
                }
            }
        }

        let total_tokens = index_tokens + topics_tokens;
        Ok(MemoryContext {
            index_content,
            index_tokens,
            topics,
            topics_tokens,
            total_tokens,
        })
    }

    pub fn format_for_prompt(&self, ctx: &MemoryContext) -> String {
        let mut output = String::new();
        output.push_str("<memory_index>\n");
        output.push_str(&ctx.index_content);
        output.push_str("\n</memory_index>\n");

        if !ctx.topics.is_empty() {
            output.push_str("\n<memory_topics>\n");
            output.push_str("The following topic files were loaded because they are relevant to the current query.\n\n");
            for topic in &ctx.topics {
                output.push_str(&format!("--- {} ---\n", topic.path));
                output.push_str(&topic.content);
                output.push_str("\n\n");
            }
            output.push_str("</memory_topics>\n");
        }
        output
    }

    fn truncate_to_tokens(&self, content: &str, max_tokens: usize) -> (String, usize) {
        let max_chars = max_tokens * 4;
        if content.len() <= max_chars {
            return (content.to_string(), content.len() / 4);
        }

        let truncated_raw = &content[..max_chars];
        let last_newline = truncated_raw.rfind('\n').unwrap_or(max_chars);
        let final_str = format!("{}\n\n... [truncated]", &content[..last_newline]);
        let final_tokens = final_str.len() / 4;
        (final_str, final_tokens)
    }
}
