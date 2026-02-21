# Agent Desk

Universal CLI agent monitor — desktop pet widget for Claude Code, Codex CLI, and future agents.

## Tech Stack
- **Backend**: Rust (Tauri v2 + axum HTTP server)
- **Frontend**: Single HTML file (`src/pet.html`) with Lottie animations
- **Platform**: Windows (Win32 APIs for process scanning & terminal focus)
- **Hook binary**: Standalone Rust binary (`hooks/`) called by CLI agents

## Project Structure

```
agent-desk/
├── src/
│   └── pet.html              # Desktop pet widget (only UI)
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs            # Entry: config → HTTP thread → Tauri app
│   │   ├── server.rs         # axum API routes + scan_and_merge logic
│   │   ├── events.rs         # EventStore: mtime-cached JSONL read/write
│   │   ├── session.rs        # SessionTracker: HashMap + periodic disk flush
│   │   ├── sse.rs            # SSEBroadcaster: tokio broadcast channel
│   │   ├── remote.rs         # Telegram / DingTalk / WeChat push
│   │   ├── process/
│   │   │   ├── mod.rs
│   │   │   └── scanner.rs    # Win32 Toolhelp32 process scan + CPU activity
│   │   ├── adapter/
│   │   │   ├── mod.rs        # AdapterRegistry: multi-agent scanner cache
│   │   │   ├── claude_code.rs
│   │   │   └── codex.rs
│   │   ├── focus.rs          # Terminal focus: parent-chain + title matching
│   │   ├── tray.rs           # System tray: Show Widget / Quit
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

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/all?after=ts | Combined: status + processes + events |
| GET | /api/events?after=ts | Events from events.jsonl |
| GET | /api/sessions | Live processes merged with tracker status |
| GET | /api/status | Aggregate state for pet widget |
| GET | /api/stream | SSE endpoint (real-time push) |
| POST | /api/hook?event=X | Lightweight hook receiver (status-only) |
| POST | /api/signal | Full event: session update → log → SSE → remote |
| POST | /api/focus | Win32 terminal focus via process-tree tracing |
| POST | /api/clear | Mark all events cleared |

## Hooks (configured in `~/.claude/settings.json`)

All 6 hooks call the same Rust binary with different `--event` flags:
- **Fast** (curl to /api/hook): `UserPromptSubmit`, `PreToolUse`
- **Heavy** (reads stdin, POSTs /api/signal): `Stop`, `Notification`, `SessionStart`, `SessionEnd`

Hook binary: `D:/Lab/agent-desk/hooks/target/release/agent-desk-hook.exe`

## Build

```bash
export PATH="$HOME/.cargo/bin:$PATH"

# Main app
cd D:/Lab/agent-desk/src-tauri && cargo build --release
# Run: target/release/agent-desk.exe

# Hook binary
cd D:/Lab/agent-desk/hooks && cargo build --release
```

## Key Architecture Decisions

- **No dashboard** — pet.html is the only UI (always-on-top Tauri window)
- **Arc<Config>** — shared across async tasks, no deep cloning
- **Arc<Vec<ProcessInfo>>** — scanner cache shared via Arc, not deep clone
- **spawn_blocking** — all Win32 syscalls and file I/O run off tokio threads
- **Single OpenProcess** — scanner opens process handle once per match (was 3x)
- **UTF-16 direct comparison** — process name matching without heap allocation
- **Session CWD from hooks** — process scanner CWD is unreliable (exe directory)
- **Duplicate prevention** — TCP port check at startup in lib.rs

## Known Limitations

- Process CWD: uses exe directory as approximation (real CWD needs NtQueryInformationProcess + PEB read). Session tracker CWD from hooks is used as primary source.
- Pet character: currently just a Lottie-generated colored circle with eyes. Needs real character art (Phase 4).
- 10 dead-code warnings (config fields reserved for future features)

## Config

- App config: `D:/Lab/agent-desk/config/config.yaml`
- Hooks: `C:/Users/admin/.claude/settings.json`
- Tauri: `src-tauri/tauri.conf.json`
- Window permissions: `src-tauri/capabilities/default.json`
