mod claude_code;
mod codex;

use std::sync::{Arc, Mutex, RwLock};
use crate::process::{ProcessInfo, ProcessScanner};

pub struct AdapterEntry {
    pub name: String,
    pub scanner: ProcessScanner,
}

pub struct AdapterRegistry {
    adapters: Mutex<Vec<AdapterEntry>>,
    /// Cached process list — wrapped in Arc for cheap sharing (no deep clone).
    cache: RwLock<Arc<Vec<ProcessInfo>>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        let mut adapters = Vec::new();

        // Claude Code adapter
        adapters.push(AdapterEntry {
            name: "claude_code".to_string(),
            scanner: ProcessScanner::new(
                "claude_code",
                &["claude.exe", "claude"],
                &["chrome-native-host.exe", "chrome-native-host"],
            ),
        });

        // Codex CLI adapter
        adapters.push(AdapterEntry {
            name: "codex".to_string(),
            scanner: ProcessScanner::new(
                "codex",
                &["codex.exe", "codex"],
                &[],
            ),
        });

        Self {
            adapters: Mutex::new(adapters),
            cache: RwLock::new(Arc::new(Vec::new())),
        }
    }

    /// Trigger a fresh scan from all adapters.
    pub fn scan_all(&self) {
        let mut results = Vec::new();
        let mut adapters = self.adapters.lock().unwrap();
        for adapter in adapters.iter_mut() {
            results.extend(adapter.scanner.scan());
        }
        drop(adapters);
        let mut cache = self.cache.write().unwrap();
        *cache = Arc::new(results);
    }

    /// Get cached process list — cheap Arc clone, no deep copy.
    pub fn get_cached(&self) -> Arc<Vec<ProcessInfo>> {
        let cache = self.cache.read().unwrap();
        Arc::clone(&cache)
    }
}
