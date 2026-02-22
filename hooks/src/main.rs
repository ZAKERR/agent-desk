//! agent-desk-hook — lightweight hook forwarder.
//!
//! Reads JSON from stdin (Claude Code hook payload),
//! adds the event type, and POSTs to the Agent Desk server.
//!
//! Usage:
//!   agent-desk-hook --event stop [--port 15924]
//!
//! Handles all hook types:
//!   Light (→ /api/hook):  user_prompt, pre_tool
//!   Heavy (→ /api/signal): stop, notification, session_start, session_end
//!   Permission (→ /api/permission-request): permission_request (long-poll, stdout response)

use std::io::Read;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse --event and --port
    let mut event = String::new();
    let mut port: u16 = 15924;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--event" | "-e" => {
                i += 1;
                if i < args.len() {
                    event = args[i].clone();
                }
            }
            "--port" | "-p" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().unwrap_or(15924);
                }
            }
            _ => {}
        }
        i += 1;
    }

    if event.is_empty() {
        eprintln!("Usage: agent-desk-hook --event <event_type> [--port <port>]");
        process::exit(1);
    }

    // Read stdin (Claude Code sends hook JSON payload via stdin)
    let mut stdin_buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin_buf);

    let mut data: serde_json::Value = if stdin_buf.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&stdin_buf).unwrap_or_else(|_| serde_json::json!({}))
    };

    // Inject event type and our PID into payload.
    if let Some(obj) = data.as_object_mut() {
        obj.insert("event".into(), serde_json::json!(event));
        obj.insert("hook_pid".into(), serde_json::json!(std::process::id()));
    }

    // Route to the appropriate endpoint
    match event.as_str() {
        // Light hooks → /api/hook (just status update, no event log)
        "user_prompt" | "pre_tool" => {
            // Known light event
        }
        "permission_request" => {
            // Known permission event (handled below)
        }
        "stop" | "notification" | "session_start" | "session_end" => {
            // Known heavy event (handled below)
        }
        other => {
            if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                eprintln!("agent-desk-hook: unrecognized event '{}', forwarding to /api/signal", other);
            }
        }
    }

    // Route to the appropriate endpoint
    match event.as_str() {
        "user_prompt" | "pre_tool" => {
            let url = format!("http://127.0.0.1:{}/api/hook?event={}", port, event);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(3)))
                .build()
                .new_agent();

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(&data);

            if let Err(e) = result {
                if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                    eprintln!("agent-desk-hook: {} -> {}", url, e);
                }
            }
        }
        // Permission request → /api/permission-request (long-poll, stdout response)
        "permission_request" => {
            let url = format!("http://127.0.0.1:{}/api/permission-request", port);
            // Long timeout: wait for user to respond (up to 660s)
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(660)))
                .build()
                .new_agent();

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(&data);

            match result {
                Ok(mut resp) => {
                    // Read response body and write to stdout for Claude Code
                    if let Ok(body) = resp.body_mut().read_to_string() {
                        println!("{}", body);
                    }
                }
                Err(e) => {
                    // Server not running or timeout — exit silently
                    if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                        eprintln!("agent-desk-hook: {} -> {}", url, e);
                    }
                    // Don't output anything → Claude Code falls back to default behavior
                }
            }
        }
        // Heavy hooks → /api/signal (full pipeline)
        _ => {
            let url = format!("http://127.0.0.1:{}/api/signal", port);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(3)))
                .build()
                .new_agent();

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(&data);

            if let Err(e) = result {
                if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                    eprintln!("agent-desk-hook: {} -> {}", url, e);
                }
            }
        }
    }
}
