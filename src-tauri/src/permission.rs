//! Permission request/response store.
//!
//! Claude Code `PermissionRequest` hook sends a payload via the hook binary.
//! The hook binary POSTs to `/api/permission-request` which registers the
//! request here and blocks (long-poll) until the user responds from the UI.
//!
//! The UI calls `/api/permission-respond` which sends the decision through
//! a oneshot channel back to the waiting hook handler.

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tokio::sync::oneshot;

use crate::protocol::PermissionDecisionKind;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionRequest {
    pub id: String,
    pub session_id: String,
    pub cwd: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub permission_suggestions: Value,
    pub timestamp: f64,
    pub timeout_secs: u64,
}

pub struct PermissionStore {
    /// Pending requests (keyed by id).
    requests: Mutex<HashMap<String, PermissionRequest>>,
    /// Oneshot senders waiting for decisions (keyed by request id).
    senders: Mutex<HashMap<String, oneshot::Sender<PermissionDecisionKind>>>,
    /// Session-scoped auto-approvals: (session_id, tool_name) → auto-approve.
    session_rules: Mutex<HashSet<(String, String)>>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            senders: Mutex::new(HashMap::new()),
            session_rules: Mutex::new(HashSet::new()),
        }
    }

    /// Register a new permission request. Returns a oneshot Receiver that
    /// the hook handler should await for the user's decision.
    pub fn register(
        &self,
        req: PermissionRequest,
    ) -> oneshot::Receiver<PermissionDecisionKind> {
        let (tx, rx) = oneshot::channel();
        let id = req.id.clone();
        mutex_lock!(self.requests).insert(id.clone(), req);
        mutex_lock!(self.senders).insert(id, tx);
        rx
    }

    /// Send a decision for a pending request. Returns true if sent.
    pub fn respond(&self, id: &str, decision: PermissionDecisionKind) -> bool {
        mutex_lock!(self.requests).remove(id);
        if let Some(tx) = mutex_lock!(self.senders).remove(id) {
            tx.send(decision).is_ok()
        } else {
            false
        }
    }

    /// Get all pending requests (for UI display).
    pub fn get_pending(&self) -> Vec<PermissionRequest> {
        mutex_lock!(self.requests).values().cloned().collect()
    }

    /// Clean up a request (e.g. on timeout).
    pub fn remove(&self, id: &str) {
        mutex_lock!(self.requests).remove(id);
        mutex_lock!(self.senders).remove(id);
    }

    /// Add a session-scoped auto-approve rule.
    pub fn add_session_rule(&self, session_id: &str, tool_name: &str) {
        mutex_lock!(self.session_rules).insert((session_id.to_string(), tool_name.to_string()));
    }

    /// Check if a tool is auto-approved for this session.
    pub fn check_session_rule(&self, session_id: &str, tool_name: &str) -> bool {
        mutex_lock!(self.session_rules).contains(&(session_id.to_string(), tool_name.to_string()))
    }

    /// Clear all session rules for a session (on session end).
    pub fn clear_session_rules(&self, session_id: &str) {
        mutex_lock!(self.session_rules).retain(|(sid, _)| sid != session_id);
    }
}
