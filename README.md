# Lure

Lure 是使用 [GPUI-CE](https://github.com/gpui-ce/gpui-ce)、[rio-vt](https://crates.io/crates/rio-vt) 与 Rust PTY 构建的 Pi 原生桌面终端。项目不包含 Tauri、React、TypeScript 或 WebView。

## 前置条件

按 Pi 官方方式安装并完成模型认证，确认终端中可以执行：

```bash
pi --version
```

项目由 `rust-toolchain.toml` 固定 Rust 1.97。

## 开始开发

```bash
make dev                    # 首次自动安装项目专用的 cargo-watch；之后监听源码变化并重启
make run                    # 编译并启动一次
make build                  # 构建 release 可执行文件
```

`make dev` 首次运行时将固定版本的 `cargo-watch` 安装在被 Git 忽略的 `target/dev-tools` 中，不会安装到全局 Cargo bin 目录，也不参与应用构建或打包。它会在保存 Rust 源码后重新编译并重启窗口，不是无需重启的热更新；重启会结束当前 Pi PTY 会话。`make run` 等价于 `cargo run -p lure-desktop`。

应用从 `$HOME/.pi/projects.yaml` 加载项目；首次进入首页时不启动 Pi。点击项目卡片或侧栏项目后，应用在对应目录通过 PTY 启动普通交互模式的 Pi。rio-vt 维护 ANSI/VT 状态，GPUI 绘制可见网格；侧栏用于切换多个项目会话，返回首页不会停止后台 Pi。也可通过 `LURE_PI_PATH` 指定 Pi 可执行文件。

macOS 使用原生交通灯进行关闭、缩放与最小化到 Dock；拖动窗口、系统平铺及第三方窗口管理工具使用原生窗口机制。最小化不会停止 Pi，关闭窗口会停止 Pi。本应用不另设后台托盘。

要在 Finder / Dock 中显示 Lure 图标，需生成应用包（依赖 Homebrew 的 `librsvg`，提供 `rsvg-convert`，以及 Xcode 命令行工具的 `iconutil`）：

```bash
scripts/package-macos.sh
open target/release/Lure.app
```

直接执行 `cargo run` 不会加载 `.app` 的 Dock 图标。

## 验证

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

工程边界与模块职责见 [`docs/overview.md`](docs/overview.md)，界面规范见 [`docs/ui-design.md`](docs/ui-design.md)。
