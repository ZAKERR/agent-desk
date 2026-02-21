use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::Value;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use tauri::Manager;

use crate::focus;
use crate::server::AppState;

const ICON_SIZE: u32 = 32;

/// Pre-built RGBA circle icons for each pet state.
static ICONS: LazyLock<HashMap<&str, Vec<u8>>> = LazyLock::new(|| {
    let states: [(&str, u8, u8, u8); 6] = [
        ("sleeping", 0x7f, 0x84, 0x9c),
        ("idle", 0x89, 0xb4, 0xfa),
        ("thinking", 0xfa, 0xb3, 0x87),
        ("done", 0xa6, 0xe3, 0xa1),
        ("attention", 0xf9, 0xe2, 0xaf),
        ("error", 0xf3, 0x8b, 0xa8),
    ];
    let mut map = HashMap::new();
    for (name, r, g, b) in states {
        map.insert(name, generate_circle_icon(r, g, b, ICON_SIZE));
    }
    map
});

/// Monotonic counter — ensures unique menu-item IDs across rebuilds.
static MENU_GEN: AtomicU64 = AtomicU64::new(0);

/// Session-click mapping: menu-item ID → (CWD, PID).
static SESSION_MAP: LazyLock<Mutex<HashMap<String, (String, Option<u32>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Last hash of tray menu content — skip rebuild if unchanged.
static LAST_TRAY_HASH: LazyLock<Mutex<u64>> = LazyLock::new(|| Mutex::new(0));

// ---------------------------------------------------------------------------
// Icon generation
// ---------------------------------------------------------------------------

fn generate_circle_icon(r: u8, g: u8, b: u8, size: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let radius = center - 1.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let offset = ((y * size + x) * 4) as usize;

            if dist <= radius - 0.5 {
                buf[offset] = r;
                buf[offset + 1] = g;
                buf[offset + 2] = b;
                buf[offset + 3] = 255;
            } else if dist <= radius + 0.5 {
                let alpha = ((radius + 0.5 - dist) * 255.0) as u8;
                buf[offset] = r;
                buf[offset + 1] = g;
                buf[offset + 2] = b;
                buf[offset + 3] = alpha;
            }
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn state_label(state: &str) -> &'static str {
    match state {
        "sleeping" => "\u{5728}\u{7761}\u{89c9} zzZ",
        "idle"     => "\u{5728}\u{53d1}\u{5446}",
        "thinking" => "\u{5728}\u{5e72}\u{6d3b}...",
        "done"     => "\u{5e72}\u{5b8c}\u{5566}\u{ff01}",
        "attention"=> "\u{9700}\u{8981}\u{4f60}\u{ff01}",
        "error"    => "\u{51fa}\u{9519}\u{4e86}\u{ff01}",
        _          => "???",
    }
}

fn state_emoji(state: &str) -> &'static str {
    match state {
        "sleeping" => "\u{1f4a4}",  // 💤
        "idle"     => "\u{1f63a}",  // 😺
        "thinking" => "\u{1f525}",  // 🔥
        "done"     => "\u{2705}",   // ✅
        "attention"=> "\u{1f514}",  // 🔔
        "error"    => "\u{274c}",   // ❌
        _          => "\u{1f63e}",  // 😾
    }
}

fn status_text(status: &str) -> &'static str {
    match status {
        "active"  => "\u{5e72}\u{6d3b}\u{4e2d}",
        "waiting" => "\u{7b49}\u{4f60}\u{64cd}\u{4f5c}",
        "stopped" => "\u{5df2}\u{5b8c}\u{6210}",
        _         => "\u{672a}\u{77e5}",
    }
}

fn project_name(cwd: &str) -> &str {
    cwd.rsplit(['/', '\\']).next().unwrap_or(cwd)
}

// ---------------------------------------------------------------------------
// Setup (called once at startup)
// ---------------------------------------------------------------------------

pub fn setup_tray(
    app: &tauri::App,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let seq = MENU_GEN.fetch_add(1, Ordering::Relaxed);

    let header = MenuItem::with_id(
        app,
        format!("header_{}", seq),
        "Agent Desk \u{2014} \u{5728}\u{7761}\u{89c9} zzZ",
        false,
        None::<&str>,
    )?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, format!("quit_{}", seq), "\u{274c} \u{9000}\u{51fa}", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&header, &sep, &quit])?;

    let initial_icon = ICONS.get("sleeping").unwrap();
    let icon = tauri::image::Image::new(initial_icon, ICON_SIZE, ICON_SIZE);

    let panel_w = state.config.island.panel_width;
    let panel_h = state.config.island.panel_height;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .tooltip("Agent Desk \u{2014} \u{5728}\u{7761}\u{89c9} zzZ")
        .on_tray_icon_event({
            use std::sync::atomic::{AtomicU64, Ordering as AtOrd};
            static LAST_TOGGLE: AtomicU64 = AtomicU64::new(0);
            move |tray, event| {
                let is_left_click = matches!(
                    event,
                    TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. }
                    | TrayIconEvent::DoubleClick { button: tauri::tray::MouseButton::Left, .. }
                );
                if !is_left_click { return; }

                // Debounce: ignore toggles within 400ms of each other
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let prev = LAST_TOGGLE.swap(now, AtOrd::Relaxed);
                if now.saturating_sub(prev) < 400 { return; }

                // Left click → show (if hidden) + expand island (non-blocking)
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("island") {
                    let _ = w.show();
                    let _ = w.eval("if(window.onExpand)window.onExpand()");
                    std::thread::spawn(move || {
                        crate::island::expand(&w, panel_w, panel_h);
                    });
                }
            }
        })
        .on_menu_event(move |app, event| {
            let id: &str = event.id.as_ref();

            // Session click → focus terminal
            if let Some((cwd, pid)) = SESSION_MAP.lock().unwrap().get(id).cloned() {
                let cached = state.registry.get_cached();
                focus::find_and_focus_terminal_with_pid(&cwd, &cached, pid);
            } else if id.starts_with("show_") {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("island") {
                    let _ = w.show();
                }
            } else if id.starts_with("clear_") {
                state.event_store.clear_all();
                state.sse.broadcast("clear", serde_json::json!({}));
                let _ = state.notify_tray.send(());
            } else if id.starts_with("quit_") {
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Periodic update (called from tray-updater thread)
// ---------------------------------------------------------------------------

pub fn update_tray(
    handle: &AppHandle,
    state: &AppState,
    status: &Value,
    processes: &[Value],
) {
    let tray = match handle.tray_by_id("main") {
        Some(t) => t,
        None => return,
    };

    let state_str = status
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("sleeping");

    let session_count = processes.len();

    // 1. Icon
    if let Some(rgba) = ICONS.get(state_str) {
        let icon = tauri::image::Image::new(rgba, ICON_SIZE, ICON_SIZE);
        let _ = tray.set_icon(Some(icon));
    }

    // 1b. Push state to island webview (direct sync, no HTTP roundtrip)
    if let Some(w) = handle.get_webview_window("island") {
        let _ = w.eval(&format!(
            "if(window.onTrayState)window.onTrayState('{}',{})",
            state_str, session_count
        ));
    }

    // 2. Tooltip
    let unread = state.last_seen_ts.read().ok().map(|ts| {
        state.event_store.get_events(*ts).len()
    }).unwrap_or(0);

    let tooltip = if session_count == 0 && unread == 0 {
        format!("Agent Desk \u{2014} {}", state_label(state_str))
    } else if session_count == 0 {
        format!(
            "Agent Desk \u{2014} {} \u{00b7} {}\u{6761}\u{672a}\u{8bfb}",
            state_label(state_str), unread,
        )
    } else if unread == 0 {
        format!(
            "Agent Desk \u{2014} {} \u{00b7} {}\u{4e2a}\u{4f1a}\u{8bdd}",
            state_label(state_str), session_count,
        )
    } else {
        format!(
            "Agent Desk \u{2014} {} \u{00b7} {}\u{4e2a}\u{4f1a}\u{8bdd} \u{00b7} {}\u{6761}\u{672a}\u{8bfb}",
            state_label(state_str), session_count, unread,
        )
    };
    let _ = tray.set_tooltip(Some(&tooltip));

    // 3. Menu — skip rebuild if content hash unchanged
    let mut hasher = DefaultHasher::new();
    state_str.hash(&mut hasher);
    unread.hash(&mut hasher);
    for p in processes {
        if let Some(obj) = p.as_object() {
            if let Some(v) = obj.get("pid") { v.to_string().hash(&mut hasher); }
            if let Some(v) = obj.get("status") { v.to_string().hash(&mut hasher); }
            if let Some(v) = obj.get("cwd") { v.to_string().hash(&mut hasher); }
            if let Some(v) = obj.get("notification_type") { v.to_string().hash(&mut hasher); }
        }
    }
    let new_hash = hasher.finish();

    let should_rebuild = {
        let mut last = LAST_TRAY_HASH.lock().unwrap_or_else(|e| e.into_inner());
        if *last == new_hash {
            false
        } else {
            *last = new_hash;
            true
        }
    };

    if should_rebuild {
        match build_menu(handle, state, state_str, processes) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(e) => tracing::error!("Failed to build tray menu: {}", e),
        }
    }
}

// ---------------------------------------------------------------------------
// Menu builder
// ---------------------------------------------------------------------------

fn build_menu(
    handle: &AppHandle,
    state: &AppState,
    state_str: &str,
    processes: &[Value],
) -> Result<Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let seq = MENU_GEN.fetch_add(1, Ordering::Relaxed);
    let menu = Menu::new(handle)?;

    // ── Status header ──
    let header_text = format!(
        "{} Agent Desk \u{2014} {}",
        state_emoji(state_str),
        state_label(state_str),
    );
    menu.append(&MenuItem::with_id(
        handle, format!("header_{}", seq), &header_text, false, None::<&str>,
    )?)?;

    menu.append(&PredefinedMenuItem::separator(handle)?)?;

    // ── Sessions ──
    let mut session_map = SESSION_MAP.lock().unwrap();
    session_map.clear();

    if processes.is_empty() {
        menu.append(&MenuItem::with_id(
            handle, format!("nosess_{}", seq),
            "\u{6ca1}\u{6709}\u{6d3b}\u{8dc3}\u{7684}\u{4f1a}\u{8bdd}",
            false, None::<&str>,
        )?)?;
    } else {
        for (i, proc) in processes.iter().enumerate() {
            let cwd = proc.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
            let proc_status = proc.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            let name = project_name(cwd);

            let indicator = match proc_status {
                "active"  => "\u{1f525}",
                "waiting" => "\u{1f514}",
                "stopped" => "\u{2705}",
                _         => "\u{25cb}",
            };

            let label = format!("{} {} ({})", indicator, name, status_text(proc_status));
            let pid = proc.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
            let id = format!("sess_{}_{}", seq, i);
            session_map.insert(id.clone(), (cwd.to_string(), pid));

            menu.append(&MenuItem::with_id(
                handle, &id, &label, true, None::<&str>,
            )?)?;
        }
    }

    drop(session_map);

    // ── Recent events (last 5) ──
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let events = state.event_store.get_events(now - state.config.general.session_ttl as f64);
    let recent: Vec<_> = events.iter().rev().take(5).collect();

    if !recent.is_empty() {
        menu.append(&PredefinedMenuItem::separator(handle)?)?;
        menu.append(&MenuItem::with_id(
            handle, format!("evthdr_{}", seq),
            "\u{1f4dd} \u{6700}\u{8fd1}\u{52a8}\u{6001}",
            false, None::<&str>,
        )?)?;

        for (i, evt) in recent.iter().enumerate() {
            let first_line = evt.message.lines().next().unwrap_or(&evt.message);
            let display = if first_line.chars().count() > 60 {
                format!("{}...", first_line.chars().take(57).collect::<String>())
            } else {
                first_line.to_string()
            };
            menu.append(&MenuItem::with_id(
                handle, format!("evt_{}_{}", seq, i), &display, false, None::<&str>,
            )?)?;
        }
    }

    // ── Bottom ──
    menu.append(&PredefinedMenuItem::separator(handle)?)?;
    menu.append(&MenuItem::with_id(
        handle, format!("show_{}", seq),
        "\u{1f441} \u{663e}\u{793a}\u{7a97}\u{53e3}",
        true, None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        handle, format!("clear_{}", seq),
        "\u{1f9f9} \u{6e05}\u{7406}\u{52a8}\u{6001}",
        true, None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        handle, format!("quit_{}", seq),
        "\u{274c} \u{9000}\u{51fa}",
        true, None::<&str>,
    )?)?;

    Ok(menu)
}

// ---------------------------------------------------------------------------
// Toast notification
// ---------------------------------------------------------------------------

pub fn send_notification(handle: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = handle
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

/// Play a system notification sound via Win32 MessageBeep.
///
/// `sound_type`: "asterisk" | "hand" | "question" | "exclamation" | "default"
pub fn play_notification_sound(sound_type: &str) {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBeep(uType: u32) -> i32;
        }
        let code = match sound_type {
            "hand"        => 0x00000010,
            "question"    => 0x00000020,
            "exclamation" => 0x00000030,
            "asterisk"    => 0x00000040,
            _             => 0x00000000, // "default" or unknown
        };
        unsafe {
            MessageBeep(code);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = sound_type;
    }
}
