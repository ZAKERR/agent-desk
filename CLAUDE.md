# Agent Desk

Universal CLI agent monitor — desktop pet widget for Claude Code, Codex CLI, and future agents.

## Tech Stack

- **Backend**: Rust (Tauri v2 + axum HTTP server)
- **Frontend**: Single HTML file (`src/pet.html`) with Lottie animations
- **Platform**: Windows (Win32 APIs for process scanning & terminal focus)
- **Hook binary**: Standalone Rust binary (`hooks/`) called by CLI agents

## Build & Run

```bash
export PATH="$HOME/.cargo/bin:$PATH"

# Main app
cd D:/Lab/agent-desk/src-tauri && cargo build --release
# Run: target/release/agent-desk.exe

# Hook binary
cd D:/Lab/agent-desk/hooks && cargo build --release
```

## Project Structure

```
agent-desk/
├── src/
│   └── pet.html              # Dynamic Island UI (only UI file)
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs            # Entry: config → HTTP thread → Tauri app
│   │   ├── server.rs         # axum API routes + scan_and_merge logic
│   │   ├── events.rs         # EventStore: mtime-cached JSONL read/write
│   │   ├── session.rs        # SessionTracker: HashMap + periodic disk flush
│   │   ├── sse.rs            # SSEBroadcaster: tokio broadcast channel
│   │   ├── remote.rs         # Telegram / DingTalk / WeChat push
│   │   ├── island.rs         # Dynamic Island: SetWindowRgn, expand/collapse, toggle
│   │   ├── permission.rs     # PermissionStore: pending requests + oneshot channels
│   │   ├── chat.rs           # ChatReader: JSONL incremental reader + UUID dedup
│   │   ├── process/
│   │   │   ├── mod.rs
│   │   │   └── scanner.rs    # Win32 Toolhelp32 process scan + CPU activity
│   │   ├── adapter/
│   │   │   ├── mod.rs        # AdapterRegistry: multi-agent scanner cache
│   │   │   ├── claude_code.rs
│   │   │   └── codex.rs
│   │   ├── focus.rs          # Terminal focus: parent-chain + title matching
│   │   ├── tray.rs           # System tray: icon, menu, toast, sound
│   │   └── config.rs         # YAML config loader
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── icons/
├── hooks/
│   └── src/main.rs           # Hook binary (all 6 event types)
└── config/
    └── config.yaml
```

## API Endpoints (HTTP, port 15924)

### Core
| Method | Path | Description |
|--------|------|-------------|
| GET | /api/all?after=ts | Combined: status + processes + events |
| GET | /api/events?after=ts | Events from events.jsonl |
| GET | /api/sessions | Live processes merged with tracker status |
| GET | /api/status | Aggregate state for pet widget |
| GET | /api/stream | SSE endpoint (real-time push) |

### Hooks
| Method | Path | Description |
|--------|------|-------------|
| POST | /api/hook?event=X | Lightweight hook receiver (status-only) |
| POST | /api/signal | Full event: session update → log → SSE → remote |

### Settings & Island
| Method | Path | Description |
|--------|------|-------------|
| GET | /api/settings | Read settings (hotkey, autostart, sound) |
| POST | /api/settings | Save settings (runtime + config.yaml + OS autostart) |
| POST | /api/island/hide | Hide island window |
| POST | /api/hotkey/capture | Unregister hotkey for JS key capture |
| POST | /api/hotkey/save | Register new hotkey + write config.yaml |

### Permissions
| Method | Path | Description |
|--------|------|-------------|
| POST | /api/permission-request | Hook long-poll (up to 600s) |
| POST | /api/permission-respond | UI sends decision |
| GET | /api/permissions | Get pending requests |

## Hooks (configured in `~/.claude/settings.json`)

All 6 hooks call the same Rust binary with different `--event` flags:
- **Fast** (curl to /api/hook): `UserPromptSubmit`, `PreToolUse`
- **Heavy** (reads stdin, POSTs /api/signal): `Stop`, `Notification`, `SessionStart`, `SessionEnd`

Hook binary: `D:/Lab/agent-desk/hooks/target/release/agent-desk-hook.exe`

## Key Architecture Decisions

- **No dashboard** — pet.html is the only UI (always-on-top Dynamic Island window)
- **SetWindowRgn** — opaque window + GDI region clip (WebView2 transparent is broken)
- **Arc<Config>** — shared across async tasks; mutable settings use AtomicBool/RwLock in AppState
- **spawn_blocking** — all Win32 syscalls and file I/O run off tokio threads
- **Session CWD from hooks** — process scanner CWD is unreliable (exe directory)
- **Autostart** — `tauri-plugin-autostart` manages OS registry; config.yaml persists preference
- **Duplicate prevention** — TCP port check at startup in lib.rs

## Config

- App config: `D:/Lab/agent-desk/config/config.yaml`
- Hooks: `C:/Users/admin/.claude/settings.json`
- Tauri: `src-tauri/tauri.conf.json`
- Window permissions: `src-tauri/capabilities/default.json`

## Known Limitations

- Process CWD: uses exe directory as approximation. Session tracker CWD from hooks is primary source.
- 3 dead-code warnings (adapter fields reserved for future features)
