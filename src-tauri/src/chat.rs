//! Claude Code JSONL chat reader.
//!
//! Reads `~/.claude/projects/<project-dir>/<session-uuid>.jsonl`
//! incrementally, deduplicates streaming assistant messages by UUID,
//! and returns parsed chat messages.
//!
//! Two output formats:
//! - v1 (`ChatMessage`): flat role/content — used by `/api/chat`
//! - v2 (`EnrichedMessage`): typed events with model/cost — used by `/api/chat/v2`

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ─── v1 types (unchanged) ───────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub uuid: String,
    pub tool_uses: Vec<ChatToolUse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatToolUse {
    pub name: String,
    #[serde(rename = "type")]
    pub tool_type: String,
}

// ─── v2 types (enriched) ────────────────────────────────

/// Typed chat event — discriminated union serialized via `#[serde(tag = "type")]`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Text { role: String, content: String },
    ToolCall { name: String, input: Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    Thinking { summary: String },
}

/// Enriched message with model info and cost metadata.
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedMessage {
    pub uuid: String,
    pub timestamp: String,
    pub event: ChatEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ─── Session cache ──────────────────────────────────────

struct SessionCache {
    offset: u64,
    messages: Vec<ChatMessage>,
    enriched: Vec<EnrichedMessage>,
    /// UUID → index in messages vec (for dedup of streaming updates)
    uuid_index: HashMap<String, usize>,
    /// UUID → index in enriched vec (for dedup)
    enriched_uuid_index: HashMap<String, usize>,
    last_accessed: Instant,
}

pub struct ChatReader {
    cache: Mutex<HashMap<String, SessionCache>>,
}

impl ChatReader {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Read messages for a session, returning (messages, next_index).
    /// `after` is the index to start from (for incremental reads).
    pub fn read_messages(
        &self,
        session_id: &str,
        cwd: &str,
        after: usize,
    ) -> (Vec<ChatMessage>, usize) {
        self.ensure_parsed(session_id, cwd);
        let cache_key = format!("{}:{}", session_id, cwd);
        let cache_map = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache_map.get(&cache_key) {
            let total = entry.messages.len();
            if after >= total {
                return (vec![], total);
            }
            let slice = entry.messages[after..].to_vec();
            (slice, total)
        } else {
            (vec![], 0)
        }
    }

    /// Read enriched (v2) messages for a session.
    pub fn read_enriched(
        &self,
        session_id: &str,
        cwd: &str,
        after: usize,
    ) -> (Vec<EnrichedMessage>, usize) {
        self.ensure_parsed(session_id, cwd);
        let cache_key = format!("{}:{}", session_id, cwd);
        let cache_map = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache_map.get(&cache_key) {
            let total = entry.enriched.len();
            if after >= total {
                return (vec![], total);
            }
            let slice = entry.enriched[after..].to_vec();
            (slice, total)
        } else {
            (vec![], 0)
        }
    }

    /// Parse new lines from the JSONL file into both v1 and v2 caches.
    fn ensure_parsed(&self, session_id: &str, cwd: &str) {
        let path = session_file_path(session_id, cwd);
        if !path.exists() {
            return;
        }

        let cache_key = format!("{}:{}", session_id, cwd);
        let mut cache_map = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let entry = cache_map.entry(cache_key).or_insert_with(|| SessionCache {
            offset: 0,
            messages: Vec::new(),
            enriched: Vec::new(),
            uuid_index: HashMap::new(),
            enriched_uuid_index: HashMap::new(),
            last_accessed: Instant::now(),
        });
        entry.last_accessed = Instant::now();

        // Read new lines from file
        if let Ok(mut file) = File::open(&path) {
            if let Ok(meta) = file.metadata() {
                let file_len = meta.len();
                if file_len > entry.offset {
                    let _ = file.seek(SeekFrom::Start(entry.offset));
                    let reader = BufReader::new(&file);
                    for line in reader.lines() {
                        let line = match line {
                            Ok(l) => l,
                            Err(_) => break,
                        };
                        if line.trim().is_empty() { continue; }
                        if let Ok(row) = serde_json::from_str::<Value>(&line) {
                            // v1 parsing
                            if let Some(msg) = parse_jsonl_row(&row) {
                                let uuid = msg.uuid.clone();
                                if !uuid.is_empty() {
                                    if let Some(&idx) = entry.uuid_index.get(&uuid) {
                                        entry.messages[idx] = msg;
                                    } else {
                                        let idx = entry.messages.len();
                                        entry.uuid_index.insert(uuid, idx);
                                        entry.messages.push(msg);
                                    }
                                } else {
                                    entry.messages.push(msg);
                                }
                            }
                            // v2 parsing — produces multiple events per row
                            for em in parse_enriched_row(&row) {
                                let uuid = em.uuid.clone();
                                if !uuid.is_empty() {
                                    if let Some(&idx) = entry.enriched_uuid_index.get(&uuid) {
                                        entry.enriched[idx] = em;
                                    } else {
                                        let idx = entry.enriched.len();
                                        entry.enriched_uuid_index.insert(uuid, idx);
                                        entry.enriched.push(em);
                                    }
                                } else {
                                    entry.enriched.push(em);
                                }
                            }
                        }
                    }
                    entry.offset = file_len;
                }
            }
        }
    }

    /// Evict session caches not accessed within `max_age`.
    pub fn evict_stale(&self, max_age: Duration) {
        let mut cache_map = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let cutoff = Instant::now() - max_age;
        cache_map.retain(|_, entry| entry.last_accessed >= cutoff);
    }
}

/// Map CWD to the Claude Code project directory name.
/// Claude replaces `\` `/` `:` `.` with `-`.
fn cwd_to_project_dir(cwd: &str) -> String {
    cwd.replace('\\', "-")
        .replace('/', "-")
        .replace(':', "-")
        .replace('.', "-")
}

/// Build the path to a session's JSONL file.
fn session_file_path(session_id: &str, cwd: &str) -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let project_dir = cwd_to_project_dir(cwd);

    PathBuf::from(&home)
        .join(".claude")
        .join("projects")
        .join(&project_dir)
        .join(format!("{}.jsonl", session_id))
}

// ─── v1 parsing (unchanged) ─────────────────────────────

/// Parse a single JSONL row into a ChatMessage (if it's user or assistant).
fn parse_jsonl_row(row: &Value) -> Option<ChatMessage> {
    let row_type = row.get("type").and_then(|v| v.as_str()).unwrap_or("");

    // Skip progress, system, result rows
    if row_type != "user" && row_type != "assistant" {
        return None;
    }

    let uuid = row.get("uuid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let timestamp = row.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let message = row.get("message")?;
    let role = message.get("role").and_then(|v| v.as_str()).unwrap_or(row_type);

    let (content, tool_uses) = parse_message_content(message);

    if content.is_empty() && tool_uses.is_empty() {
        return None;
    }

    Some(ChatMessage {
        role: role.to_string(),
        content,
        timestamp,
        uuid,
        tool_uses,
    })
}

/// Extract text content and tool_use entries from a message.
fn parse_message_content(message: &Value) -> (String, Vec<ChatToolUse>) {
    let mut texts = Vec::new();
    let mut tools = Vec::new();

    let content = message.get("content");

    match content {
        // String content (simple user messages)
        Some(Value::String(s)) => {
            texts.push(s.clone());
        }
        // Array content (assistant messages with blocks)
        Some(Value::Array(blocks)) => {
            for block in blocks {
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            texts.push(text.to_string());
                        }
                    }
                    "tool_use" => {
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                        tools.push(ChatToolUse {
                            name: name.to_string(),
                            tool_type: "tool_use".to_string(),
                        });
                    }
                    "tool_result" => {
                        // Skip tool results in display
                    }
                    "thinking" => {
                        // Skip thinking blocks
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    (texts.join("\n"), tools)
}

// ─── v2 parsing (enriched) ──────────────────────────────

/// Parse a single JSONL row into zero or more EnrichedMessages.
///
/// A single "assistant" row may produce multiple events:
/// text, tool_call, thinking — each as a separate EnrichedMessage.
fn parse_enriched_row(row: &Value) -> Vec<EnrichedMessage> {
    let row_type = row.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if row_type != "user" && row_type != "assistant" {
        return vec![];
    }

    let uuid = row.get("uuid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let timestamp = row.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let message = match row.get("message") {
        Some(m) => m,
        None => return vec![],
    };

    let model = message.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());

    let usage = row.get("usage").and_then(|u| {
        let input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        if input > 0 || output > 0 {
            Some(TokenUsage { input_tokens: input, output_tokens: output })
        } else {
            None
        }
    });

    let content = message.get("content");
    let mut events = Vec::new();
    let mut seq = 0u32;

    // Helper: generate unique UUID for sub-events within a row
    let make_uuid = |base: &str, seq: u32| -> String {
        if seq == 0 { base.to_string() } else { format!("{}:{}", base, seq) }
    };

    match content {
        Some(Value::String(s)) => {
            let role = message.get("role").and_then(|v| v.as_str()).unwrap_or(row_type);
            events.push(EnrichedMessage {
                uuid: make_uuid(&uuid, seq),
                timestamp: timestamp.clone(),
                event: ChatEvent::Text { role: role.to_string(), content: s.clone() },
                model: model.clone(),
                usage: usage.clone(),
            });
        }
        Some(Value::Array(blocks)) => {
            for block in blocks {
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                let role = message.get("role").and_then(|v| v.as_str()).unwrap_or(row_type);
                                events.push(EnrichedMessage {
                                    uuid: make_uuid(&uuid, seq),
                                    timestamp: timestamp.clone(),
                                    event: ChatEvent::Text { role: role.to_string(), content: text.to_string() },
                                    model: model.clone(),
                                    // Only attach usage to the first event
                                    usage: if seq == 0 { usage.clone() } else { None },
                                });
                                seq += 1;
                            }
                        }
                    }
                    "tool_use" => {
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
                        let input = block.get("input").cloned().unwrap_or(Value::Object(serde_json::Map::new()));
                        events.push(EnrichedMessage {
                            uuid: make_uuid(&uuid, seq),
                            timestamp: timestamp.clone(),
                            event: ChatEvent::ToolCall { name, input },
                            model: model.clone(),
                            usage: if seq == 0 { usage.clone() } else { None },
                        });
                        seq += 1;
                    }
                    "tool_result" => {
                        let tool_use_id = block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let is_error = block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                        let result_content = match block.get("content") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(arr)) => {
                                // Extract text from content blocks
                                arr.iter()
                                    .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }
                            _ => String::new(),
                        };
                        if !result_content.is_empty() || is_error {
                            events.push(EnrichedMessage {
                                uuid: make_uuid(&uuid, seq),
                                timestamp: timestamp.clone(),
                                event: ChatEvent::ToolResult { tool_use_id, content: result_content, is_error },
                                model: None,
                                usage: None,
                            });
                            seq += 1;
                        }
                    }
                    "thinking" => {
                        if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                            // Summarize: first 200 chars
                            let summary = if thinking.len() > 200 {
                                format!("{}...", &thinking[..200])
                            } else {
                                thinking.to_string()
                            };
                            events.push(EnrichedMessage {
                                uuid: make_uuid(&uuid, seq),
                                timestamp: timestamp.clone(),
                                event: ChatEvent::Thinking { summary },
                                model: model.clone(),
                                usage: None,
                            });
                            seq += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    events
}
