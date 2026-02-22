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
use std::collections::HashMap;
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
}

pub struct PermissionStore {
    /// Pending requests (keyed by id).
    requests: Mutex<HashMap<String, PermissionRequest>>,
    /// Oneshot senders waiting for decisions (keyed by request id).
    senders: Mutex<HashMap<String, oneshot::Sender<PermissionDecisionKind>>>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            senders: Mutex::new(HashMap::new()),
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
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), req);
        self.senders.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        rx
    }

    /// Send a decision for a pending request. Returns true if sent.
    pub fn respond(&self, id: &str, decision: PermissionDecisionKind) -> bool {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        if let Some(tx) = self.senders.lock().unwrap_or_else(|e| e.into_inner()).remove(id) {
            tx.send(decision).is_ok()
        } else {
            false
        }
    }

    /// Get all pending requests (for UI display).
    pub fn get_pending(&self) -> Vec<PermissionRequest> {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
    }

    /// Clean up a request (e.g. on timeout).
    pub fn remove(&self, id: &str) {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        self.senders.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    }
}
