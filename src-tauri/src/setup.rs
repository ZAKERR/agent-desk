//! Auto-configure Claude Code hooks on first launch.
//!
//! Finds the bundled `agent-desk-hook.exe` next to the main executable,
//! then ensures `~/.claude/settings.json` has hook entries for all events.

use serde_json::{json, Value};
use std::path::PathBuf;

/// Claude Code hook name → agent-desk-hook `--event` argument.
///
/// NOTE: PreToolUse is intentionally excluded. It blocks the terminal and
/// prevents the user from approving permissions there. PermissionRequest
/// is the correct hook — it fires when Claude Code's native permission
/// system triggers and allows agent-desk to approve, with timeout fallback
/// to the terminal prompt.
const HOOK_EVENTS: &[(&str, &str)] = &[
    ("UserPromptSubmit", "user_prompt"),
    ("Stop", "stop"),
    ("Notification", "notification"),
    ("SessionStart", "session_start"),
    ("SessionEnd", "session_end"),
    ("PermissionRequest", "permission_request"),
];

/// Locate `agent-desk-hook.exe` next to the running executable.
fn hook_binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let hook = exe.parent()?.join("agent-desk-hook.exe");
    hook.exists().then_some(hook)
}

/// Kill any orphaned hook daemon from a previous run.
/// Checks if anything is listening on the daemon port (port+1) and tries to connect.
pub fn kill_orphaned_daemon(port: u16) {
    let daemon_port = port + 1;
    let addr = format!("127.0.0.1:{}", daemon_port);
    // If we can connect, something is listening — kill it via taskkill
    if std::net::TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        std::time::Duration::from_millis(100),
    ).is_ok() {
        tracing::info!("Orphaned hook daemon detected on port {}, killing...", daemon_port);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/IM", "agent-desk-hook.exe"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .status();
        }
        // Brief wait for port to be released
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Spawn the hook daemon process (persistent TCP relay).
/// The daemon reuses HTTP connections for lower per-hook latency.
/// Returns the daemon PID so it can be killed on exit.
pub fn spawn_hook_daemon(port: u16) -> Option<u32> {
    let hook_path = match hook_binary_path() {
        Some(p) => p,
        None => {
            tracing::debug!("Hook binary not found, skipping daemon spawn");
            return None;
        }
    };

    use std::process::Command;
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(&hook_path);
    cmd.args(["--daemon", "--port", &port.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    match cmd.spawn()
    {
        Ok(child) => {
            let pid = child.id();
            tracing::info!("Hook daemon spawned (PID {})", pid);
            Some(pid)
        }
        Err(e) => {
            tracing::warn!("Failed to spawn hook daemon: {}", e);
            None
        }
    }
}

/// Kill the hook daemon by PID.
pub fn kill_hook_daemon(pid: u32) {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        unsafe {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(handle, 0);
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                tracing::info!("Hook daemon killed (PID {})", pid);
            }
        }
    }
    #[cfg(not(windows))]
    {
        use std::process::Command;
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}

/// `%USERPROFILE%/.claude/settings.json`
fn claude_settings_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// Check if a hook entry (flat or nested) contains the given substring in its command.
///
/// Flat format:  `{ "type": "command", "command": "...agent-desk-hook..." }`
/// Nested format: `{ "hooks": [{ "type": "command", "command": "...agent-desk-hook..." }] }`
fn item_contains_hook(item: &Value, needle: &str) -> bool {
    // Flat: item.command
    if let Some(cmd) = item.get("command").and_then(|c| c.as_str()) {
        if cmd.contains(needle) {
            return true;
        }
    }
    // Nested: item.hooks[*].command
    if let Some(hooks_arr) = item.get("hooks").and_then(|h| h.as_array()) {
        for hook in hooks_arr {
            if let Some(cmd) = hook.get("command").and_then(|c| c.as_str()) {
                if cmd.contains(needle) {
                    return true;
                }
            }
        }
    }
    false
}

/// Ensure all Agent Desk hooks are present in `~/.claude/settings.json`.
///
/// - Missing file → created with full hooks config
/// - Missing `hooks` key → added
/// - Missing events → appended (user's other hooks preserved)
/// - Existing agent-desk-hook entries → path updated (handles reinstall to new location)
pub fn ensure_hooks_configured() {
    let hook_path = match hook_binary_path() {
        Some(p) => p,
        None => {
            tracing::debug!("Hook binary not found next to exe, skipping auto-configure");
            return;
        }
    };

    let settings_path = match claude_settings_path() {
        Some(p) => p,
        None => {
            tracing::warn!("Cannot determine home directory, skipping hooks auto-configure");
            return;
        }
    };

    // Ensure ~/.claude/ directory exists
    if let Some(parent) = settings_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("Failed to create {}: {}", parent.display(), e);
            return;
        }
    }

    // Read existing settings or start fresh
    let mut settings: Value = if settings_path.exists() {
        std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };

    let root = match settings.as_object_mut() {
        Some(obj) => obj,
        None => {
            tracing::warn!("settings.json is not a JSON object, skipping");
            return;
        }
    };

    // Ensure "hooks" is an object
    if !root.get("hooks").is_some_and(|v| v.is_object()) {
        root.insert("hooks".into(), json!({}));
    }
    let hooks = root["hooks"].as_object_mut().unwrap();

    // Use forward slashes — Claude Code executes hooks via bash, which eats backslashes
    let hook_cmd_path = hook_path.to_string_lossy().replace('\\', "/");
    let mut changed = false;

    // Clean up stale PreToolUse hook (removed — it blocks the terminal).
    if let Some(Value::Array(arr)) = hooks.get_mut("PreToolUse") {
        let before = arr.len();
        arr.retain(|item| !item_contains_hook(item, "agent-desk-hook"));
        if arr.len() < before {
            changed = true;
            tracing::info!("Removed stale PreToolUse agent-desk-hook entry");
        }
        // Remove the key entirely if the array is now empty
        if arr.is_empty() {
            hooks.remove("PreToolUse");
        }
    }

    for &(claude_event, hook_arg) in HOOK_EVENTS {
        let command = format!("{} --event {}", hook_cmd_path, hook_arg);
        // PermissionRequest is a long-poll: hook blocks until user responds.
        // Needs a large timeout so Claude Code doesn't kill the hook early.
        let hook_obj = if claude_event == "PermissionRequest" {
            json!({ "type": "command", "command": command, "timeout": 600 })
        } else {
            json!({ "type": "command", "command": command })
        };
        let entry = json!({ "hooks": [hook_obj] });

        match hooks.get_mut(claude_event) {
            Some(Value::Array(arr)) => {
                // Find existing agent-desk-hook entry (check both nested and flat formats)
                let idx = arr.iter().position(|item| item_contains_hook(item, "agent-desk-hook"));
                match idx {
                    Some(i) if arr[i] == entry => {} // already up-to-date
                    Some(i) => {
                        arr[i] = entry;
                        changed = true;
                    }
                    None => {
                        arr.push(entry);
                        changed = true;
                    }
                }
            }
            _ => {
                // Missing or non-array → create
                hooks.insert(claude_event.into(), json!([entry]));
                changed = true;
            }
        }
    }

    // NOTE: We intentionally do NOT modify permissions.allow.
    // Claude Code's native permission system handles terminal prompts.
    // The PermissionRequest hook gives agent-desk a chance to approve,
    // with automatic fallback to the terminal when it times out.

    if !changed {
        tracing::debug!("Hooks already configured, no changes needed");
        return;
    }

    match serde_json::to_string_pretty(&settings) {
        Ok(json_str) => match std::fs::write(&settings_path, json_str) {
            Ok(_) => tracing::info!("Auto-configured hooks in {}", settings_path.display()),
            Err(e) => tracing::warn!("Failed to write {}: {}", settings_path.display(), e),
        },
        Err(e) => tracing::warn!("Failed to serialize settings: {}", e),
    }
}
