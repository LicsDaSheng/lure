# Lure 工程结构概览

## 1. 项目定位

Lure 是已安装 Pi 的原生桌面终端客户端。应用不复制 Pi 的 Agent、会话或 TUI 能力，而是在用户选择的工作目录中通过 Rust 创建 PTY，启动普通交互模式的 `pi`，再由 xterm.js 原样呈现 Pi TUI。

首期技术栈：

- 桌面框架：Tauri 2
- 终端前端：React 19 + xterm.js
- PTY 与进程管理：Rust + `portable-pty`
- 包管理器：pnpm
- 初始平台：macOS，PTY crate 与前端协议保留跨平台边界

## 2. 运行边界

```text
键盘 / 粘贴 / 鼠标 / resize
            │
            ▼
      React + xterm.js
            │ Tauri command/event
            ▼
       lure-desktop
            │
            ▼
         lure-pty
            │ PTY 字节流
            ▼
      pi（交互模式）
            │
            ▼
          Pi TUI
```

- Rust 只管理 PTY、Pi 子进程、输入输出和尺寸，不解析 ANSI 内容。
- xterm.js 负责 VT/ANSI 呈现、scrollback、选择、复制粘贴和输入编码。
- Pi 负责模型认证、Agent、工具、skills、extensions、slash commands、会话和完整 TUI。
- 同一桌面终端会话只运行一个 Pi 进程，不同时启动 RPC 模式。

## 3. Rust crate 职责

### `lure-pty`

终端进程边界，负责：

- 校验并规范化工作目录；
- 定位 `pi` 可执行文件；
- 使用 `portable-pty` 创建伪终端；
- 在 PTY 中启动普通交互模式 Pi；
- 暴露输入、resize、停止和输出事件；
- 子进程退出时关闭 reader 并报告退出状态。

该 crate 不依赖 Tauri，也不理解 ANSI、Pi 消息或会话格式。

### `lure-rpc` / `lure-core` / `lure-session`

保留为历史兼容与独立能力代码，但不再位于桌面应用的默认运行链路。新终端工作流不得同时连接 RPC 与 PTY。

### `lure-desktop`

作为组合根，将 `lure-pty` 暴露为窄 Tauri API：

- `start_terminal`
- `write_terminal`
- `resize_terminal`
- `stop_terminal`
- `get_default_workspace`

并发送：

- `lure://terminal-output`
- `lure://terminal-exit`

应用退出时必须停止 Pi 子进程并回收 PTY。

## 4. 前端职责

前端主工作区只呈现一个 Pi 终端：

- 在容器挂载后创建 xterm.js；
- 订阅 Rust 输出并写入终端；
- 将 xterm 输入转发给 Rust；
- 使用 FitAddon 计算列数和行数并同步 PTY；
- 显示启动、失败和退出状态；
- 提供工作目录选择、启动/重启和停止操作；
- 保留自定义窗口标题栏，不在终端上叠加账户、协作或 Agent 状态 UI。

终端输出是显示事实来源，Redux 不再参与默认终端会话状态。

## 5. 环境与进程

GUI 从 Finder、Dock 或 Spotlight 启动时不会加载 shell rc。启动 Pi 前继续使用既有登录 shell 环境合成规则，并强制补全运行时 `PATH`。PTY 设置至少包含：

- `TERM=xterm-256color`
- `COLORTERM=truecolor`
- 当前工作目录
- xterm.js 实际列数和行数

`LURE_PI_PATH` 仍可覆盖 Pi 可执行文件位置。

## 6. 验证命令

```bash
pnpm test
pnpm check
pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

关键行为还需在 Tauri 应用中验证：Pi 启动、输入、ANSI 呈现、resize、重启、停止和窗口退出清理。
