use tokio::sync::broadcast;
use serde_json::Value;

const CHANNEL_CAPACITY: usize = 100;

#[derive(Clone)]
pub struct SSEBroadcaster {
    tx: broadcast::Sender<String>,
}

impl SSEBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    /// Broadcast a message to all SSE clients.
    pub fn broadcast(&self, event_type: &str, data: Value) {
        let mut payload = data;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("type".to_string(), Value::String(event_type.to_string()));
        } else {
            payload = serde_json::json!({ "type": event_type });
        }
        let msg = serde_json::to_string(&payload).unwrap_or_default();
        // Ignore send error (no receivers is ok)
        let _ = self.tx.send(msg);
    }

    /// Subscribe to SSE events. Returns a receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Number of active receivers (approximate).
    pub fn client_count(&self) -> usize {
        self.tx.len()
    }
}
