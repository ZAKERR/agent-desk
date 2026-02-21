//! Claude Code JSONL chat reader.
//!
//! Reads `~/.claude/projects/<project-dir>/<session-uuid>.jsonl`
//! incrementally, deduplicates streaming assistant messages by UUID,
//! and returns parsed chat messages.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;

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

struct SessionCache {
    offset: u64,
    messages: Vec<ChatMessage>,
    /// UUID → index in messages vec (for dedup of streaming updates)
    uuid_index: HashMap<String, usize>,
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
        let path = session_file_path(session_id, cwd);
        if !path.exists() {
            return (vec![], 0);
        }

        let cache_key = format!("{}:{}", session_id, cwd);
        let mut cache_map = self.cache.lock().unwrap();
        let entry = cache_map.entry(cache_key).or_insert_with(|| SessionCache {
            offset: 0,
            messages: Vec::new(),
            uuid_index: HashMap::new(),
        });

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
                            if let Some(msg) = parse_jsonl_row(&row) {
                                let uuid = msg.uuid.clone();
                                if !uuid.is_empty() {
                                    if let Some(&idx) = entry.uuid_index.get(&uuid) {
                                        // Streaming update — replace existing
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
                        }
                    }
                    entry.offset = file_len;
                }
            }
        }

        let total = entry.messages.len();
        if after >= total {
            return (vec![], total);
        }

        let slice = entry.messages[after..].to_vec();
        (slice, total)
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
