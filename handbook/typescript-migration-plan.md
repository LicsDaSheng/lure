# TypeScript 迁移技术方案（Rust → TypeScript）

> 分支：`refactor/typescript-rewrite`
> 性质：技术选型改道，**产品特性能力不变**，仅替换实现技术栈。

## 1. 背景与目标

- 原 lure 是 Rust 复刻（Cargo workspace，`lure-core` ~27k 行 + 12k 行测试、80 个测试文件）。
- 上游事实源 `nanobot` 本身是 **Python**（`pyproject.toml`/`uv.lock`）；原 lure 是它的 Rust 复刻。
- 本次改道：把后端领域逻辑从 Rust 换成 **TypeScript**，产品能力（agent loop / provider / bus / channel / cron / memory / mcp / tool / gateway / webui / 桌面）不做增删。
- 前端已自研为 React + shadcn/ui + Tailwind v4（TS），本次再引入 assistant-ui 承载对话面。
- 桌面壳保留 Tauri V2（仅壳），业务能力由进程内 Node 后端提供。
- 迁移的**可执行规格**分两层：领域行为以原 Rust 版测试为规格（`main` 分支 `crates/lure-core/tests`）；前端协议契约适配 **assistant-ui 的最佳实践协议契约**（`ThreadMessage`/part 模型 + 自定义 runtime 适配器）。

## 2. 决策总览（已定稿）

| 维度 | 选型 | 依据 |
|---|---|---|
| 运行时 | **Node 22 LTS** | 对齐 openclaw / deepseek-harness 领域主流 |
| 包管理 / 工作区 | **pnpm workspaces** | deepseek-harness 同款；前端 bun → pnpm 一次性迁移 |
| 语言 / 契约 | TS `strict` + **zod v4** + 判别联合 + `neverthrow Result` | openclaw 用 zod v4；对齐 serde |
| HTTP / WS | **Hono**（`@hono/node-server` + `@hono/node-ws`） | 极简可组合，最贴 axum，Node 一等支持 |
| provider / agent loop | **手写**（fetch/undici + SSE，**仅 OpenAI-compat**） | 参考项目无一用 Vercel AI SDK |
| MCP | **`@modelcontextprotocol/sdk`** | deepseek-harness 实测 v1.12 |
| 持久化 | fs JSONL/JSON，**磁盘格式不变** | 数据无缝迁移（config.json / sessions/*.jsonl / history.jsonl / MEMORY.md / cron/* / webui/*） |
| 桌面 | **Tauri V2 仅壳** + Node sidecar（Node SEA 单文件） | 轻量；业务全在 Node |
| 前端 | React 18 + **assistant-ui** + shadcn/ui + Tailwind v4 | assistant-ui 承载通用 agent 对话面 |
| 测试 | vitest（单测/集成）+ Playwright（E2E） | 现网 + 参考一致 |
| lint / format | oxlint + oxfmt（备选 biome） | openclaw / deepseek 均用 oxc 系 |
| 打包 | tsdown/esbuild（lib）+ vite（前端）+ Node SEA（sidecar） | deepseek 用 tsdown |

已明确**删除**的能力：

- **CLI**：不再提供 `lure` 二进制与 `agent/onboard/version` 子命令。
  - `onboard` 语义下沉为 `@lure/core` 的幂等 `ensureWorkspace()`，由后端启动时调用（首启初始化 `~/.lure` 数据目录与模板，不覆盖已有用户文件）。
  - headless 模式改由 `desktop/backend` 提供（E2E 直接驱动它，不需要窗口/壳）。

## 3. 目标目录结构（desktop 聚合版）

一级目录只表达三件事：**产品入口（desktop）+ 核心能力（packages）+ 验证（e2e）**。

```
lure/
├── desktop/
│   ├── ui/              # React + assistant-ui + shadcn + Tailwind 前端
│   ├── shell/           # Tauri 壳（Rust，仅 spawn backend + 窗口）
│   └── backend/         # Node 后端可执行入口（SEA 单文件；parse flags → ensureWorkspace → createServer）
├── packages/            # 核心能力（跨面复用，不变）
│   ├── schema/          # @lure/schema — zod v4 契约（config/API/turn 事件）
│   ├── core/            # @lure/core   — 领域库（16 模块 + webui 领域/存储）
│   └── server/          # @lure/server — Hono HTTP+WS 传输层
├── e2e/                 # Playwright 契约测试
├── package.json         # pnpm workspaces 根
├── pnpm-workspace.yaml
└── tsconfig.base.json
```

一级目录语义：

| 目录 | 语义 | 类型 |
|---|---|---|
| `desktop/` | 桌面 App：界面（ui）+ 外壳（shell）+ 后端入口（backend） | 产品入口 |
| `packages/` | 契约（schema）/ 领域（core）/ 传输（server） | 核心能力 |
| `e2e/` | 契约测试 | 验证 |

## 4. 模块映射（Rust → TS）

| Rust 模块 | TS 位置 | 说明 |
|---|---|---|
| `lure-core::config` | `packages/core/config` | zod schema，字段名与 serde **字节级一致** |
| `lure-core::provider` | `packages/core/provider` | 手写 OpenAI-compat（fetch + SSE 流式），仅 OpenAI-compat |
| `lure-core::agent` | `packages/core/agent` | async loop / scheduler（AbortController 实现 `/stop` 取消与 cron 让位） |
| `lure-core::bus` | `packages/core/bus` | 进程内类型化消息队列 |
| `lure-core::channel` | `packages/core/channel` | turn 事件路由契约（传输在 server） |
| `lure-core::session` | `packages/core/session` | fs JSONL，key（base64url 文件名）不变 |
| `lure-core::memory` | `packages/core/memory` | MEMORY.md / history / dream consolidation |
| `lure-core::tool` | `packages/core/tool` | registry / file / shell / mcp / search |
| `lure-core::mcp` | `packages/core/mcp` | `@modelcontextprotocol/sdk` stdio |
| `lure-core::cron` | `packages/core/cron` | 表达式解析 + interval task + 持久化 |
| `lure-core::trigger` | `packages/core/trigger` | at-least-once 投递队列 |
| `lure-core::gateway` | `packages/core/gateway` | 编排 |
| `lure-core::command` | `packages/core/command` | slash 命令三层路由 |
| `lure-core::security` | `packages/core/security` | workspace 路径边界（path.resolve + 前缀 + realpath 校验） |
| `lure-core::api` | `packages/core/api` | OpenAI-compat API 表面（传输无关） |
| `lure-core::pairing` | `packages/core/pairing` | 配对码 |
| `lure-core::webui`（领域/存储） | `packages/core/webui` | transcript / tokens / mux / session_index |
| `lure-core::webui`（传输） | `packages/server` | Hono HTTP+WS（axum 等价物） |
| `lure-core::runtime` | 删除 | Node 原生无 block_on 概念 |
| `lure-cli` | 删除 | CLI 移除 |
| `lure-desktop` | `desktop/shell` | Tauri 壳 |

## 5. 类型与契约纪律（对标 Rust 的类型化哲学）

- TS `strict` + `verbatimModuleSyntax` + 显式错误类型。
- `zod v4` 取代 serde：config / API / 消息 / turn 事件的统一 schema，**字段名与现有 serde 磁盘格式字节级一致**。
- Rust `enum` → 判别联合 + 穷尽 `switch`。
- Rust `Result<T,E>` → `neverthrow` 的 `Result`（结构化错误，不裸 throw）。
- 保持「解析 / 领域 / 运行时 / 存储 / 传输」分层；不引入 DI 框架。

## 6. 运行时与并发模型（对标单 tokio 运行时）

- Node 22 单线程事件循环覆盖 IO 密集负载；必要时 `worker_threads` 处理 CPU 密集。
- `AgentLoopScheduler` → 每 session 一条 async 队列 + `AbortController` 实现 `/stop` 取消与 cron 让位。
- `AsyncBus` → 进程内类型化消息总线。
- cron → 自实现 interval task（30s 轮询，`LURE_CRON_POLL_MS` 覆盖）。
- MCP → `@modelcontextprotocol/sdk` + `child_process.spawn` stdio。

## 7. 前端（assistant-ui）

- 对话面（消息列表 / 输入框 / 流式 / markdown / tool-call 卡片 / reasoning 块）用 assistant-ui 的 `Thread`/`Message`/`Composer`/`ToolUI`/`Markdown`。
- 写一个自定义 runtime 适配器 `useLureRuntime`（基于 `useExternalStoreRuntime` + 消息转换器 + 流处理器），把 lure 的 WS turn 事件协议（`turn_id` 键控、reasoning/tool-call/streaming）翻译成 assistant-ui 的 `ThreadMessage`/part 模型；`append`（提交）、`cancel`（`/stop`）双向映射。
- 侧栏 / settings / activity-timer 等 app 专属逻辑仍自研（shadcn）。
- 放弃手写 `useTypewriter` / 自研 reasoning 拆分，改用 assistant-ui 内置平滑 markdown + `ReasoningPart`。

## 8. 迁移路线（每 Phase 以「移植对应 Rust 测试为 vitest → RED → 实现 → GREEN」收尾）

| Phase | 内容 | 对应 Rust 测试 |
|---|---|---|
| 0 | pnpm workspaces 脚手架 + tsconfig + `@lure/schema`（config 字段名对齐 serde）+ 5 个 spike | `config_*` |
| 1 | 纵向切片：provider(OpenAI-compat) → agent loop → `desktop/backend` 启动（`ensureWorkspace`） | `provider_*`、`model_runtime_resolver`、`agent_*`、`subagent*`、`skills_loader`、`system_prompt`、`api_openai` |
| 2 | session + memory + tool(file/shell) | `session_*`、`memory_*`、`tool_file`、`tool_edit`、`tool_shell_policy`、`tool_search`、`tool_registry`、`tool_setup`、`tool_cron` |
| 3 | bus + channel + `@lure/server`(Hono HTTP+WS) + webui 契约 | `bus_*`、`channel_*`、`webui_*`、`gateway_*` |
| 4 | cron + trigger + pairing + command + mcp + api | `cron_*`、`trigger_queue`、`pairing_store`、`command_router`、`mcp_client`、`tool_mcp` |
| 5 | `desktop/shell` spawn sidecar + Docker/CI/Makefile 改造 + E2E 对齐 | `headless`、`e2e/*` |

## 9. 风险与验证项（Phase 0 spike）

| # | 验证项 | 失败回退 |
|---|---|---|
| 1 | assistant-ui 自定义 runtime 适配器能否干净映射 lure WS turn 协议（reasoning/tool-call/streaming 三段） | 回退手写 shadcn 对话面 |
| 2 | Hono + `@hono/node-ws` 复刻 axum 复用协议 + token 握手 | Fastify 或手写 `ws` |
| 3 | `@modelcontextprotocol/sdk` stdio 子进程 | `child_process.spawn` 手写 JSON-RPC |
| 4 | Node SEA 出单文件 + 内嵌前端产物（取代 rust-embed） | esbuild 单文件 + 启动脚本 |
| 5 | SSE 流式解析与 reqwest 行为一致 | 手写 ReadableStream 增量解析 |

## 10. 门禁 / CI 变化

- 删除：`cargo fmt/clippy/test`（仅剩 `desktop/shell` 壳需要）。
- 新增：`pnpm typecheck`（tsc）、`pnpm lint`（oxlint）、`pnpm test`（vitest）、`pnpm build`。
- 保留：Playwright E2E、前端 vitest。
- 测试门禁仍遵循 TDD + 80% 覆盖率；上游测试无法等价复刻时，在 handbook 记录缺口与原因。

## 11. 参考项目选型对照

| 维度 | openclaw | deepseek-harness | hermes-agent | nanobot(上游) |
|---|---|---|---|---|
| 运行时 | Node 22 | Node 22/24 | Node ≥22 | Python 3.11+ |
| 包管理 | npm workspaces | pnpm 11 workspaces | npm workspaces | uv/hatch |
| LLM 抽象 | 自研 `src/providers` | 自研 `dsh-llm` | Python anthropic/openai | anthropic+openai |
| Agent loop | 自研 | 自研（cordis 插件） | 自研 | 自研 |
| MCP | — | `@modelcontextprotocol/sdk` v1.12 | — | `mcp` py 包 |
| HTTP | Express v5 | 自研 cordis server | — | aiohttp/httpx |
| 契约校验 | zod v4 | schemastery(自研) | — | pydantic |
| 聊天 UI | 自研 | 自研 UI primitives | 自研 web/tui | 自研 webui |
| lint | oxlint + oxfmt | oxlint + tsdown + vitest | eslint + typescript-eslint | ruff/basedpyright |
