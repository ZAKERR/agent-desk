use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::HookEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub ts: f64,
    pub event: HookEvent,
    pub session_id: String,
    pub cwd: String,
    pub message: String,
    #[serde(default)]
    pub notification_type: String,
    #[serde(default)]
    pub last_assistant_message: String,
    #[serde(default = "default_level")]
    pub level: u8,
    #[serde(default)]
    pub cleared: bool,
}

fn default_level() -> u8 { 1 }

struct EventCache {
    events: Vec<Event>,
    last_mtime: Option<SystemTime>,
    last_size: u64,
}

pub struct EventStore {
    path: PathBuf,
    max_age: u64,
    cache: RwLock<EventCache>,
}

impl EventStore {
    pub fn new(path: String, max_age: u64) -> Self {
        Self {
            path: PathBuf::from(&path),
            max_age,
            cache: RwLock::new(EventCache {
                events: Vec::new(),
                last_mtime: None,
                last_size: 0,
            }),
        }
    }

    /// Read events, using mtime cache to avoid re-reading unchanged files.
    pub fn get_events(&self, after_ts: f64) -> Vec<Event> {
        self.refresh_cache();

        let cache = read_lock!(self.cache);
        if after_ts > 0.0 {
            cache.events.iter()
                .filter(|e| !e.cleared && e.ts > after_ts)
                .cloned()
                .collect()
        } else {
            cache.events.iter()
                .filter(|e| !e.cleared)
                .cloned()
                .collect()
        }
    }

    /// Refresh cache if file has changed (mtime or size differ).
    fn refresh_cache(&self) {
        let meta = fs::metadata(&self.path).ok();
        let current_mtime = meta.as_ref().and_then(|m| m.modified().ok());
        let current_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

        // Check if refresh needed
        {
            let cache = read_lock!(self.cache);
            if cache.last_mtime == current_mtime && cache.last_size == current_size {
                return;
            }
        }

        // Re-read file
        let events = self.read_file();
        let mut cache = write_lock!(self.cache);
        cache.events = events;
        cache.last_mtime = current_mtime;
        cache.last_size = current_size;
    }

    fn read_file(&self) -> Vec<Event> {
        let file = match fs::File::open(&self.path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let line = line.trim();
            if line.is_empty() { continue; }
            match serde_json::from_str::<Event>(line) {
                Ok(evt) => events.push(evt),
                Err(_) => continue,
            }
        }
        events
    }

    /// Append a new event to the store and persist to disk.
    pub fn append_event(&self, event: Event) {
        // Write to file
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = writeln!(file, "{}", json);
            }
        }

        // Update in-memory cache
        let mut cache = write_lock!(self.cache);
        cache.events.push(event);
        // Update metadata so next refresh_cache() doesn't re-read
        if let Ok(meta) = fs::metadata(&self.path) {
            cache.last_mtime = meta.modified().ok();
            cache.last_size = meta.len();
        }
    }

    /// Mark all events as cleared.
    pub fn clear_all(&self) {
        let mut cache = write_lock!(self.cache);
        for evt in &mut cache.events {
            evt.cleared = true;
        }

        // Rewrite file
        if let Ok(mut file) = fs::File::create(&self.path) {
            for evt in &cache.events {
                if let Ok(json) = serde_json::to_string(evt) {
                    let _ = writeln!(file, "{}", json);
                }
            }
        }

        // Update cache metadata
        if let Ok(meta) = fs::metadata(&self.path) {
            cache.last_mtime = meta.modified().ok();
            cache.last_size = meta.len();
        }
    }

    /// Remove events older than max_age.
    pub fn compact(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let cutoff = now - self.max_age as f64;

        let mut cache = write_lock!(self.cache);
        cache.events.retain(|e| e.ts >= cutoff);

        if let Ok(mut file) = fs::File::create(&self.path) {
            for evt in &cache.events {
                if let Ok(json) = serde_json::to_string(evt) {
                    let _ = writeln!(file, "{}", json);
                }
            }
        }

        if let Ok(meta) = fs::metadata(&self.path) {
            cache.last_mtime = meta.modified().ok();
            cache.last_size = meta.len();
        }
    }
}
