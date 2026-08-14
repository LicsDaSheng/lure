# Project Instructions

## 重要原则

- 默认使用中文进行沟通、计划、总结、文档、注释和提交信息编写。
- 保持 git 历史清晰、可追踪：
  - 修改前后都要检查状态。
  - 只显式暂存本次需要提交的路径。
  - 除非用户明确要求，否则不要回滚用户已有改动。
  - 除非用户明确要求，否则不要使用破坏性的 git 命令。

## Shell

- 所有 shell 命令都必须通过 `rtk` 执行，也就是在原始命令前加上 `rtk`。
  - 示例：使用 `rtk pnpm test`、`rtk git status`。

## 项目背景

- 这是 lure 的 **TypeScript 重写**（技术选型改道：后端从 Rust 替换为 TypeScript，产品特性能力不变）。
- 当前分支：`refactor/typescript-rewrite`（`main` 分支仍是 Rust 基线，保留完整历史作参照）。
- 上游事实来源：`/Users/scottlee/workspace/github/nanobot`（Python）。
- 迁移的**可执行规格**分两层：
  - **领域行为**（agent loop / provider / session / memory / cron / tool 等）以原 Rust 版测试（`main` 分支 `crates/lure-core/tests`）为可执行规格，上游 nanobot 为事实来源；确认前不自行发明产品行为。
  - **前端协议契约**（对话面的消息 / 流式 / 工具 / 推理）适配 **assistant-ui 的最佳实践协议契约**（`ThreadMessage` / part 模型 + 自定义 runtime 适配器），不逐字复刻旧 webui WS turn 协议。

## 技术栈（已定稿，详见 handbook/typescript-migration-plan.md）

- 运行时 **Node 22 LTS**；包管理 **pnpm workspaces**。
- 语言/契约：TypeScript `strict` + **zod v4** + 判别联合 + `neverthrow Result`。
- HTTP/WS：**Hono**（`@hono/node-server` + `@hono/node-ws`）。
- provider/agent loop：**手写**（fetch/undici + SSE），**仅 OpenAI-compat**（不复刻原生 Anthropic/Bedrock）。
- MCP：**`@modelcontextprotocol/sdk`**。
- 前端：React 18 + **assistant-ui**（对话面）+ shadcn/ui + Tailwind v4。
- 测试：vitest + Playwright；lint：oxlint；打包：tsdown/esbuild + vite + Node SEA。

## 目录结构

```
desktop/
├── ui/          # React + assistant-ui + shadcn 前端
├── shell/       # Tauri 壳（Rust，仅 spawn 后端 + 窗口）
└── backend/     # Node 后端可执行入口（SEA 单文件 sidecar）
packages/
├── schema/      # @lure/schema — zod v4 契约
├── core/        # @lure/core   — 领域库
└── server/      # @lure/server — Hono 传输层
e2e/             # Playwright 契约测试
```

## 架构原则

- 以 **package 边界**作为模块化机制（pnpm workspaces）。
- 保持「解析 / 领域 / 运行时 / 存储 / 传输」清晰边界。
- 优先类型化 API、zod schema、判别联合与结构化错误；避免字符串控制流。
- 只有真实跨包复用需求才拆分包；避免万能包或泛化工具模块。
- webui 的领域/存储（transcript / tokens / mux / session_index）在 `@lure/core`；传输（Hono HTTP+WS）在 `@lure/server`。

## 实现指导

- 采用 TDD：先移植对应 Rust 测试为 vitest（RED），再实现（GREEN），验证覆盖率 ≥ 80%。
- 先做窄而可运行的纵向闭环，再展开外围。
- 磁盘格式（config.json / sessions/*.jsonl / history.jsonl / MEMORY.md / cron/* / webui/*）与现有 serde 字段名**字节级一致**。
- 前端对话面用 assistant-ui，自定义 runtime 适配器（`useExternalStoreRuntime`）对接 WS turn 协议。
- 上游契约确认前，保持公开 API 最小化。

## 门禁

- `pnpm typecheck`（tsc）、`pnpm lint`（oxlint）、`pnpm test`（vitest）、`pnpm build`。
- 测试覆盖率 ≥ 80%；上游测试无法等价复刻时，在 handbook 记录缺口与原因。

## Git 提交格式

- 提交信息中文优先：

```text
feat: 描述
- 细节一
- 细节二
```
