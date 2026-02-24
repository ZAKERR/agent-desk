//! Hook daemon — persistent TCP relay that reuses HTTP connections.
//!
//! Listens on `127.0.0.1:{port+1}` (e.g. 15925).
//! Protocol: client sends one JSON line, daemon forwards to agent-desk
//! server using a persistent ureq Agent, then writes response line back.
//!
//! This avoids per-hook HTTP connection setup overhead.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

/// Run the daemon. Blocks forever (until process killed).
pub fn run(port: u16) {
    let daemon_port = port + 1;
    let addr = format!("127.0.0.1:{}", daemon_port);

    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("agent-desk-hook daemon: failed to bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    eprintln!("agent-desk-hook daemon listening on {}", addr);

    // Persistent HTTP agent — reuses TCP connections to the main server
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(660)))
        .build()
        .new_agent();

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Read one JSON line from client
        let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|_| {
            // If clone fails, just skip this connection
            return stream.try_clone().unwrap();
        }));
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            let _ = stream.write_all(b"{\"ok\":false,\"error\":\"empty\"}\n");
            continue;
        }

        let data: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => {
                let _ = stream.write_all(b"{\"ok\":false,\"error\":\"parse\"}\n");
                continue;
            }
        };

        let event = data.get("event").and_then(|v| v.as_str()).unwrap_or("");

        // Route and forward
        let response = match event {
            "user_prompt" => {
                let url = format!("http://127.0.0.1:{}/api/hook?event={}", port, event);
                match agent.post(&url).header("Content-Type", "application/json").send_json(&data) {
                    Ok(mut r) => r.body_mut().read_to_string().unwrap_or_default(),
                    Err(_) => "{\"ok\":false}".to_string(),
                }
            }
            "pre_tool" => {
                // PreToolUse: blocking long-poll to /api/pre-tool-check.
                // Build structured payload from hook data.
                let tool_name = data.get("tool_name")
                    .or_else(|| data.get("toolName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool_input = data.get("tool_input")
                    .or_else(|| data.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let session_id = data.get("session_id")
                    .or_else(|| data.get("sessionId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cwd = data.get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let payload = serde_json::json!({
                    "session_id": session_id,
                    "cwd": cwd,
                    "tool_name": tool_name,
                    "tool_input": tool_input,
                    "raw": &data,
                });

                let url = format!("http://127.0.0.1:{}/api/pre-tool-check", port);
                match agent.post(&url).header("Content-Type", "application/json").send_json(&payload) {
                    Ok(mut r) => r.body_mut().read_to_string().unwrap_or_default(),
                    Err(_) => String::new(), // empty = no output, Claude Code proceeds normally
                }
            }
            "permission_request" => {
                let url = format!("http://127.0.0.1:{}/api/permission-request", port);
                match agent.post(&url).header("Content-Type", "application/json").send_json(&data) {
                    Ok(mut r) => r.body_mut().read_to_string().unwrap_or_default(),
                    Err(_) => String::new(), // empty = Claude Code falls back
                }
            }
            _ => {
                let url = format!("http://127.0.0.1:{}/api/signal", port);
                match agent.post(&url).header("Content-Type", "application/json").send_json(&data) {
                    Ok(mut r) => r.body_mut().read_to_string().unwrap_or_default(),
                    Err(_) => "{\"ok\":false}".to_string(),
                }
            }
        };

        let _ = writeln!(stream, "{}", response);
    }
}

/// Try to send a hook payload via the daemon. Returns Some(response) on success.
pub fn try_send(port: u16, data: &serde_json::Value) -> Option<String> {
    let daemon_port = port + 1;
    let addr = format!("127.0.0.1:{}", daemon_port);

    // Quick connect with short timeout
    let stream = std::net::TcpStream::connect_timeout(
        &addr.parse().ok()?,
        std::time::Duration::from_millis(50),
    ).ok()?;

    // Set read timeout (permission_request and pre_tool can take up to 660s)
    let event = data.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let read_timeout = if event == "permission_request" || event == "pre_tool" {
        std::time::Duration::from_secs(660)
    } else {
        std::time::Duration::from_secs(5)
    };
    let _ = stream.set_read_timeout(Some(read_timeout));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));

    let mut stream_w = stream.try_clone().ok()?;
    let json_line = serde_json::to_string(data).ok()?;
    writeln!(stream_w, "{}", json_line).ok()?;

    // Read response line
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).ok()?;

    Some(response.trim().to_string())
}
