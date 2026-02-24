use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::SessionStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub status: SessionStatus,
    #[serde(default)]
    pub started_at: f64,
    #[serde(default)]
    pub updated_at: f64,
    #[serde(default)]
    pub last_message: Option<String>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub notification_message: Option<String>,
    #[serde(default)]
    pub agent_pid: Option<u32>,
    #[serde(default)]
    pub parent_session_id: Option<String>,
}

pub struct SessionTracker {
    sessions: RwLock<HashMap<String, SessionInfo>>,
    path: PathBuf,
    dirty: AtomicBool,
}

fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

impl SessionTracker {
    pub fn new(path: String) -> Self {
        let path = PathBuf::from(&path);
        let mut sessions = Self::load_from_file(&path);

        // Startup cleanup: demote stale "active" sessions.
        // Sessions stuck in Active from a previous run (no hook will update them)
        // get demoted to Idle so they don't show as "Working..." on restart.
        let now = now_ts();
        let stale_threshold = 300.0; // 5 minutes
        let mut cleaned = false;
        for info in sessions.values_mut() {
            let is_stale = (now - info.updated_at) > stale_threshold;
            let should_demote = is_stale && matches!(
                info.status,
                SessionStatus::Active | SessionStatus::Waiting | SessionStatus::Stopped
            );
            if should_demote {
                info.status = SessionStatus::Idle;
                // Do NOT update updated_at — keep old timestamp so Phase 2
                // matching won't treat this as a recently-active session.
                cleaned = true;
            }
        }

        let dirty = AtomicBool::new(cleaned);
        Self {
            sessions: RwLock::new(sessions),
            path,
            dirty,
        }
    }

    fn load_from_file(path: &PathBuf) -> HashMap<String, SessionInfo> {
        match fs::read_to_string(path) {
            Ok(contents) => {
                serde_json::from_str(&contents).unwrap_or_default()
            }
            Err(_) => HashMap::new(),
        }
    }

    /// Register a new session.
    pub fn register(&self, session_id: &str, cwd: &str, model: Option<&str>, agent_pid: Option<u32>) {
        let now = now_ts();
        let info = SessionInfo {
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            model: model.map(|s| s.to_string()),
            status: SessionStatus::Idle,
            started_at: now,
            updated_at: now,
            last_message: None,
            notification_type: None,
            notification_message: None,
            agent_pid,
            parent_session_id: None,
        };
        let mut sessions = write_lock!(self.sessions);
        sessions.insert(session_id.to_string(), info);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Update session fields.
    pub fn update(&self, session_id: &str, updates: SessionUpdate) {
        let now = now_ts();
        let mut sessions = write_lock!(self.sessions);
        let entry = sessions.entry(session_id.to_string()).or_insert_with(|| {
            SessionInfo {
                session_id: session_id.to_string(),
                cwd: String::new(),
                model: None,
                status: SessionStatus::Unknown,
                started_at: now,
                updated_at: now,
                last_message: None,
                notification_type: None,
                notification_message: None,
                agent_pid: None,
                parent_session_id: None,
            }
        });

        if let Some(status) = updates.status {
            entry.status = status;
        }
        if let Some(cwd) = updates.cwd {
            entry.cwd = cwd;
        }
        if let Some(msg) = updates.last_message {
            entry.last_message = Some(msg);
        }
        if let Some(nt) = updates.notification_type {
            entry.notification_type = Some(nt);
        }
        if let Some(nm) = updates.notification_message {
            entry.notification_message = Some(nm);
        }
        if let Some(pid) = updates.agent_pid {
            entry.agent_pid = Some(pid);
        }
        if let Some(parent) = updates.parent_session_id {
            entry.parent_session_id = Some(parent);
        }
        entry.updated_at = now;
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Get sessions updated within TTL.
    pub fn get_active(&self, ttl: u64) -> HashMap<String, SessionInfo> {
        let now = now_ts();
        let cutoff = now - ttl as f64;
        let sessions = read_lock!(self.sessions);
        sessions.iter()
            .filter(|(_, info)| info.updated_at >= cutoff)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Resolve a short ID prefix to full session ID.
    pub fn resolve_short_id(&self, prefix: &str) -> Option<String> {
        let sessions = read_lock!(self.sessions);
        let matches: Vec<&String> = sessions.keys()
            .filter(|k| k.starts_with(prefix))
            .collect();
        if matches.len() == 1 {
            Some(matches[0].clone())
        } else {
            None
        }
    }

    /// Remove a session by ID.
    pub fn remove(&self, session_id: &str) {
        let mut sessions = write_lock!(self.sessions);
        if sessions.remove(session_id).is_some() {
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    /// Purge sessions that ended more than `ttl` seconds ago.
    pub fn purge_stale(&self, ttl: u64) {
        let now = now_ts();
        let cutoff = now - ttl as f64;
        let mut sessions = write_lock!(self.sessions);
        let before = sessions.len();
        sessions.retain(|_, info| {
            !(info.status == SessionStatus::Ended && info.updated_at < cutoff)
        });
        if sessions.len() < before {
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    /// Flush to disk if dirty. Call periodically.
    pub fn flush_if_dirty(&self) {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        let sessions = read_lock!(self.sessions);
        let json = serde_json::to_string_pretty(&*sessions).unwrap_or_default();
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, json);
    }
}

/// Builder for session updates (avoids needing many optional params).
#[derive(Default)]
pub struct SessionUpdate {
    pub status: Option<SessionStatus>,
    pub cwd: Option<String>,
    pub last_message: Option<String>,
    pub notification_type: Option<String>,
    pub notification_message: Option<String>,
    pub agent_pid: Option<u32>,
    pub parent_session_id: Option<String>,
}
