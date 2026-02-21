use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        Json,
    },
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};

use crate::adapter::AdapterRegistry;
use crate::config::Config;
use crate::events::{Event, EventStore};
use crate::focus;
use crate::remote;
use crate::session::{SessionTracker, SessionUpdate};
use crate::chat::ChatReader;
use crate::permission::PermissionStore;
use crate::sse::SSEBroadcaster;

pub struct AppState {
    pub config: Arc<Config>,
    pub event_store: EventStore,
    pub session_tracker: SessionTracker,
    pub sse: SSEBroadcaster,
    pub registry: AdapterRegistry,
    pub notify_tray: std::sync::mpsc::Sender<()>,
    pub app_handle: std::sync::OnceLock<tauri::AppHandle>,
    pub last_seen_ts: RwLock<f64>,
    pub permissions: PermissionStore,
    pub chat_reader: ChatReader,
}

impl AppState {
    pub fn new(config: Config) -> (Self, std::sync::mpsc::Receiver<()>) {
        let event_store = EventStore::new(
            config.manager.events_file.clone(),
            config.manager.max_events_age,
        );
        let session_tracker =
            SessionTracker::new(config.general.sessions_file.clone());
        let sse = SSEBroadcaster::new();
        let registry = AdapterRegistry::new();
        let permissions = PermissionStore::new();
        let chat_reader = ChatReader::new();
        let (tx, rx) = std::sync::mpsc::channel();

        (Self {
            config: Arc::new(config),
            event_store,
            session_tracker,
            sse,
            registry,
            notify_tray: tx,
            app_handle: std::sync::OnceLock::new(),
            last_seen_ts: RwLock::new(0.0),
            permissions,
            chat_reader,
        }, rx)
    }
}

pub async fn run_server(state: Arc<AppState>) {
    let port = state.config.manager.port;

    // Background: periodic SSE refresh
    let sse_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            sse_state.sse.broadcast("refresh", json!({}));
        }
    });

    // Background: session tracker flush (sync file I/O → spawn_blocking)
    let flush_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            let s = flush_state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                s.session_tracker.flush_if_dirty();
            })
            .await;
        }
    });

    // Background: hourly event compaction (sync file I/O → spawn_blocking)
    let compact_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            let s = compact_state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                s.event_store.compact();
            })
            .await;
        }
    });

    // Background: process scanner (Win32 syscalls → spawn_blocking)
    let scan_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            let s = scan_state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                s.registry.scan_all();
            })
            .await;
        }
    });

    // CORS: allow tauri://localhost and browser origins to reach the API
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/all", get(api_all))
        .route("/api/events", get(api_events))
        .route("/api/sessions", get(api_sessions))
        .route("/api/status", get(api_status))
        .route("/api/stream", get(api_stream))
        .route("/api/hook", post(api_hook))
        .route("/api/signal", post(api_signal))
        .route("/api/focus", post(api_focus))
        .route("/api/clear", post(api_clear))
        .route("/api/mark_read", post(api_mark_read))
        .route("/api/session/{id}", delete(api_delete_session))
        .route("/api/eval", post(api_eval))
        .route("/api/island/expand", post(api_island_expand))
        .route("/api/island/collapse", post(api_island_collapse))
        .route("/api/permission-request", post(api_permission_request))
        .route("/api/permission-respond", post(api_permission_respond))
        .route("/api/permissions", get(api_permissions))
        .route("/api/chat", get(api_chat))
        .layer(cors)
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind HTTP server");
    tracing::info!("HTTP server listening on {}", addr);

    axum::serve(listener, app)
        .await
        .expect("HTTP server error");
}

// --- Shared helpers ---

pub fn scan_and_merge(state: &AppState) -> Vec<Value> {
    let processes = state.registry.get_cached();
    let tracked = state.session_tracker.get_active(86400);

    // Strategy: session tracker is the source of truth (CWD, status from hooks).
    // Process scanner provides PID/uptime/create_time.
    // Match by CWD, fallback by recency.

    let mut matched_sessions: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut result = Vec::new();

    // Build CWD → tracker info lookup (normalized)
    let mut cwd_tracker: HashMap<String, Vec<&crate::session::SessionInfo>> = HashMap::new();
    for (_sid, info) in &tracked {
        if info.status == "ended" || matched_sessions.contains(&info.session_id) {
            continue;
        }
        let tcwd = info.cwd.replace('/', "\\").to_lowercase();
        let tcwd = tcwd.trim_end_matches('\\').to_string();
        if tcwd.is_empty() { continue; }
        cwd_tracker.entry(tcwd).or_default().push(info);
    }

    for proc in processes.iter() {
        let pcwd = proc.cwd.replace('/', "\\").to_lowercase();
        let pcwd_norm = pcwd.trim_end_matches('\\');

        // Try CWD match
        let tinfo = cwd_tracker.get(pcwd_norm).and_then(|entries| {
            entries.iter()
                .filter(|e| !matched_sessions.contains(&e.session_id))
                .max_by(|a, b| a.updated_at.partial_cmp(&b.updated_at).unwrap_or(std::cmp::Ordering::Equal))
                .copied()
        });

        if let Some(info) = tinfo {
            matched_sessions.insert(info.session_id.clone());
        }

        // Fallback: pair with unmatched tracker entries
        let tinfo = tinfo.or_else(|| {
            tracked.values()
                .filter(|i| i.status != "ended" && !matched_sessions.contains(&i.session_id))
                .max_by(|a, b| a.updated_at.partial_cmp(&b.updated_at).unwrap_or(std::cmp::Ordering::Equal))
        });

        if let Some(info) = tinfo {
            matched_sessions.insert(info.session_id.clone());
        }

        let tracker_status = tinfo.map(|i| i.status.as_str()).unwrap_or("");
        let status = match tracker_status {
            "waiting" | "idle" => "waiting",
            "stopped" | "ended" => "stopped",
            _ => "active",
        };

        let display_cwd = tinfo
            .map(|i| i.cwd.as_str())
            .filter(|c| !c.is_empty())
            .unwrap_or(&proc.cwd);

        result.push(json!({
            "pid": proc.pid,
            "name": proc.name,
            "agent_type": proc.agent_type,
            "cwd": display_cwd,
            "uptime": proc.uptime,
            "create_time": proc.create_time,
            "status": status,
            "session_id": tinfo.map(|i| i.session_id.as_str()).unwrap_or(""),
            "notification_type": tinfo.and_then(|i| i.notification_type.as_deref()).unwrap_or(""),
            "notification_message": tinfo.and_then(|i| i.notification_message.as_deref()).unwrap_or(""),
            "last_message": tinfo.and_then(|i| i.last_message.as_deref()).unwrap_or(""),
        }));
    }
    result
}

pub fn compute_state(processes: &[Value]) -> Value {
    let active_count = processes.len();
    let mut waiting_count = 0;
    let mut working_count = 0;

    for proc in processes {
        match proc.get("status").and_then(|s| s.as_str()) {
            Some("waiting") => waiting_count += 1,
            Some("active") => working_count += 1,
            _ => {}
        }
    }

    let state = if active_count == 0 {
        "sleeping"
    } else if waiting_count > 0 {
        "attention"
    } else if working_count > 0 {
        "thinking"
    } else {
        "done"
    };

    json!({
        "state": state,
        "active_processes": active_count,
        "pending_actions": waiting_count,
    })
}

// --- API handlers ---

#[derive(Deserialize)]
struct AfterQuery {
    after: Option<f64>,
}

async fn api_all(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AfterQuery>,
) -> Json<Value> {
    let after_ts = q.after.unwrap_or(0.0);
    let processes = scan_and_merge(&state);
    let status = compute_state(&processes);
    let events = state.event_store.get_events(after_ts);

    Json(json!({
        "status": status,
        "processes": processes,
        "events": events,
    }))
}

async fn api_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AfterQuery>,
) -> Json<Value> {
    let after_ts = q.after.unwrap_or(0.0);
    let events = state.event_store.get_events(after_ts);
    Json(json!({ "events": events }))
}

async fn api_sessions(State(state): State<Arc<AppState>>) -> Json<Value> {
    let processes = scan_and_merge(&state);
    Json(json!({ "processes": processes }))
}

async fn api_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let processes = scan_and_merge(&state);
    let mut status = compute_state(&processes);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let recent = state.event_store.get_events(now - 300.0).len();
    let last_seen = *state.last_seen_ts.read().unwrap_or_else(|e| e.into_inner());
    let unread_count = state.event_store.get_events(last_seen).len();
    if let Some(obj) = status.as_object_mut() {
        obj.insert("recent_events".to_string(), json!(recent));
        obj.insert("unread_count".to_string(), json!(unread_count));
    }
    Json(status)
}

async fn api_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.sse.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(msg) => Some(Ok(SseEvent::default().data(msg))),
        Err(_) => None, // Lagged — skip
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct HookQuery {
    event: Option<String>,
}

async fn api_hook(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HookQuery>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let event = q.event.unwrap_or_default();
    let sid = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("");

    if !sid.is_empty() && (event == "user_prompt" || event == "pre_tool") {
        state.session_tracker.update(
            sid,
            SessionUpdate {
                status: Some("active".to_string()),
                cwd: Some(cwd.to_string()),
                // Clear stale notification on new activity
                notification_type: Some(String::new()),
                notification_message: Some(String::new()),
                ..Default::default()
            },
        );
        state.sse.broadcast(
            "activity",
            json!({
                "event": event,
                "session_id": sid,
                "cwd": cwd,
            }),
        );
    }

    Json(json!({ "ok": true }))
}

/// Full signal handler — replaces notify.py.
///
/// Called by hook scripts (notify_claude.py / notify_codex.py) via POST /api/signal.
/// Pipeline: session update → event log → SSE broadcast → remote channels.
async fn api_signal(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let event = body
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let sid = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
    let ntype = body
        .get("notification_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let nmsg = body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let last_msg = body
        .get("last_assistant_message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // --- 1. Update session state ---
    if !sid.is_empty() {
        match event {
            "session_start" => {
                state.session_tracker.register(
                    sid,
                    cwd,
                    if model.is_empty() { None } else { Some(model) },
                    None,
                );
            }
            "session_end" => {
                state.session_tracker.update(
                    sid,
                    SessionUpdate {
                        status: Some("ended".into()),
                        cwd: Some(cwd.into()),
                        ..Default::default()
                    },
                );
            }
            "stop" => {
                state.session_tracker.update(
                    sid,
                    SessionUpdate {
                        status: Some("waiting".into()),
                        cwd: Some(cwd.into()),
                        last_message: if last_msg.is_empty() {
                            None
                        } else {
                            Some(last_msg.into())
                        },
                        // Clear notification on stop (back to prompt)
                        notification_type: Some(String::new()),
                        notification_message: Some(String::new()),
                        ..Default::default()
                    },
                );
            }
            "notification" => {
                let status = if ntype == "permission_prompt" {
                    "waiting"
                } else {
                    "idle"
                };
                state.session_tracker.update(
                    sid,
                    SessionUpdate {
                        status: Some(status.into()),
                        cwd: Some(cwd.into()),
                        notification_type: if ntype.is_empty() {
                            None
                        } else {
                            Some(ntype.into())
                        },
                        notification_message: if nmsg.is_empty() {
                            None
                        } else {
                            Some(nmsg.into())
                        },
                        ..Default::default()
                    },
                );
            }
            _ => {}
        }
    }

    // --- 2. Format human-readable message ---
    let short_sid = if sid.len() > 8 { &sid[..8] } else { sid };
    let message = format_event_message(event, short_sid, cwd, ntype, nmsg, last_msg, model);

    // --- 3. Append to event log ---
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let short_id = &uuid::Uuid::new_v4().to_string()[..6];
    let level = match event {
        "session_start" | "session_end" => 1,
        "stop" => 2,
        "notification" => 3,
        _ => 1,
    };

    let evt = Event {
        id: format!("evt_{}_{}", now as u64, short_id),
        ts: now,
        event: event.to_string(),
        session_id: sid.to_string(),
        cwd: cwd.to_string(),
        message: message.clone(),
        notification_type: ntype.to_string(),
        last_assistant_message: last_msg.to_string(),
        level,
        cleared: false,
    };
    state.event_store.append_event(evt);

    // --- 4. SSE broadcast ---
    state.sse.broadcast(
        "event",
        json!({
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "message": &message,
        }),
    );

    // --- 5. Notify tray to refresh ---
    let _ = state.notify_tray.send(());

    // --- 6. Windows toast notification for stop and notification events ---
    if event == "stop" || event == "notification" {
        if let Some(handle) = state.app_handle.get() {
            let proj = cwd.rsplit(['/', '\\']).next().unwrap_or(cwd);
            let (title, body) = match event {
                "stop" => {
                    let truncated = if last_msg.chars().count() > 200 {
                        format!("{}...", last_msg.chars().take(197).collect::<String>())
                    } else {
                        last_msg.to_string()
                    };
                    (format!("\u{2705} \u{4efb}\u{52a1}\u{5b8c}\u{6210} \u{2014} {}", proj), truncated)
                    // ✅ 任务完成 — project
                }
                "notification" => match ntype {
                    "permission_prompt" => {
                        (format!("\u{1f514} \u{9700}\u{8981}\u{64cd}\u{4f5c} \u{2014} {}", proj), nmsg.to_string())
                        // 🔔 需要操作 — project
                    }
                    "idle_prompt" => {
                        (format!("\u{1f4a4} \u{7b49}\u{5f85}\u{8f93}\u{5165} \u{2014} {}", proj),
                         "\u{7b49}\u{5f85}\u{8f93}\u{5165}\u{4e2d}...".to_string())
                        // 💤 等待输入 — project
                    }
                    _ => {
                        (format!("\u{1f4e2} \u{901a}\u{77e5} \u{2014} {}", proj), nmsg.to_string())
                        // 📢 通知 — project
                    }
                },
                _ => (String::new(), String::new()),
            };
            if !title.is_empty() {
                crate::tray::send_notification(handle, &title, &body);
                crate::tray::play_notification_sound();
            }
        }
    }

    // --- 7. Remote channels (async, fire-and-forget) ---
    // Arc::clone is cheap — no deep copy of Config
    let cfg = Arc::clone(&state.config);
    let msg = message.clone();
    tokio::spawn(async move {
        remote::dispatch_remote(&cfg.telegram, &cfg.dingtalk, &cfg.wechat, &msg).await;
    });

    Json(json!({ "ok": true }))
}

/// Format a human-readable event message (same logic as Python's format_message).
fn format_event_message(
    event: &str,
    short_sid: &str,
    cwd: &str,
    ntype: &str,
    nmsg: &str,
    last_msg: &str,
    model: &str,
) -> String {
    match event {
        "stop" => {
            let truncated = if last_msg.chars().count() > 300 {
                format!("{}...", last_msg.chars().take(297).collect::<String>())
            } else {
                last_msg.to_string()
            };
            format!("[Done] {}\n{}\n{}", short_sid, cwd, truncated)
        }
        "notification" => match ntype {
            "permission_prompt" => format!("[Confirm] {}\n{}", short_sid, nmsg),
            "idle_prompt" => format!("[Idle] {} waiting for input", short_sid),
            _ => format!("[Notice] {}\n{}", short_sid, nmsg),
        },
        "session_start" => {
            let m = if model.is_empty() { "unknown" } else { model };
            format!("[Start] {} | {} | {}", short_sid, m, cwd)
        }
        "session_end" => format!("[End] {}", short_sid),
        _ => format!("[{}] {}", event, short_sid),
    }
}

async fn api_focus(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
    let req_pid = body.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);

    if cwd.is_empty() && req_pid.is_none() {
        return Json(json!({ "ok": false, "error": "no cwd or pid" }));
    }

    // Resolve PID from scan_and_merge if not provided
    let pid = req_pid.or_else(|| {
        if cwd.is_empty() { return None; }
        let cwd_norm = cwd.replace('/', "\\").to_lowercase();
        let merged = scan_and_merge(&state);
        merged.iter().find_map(|proc| {
            let pcwd = proc.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
            if pcwd.replace('/', "\\").to_lowercase() == cwd_norm {
                proc.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32)
            } else {
                None
            }
        })
    });

    let cached = state.registry.get_cached();
    let ok = focus::find_and_focus_terminal_with_pid(cwd, &cached, pid);
    Json(json!({ "ok": ok }))
}

/// Debug: eval JS in pet webview
async fn api_eval(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let js = body.get("js").and_then(|v| v.as_str()).unwrap_or("");
    if js.is_empty() {
        return Json(json!({ "ok": false, "error": "no js" }));
    }
    if let Some(handle) = state.app_handle.get() {
        use tauri::Manager;
        if let Some(w) = handle.get_webview_window("island") {
            let _ = tauri::WebviewWindow::eval(&w, js);
            return Json(json!({ "ok": true }));
        }
    }
    Json(json!({ "ok": false, "error": "no webview" }))
}

async fn api_mark_read(State(state): State<Arc<AppState>>) -> Json<Value> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    if let Ok(mut ts) = state.last_seen_ts.write() {
        *ts = now;
    }
    // Notify tray to refresh unread count in tooltip
    let _ = state.notify_tray.send(());
    Json(json!({ "ok": true }))
}

async fn api_clear(State(state): State<Arc<AppState>>) -> Json<Value> {
    state.event_store.clear_all();
    state.sse.broadcast("clear", json!({}));
    Json(json!({ "ok": true }))
}

async fn api_delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Value> {
    state.session_tracker.remove(&id);
    state.session_tracker.flush_if_dirty();
    state.sse.broadcast("refresh", json!({}));
    Json(json!({ "ok": true }))
}

async fn api_island_expand(State(state): State<Arc<AppState>>) -> Json<Value> {
    if let Some(handle) = state.app_handle.get() {
        use tauri::Manager;
        if let Some(w) = handle.get_webview_window("island") {
            crate::island::expand(&w);
            return Json(json!({ "ok": true }));
        }
    }
    Json(json!({ "ok": false, "error": "no island window" }))
}

async fn api_island_collapse(State(state): State<Arc<AppState>>) -> Json<Value> {
    if let Some(handle) = state.app_handle.get() {
        use tauri::Manager;
        if let Some(w) = handle.get_webview_window("island") {
            crate::island::collapse(&w);
            return Json(json!({ "ok": true }));
        }
    }
    Json(json!({ "ok": false, "error": "no island window" }))
}

// ─── Permission endpoints ───────────────────────────────

/// Hook binary POSTs here and blocks until user responds (long-poll).
async fn api_permission_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let tool_name = body.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let tool_input = body.get("tool_input").cloned().unwrap_or(json!({}));
    let permission_suggestions = body.get("permission_suggestions").cloned().unwrap_or(json!([]));

    let id = uuid::Uuid::new_v4().to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();

    let req = crate::permission::PermissionRequest {
        id: id.clone(),
        session_id: session_id.clone(),
        cwd: cwd.clone(),
        tool_name: tool_name.clone(),
        tool_input: tool_input.clone(),
        permission_suggestions: permission_suggestions.clone(),
        timestamp: now,
    };

    let rx = state.permissions.register(req);

    // SSE broadcast + sound + auto-expand island
    state.sse.broadcast("permission_request", json!({
        "id": &id,
        "tool_name": &tool_name,
        "session_id": &session_id,
    }));
    let _ = state.notify_tray.send(());

    if let Some(handle) = state.app_handle.get() {
        use tauri::Manager;
        if let Some(w) = handle.get_webview_window("island") {
            crate::island::expand(&w);
            let _ = w.eval("if(window.onExpand)window.onExpand();fetchPermissions();");
        }
        crate::tray::play_notification_sound();
    }

    // Long-poll: wait for decision (timeout 600s)
    let decision = tokio::time::timeout(
        tokio::time::Duration::from_secs(600),
        rx,
    ).await;

    match decision {
        Ok(Ok(d)) => {
            // Build the hookSpecificOutput that Claude Code expects
            let behavior = match d.decision.as_str() {
                "allow" => "approve",
                "always_allow" => "approve",
                "deny" => "deny",
                other => other,
            };

            // For "always_allow", include updated_permissions from original suggestions
            let updated_permissions = if d.decision == "always_allow" {
                permission_suggestions.clone()
            } else {
                json!([])
            };

            Json(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {
                        "behavior": behavior,
                        "updatedPermissions": updated_permissions,
                    }
                }
            }))
        }
        _ => {
            // Timeout or channel closed — clean up and return deny
            state.permissions.remove(&id);
            Json(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {
                        "behavior": "deny",
                        "updatedPermissions": [],
                    }
                }
            }))
        }
    }
}

/// UI calls this to send a decision.
async fn api_permission_respond(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let decision_str = body.get("decision").and_then(|v| v.as_str()).unwrap_or("deny");

    let decision = crate::permission::PermissionDecision {
        decision: decision_str.to_string(),
    };

    let ok = state.permissions.respond(id, decision);
    Json(json!({ "ok": ok }))
}

/// UI polls this to get pending permission requests.
async fn api_permissions(State(state): State<Arc<AppState>>) -> Json<Value> {
    let requests = state.permissions.get_pending();
    Json(json!({ "requests": requests }))
}

// ─── Chat endpoint ──────────────────────────────────────

#[derive(Deserialize)]
struct ChatQuery {
    session_id: Option<String>,
    cwd: Option<String>,
    after: Option<usize>,
}

async fn api_chat(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatQuery>,
) -> Json<Value> {
    let session_id = q.session_id.unwrap_or_default();
    let cwd = q.cwd.unwrap_or_default();
    let after = q.after.unwrap_or(0);

    if session_id.is_empty() || cwd.is_empty() {
        return Json(json!({ "messages": [], "next_index": 0 }));
    }

    let s = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        s.chat_reader.read_messages(&session_id, &cwd, after)
    }).await.unwrap_or_else(|_| (vec![], 0));

    Json(json!({
        "messages": result.0,
        "next_index": result.1,
    }))
}
