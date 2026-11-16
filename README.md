# Lure

Lure 是基于 Tauri 2、React、Rust PTY 和 xterm.js 构建的 Pi 桌面终端。

## 前置条件

按 Pi 官方方式安装并完成模型认证，确认终端中可以执行：

```bash
pi --version
```

## 开始开发

```bash
pnpm install
pnpm tauri dev
```

应用在所选工作目录中通过 PTY 启动普通交互模式的 Pi，并由 xterm.js 原样呈现 Pi TUI。模型、会话、工具、skills、extensions 和 slash commands 均由 Pi 提供。

工程边界、目录职责与验证命令见 [`docs/overview.md`](docs/overview.md)。
