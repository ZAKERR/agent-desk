// Codex CLI adapter — hook event parsing.
// Process scanning is handled by ProcessScanner in process/scanner.rs.

use serde_json::Value;

/// Map Codex hook events to unified event names.
pub fn map_hook_event(event_name: &str, data: &Value) -> (String, String) {
    let sid = data.get("session_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let short_sid = &sid[..sid.len().min(8)];
    let cwd = data.get("cwd").and_then(|v| v.as_str()).unwrap_or("");

    let (unified_event, message) = match event_name {
        "after_agent" => {
            let output = data.get("output").and_then(|v| v.as_str()).unwrap_or("");
            let truncated = if output.len() > 300 { &output[..300] } else { output };
            ("done".to_string(), format!("[Codex Done] {}\n{}\n{}", short_sid, cwd, truncated))
        }
        "after_tool_use" => {
            let tool = data.get("tool_name").and_then(|v| v.as_str()).unwrap_or("tool");
            ("active".to_string(), format!("[Codex Tool] {} | {}", short_sid, tool))
        }
        _ => {
            (event_name.to_string(), format!("[Codex {}] {}", event_name, short_sid))
        }
    };

    (unified_event, message)
}
