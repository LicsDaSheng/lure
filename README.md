# Lure

Lure 是使用 [GPUI-CE](https://github.com/gpui-ce/gpui-ce)、[Alacritty 终端内核](https://github.com/alacritty/alacritty)与 Rust PTY 构建的 Pi 原生桌面终端。项目不包含 Tauri、React、TypeScript 或 WebView。

## 前置条件

按 Pi 官方方式安装并完成模型认证，确认终端中可以执行：

```bash
pi --version
```

项目由 `rust-toolchain.toml` 固定 Rust 1.97。

## 开始开发

```bash
cargo run -p lure-desktop
```

应用会创建 `~/lure` 作为默认工作目录，通过 PTY 启动普通交互模式的 Pi，由 Alacritty 终端内核维护 ANSI/VT 状态并在 GPUI 窗口中绘制可见网格。也可通过 `LURE_PI_PATH` 指定 Pi 可执行文件。

## 验证

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

工程边界与模块职责见 [`docs/overview.md`](docs/overview.md)，界面规范见 [`docs/ui-design.md`](docs/ui-design.md)。
