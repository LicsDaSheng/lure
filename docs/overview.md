# Lure 工程结构概览

## 1. 项目定位

Lure 是已安装 Pi 的原生桌面终端客户端。应用不复制 Pi 的 Agent、会话或 TUI 能力，而是在用户选择的工作目录中通过 Rust 创建 PTY，启动普通交互模式的 `pi`，再由 GPUI-CE 直接绘制终端网格。

当前技术栈：

- 桌面与展示框架：GPUI-CE（纯 Rust）
- ANSI/VT 状态机：`vt100`
- PTY 与进程管理：Rust + `portable-pty`
- 异步消息通道：`async-channel`
- 初始平台：macOS，PTY 与按键编码保留跨平台边界

项目不包含 Tauri、WebView、React、TypeScript、Node.js 或前端包管理器。

## 2. 运行边界

```text
键盘 / 粘贴 / resize
          │
          ▼
     GPUI-CE 窗口
          │
          ▼
   VT 网格与按键编码
          │ 原始字节
          ▼
       lure-pty
          │ PTY
          ▼
     pi（交互模式）
          │
          ▼
        Pi TUI
```

- Rust 展示层只解释 ANSI/VT 控制序列以绘制字符网格，不理解 Pi 的 Agent、工具、消息或会话语义。
- Pi 负责模型认证、Agent、工具、skills、extensions、slash commands、会话和完整 TUI。
- 同一桌面终端会话只运行一个 Pi 进程，不同时启动 RPC 模式。

## 3. Rust crate 职责

### `lure-pty`

负责工作目录校验、Pi 可执行文件定位、登录 shell 环境合成、PTY 创建、输入输出、resize、停止与退出事件。该 crate 不依赖 GPUI，也不解析 ANSI 或 Pi 语义。

### `lure-desktop`

纯 Rust 组合根，负责：

- 创建 GPUI 窗口与自定义标题栏；
- 将 PTY 输出交给 `vt100` 状态机；
- 按字符单元绘制颜色、粗体、斜体、下划线和光标；
- 将键盘、粘贴和窗口尺寸转换为 PTY 输入与字符网格；
- 选择工作目录、启动、重启、停止和错误恢复；
- 窗口关闭时停止 Pi 子进程。

GPUI-CE 依赖固定到上游 Git commit，避免未审阅的框架漂移。

## 4. 生命周期

1. 创建 `~/lure` 默认工作目录并打开 GPUI 窗口。
2. 后台线程合成登录 shell 环境并创建 Pi PTY。
3. PTY reader 事件经 `async-channel` 回到 GPUI 主线程。
4. 输出更新 VT 网格并触发重绘；窗口尺寸变化同步到 VT 网格和 PTY。
5. 选择新目录或重启时先停止旧进程，并用 generation 丢弃旧会话事件。
6. 停止、退出或错误时保留已绘制终端内容并显示恢复面板。
7. 关闭窗口或销毁根视图时停止子进程。

## 5. 验证命令

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

真实 Pi 冒烟测试：

```bash
LURE_RUN_REAL_PI_PTY=1 cargo test -p lure-pty --test real_pi
```
