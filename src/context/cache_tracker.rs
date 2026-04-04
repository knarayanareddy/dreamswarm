use sha2::{Sha256, Digest};
use serde_json::Value;
use tracing::info;

#[derive(Debug, Clone)]
pub struct CacheTracker {
    current_mode: String,
    active_tools_hash: u64,
    system_prompt_hash: u64,
    memory_index_hash: u64,
    feature_flags: Vec<StickyFlag>,
    model_id: String,
    cache_breaks: u32,
    last_prompt_hash: u64,
}

#[derive(Debug, Clone)]
pub struct StickyFlag {
    pub name: String,
    pub active: bool,
    pub sticky: bool,
}

impl CacheTracker {
    pub fn new() -> Self {
        Self {
            current_mode: "default".to_string(),
            active_tools_hash: 0,
            system_prompt_hash: 0,
            memory_index_hash: 0,
            feature_flags: Vec::new(),
            model_id: String::new(),
            cache_breaks: 0,
            last_prompt_hash: 0,
        }
    }

    pub fn activate_flag(&mut self, name: &str) {
        if let Some(flag) = self.feature_flags.iter_mut().find(|f| f.name == name) {
            flag.active = true;
        } else {
            self.feature_flags.push(StickyFlag {
                name: name.to_string(),
                active: true,
                sticky: true,
            });
        }
    }

    pub fn deactivate_flag(&mut self, name: &str) -> bool {
        if let Some(flag) = self.feature_flags.iter_mut().find(|f| f.name == name) {
            if flag.sticky && flag.active {
                info!("Cannot deactivate sticky flag '{}' - would break prompt cache", name);
                return false;
            }
            flag.active = false;
        }
        true
    }

    pub fn record_prompt(&mut self, prompt: &str) {
        let new_hash = self.hash_string(prompt);
        if new_hash != self.last_prompt_hash && self.last_prompt_hash != 0 {
            self.cache_breaks += 1;
            info!("Prompt cache break #{} detected", self.cache_breaks);
        }
        self.last_prompt_hash = new_hash;
    }

    pub fn update_mode(&mut self, mode: &str) -> bool {
        if self.current_mode != mode {
            self.current_mode = mode.to_string();
            true
        } else {
            false
        }
    }

    pub fn update_model(&mut self, model_id: &str) -> bool {
        if self.model_id != model_id {
            self.model_id = model_id.to_string();
            true
        } else {
            false
        }
    }

    pub fn update_memory_index(&mut self, index_content: &str) -> bool {
        let new_hash = self.hash_string(index_content);
        if self.memory_index_hash != new_hash {
            self.memory_index_hash = new_hash;
            true
        } else {
            false
        }
    }

    fn hash_string(&self, s: &str) -> u64 {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        let result = hasher.finalize();
        u64::from_be_bytes(result[..8].try_into().unwrap())
    }

    pub fn total_cache_breaks(&self) -> u32 {
        self.cache_breaks
    }

    pub fn active_flags(&self) -> Vec<String> {
        self.feature_flags.iter().filter(|f| f.active).map(|f| f.name.clone()).collect()
    }
}
