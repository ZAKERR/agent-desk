//! Permission request/response store.
//!
//! Claude Code `PermissionRequest` hook sends a payload via the hook binary.
//! The hook binary POSTs to `/api/permission-request` which registers the
//! request here and blocks (long-poll) until the user responds from the UI.
//!
//! The UI calls `/api/permission-respond` which sends the decision through
//! a oneshot channel back to the waiting hook handler.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

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

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionDecision {
    /// "allow" | "deny" | "always_allow"
    pub decision: String,
}

pub struct PermissionStore {
    /// Pending requests (keyed by id).
    requests: Mutex<HashMap<String, PermissionRequest>>,
    /// Oneshot senders waiting for decisions (keyed by request id).
    senders: Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>,
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
    ) -> oneshot::Receiver<PermissionDecision> {
        let (tx, rx) = oneshot::channel();
        let id = req.id.clone();
        self.requests.lock().unwrap().insert(id.clone(), req);
        self.senders.lock().unwrap().insert(id, tx);
        rx
    }

    /// Send a decision for a pending request. Returns true if sent.
    pub fn respond(&self, id: &str, decision: PermissionDecision) -> bool {
        self.requests.lock().unwrap().remove(id);
        if let Some(tx) = self.senders.lock().unwrap().remove(id) {
            tx.send(decision).is_ok()
        } else {
            false
        }
    }

    /// Get all pending requests (for UI display).
    pub fn get_pending(&self) -> Vec<PermissionRequest> {
        self.requests.lock().unwrap().values().cloned().collect()
    }

    /// Clean up a request (e.g. on timeout).
    pub fn remove(&self, id: &str) {
        self.requests.lock().unwrap().remove(id);
        self.senders.lock().unwrap().remove(id);
    }
}
