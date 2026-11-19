# Lure 工程结构概览

## 产品边界

Lure 是已安装 Pi 的原生桌面终端客户端。应用不实现 Pi 的认证、模型、Agent、工具或会话语义；只在用户选择的项目目录中通过 PTY 启动交互式 `pi`，将输出交给 rio-vt 维护终端状态，再由 GPUI-CE 绘制字符网格及图片。

项目列表存储在 `$HOME/.pi/projects.yaml`，由 Lure 独占读写。应用不创建 `.pi` 目录，也不访问其中其他文件。

## Rust crate 职责

- `lure-pty`：检测 Pi 可执行文件、合成登录 shell 环境、校验工作目录、创建和管理 PTY 与子进程、转发字节并同步终端尺寸。不依赖 GPUI，也不解析终端或 Pi 业务语义。
- `lure-desktop`：管理 GPUI 窗口、项目列表、侧栏、每项目独立的终端会话及故障恢复；将原始 PTY 输出交给 rio-vt，绘制其可见网格和图片，编码输入并向 PTY 转发终端查询回复。

应用不使用 WebView、Tauri 或前端运行时。同一项目目录只对应一个已打开的 Pi 子进程；不并发启动该会话的 RPC 子进程。

## 页面与会话生命周期

1. 启动时从 `$HOME/.pi/projects.yaml` 加载项目并显示首页；不自动启动 Pi。
2. 用户通过项目卡片或侧栏打开项目时，在该目录启动 Pi。打开已有会话只切换前台，不重启进程。
3. 首页与终端页共用可折叠的项目侧栏。侧栏展示已保存的项目，切换会话不停止后台进程；从终端返回首页也保留会话。
4. 每个会话持有独立 PTY、终端状态与图片纹理。会话 id 和 generation 隔离关闭及重启后的迟到消息；后台输出仍更新对应终端状态。
5. 窗口或侧栏尺寸变化时同步终端字符网格与 PTY winsize。侧栏仅切换项目，终端区域右上角的 X 关闭按钮停止活动项目的 Pi、回收该会话并返回首页，其他项目继续运行；关闭窗口回收全部 Pi 子进程。

界面与终端交互的详细约束见 [`ui-design.md`](ui-design.md)。

## 验证

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

真实 Pi 的 PTY 冒烟测试：`LURE_RUN_REAL_PI_PTY=1 cargo test -p lure-pty --test real_pi`。
