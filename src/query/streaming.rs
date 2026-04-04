//! SSE stream parser for real-time token streaming from LLM providers.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

/// A single chunk of data from a streaming response.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Delta text content, if any.
    pub text: Option<String>,
    /// Whether this is the final chunk.
    pub is_done: bool,
    /// Stop reason, if this is the final chunk.
    pub stop_reason: Option<String>,
}

/// Parse a single SSE line into a `StreamChunk`, handling both Anthropic and
/// OpenAI stream formats.
pub fn parse_sse_line(line: &str, provider: &str) -> Option<StreamChunk> {
    let data = line.strip_prefix("data: ")?;

    if data == "[DONE]" {
        return Some(StreamChunk {
            text: None,
            is_done: true,
            stop_reason: Some("stop".to_string()),
        });
    }

    let json: Value = serde_json::from_str(data).ok()?;

    match provider {
        "anthropic" => parse_anthropic_chunk(&json),
        "openai" => parse_openai_chunk(&json),
        _ => None,
    }
}

fn parse_anthropic_chunk(json: &Value) -> Option<StreamChunk> {
    let event_type = json["type"].as_str()?;
    match event_type {
        "content_block_delta" => {
            let text = json["delta"]["text"].as_str()?.to_string();
            Some(StreamChunk {
                text: Some(text),
                is_done: false,
                stop_reason: None,
            })
        }
        "message_stop" => Some(StreamChunk {
            text: None,
            is_done: true,
            stop_reason: json["stop_reason"].as_str().map(|s| s.to_string()),
        }),
        _ => None,
    }
}

fn parse_openai_chunk(json: &Value) -> Option<StreamChunk> {
    let choice = &json["choices"][0];
    let finish_reason = choice["finish_reason"].as_str();
    let text = choice["delta"]["content"].as_str().map(|s| s.to_string());

    if finish_reason.is_some() {
        return Some(StreamChunk {
            text,
            is_done: true,
            stop_reason: finish_reason.map(|s| s.to_string()),
        });
    }

    Some(StreamChunk {
        text,
        is_done: false,
        stop_reason: None,
    })
}
