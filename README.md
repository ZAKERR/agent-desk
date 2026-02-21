# Agent Desk

Universal CLI agent monitor — a desktop Dynamic Island widget for [Claude Code](https://docs.anthropic.com/en/docs/claude-code), [Codex CLI](https://github.com/openai/codex), and future coding agents.

![Platform: Windows](https://img.shields.io/badge/platform-Windows-blue)
![Rust](https://img.shields.io/badge/built%20with-Rust-orange)

## Features

- **Dynamic Island** — always-on-top pill at screen top, expands on hover to show sessions
- **Multi-agent monitoring** — tracks all running Claude Code / Codex sessions simultaneously
- **Permission approval** — approve or deny tool calls directly from the widget (no terminal switching)
- **Real-time updates** — SSE-based live status (working / ready / waiting for input)
- **System tray** — dynamic icon, session list, toast notifications, per-event sound alerts
- **Global hotkey** — configurable shortcut (default `Alt+D`) to show/hide the island
- **Autostart** — optional boot-time launch via OS-level autostart
- **Remote push** — Telegram / DingTalk / WeChat notifications (optional)

## Quick Start

### Prerequisites

- Windows 10/11
- [Rust](https://rustup.rs/) toolchain
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) or [Codex CLI](https://github.com/openai/codex)

### Build

```bash
# Main app
cd src-tauri && cargo build --release

# Hook binary
cd hooks && cargo build --release
```

### Configure Hooks

Add to `~/.claude/settings.json` (adjust path to your clone):

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "type": "command", "command": "/path/to/agent-desk-hook.exe --event user_prompt" }],
    "PreToolUse": [{ "type": "command", "command": "/path/to/agent-desk-hook.exe --event pre_tool" }],
    "Stop": [{ "type": "command", "command": "/path/to/agent-desk-hook.exe --event stop" }],
    "Notification": [{ "type": "command", "command": "/path/to/agent-desk-hook.exe --event notification" }],
    "SessionStart": [{ "type": "command", "command": "/path/to/agent-desk-hook.exe --event session_start" }],
    "SessionEnd": [{ "type": "command", "command": "/path/to/agent-desk-hook.exe --event session_end" }]
  }
}
```

Hook binary location: `hooks/target/release/agent-desk-hook.exe`

### Run

```bash
src-tauri/target/release/agent-desk.exe
```

On first run, `config/config.yaml` is auto-created from the example template. Edit it to configure remote notifications or customize the island appearance.

## Configuration

Config file: `config/config.yaml` (auto-created from `config.example.yaml`)

Config search order: exe directory > working directory > `%APPDATA%/agent-desk/`

### Key settings

| Section | Key | Default | Description |
|---------|-----|---------|-------------|
| `island` | `hotkey` | `"Alt+D"` | Global show/hide shortcut |
| `island` | `autostart` | `false` | Launch on system boot |
| `island` | `sound_enabled` | `true` | Play sounds on events |
| `island` | `sound_stop` | `"asterisk"` | Sound for task completion |
| `island` | `sound_notification` | `"exclamation"` | Sound for input requests |
| `island` | `sound_permission` | `"question"` | Sound for permission prompts |
| `telegram` | `enabled` | `false` | Telegram push notifications |
| `dingtalk` | `enabled` | `false` | DingTalk push notifications |
| `wechat` | `enabled` | `false` | WeChat push notifications |

All settings can also be changed from the island's built-in Settings panel.

## Architecture

```
Hook events ──> agent-desk-hook.exe ──> HTTP API (port 15924)
                                              │
                                    ┌─────────┼─────────┐
                                    │         │         │
                              SessionTracker  SSE    EventLog
                                    │         │         │
                              ProcessScanner  │    Remote Push
                                    │         │
                              ┌─────┴─────────┴──────┐
                              │    Dynamic Island     │
                              │  (always-on-top pill) │
                              └───────────────────────┘
```

## Acknowledgments

UI design inspired by [claude-island](https://github.com/farouqaldori/claude-island) by [@farouqaldori](https://github.com/farouqaldori).

## License

MIT
