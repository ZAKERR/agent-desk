# Agent Desk

[English](README.md) | [中文](#agent-desk)

通用 CLI Agent 监控器 — 桌面灵动岛组件，支持 [Claude Code](https://docs.anthropic.com/en/docs/claude-code)、[Codex CLI](https://github.com/openai/codex) 及未来的编码 Agent。

![Platform: Windows](https://img.shields.io/badge/platform-Windows-blue)
![Rust](https://img.shields.io/badge/built%20with-Rust-orange)

## 功能特性

- **灵动岛** — 屏幕顶部常驻悬浮胶囊，悬停展开显示会话信息
- **多 Agent 监控** — 同时追踪所有运行中的 Claude Code / Codex 会话
- **权限审批** — 直接在组件中批准或拒绝工具调用（无需切换终端）
- **实时更新** — 基于 SSE 的实时状态推送（工作中 / 就绪 / 等待输入）
- **系统托盘** — 动态图标、会话列表、系统通知、按事件类型的声音提醒
- **全局热键** — 可配置快捷键（默认 `Alt+D`）显示/隐藏灵动岛
- **开机自启** — 可选的系统级开机启动
- **远程推送** — Telegram / 钉钉 / 微信通知（可选）

## 截图

| 灵动岛（收起状态） | 系统托盘菜单 |
|:---:|:---:|
| ![Bar](image/Bar.png) | ![Menu](image/Menu.png) |

## 快速开始

### 方式 A：安装包安装（推荐）

1. 从 [Releases](https://github.com/ZAKERR/agent-desk/releases) 下载 `.msi` 或 `-setup.exe` 安装包
2. 运行安装程序
3. 启动 Agent Desk

首次启动会自动配置 Claude Code hooks（写入 `~/.claude/settings.json`），**无需任何手动配置**。

### 方式 B：从源码编译

#### 环境要求

- Windows 10/11
- [Rust](https://rustup.rs/) 工具链（包含 `cargo`）
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) 或 [Codex CLI](https://github.com/openai/codex)

#### 编译 & 运行

```bash
# 1. 编译 hook 二进制
cd hooks && cargo build --release && cd ..

# 2. 复制 hook 到主程序目录（自动配置需要）
mkdir -p src-tauri/binaries
cp hooks/target/release/agent-desk-hook.exe src-tauri/binaries/

# 3. 编译主程序
cd src-tauri && cargo build --release && cd ..

# 4. 复制 hook 到编译输出目录
cp hooks/target/release/agent-desk-hook.exe src-tauri/target/release/

# 5. 运行
src-tauri/target/release/agent-desk.exe
```

首次启动会自动：
- 从模板创建 `config/config.yaml` 配置文件
- 在 `~/.claude/settings.json` 中配置 hook 条目（需要 `agent-desk-hook.exe` 与主程序在同一目录）

#### 手动 Hook 配置（仅在自动配置不生效时）

如果你将 hook 放在了其他位置，需在 `~/.claude/settings.json` 中添加：

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event user_prompt" }] }],
    "PreToolUse": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event pre_tool" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event stop" }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event notification" }] }],
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event session_start" }] }],
    "SessionEnd": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event session_end" }] }]
  }
}
```

> **注意**: Hook 路径**必须使用正斜杠**（`C:/path/to/...`）。Claude Code 通过 bash 执行 hooks，反斜杠会被当作转义字符处理。

## 配置

配置文件：`config/config.yaml`（从 `config.example.yaml` 自动创建）

配置搜索顺序：程序目录 > 工作目录 > `%APPDATA%/agent-desk/`

### 主要设置项

| 分类 | 键名 | 默认值 | 说明 |
|------|------|--------|------|
| `island` | `hotkey` | `"Alt+D"` | 全局显示/隐藏快捷键 |
| `island` | `autostart` | `false` | 开机自动启动 |
| `island` | `sound_enabled` | `true` | 事件声音提醒 |
| `island` | `sound_stop` | `"asterisk"` | 任务完成提示音 |
| `island` | `sound_notification` | `"exclamation"` | 输入请求提示音 |
| `island` | `sound_permission` | `"question"` | 权限请求提示音 |
| `telegram` | `enabled` | `false` | Telegram 推送通知 |
| `dingtalk` | `enabled` | `false` | 钉钉推送通知 |
| `wechat` | `enabled` | `false` | 微信推送通知 |

所有设置也可在灵动岛内置的设置面板中修改。

## 架构

```
Hook 事件 ──> agent-desk-hook.exe ──> HTTP API (端口 15924)
                                              │
                                    ┌─────────┼─────────┐
                                    │         │         │
                              会话追踪器     SSE     事件日志
                                    │         │         │
                              进程扫描器      │    远程推送
                                    │         │
                              ┌─────┴─────────┴──────┐
                              │       灵动岛          │
                              │  (常驻悬浮胶囊)       │
                              └───────────────────────┘
```

## 常见问题

### Hook 报错：`agent-desk-hook.exe: command not found`

Hook 路径**必须使用正斜杠**（`C:/Program Files/Agent Desk/agent-desk-hook.exe`）。Claude Code 通过 bash 执行 hooks，反斜杠会被当作转义字符并被忽略。自动配置已正确处理此问题，但如果手动配置请务必使用 `/`。

### 灵动岛在 Claude Code 结束后仍显示"工作中..."

Agent Desk 没有收到 `Stop` hook 事件。常见原因：
- Hook 二进制未找到（参见上方错误）
- Claude Code 结束时 Agent Desk 未运行
- 端口 15924 被防火墙阻止

检查 `~/.claude/settings.json` 是否配置了全部 6 个 hook 事件。也可手动验证：
```bash
echo '{}' | agent-desk-hook.exe --event stop
```

### "Agent Desk is already running on port 15924"

同一时间只能运行一个实例。关闭现有实例（系统托盘 → 退出）或强制终止：
```bash
taskkill /F /IM agent-desk.exe
```

### 安装后 hooks 未自动配置

自动配置要求 `agent-desk-hook.exe` 与主程序 `agent-desk.exe` 在同一目录。请检查：
1. 安装目录中两个文件都存在
2. `~/.claude/` 目录可写
3. 查看 `~/.claude/settings.json` 中是否有 `hooks` 配置

### 灵动岛消失 / 不可见

- 按 `Alt+D`（默认热键）切换显示
- 右键系统托盘图标 → "显示灵动岛"
- 在展开模式下其他应用获取焦点时灵动岛会隐藏 — 将鼠标悬停在屏幕顶部中央的胶囊区域

### 灵动岛中不显示会话

- 确认 Claude Code（或 Codex）正在运行
- 必须已配置 hooks — 检查 `~/.claude/settings.json`
- 在 Agent Desk 运行后启动新的 Claude Code 会话（已有会话在触发 hook 事件前不会显示）

### 权限审批不工作

`PermissionRequest` hook 需要单独配置（不在自动配置范围内）。在 `~/.claude/settings.json` 中添加：
```json
"PermissionRequest": [{ "hooks": [{ "type": "command", "command": "C:/path/to/agent-desk-hook.exe --event permission_request", "timeout": 86400 }] }]
```

### 源码编译：`cargo: command not found`

Cargo 不在默认的 bash PATH 中。在命令前加上：
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
```

### 源码编译：重新编译时文件锁定错误

Windows 会锁定运行中的 exe。先终止进程：
```bash
taskkill /F /IM agent-desk.exe
cd src-tauri && cargo build --release
```

### 端口 15924 与其他程序冲突

修改 `config/config.yaml` 中 `manager` 下的 `port` 值。同时更新 hook 的端口参数：
```json
"command": "agent-desk-hook.exe --event stop --port 15925"
```

## 致谢

- UI 概念灵感来自 Apple [灵动岛](https://support.apple.com/guide/iphone/use-the-dynamic-island-iph28f50d10d/ios)
- 架构设计和技术选型参考了 [claude-island](https://github.com/farouqaldori/claude-island)（[@farouqaldori](https://github.com/farouqaldori)）

## 许可证

MIT
