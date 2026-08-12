# Lure

用 Rust 实现的轻量 AI agent 框架，后端行为对齐 [`nanobot`](https://github.com/HKUDS/nanobot)。提供 CLI 命令行与 desktop 桌面应用两种入口。桌面前端为自有重写（React + shadcn/ui + Tailwind v4），桌面外壳为 Tauri V2（仅外壳，能力仍由进程内 HTTP/WS 提供）。

## 架构

Cargo workspace，以 crate 边界作为主要模块化机制：

```
lure/
├── Cargo.toml                  # workspace 根
├── frontend/
│   ├── app/                    # 自有 WebUI（React + shadcn/ui + Tailwind v4）
│   └── dist/                   # 前端构建产物（由 lure-desktop 内嵌，gitignore）
└── crates/
    ├── lure-core/              # 核心领域库
    │   └── src/
    │       ├── agent/          # agent loop / runner / context / 异步调度核心 / subagent 执行
    │       ├── api/            # OpenAI-compatible API 表面（传输无关）
    │       ├── bus/            # InboundMessage / OutboundMessage / 同步与异步消息总线
    │       ├── channel/        # channel 契约（access/contract）+ websocket 渠道（turn 事件路由）
    │       ├── config/         # 配置 schema / 路径 / 读写 / preset
    │       ├── cron/           # cron 调度（tokio interval task）/ 持久化 / submit 语义
    │       ├── gateway/        # gateway 编排
    │       ├── mcp/            # MCP 异步客户端（JSON-RPC / stdio 传输 / 瞬时重试）
    │       ├── memory/         # 长期记忆 / history / dream consolidation
    │       ├── provider/       # LLM provider 契约 / OpenAI-compatible（全异步，reqwest）
    │       ├── security/       # workspace 路径边界
    │       ├── session/        # session 存储 / 缓存 / goal 状态
    │       ├── tool/           # tool trait / registry / 文件与 shell 工具 / MCP 纯变换
    │       ├── trigger/        # trigger at-least-once 队列 / 异步投递循环
    │       └── webui/          # WebUI 后端（axum HTTP + WS）/ 复用协议 / transcript / hub
    ├── lure-cli/               # CLI（二进制 `lure`）
    └── lure-desktop/           # 桌面应用（Tauri V2 外壳，二进制 `lure-desktop`）
```

设计原则：优先类型化 API、枚举、trait 与结构化错误；保持解析、领域逻辑、运行时执行、存储与传输之间的清晰边界。

### 异步运行时架构

lure 采用单一 tokio 异步运行时（multi-thread），全 IO 面异步化：

- **单实例调度核心**：`AgentLoopScheduler` 常驻消费 `AsyncBus`，按 session 串行调度（pending 队列、`/stop` 取消、cron 让位）；WS 渠道与 cron 经同一 bus 进单实例 AgentLoop。
- **WebUI**：axum 统一 HTTP + WebSocket（`/webui/bootstrap`、`/api/*`、WS 复用协议）；浏览器消息经 `BusTurnRunner` 进 bus，turn 事件按 `turn_id` 流式回推。
- **cron**：`AsyncCronScheduler` 为 tokio interval task，到期 job 经 submit 语义投递进 bus。
- **subagent**：`SubagentRunner` 后台执行 + announce 经 bus 回灌 + exec 级联终止。
- **MCP**：`McpClient` 传输无关 JSON-RPC 客户端（stdio 传输、enabled-tools 过滤、瞬时重试）。

同步路径仅剩少数桥接点（`runtime::block_on`，mux/gateway 同步 handle_frame 使用）。

## 构建与测试

需要 Rust（edition 2021，`rust-version = 1.85`）。desktop 构建还需要 `bun`（构建 `frontend/app` 前端）。

```bash
cargo build
cargo test --all-targets --all-features
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

> 本仓库约定所有 shell 命令通过 `rtk` 执行（在原始命令前加 `rtk`），例如 `rtk cargo test`。
> `provider_deepseek_smoke` 是显式 opt-in 真实网络测试；如果环境中设置了
> `DEEPSEEK_API_KEY`，全量测试会尝试出网。做本地无网络验证时可使用：
> `rtk env -u DEEPSEEK_API_KEY cargo test --all-targets --all-features`。

## Quickstart

### CLI

```bash
# 初始化 Lure 自己的数据目录
cargo run --bin lure -- onboard

# 离线试跑（EchoProvider，适合黑盒测试和本地 smoke）
cargo run --bin lure -- agent -m "你好" --model echo \
  --config ~/.lure/config.json --workspace ~/.lure/workspace

# 真实 provider（使用 config 默认模型；需 API key）
cargo run --bin lure -- agent -m "你好" \
  --config ~/.lure/config.json --workspace ~/.lure/workspace
```

`lure onboard` 默认创建 `~/.lure/config.json`、`~/.lure/workspace/`、`cli-apps/`、`cron/`、`history/`、`webui/`，以及 workspace 下的 `sessions/`、`prompts/`、`skills/`、`triggers/`、`cron/`、`memory/` 和基础模板文件。重复执行不会覆盖已有 `config.json`、`USER.md`、`SOUL.md` 等用户文件；测试或自定义安装可用 `--root /path/to/.lure`。

### Desktop

```bash
# 确保先跑过 onboard
cargo run --bin lure -- onboard

# 构建前端（首次或前端源码有变更时；产物输出到 frontend/dist）
cd frontend/app && bun install && bun run build && cd ../..

# 启动桌面应用
cargo run --bin lure-desktop -- --model echo
```

Tauri V2 窗口加载进程内 WebUI（自有 shadcn/Tailwind 前端）；所有能力由 desktop 进程内 HTTP/WS 提供（不跑独立 web 服务，也不走 Tauri IPC）。`--model`/`--preset`/`--config`/`--workspace` 参数与 CLI 对齐。

> 前端开发热更：一端跑 `cargo run --bin lure-desktop -- --headless --http-port 1789 --model echo`
> 起后端，另一端在 `frontend/app` 跑 `LURE_BACKEND=http://127.0.0.1:1789 bun run dev`，
> 浏览器打开 Vite dev server（`/webui`、`/api` 已代理到后端，WS 走 bootstrap 返回的绝对地址）。

## CLI 用法

```bash
# 一次性对话：跑完 agent loop，保存 user/assistant turn 并输出回复
cargo run --bin lure -- agent -m "你好" --model echo --workspace /path/to/workspace

# 交互模式：持续复用同一 session；输入 exit、quit、/exit、/quit 或 :q 退出
cargo run --bin lure -- agent --model echo \
  --config ~/.lure/config.json --workspace ~/.lure/workspace --session cli:direct

# 真实 provider（--model 与 --preset 互斥，二选一）
cargo run --bin lure -- agent -m "你好" --model deepseek-v4-pro          # 需 DEEPSEEK_API_KEY
cargo run --bin lure -- agent -m "你好" --config ~/.nanobot/config.json --preset fast

# 无子命令：打印版本
cargo run --bin lure
```

`--workspace` 缺省时使用 `~/.nanobot/workspace`，`--config` 缺省时使用 `~/.nanobot/config.json`（对齐上游默认路径）。如需使用 Lure 自己的数据目录，先执行 `lure onboard`，再显式传入 `--config ~/.lure/config.json --workspace ~/.lure/workspace`。会话以 JSONL 持久化在 `<workspace>/sessions/` 下，文件名为 session key 的 base64url 编码。provider 身份、api_base、model 与生成参数均由 resolver 解析出的不可变 runtime 决定，并挂载工具运行时（tool-call 循环）：workspace 绑定的 `read_file`/`write_file` 默认可用；`exec` shell 工具需在 config 中显式开启并配置 allow 模式：

```jsonc
{
  "tools": {
    "exec": { "enabled": true, "allow": ["^ls\\b", "^cat\\b"], "deny": [] }
  }
}
```

CLI 默认挂载 workspace 绑定的长期记忆：每轮把 `MEMORY.md` 的记忆块注入上下文，并把 user/assistant 内容记入 `history.jsonl`（供后续 dream consolidation）。

## 贡献约定

- 默认使用中文沟通、提交信息、文档与注释。
- 修改前后检查 git 状态，只显式暂存本次需要提交的路径。
- 引入 Rust 代码后，阶段验收默认包含 `cargo fmt`、`cargo clippy`、`cargo test`。
- 详见 [AGENTS.md](AGENTS.md)。
