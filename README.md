# Lure

Lure 是基于 Tauri 2、React 和 Rust 构建的 Pi 桌面客户端。

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

应用启动后选择工作目录并点击“连接 Pi”，即可通过 `pi --mode rpc` 发送文本消息、查看流式回复与工具运行状态，并可停止任务或断开连接。

工程边界、目录职责与验证命令见 [`docs/overview.md`](docs/overview.md)。
