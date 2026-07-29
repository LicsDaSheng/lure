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
- **完成**：`AgentLoop`/`AgentRunner`/`ContextBuilder` 纵向闭环、`LlmProvider` trait + `EchoProvider`（`--model echo` 显式离线脚手架）、turn 保存与历史可读、结构化 `AgentError`、`ProgressEvent` 事件（含 `Finalizing` 阶段信号）、CLI one-shot + interactive REPL、`--session`/`--show-reasoning`。**完整交互 UX**：实时流式渲染 + tool/progress 行 + reasoning delta 句级缓冲流 + spinner 后台定时动画 + 阶段语义标签（Thinking / Calling `<tool>` / Finalizing）+ Ctrl-C 中断当前 turn + 单轮 provider 错误不退出会话。
- **待补**：slash commands。

## Phase 4: Provider 与模型运行时

- **状态**：partial
- **完成**：preset 解析顺序、OpenAI-compatible 请求/响应/错误分类、SSE 流式消费（`StreamAssembler` 增量组装含 tool_calls 跨块拼装）、provider registry 选择顺序、`UreqTransport`、stateful `ModelRuntimeResolver`（admit/refresh/invalidate + 不可变快照）接入 AgentLoop/CLI、config 驱动 `ProvidersConfig`（api_base 覆盖/enabled 过滤/api_key 解析）、usage 归一与流式捕获。
- **待补**：OAuth、重试策略。

## Phase 5: Tool Runtime 与安全边界

- **状态**：partial
- **完成**：`Tool` trait / `ToolRegistry` / JSON Schema 子集校验 / 结果截断、`ReadFileTool`/`WriteFileTool`/`EditFileTool`（workspace 越界拒绝）、`GrepTool`+`ListDirTool`（regex + 分页 + mtime 排序）、`ExecPolicy`+`ExecTool`（allow/deny/分段）、workspace 路径边界、AgentLoop tool-call 循环（至多 8 轮）、config 驱动 `registry_from_config` + CLI 工具注册（文件工具默认，exec opt-in）。
- **待补**：apply_patch、web/mcp、并行 tool。

## Phase 6: Memory、Dream 与长期上下文

- **状态**：partial
- **完成**：`MemoryStore`（MEMORY/SOUL/USER 读写、`history.jsonl` append + cursor + 截断标记、session 过滤、legacy `HISTORY.md` 迁移）、`strip_think`、`DreamRunner` + `consolidate`（fake runner）、`ContextBuilder::with_memory` 注入顺序、AgentLoop 记忆接入（记忆块注入 + history 记录 + `consolidate` 接口）、CLI 常驻挂载 memory、`compact_history` 容量管理。
- **待补**：真实 LLM dream、触发策略（阈值/定时）、SOUL/USER 整合。

## Phase 7: Bus、Channels 与 Gateway

- **状态**：partial
- **完成**：`InboundMessage`/`OutboundMessage` 契约、`MessageBus`（FIFO 队列）、`ProgressUpdate`/`ProgressKind`（Started/ContentDelta/ToolInvoked/Final）、`Channel` trait（含 `deliver_progress`）+ 配置校验、`Gateway` 同步编排（注册/启停不丢任务/inbound→AgentLoop→outbound→路由/progress 转发/未知 channel/health）。
- **待补**：真实 channel 平台（telegram/discord…）。

## Phase 8: Automations、Cron 与 Trigger

- **状态**：partial
- **完成**：cron store 持久化、next-run 计算（at/every/**cron 表达式**）、标准 5 字段 cron 解析（`* , - /`、dow 0-7、dom/dow OR）+ UTC/本地时区 next-run（`cron::expr`）、session-bound delivery、heartbeat protected job、trigger at-least-once 投递队列（忙等/limit）、**`cron` agent 工具**（add/list/remove，绑定会话 origin，已接入 CLI）、**cron service 定时执行**（`CronService::tick` + `record_run` 推进/删一次性、`CronScheduler` 后台轮询工厂线程、`GatewayCronRunner` 经 gateway 投递）、**接入长驻宿主**（lure-desktop 常驻 `CronScheduler`，`CronTurnRunner` 跑 agent turn 并写 origin 会话 transcript → webui 下次打开可见，headless 端到端验证）。
- **待补**：IANA 具名时区（接 chrono-tz）、cron 名称/宏/扩展、run history、cron 触发时向**在线** WS 连接实时推送（当前只落 transcript）、jobs.json 并发读写加锁。

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

已完成（按价值收口）：**前端 /api stub 表面**、**dream 接真实 provider**、**真实浏览器 E2E 契约测试**（`--headless` + Playwright smoke，见 [phase-execution.md](./phase-execution.md) WebUI 契约门禁）、**CLI slash commands**、**cron 表达式**、**`cron` agent 工具**、**cron service 定时执行 + 接入 lure-desktop 长驻宿主**（排期任务后台自动跑，产出写 transcript，webui 下次打开可见）、**settings 写入大表面**（agent/provider/model-preset GET-style 写、原子落盘生效）、**settings 写入面 E2E 契约覆盖 + E2E hermetic 化**（隔离 config + 种子，消除对开发者真实 config 的隐式依赖）。**Phase 8 cron 链路已端到端打通**；**Phase 10 settings 读写双向 + 浏览器契约**已闭环。

按「用户可感知价值 × 当前覆盖缺口」排序，下一步：

1. **cron 实时推送 + IANA 时区**：cron 触发时向**在线** WS 连接实时推送（当前只落 transcript，需刷新才见）；`schedule.tz` 具名时区接 chrono-tz。
2. **E2E 扩面**：跨会话切换、new-chat、真实驱动 settings 面板 UI（当前 settings E2E 走浏览器上下文 fetch，未点透前端组件）等关键用户流补 Playwright 覆盖。
3. **channels 真实平台 / media 代理**：Gateway 接真实 channel（telegram/discord…）、webui media 上传代理。
