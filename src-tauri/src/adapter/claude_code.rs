// Claude Code adapter — hook event parsing.
// Process scanning is handled by ProcessScanner in process/scanner.rs.

use serde_json::Value;

/// Map Claude Code hook events to unified event names.
pub fn map_hook_event(event_name: &str, data: &Value) -> (String, String) {
    let sid = data.get("session_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let short_sid = &sid[..sid.len().min(8)];
    let cwd = data.get("cwd").and_then(|v| v.as_str()).unwrap_or("");

    let (unified_event, message) = match event_name {
        "user_prompt" | "pre_tool" => {
            ("active".to_string(), format!("[{}] {}", event_name, short_sid))
        }
        "stop" => {
            let last_msg = data.get("last_assistant_message")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let truncated = if last_msg.len() > 300 { &last_msg[..300] } else { last_msg };
            ("done".to_string(), format!("[Done] {}\n{}\n{}", short_sid, cwd, truncated))
        }
        "notification" => {
            let ntype = data.get("notification_type").and_then(|v| v.as_str()).unwrap_or("");
            let msg = data.get("message").and_then(|v| v.as_str()).unwrap_or("");
            match ntype {
                "permission_prompt" => {
                    ("waiting".to_string(), format!("[Confirm] {}\n{}", short_sid, msg))
                }
                "idle_prompt" => {
                    ("done".to_string(), format!("[Idle] {} waiting for input", short_sid))
                }
                _ => {
                    ("done".to_string(), format!("[Notice] {}\n{}", short_sid, msg))
                }
            }
        }
        "session_start" => {
            let model = data.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");
            ("session_start".to_string(), format!("[Start] {} | {} | {}", short_sid, model, cwd))
        }
        "session_end" => {
            ("session_end".to_string(), format!("[End] {}", short_sid))
        }
        _ => {
            (event_name.to_string(), format!("[{}] {}", event_name, short_sid))
        }
    };

    (unified_event, message)
}
