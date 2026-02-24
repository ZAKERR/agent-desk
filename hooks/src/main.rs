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

/// Walk up the process tree from our PID to find the ancestor `claude.exe`.
/// Process tree: claude.exe → bash/cmd → agent-desk-hook.exe
#[cfg(windows)]
fn find_ancestor_claude_pid() -> Option<u32> {
    use std::mem;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct PROCESSENTRY32W {
        dwSize: u32,
        cntUsage: u32,
        th32ProcessID: u32,
        th32DefaultHeapID: usize,
        th32ModuleID: u32,
        cntThreads: u32,
        th32ParentProcessID: u32,
        pcPriClassBase: i32,
        dwFlags: u32,
        szExeFile: [u16; 260],
    }

    const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    const INVALID_HANDLE_VALUE: isize = -1;

    extern "system" {
        fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> isize;
        fn Process32FirstW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
        fn Process32NextW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }

    // Take a snapshot of all processes
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return None;
    }

    // Build PID → (parent_pid, exe_name) map
    let mut entries: Vec<(u32, u32, String)> = Vec::new();
    let mut pe: PROCESSENTRY32W = unsafe { mem::zeroed() };
    pe.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

    unsafe {
        if Process32FirstW(snap, &mut pe) != 0 {
            loop {
                let name_len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                let name = OsString::from_wide(&pe.szExeFile[..name_len])
                    .to_string_lossy()
                    .to_lowercase();
                entries.push((pe.th32ProcessID, pe.th32ParentProcessID, name));
                pe.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
                if Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }

    // Walk up from our PID, looking for claude.exe
    let my_pid = std::process::id();
    let mut current = my_pid;
    for _ in 0..10 {
        // Find parent of current
        let parent = entries.iter()
            .find(|(pid, _, _)| *pid == current)
            .map(|(_, ppid, _)| *ppid);
        match parent {
            Some(ppid) if ppid != 0 && ppid != current => {
                // Check if parent is claude.exe
                if let Some((_, _, name)) = entries.iter().find(|(pid, _, _)| *pid == ppid) {
                    if name == "claude.exe" {
                        return Some(ppid);
                    }
                }
                current = ppid;
            }
            _ => break,
        }
    }

    None
}

#[cfg(not(windows))]
fn find_ancestor_claude_pid() -> Option<u32> {
    // TODO: implement for non-Windows (walk /proc on Linux)
    None
}

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

    // Inject event type, our PID, and the ancestor claude.exe PID into payload.
    if let Some(obj) = data.as_object_mut() {
        obj.insert("event".into(), serde_json::json!(event));
        obj.insert("hook_pid".into(), serde_json::json!(std::process::id()));
        if let Some(ancestor_pid) = find_ancestor_claude_pid() {
            obj.insert("agent_pid".into(), serde_json::json!(ancestor_pid));
        }
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
        if !response.is_empty() && (event == "permission_request" || event == "pre_tool") {
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
        "user_prompt" => {
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
        "pre_tool" => {
            // PreToolUse: blocking long-poll to /api/pre-tool-check.
            // Response is printed to stdout for Claude Code to read.
            let url = format!("http://127.0.0.1:{}/api/pre-tool-check", port);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(660)))
                .build()
                .new_agent();

            // Build payload with tool_name and tool_input extracted from hook data.
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
                "raw": data,
            });

            let result = agent.post(&url)
                .header("Content-Type", "application/json")
                .send_json(&payload);

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
