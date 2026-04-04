pub struct TokenCounter;

impl TokenCounter {
    /// Estimate tokens for a string using character-based heuristic
    pub fn estimate(text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        // Detect if content is primarily code
        let code_indicators = ['{', '}', '(', ')', ';', '=', '<', '>'];
        let code_char_count = text.chars().filter(|c| code_indicators.contains(c)).count();
        let code_ratio = code_char_count as f64 / text.len() as f64;

        let chars_per_token = if code_ratio > 0.05 {
            3.5 // Code is more token-dense
        } else {
            4.0 // Natural language
        };

        (text.len() as f64 / chars_per_token).ceil() as usize
    }

    /// Estimate tokens for a JSON value
    pub fn estimate_json(value: &serde_json::Value) -> usize {
        Self::estimate(&value.to_string())
    }

    /// Estimate tokens for an array of API messages
    pub fn estimate_messages(messages: &[serde_json::Value]) -> usize {
        let mut total = 0;
        for msg in messages {
            // Each message has ~4 tokens of overhead
            total += 4;
            total += Self::estimate_json(msg);
        }
        total
    }

    /// Calculate how many characters to keep for a token budget
    pub fn chars_for_tokens(tokens: usize) -> usize {
        tokens * 4 // Conservative estimate
    }
}
