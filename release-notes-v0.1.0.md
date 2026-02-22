## Install

**Just download an installer and run it:**

| File | Description |
|------|-------------|
| `.msi` | Windows standard installer (recommended) |
| `-setup.exe` | NSIS installer (alternative) |

1. Download either installer above
2. Run the installer
3. Launch Agent Desk

On first launch, Claude Code hooks are auto-configured (`~/.claude/settings.json`) — **no manual setup needed**.

<details>
<summary>Building from source</summary>

`agent-desk-hook.exe` is the hook relay binary, bundled inside the installer.
If building from source, download it separately and place it next to the main executable, or see [Build from Source](https://github.com/ZAKERR/agent-desk#option-b-build-from-source).
</details>

---

## 安装

**只需下载一个安装包，安装即可用：**

| 文件 | 说明 |
|------|------|
| `.msi` | Windows 标准安装包（推荐） |
| `-setup.exe` | NSIS 安装包（备选） |

1. 下载上方任意一个安装包
2. 双击安装
3. 启动 Agent Desk

首次启动会自动配置 Claude Code hooks（写入 `~/.claude/settings.json`），**无需任何手动配置**。

<details>
<summary>从源码编译的用户</summary>

`agent-desk-hook.exe` 是 hook 转发程序，已内置在安装包中。
如果你从源码编译，请单独下载此文件放到主程序同目录下，或参考 [Build from Source](https://github.com/ZAKERR/agent-desk#option-b-build-from-source)。
</details>
