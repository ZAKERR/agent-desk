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
        }
    }
}

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
    let config_path = find_config_path();
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

pub fn find_config_path() -> PathBuf {
    // Check next to executable first
    let exe_dir = app_dir();
    let candidate = exe_dir.join("config").join("config.yaml");
    if candidate.exists() {
        return candidate;
    }

    // Check current working directory
    let cwd = std::env::current_dir().unwrap_or_default();
    let candidate = cwd.join("config").join("config.yaml");
    if candidate.exists() {
        return candidate;
    }

    // Check %APPDATA%/agent-desk/config.yaml
    if let Ok(appdata) = std::env::var("APPDATA") {
        let candidate = PathBuf::from(appdata).join("agent-desk").join("config.yaml");
        if candidate.exists() {
            return candidate;
        }
    }

    // Default path (CWD-based)
    candidate
}
