//! agent-desk-hook — lightweight hook forwarder.
//!
//! Reads JSON from stdin (Claude Code hook payload),
//! adds the event type, and POSTs to the Agent Desk server.
//!
//! Usage:
//!   agent-desk-hook --event stop [--port 15924]
//!   agent-desk-hook --daemon [--port 15924]
//!
//! Handles all hook types:
//!   Light (→ /api/hook):  user_prompt, pre_tool
//!   Heavy (→ /api/signal): stop, notification, session_start, session_end
//!   Permission (→ /api/permission-request): permission_request (long-poll, stdout response)
//!
//! Daemon mode: listens on port+1, reuses HTTP connections for lower latency.

mod daemon;

use std::io::Read;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse --event, --port, --daemon
    let mut event = String::new();
    let mut port: u16 = 15924;
    let mut daemon_mode = false;
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
            "--daemon" => {
                daemon_mode = true;
            }
            _ => {}
        }
        i += 1;
    }

    // Daemon mode: run persistent TCP relay
    if daemon_mode {
        daemon::run(port);
        return;
    }

    if event.is_empty() {
        eprintln!("Usage: agent-desk-hook --event <event_type> [--port <port>]");
        eprintln!("       agent-desk-hook --daemon [--port <port>]");
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

    // Validate event type
    match event.as_str() {
        "user_prompt" | "pre_tool" | "permission_request"
        | "stop" | "notification" | "session_start" | "session_end" => {}
        other => {
            if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                eprintln!("agent-desk-hook: unrecognized event '{}', forwarding to /api/signal", other);
            }
        }
    }

    // Try daemon relay first (fast path — reuses HTTP connections)
    if let Some(response) = daemon::try_send(port, &data) {
        if !response.is_empty() && event == "permission_request" {
            println!("{}", response);
        }
        return;
    }

    // Fallback: direct HTTP (cold path — new connection per request)
    send_direct(port, &event, &data);
}

/// Direct HTTP send (fallback when daemon is not running).
fn send_direct(port: u16, event: &str, data: &serde_json::Value) {
    match event {
        "user_prompt" | "pre_tool" => {
            let url = format!("http://127.0.0.1:{}/api/hook?event={}", port, event);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(3)))
                .build()
                .new_agent();

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(data);

            if let Err(e) = result {
                if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                    eprintln!("agent-desk-hook: {} -> {}", url, e);
                }
            }
        }
        "permission_request" => {
            let url = format!("http://127.0.0.1:{}/api/permission-request", port);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(660)))
                .build()
                .new_agent();

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(data);

            match result {
                Ok(mut resp) => {
                    if let Ok(body) = resp.body_mut().read_to_string() {
                        println!("{}", body);
                    }
                }
                Err(e) => {
                    if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                        eprintln!("agent-desk-hook: {} -> {}", url, e);
                    }
                }
            }
        }
        _ => {
            let url = format!("http://127.0.0.1:{}/api/signal", port);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(3)))
                .build()
                .new_agent();

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(data);

            if let Err(e) = result {
                if std::env::var("AGENT_DESK_DEBUG").is_ok() {
                    eprintln!("agent-desk-hook: {} -> {}", url, e);
                }
            }
        }
    }
}
