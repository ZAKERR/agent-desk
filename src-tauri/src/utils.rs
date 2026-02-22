//! Utility macros for concise lock access.

/// Read-lock a `RwLock`, recovering from poison.
macro_rules! read_lock {
    ($l:expr) => {
        $l.read().unwrap_or_else(|e| e.into_inner())
    };
}

/// Write-lock a `RwLock`, recovering from poison.
macro_rules! write_lock {
    ($l:expr) => {
        $l.write().unwrap_or_else(|e| e.into_inner())
    };
}

/// Lock a `Mutex`, recovering from poison.
macro_rules! mutex_lock {
    ($l:expr) => {
        $l.lock().unwrap_or_else(|e| e.into_inner())
    };
}

// Macros are brought into scope by `#[macro_use]` on the module in lib.rs.
