use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub dingtalk: DingTalkConfig,
    #[serde(default)]
    pub wechat: WeChatConfig,
    #[serde(default)]
    pub manager: ManagerConfig,
    #[serde(default)]
    pub widget: WidgetConfig,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub island: IslandConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub allowed_user_ids: Vec<i64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DingTalkConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct WeChatConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub pushplus_token: String,
    #[serde(default)]
    pub serverchan_sendkey: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ManagerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_events_file")]
    pub events_file: String,
    #[serde(default = "default_max_events_age")]
    pub max_events_age: u64,
    #[serde(default = "default_true")]
    pub open_browser: bool,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            port: 15924,
            events_file: default_events_file(),
            max_events_age: 86400,
            open_browser: true,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct WidgetConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub on_top: bool,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            on_top: true,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneralConfig {
    #[serde(default = "default_sessions_file")]
    pub sessions_file: String,
    #[serde(default = "default_claude_cli")]
    pub claude_cli: String,
    #[serde(default)]
    pub git_bash_path: String,
    #[serde(default = "default_session_ttl")]
    pub session_ttl: u64,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            sessions_file: default_sessions_file(),
            claude_cli: "claude".into(),
            git_bash_path: String::new(),
            session_ttl: 86400,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IslandConfig {
    // Sizes (pixels)
    #[serde(default = "default_pill_width")]
    pub pill_width: u32,
    #[serde(default = "default_pill_width_active")]
    pub pill_width_active: u32,
    #[serde(default = "default_panel_width")]
    pub panel_width: u32,
    #[serde(default = "default_panel_height")]
    pub panel_height: u32,

    // Timing (milliseconds)
    #[serde(default = "default_auto_collapse_ms")]
    pub auto_collapse_ms: u64,
    #[serde(default = "default_hover_expand_ms")]
    pub hover_expand_ms: u64,
    #[serde(default = "default_hover_collapse_ms")]
    pub hover_collapse_ms: u64,

    // Colors (CSS format)
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_color_active")]
    pub color_active: String,
    #[serde(default = "default_color_ready")]
    pub color_ready: String,
    #[serde(default = "default_color_permission")]
    pub color_permission: String,
    #[serde(default = "default_color_notification")]
    pub color_notification: String,

    // Hotkey
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    // Transparency
    #[serde(default = "default_transparency")]
    pub transparency: String,
    #[serde(default = "default_opacity")]
    pub opacity: f64,

    // Sound (per-event type)
    #[serde(default = "default_true")]
    pub sound_enabled: bool,
    #[serde(default = "default_sound_stop")]
    pub sound_stop: String,
    #[serde(default = "default_sound_notification")]
    pub sound_notification: String,
    #[serde(default = "default_sound_permission")]
    pub sound_permission: String,

    // Autostart
    #[serde(default)]
    pub autostart: bool,

    // Permission timeout (seconds)
    #[serde(default = "default_permission_timeout")]
    pub permission_timeout_secs: u64,
}

impl Default for IslandConfig {
    fn default() -> Self {
        Self {
            pill_width: 300,
            pill_width_active: 360,
            panel_width: 480,
            panel_height: 320,
            auto_collapse_ms: 3000,
            hover_expand_ms: 400,
            hover_collapse_ms: 300,
            background: "#000000".into(),
            color_active: "#D97857".into(),
            color_ready: "#66BF73".into(),
            color_permission: "#6699FF".into(),
            color_notification: "#FFB300".into(),
            hotkey: "Alt+D".into(),
            transparency: "off".into(),
            opacity: 0.75,
            sound_enabled: true,
            sound_stop: "asterisk".into(),
            sound_notification: "exclamation".into(),
            sound_permission: "question".into(),
            autostart: false,
            permission_timeout_secs: 600,
        }
    }
}

fn default_permission_timeout() -> u64 { 600 }
fn default_hotkey() -> String { "Alt+D".into() }
fn default_transparency() -> String { "off".into() }
fn default_opacity() -> f64 { 0.75 }
fn default_pill_width() -> u32 { 300 }
fn default_pill_width_active() -> u32 { 360 }
fn default_panel_width() -> u32 { 480 }
fn default_panel_height() -> u32 { 320 }
fn default_auto_collapse_ms() -> u64 { 3000 }
fn default_hover_expand_ms() -> u64 { 400 }
fn default_hover_collapse_ms() -> u64 { 300 }
fn default_background() -> String { "#000000".into() }
fn default_color_active() -> String { "#D97857".into() }
fn default_color_ready() -> String { "#66BF73".into() }
fn default_color_permission() -> String { "#6699FF".into() }
fn default_color_notification() -> String { "#FFB300".into() }
fn default_sound_stop() -> String { "asterisk".into() }
fn default_sound_notification() -> String { "exclamation".into() }
fn default_sound_permission() -> String { "question".into() }

fn default_port() -> u16 { 15924 }
fn default_true() -> bool { true }
fn default_max_events_age() -> u64 { 86400 }
fn default_session_ttl() -> u64 { 86400 }
fn default_claude_cli() -> String { "claude".into() }

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_events_file() -> String {
    app_dir().join("events.jsonl").to_string_lossy().into_owned()
}

fn default_sessions_file() -> String {
    app_dir().join("sessions.json").to_string_lossy().into_owned()
}

pub fn load_config() -> Config {
    let mut config_path = find_config_path();

    // Auto-create config.yaml from example template on first run
    if !config_path.exists() {
        if let Some(example) = find_example_config() {
            // Create config.yaml next to the example file (not in exe dir)
            if let Some(parent) = example.parent() {
                config_path = parent.join("config.yaml");
            }
            match std::fs::copy(&example, &config_path) {
                Ok(_) => tracing::info!("Created {} from {}", config_path.display(), example.display()),
                Err(e) => tracing::warn!("Failed to copy example config: {}", e),
            }
        }
    }

    match std::fs::read_to_string(&config_path) {
        Ok(contents) => {
            serde_yaml::from_str(&contents).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse config {}: {}", config_path.display(), e);
                Config::default()
            })
        }
        Err(_) => {
            tracing::info!("No config file found at {}, using defaults", config_path.display());
            Config::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            telegram: TelegramConfig::default(),
            dingtalk: DingTalkConfig::default(),
            wechat: WeChatConfig::default(),
            manager: ManagerConfig::default(),
            widget: WidgetConfig::default(),
            general: GeneralConfig::default(),
            island: IslandConfig::default(),
        }
    }
}

/// Search for config.example.yaml in all candidate directories.
fn find_example_config() -> Option<PathBuf> {
    let name = "config.example.yaml";
    for dir in config_search_dirs() {
        let p = dir.join(name);
        if p.exists() { return Some(p); }
    }
    None
}

/// All directories to search for config files, in priority order.
fn config_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. Next to executable
    let exe_dir = app_dir();
    dirs.push(exe_dir.join("config"));

    // 2. Walk up from exe directory looking for config/ (handles dev builds
    //    and autostart where CWD != project root). E.g. exe at
    //    project/src-tauri/target/release/ → walks up to project/config/.
    {
        let mut ancestor = exe_dir.as_path();
        for _ in 0..5 {
            if let Some(parent) = ancestor.parent() {
                let candidate = parent.join("config");
                if candidate.is_dir() {
                    dirs.push(candidate);
                }
                ancestor = parent;
            } else {
                break;
            }
        }
    }

    // 3. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("config"));
    }

    // 4. Parent of CWD (covers running from src-tauri/ during development)
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(parent) = cwd.parent() {
            dirs.push(parent.join("config"));
        }
    }

    // 5. %APPDATA%/agent-desk/
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("agent-desk"));
    }

    dirs
}

pub fn find_config_path() -> PathBuf {
    let name = "config.yaml";
    for dir in config_search_dirs() {
        let candidate = dir.join(name);
        if candidate.exists() {
            return candidate;
        }
    }

    // Default: first search dir (exe-relative)
    config_search_dirs().into_iter().next()
        .unwrap_or_else(|| PathBuf::from("config"))
        .join(name)
}

/// Write config file atomically: write to .tmp, then rename.
pub fn atomic_write_config(path: &std::path::Path, content: &str) {
    let tmp = path.with_extension("yaml.tmp");
    if std::fs::write(&tmp, content).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Write island settings to config.yaml using line-based replacement.
///
/// Each entry is `(key, formatted_value)` where the key matches a YAML field
/// name under the `island:` section, and `formatted_value` is the exact YAML
/// value to write (including quotes for strings).
///
/// Example: `save_island_settings(&[("hotkey", "\"Alt+D\""), ("sound_enabled", "true")])`
pub fn save_island_settings(settings: &[(&str, &str)]) {
    let path = find_config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_content: String = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            for &(key, value) in settings {
                if trimmed.starts_with(&format!("{}:", key)) {
                    return format!("  {}: {}", key, value);
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    atomic_write_config(&path, &new_content);
}
