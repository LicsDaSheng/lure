# Phase Plans 与验收标准

## Phase 0: 项目骨架与复刻边界

- **状态**：done
- **完成**：Cargo workspace（resolver="2"）、`lure-core`+`lure-cli` crate、最小集成测试骨架、上游测试台账格式。

## Phase 1: 配置、路径与工作区基础

- **状态**：partial
- **完成**：typed config（schema + camelCase/snake 别名 + 默认值）、`load_config`/`save_config`（原子写、权限位）、路径解析（`default_config_path`/`default_workspace`/`expand_user`）、结构化 `ConfigError`、config migration（旧字段变换）、CLI `--config`/`--workspace`、model preset 解析与校验、`ProvidersConfig`/`ToolsConfig`、`lure onboard`（创建 `~/.lure/` 目录结构）。
- **待补**：env 变量插值、onboard 交互式向导。

## Phase 2: Session 与近程历史

- **状态**：partial
- **完成**：session key + base64url 文件命名、JSONL 原子存储/fsync、缓存与 LRU 驱逐、list repair（损坏行保留 session）、历史读取、offset clamp、goal 派生视图、legacy lossy stem 迁移、`delete_stored`。
- **待补**：turn continuation、weak-identity。

## Phase 3: Agent Loop 最小纵向闭环

- **状态**：partial
- **完成**：`AgentLoop`/`AgentRunner` 纵向闭环；**workspace-aware `ContextBuilder`** 按 nanobot 顺序生成单条 system message（identity/runtime/workspace、bootstrap、tool contract、memory、active/summary skills、recent history、archived summary），并在每轮冻结快照后供全部 tool 迭代复用；CLI/Desktop 生产构建路径已接线。其余已有 CLI one-shot/REPL、流式/tool/reasoning UX、Ctrl-C 与 provider 单轮容错保持不变。
- **待补**：media、runtime context block、独立 project workspace、富历史 token/工具边界治理；运行时 slash handlers。

## Phase 4: Provider 与模型运行时

- **状态**：partial
- **完成**：preset 解析顺序、OpenAI-compatible 请求/响应/错误分类、SSE 流式消费（`StreamAssembler` 增量组装含 tool_calls 跨块拼装）、**`CompletionRequest.tools` → `/chat/completions` 顶层 `tools` 透传**、**DeepSeek `reasoningEffort: "none"` → `thinking.type: "disabled"` provider 原生映射**、provider registry 选择顺序、`ReqwestTransport`、stateful `ModelRuntimeResolver` 接入 AgentLoop/CLI、config 驱动 provider、usage 归一与流式捕获。
- **待补**：OAuth、重试策略。

## Phase 5: Tool Runtime 与安全边界

- **状态**：partial
- **完成**：`Tool` trait / `ToolRegistry` / JSON Schema 子集校验 / 结果截断、文件/搜索/exec 工具与 workspace 边界、AgentLoop tool-call 循环、config 驱动注册；**每次 provider 调用（含 finalization）都从当前注册表生成 function definitions 并发送，动态注册的 cron 同样可发现**。
- **待补**：apply_patch、web/mcp、并行 tool。

## Phase 6: Memory、Dream 与长期上下文

- **状态**：partial
- **完成**：`MemoryStore`（MEMORY/SOUL/USER、history/cursor/迁移）、Dream 与阈值触发；**SOUL/USER 已与项目 AGENTS、MEMORY、recent history 一起接入生产 system prompt**，未定制 AGENTS/USER 和空 MEMORY 模板自动跳过；recent history 按 dream cursor/session 过滤、最近 50 条和保守 UTF-8 预算截断，且不会重复本轮 user message。
- **待补**：GitStore、autocompact/context governance、tiktoken 等价精确预算、unified-session history 策略。

## Phase 7: Bus、Channels 与 Gateway

- **状态**：partial
- **完成**：`InboundMessage`/`OutboundMessage` 契约、`MessageBus`（FIFO 队列）、`ProgressUpdate`/`ProgressKind`（Started/ContentDelta/ToolInvoked/Final）、`Channel` trait（含 `deliver_progress`）+ 配置校验、`Gateway` 同步编排（注册/启停不丢任务/inbound→AgentLoop→outbound→路由/progress 转发/未知 channel/health）。
- **待补**：真实 channel 平台（telegram/discord…）。

## Phase 8: Automations、Cron 与 Trigger

- **状态**：partial
- **完成**：cron store 持久化、next-run 计算（at/every/**cron 表达式**）、标准 5 字段 cron 解析（`* , - /`、dow 0-7、dom/dow OR）+ UTC/本地时区 next-run（`cron::expr`）、session-bound delivery、heartbeat protected job、trigger at-least-once 投递队列（忙等/limit）、**`cron` agent 工具**（add/list/remove，绑定会话 origin，已接入 CLI）、**cron service 定时执行**（`CronService::tick` + `record_run` 推进/删一次性、`CronScheduler` 后台轮询工厂线程、`GatewayCronRunner` 经 gateway 投递）、**接入长驻宿主**（lure-desktop 常驻 `CronScheduler`，`CronTurnRunner` 跑 agent turn 并写 origin 会话 transcript → webui 下次打开可见，headless 端到端验证）、**cron 实时推送**（`webui::hub::WsHub` 按 chat_id 索引在线连接的出站 sender；WS 连接读循环改为「读超时 + drain 推送队列」模型，客户端 attach 某会话即幂等订阅、断开注销；`CronTurnRunner` 产出后 `hub.push` 向在线连接推 `message`+`session_updated`——**在线即刻可见、无需刷新**，离线回落 transcript；单元测试覆盖 hub 路由/隔离/剪枝，真实 WS 集成测试 + headless 端到端验证全链路）、**IANA 具名时区**（`schedule.tz` 具名 IANA 名经 chrono-tz 解析，`next_after` 泛型时区逐分钟步进天然处理 DST——spring-forward 不存在的墙钟分钟自动跳过；cron 工具 `is_supported_tz` 放开为任意合法 IANA 名，非法名 add 时拒绝；测试覆盖 Shanghai(+8)/New_York(EST) 偏移 + DST 跳变 + 非法名回落）。
- **待补**：cron 名称/宏/扩展、run history、jobs.json 并发读写加锁。

## Phase 9: OpenAI-compatible API 与 SDK 表面

- **状态**：partial
- **完成**：`ChatServer`/`ChatRunner`/`ServerConfig`（tiny_http）、`POST /v1/chat/completions`（非流式 JSON + 逐 token SSE）、`GET /v1/models`、`GET /health`、鉴权/解析/校验/响应构造、`session::SessionLocks`（per-session 互斥、RAII）。
- **待补**：multipart/media 上传、SDK facade。

## Phase 10: WebUI 与 Desktop

- **状态**：partial
- **完成**：**desktop 纵向闭环**——原样 vendor 上游 React WebUI（`frontend/`，bun 构建，零改动）、`lure-desktop`（wry+tao 窗口）进程内 loopback HTTP（静态资源+SPA fallback、bootstrap/token 签发、`/api/sessions` 鉴权+DELETE）与 WS 复用协议（`webui::mux` + tungstenite transport）、agent loop 每连接独立实例（`AgentTurnRunner` 适配）。**transcript**：`TranscripStore` 在 turn 结束时写入 JSONL，`GET /api/sessions/{key}/webui-thread` 返回消息视图——桌面重开窗口可见历史对话。**dream**：`ProviderDreamRunner`（真实 LLM 驱动 memory consolidation）+ 阈值自动触发（`maybe_consolidate`，由 `AgentTurnRunner` 每轮 turn 后检查）+ config 驱动真实 provider（`build_provider`，缺 key 优雅回落 Echo）。**前端 /api 表面**：加载期读取全覆盖，按需读取（automations/file-preview/skill-detail）平稳降级，delete 改走 `GET /api/sessions/{key}/delete` + session key URL 解码（修 `%3A` 编码 404）。**settings 写入大表面**（config-backed）：`/api/settings/update`（默认 agent + 生效 preset 指针）、`/api/settings/provider/update`（api_key/api_base 覆盖）、`/api/settings/model-configurations/{create,update}`（命名 preset）——前端 `GET .../update?a=b` 携 snake_case query，映射回 lure `Config`、`validate` 后原子落盘、回派生载荷；未建模字段静默忽略、非法值回 400 不落盘。未知子路径（如 model-configurations 删除，上游无此契约）走通用 404。**E2E 契约测试**：`lure-desktop --headless`（无窗口起 server + 打印 `LURE_HTTP_URL`）+ `e2e/` Playwright smoke——加载/bootstrap → WS echo 往返 → 真实编码 key 删除回归 → **settings 写入面**（GET+query 写、响应形状、重读确认落盘、非法值 400），真实浏览器 + 真实 desktop 全装配驱动。**E2E 现 hermetic**：`--config` 隔离到 workspace 内 + 启动前复制种子 config（已配置 provider，令应用确定性进聊天界面），不再隐式依赖开发者机器上的真实 `~/.nanobot/config.json`。
- **待补**：channels 等其余变更表面、media 代理、非 macOS 窗口适配、E2E 扩面（跨会话切换/new-chat）。

## Phase 11: 打包、部署与迁移兼容

- **状态**：partial
- **完成**：config migration（`migrate_config`）、legacy session stem 迁移、legacy `HISTORY.md`→`history.jsonl` 迁移、`Dockerfile`+`docker-compose.yml` 骨架、`release-checklist.md`、CLI `--version`。
- **待补**：完整 release pipeline（CI、多平台构建、签名）。

## 下一步目标

CLI 交互体验已成体系并暂告段落：实时流式 → tool/progress 行 → reasoning delta 句级缓冲流 → spinner 后台定时动画 → 阶段语义标签（Thinking / Calling / Finalizing）→ Ctrl-C 中断当前 turn → 单轮 provider 错误容错。均以 TDD 落地，核心逻辑单测 + 本地 SSE mock 端到端验证。

已完成（按价值收口）：**前端 /api stub 表面**、**dream 接真实 provider**、**真实浏览器 E2E 契约测试**（`--headless` + Playwright smoke，见 [phase-execution.md](./phase-execution.md) WebUI 契约门禁）、**CLI slash commands**、**cron 表达式**、**`cron` agent 工具**、**cron service 定时执行 + 接入 lure-desktop 长驻宿主**（排期任务后台自动跑，产出写 transcript，webui 下次打开可见）、**settings 写入大表面**（agent/provider/model-preset GET-style 写、原子落盘生效）、**settings 写入面 E2E 契约覆盖 + E2E hermetic 化**（隔离 config + 种子，消除对开发者真实 config 的隐式依赖）、**cron 实时推送**（WsHub 在线连接注册表 + 连接读循环 drain，cron 产出实时推给在线连接，无需刷新）、**IANA 具名时区**（chrono-tz，含 DST 正确处理；cron 工具接受任意合法 IANA 名）。**Phase 8 cron 链路已端到端打通（含实时推送 + 具名时区）**；**Phase 10 settings 读写双向 + 浏览器契约**已闭环。

### 与上游的核心能力差异（2026-07-30 复盘）

外围体验（CLI UX、cron 链路、webui 读写、settings、E2E）已成体系；但与上游对照后，**真正拉开差距的是几个成规模的独立子系统**，此前在阶段表中被"partial"掩盖。完整清单见 [phase-roadmap.md](./phase-roadmap.md)「跨阶段未建模的上游子系统」。核心差异点：

- ~~MCP、Skills、Subagent、Pairing 四个子系统 lure 完全未建模~~ → **均已补齐可测核心**（见下「进度」）。
- ~~命令路由：CLI 仅 3 条 slash~~ → **CommandRouter 核心 + 全表登记已复刻**。
- **多模态**（图像生成 / 音频转写）整条链缺失（仍是缺口）。

### 进度（2026-07-30 本轮）

按 handbook 固定 TDD 循环逐个补齐了此前四个未建模子系统 + 命令路由的**可测核心**（每个先写 Rust 测试再实现，全量 `cargo test`+clippy 绿，逐个提交）：

| 子系统 | 已落地核心 | 暂缓（记台账） |
|---|---|---|
| **Pairing** | `PairingStore` 全套（31 例，对齐 test_store 全场景） | — |
| **命令路由** | `CommandRouter` 三层派发 + 谓词 + 全表登记 + /help//pairing//skill（15 例） | 运行时命令处理器接线、CLI REPL 接入 |
| **Skills** | `SkillsLoader` 两源枚举/需求过滤/YAML frontmatter/摘要（21 例）+ `/skill` 接入 | bundled skills 资产、webui skills_api |
| **Subagent** | `SubagentStatus`/registry/`cancel_by_session`/partial-progress（21 例） | 真实后台执行（需异步运行时） |
| **MCP** | 工具名净化/限长 + OpenAI schema 归一 + 畸形进度检测（14 例） | 真实连接/会话/传输（需 MCP SDK + 异步）、webui 预设 |

共同暂缓项集中在**异步运行时**（subagent 后台执行、MCP 连接、运行时命令处理器）——lure 当前为同步模型，是这些子系统"活起来"的共同前置。

> **范围决定**：provider 只做 **OpenAI-compat 一种线协议**。上游的原生 Anthropic Messages / Bedrock / Azure / Copilot / Codex 协议与 fallback 链**明确不复刻**——不视为缺口。多模态若做，也走 OpenAI 兼容端点实现。

### 下一步优先级（按「用户可感知价值 × 与上游差距」排序）

1. **系统提示词与真实工具发现闭环（P0，2026-08-13 已完成）**：富 system prompt、skills/recent history、生产接线和 provider `tools` schema 已按 TDD 落地；剩余 media/runtime-context/project-scope 单列为后续，不阻塞当前真实工具调用。
2. **命令路由运行时接线 + subagent 工具注册**（P1）：把已实现核心接到 CLI/Desktop 常驻 AgentLoop。
3. **MCP 会话能力补齐**（P1）：在既有 stdio client 上补 HTTP/SSE、重连与 provider 工具注册。
4. **E2E 扩面 / cron run history**（P2）：跨会话/new-chat 浏览器覆盖、cron 执行历史与 jobs.json 并发加锁。

P0 验收证据：`system_prompt.rs`、`agent_memory.rs`、`agent_tool_loop.rs`、
`provider_openai.rs` 定向测试通过；`cargo fmt --check`、全目标/全 feature Clippy（warnings deny）
与移除 `DEEPSEEK_API_KEY` 后的全量 Cargo tests 通过。仓库当前未配置 Rust coverage 工具，
本轮未生成覆盖率百分比。
