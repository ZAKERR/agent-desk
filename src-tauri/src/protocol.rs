//! Wire protocol types — typed enums and payloads for all API boundaries.
//!
//! Every hook event, session status, and API payload is defined here as the
//! single source of truth. Replaces free-form String fields with compile-time
//! checked enums.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

// ─── Hook Event Types ────────────────────────────────────

/// All hook event types sent by the hook binary.
/// Unknown events deserialize to `Unknown` via `#[serde(other)]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    UserPrompt,
    PreTool,
    Stop,
    Notification,
    SessionStart,
    SessionEnd,
    PermissionRequest,
    #[serde(other)]
    Unknown,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::UserPrompt => write!(f, "user_prompt"),
            Self::PreTool => write!(f, "pre_tool"),
            Self::Stop => write!(f, "stop"),
            Self::Notification => write!(f, "notification"),
            Self::SessionStart => write!(f, "session_start"),
            Self::SessionEnd => write!(f, "session_end"),
            Self::PermissionRequest => write!(f, "permission_request"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// ─── Session Status ──────────────────────────────────────

/// Internal session status. Serializes to snake_case strings for
/// backward-compatible JSON persistence (sessions.json / events.jsonl).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Active,
    Waiting,
    Ended,
    Stopped,
    #[serde(other)]
    Unknown,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Active => write!(f, "active"),
            Self::Waiting => write!(f, "waiting"),
            Self::Ended => write!(f, "ended"),
            Self::Stopped => write!(f, "stopped"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// ─── Permission Decision ─────────────────────────────────

/// Permission decision sent by the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecisionKind {
    Allow,
    Deny,
    AlwaysAllow,
}

impl PermissionDecisionKind {
    /// Map to Claude Code's hookSpecificOutput behavior string.
    pub fn to_behavior(&self) -> &'static str {
        match self {
            Self::Allow | Self::AlwaysAllow => "approve",
            Self::Deny => "deny",
        }
    }
}

// ─── Default helpers for serde ───────────────────────────

fn default_json_object() -> Value {
    Value::Object(serde_json::Map::new())
}

fn default_json_array() -> Value {
    Value::Array(Vec::new())
}

// ─── API Payloads ────────────────────────────────────────

/// POST /api/signal — full event from hook binary.
#[derive(Debug, Clone, Deserialize)]
pub struct SignalPayload {
    pub event: HookEvent,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub notification_type: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub last_assistant_message: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub hook_pid: Option<u32>,
    #[serde(default)]
    pub parent_session_id: Option<String>,
}

/// POST /api/hook body — lightweight status update.
#[derive(Debug, Clone, Deserialize)]
pub struct HookPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
}

/// POST /api/permission-request — tool permission from hook binary.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionRequestPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default = "default_json_object")]
    pub tool_input: Value,
    #[serde(default = "default_json_array")]
    pub permission_suggestions: Value,
}

/// POST /api/permission-respond — user decision from UI.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionRespondPayload {
    pub id: String,
    pub decision: PermissionDecisionKind,
}

/// POST /api/chat/send — send a message to a Claude Code session via SendInput.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatSendPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    pub message: String,
    #[serde(default)]
    pub pid: Option<u32>,
    /// If true, send even when session is active (not waiting). Default false.
    #[serde(default)]
    pub force: bool,
}
