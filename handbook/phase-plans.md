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
- **完成**：cron store 持久化、next-run 计算、session-bound delivery、heartbeat protected job、trigger at-least-once 投递队列（忙等/limit）。
- **待补**：完整 cron 表达式、cron/trigger 工具化。

## Phase 9: OpenAI-compatible API 与 SDK 表面

- **状态**：partial
- **完成**：`ChatServer`/`ChatRunner`/`ServerConfig`（tiny_http）、`POST /v1/chat/completions`（非流式 JSON + 逐 token SSE）、`GET /v1/models`、`GET /health`、鉴权/解析/校验/响应构造、`session::SessionLocks`（per-session 互斥、RAII）。
- **待补**：multipart/media 上传、SDK facade。

## Phase 10: WebUI 与 Desktop

- **状态**：partial
- **完成**：**desktop 纵向闭环**——原样 vendor 上游 React WebUI（`frontend/`，bun 构建，零改动）、`lure-desktop`（wry+tao 窗口）进程内 loopback HTTP（静态资源+SPA fallback、bootstrap/token 签发、`/api/sessions` 鉴权+DELETE）与 WS 复用协议（`webui::mux` + tungstenite transport）、agent loop 每连接独立实例（`AgentTurnRunner` 适配）。**transcript**：`TranscripStore` 在 turn 结束时写入 JSONL，`GET /api/sessions/{key}/webui-thread` 返回消息视图——桌面重开窗口可见历史对话。**dream**：`ProviderDreamRunner`（真实 LLM 驱动 memory consolidation）+ 阈值自动触发（`maybe_consolidate`，由 `AgentTurnRunner` 每轮 turn 后检查）。新增 42 个测试，smoke 验证通过。
- **待补**：settings/skills/commands 等 /api 大表面、非 macOS 窗口适配。

## Phase 11: 打包、部署与迁移兼容

- **状态**：partial
- **完成**：config migration（`migrate_config`）、legacy session stem 迁移、legacy `HISTORY.md`→`history.jsonl` 迁移、`Dockerfile`+`docker-compose.yml` 骨架、`release-checklist.md`、CLI `--version`。
- **待补**：完整 release pipeline（CI、多平台构建、签名）。

## 下一步目标

CLI 交互体验已成体系并暂告段落：实时流式 → tool/progress 行 → reasoning delta 句级缓冲流 → spinner 后台定时动画 → 阶段语义标签（Thinking / Calling / Finalizing）→ Ctrl-C 中断当前 turn → 单轮 provider 错误容错。均以 TDD 落地，核心逻辑单测 + 本地 SSE mock 端到端验证。

按「用户可感知价值 × 当前覆盖缺口」排序，下一步：

1. **补前端所需 /api stub**：用 `lure-desktop --model echo` 启动后观察前端还调用哪些 /api 端点（`/api/settings`、`/api/webui/sidebar-state` 等），逐个补空载荷 stub → 页面完整可用。
2. **dream 接入 config 驱动的真实 provider**：`ProviderDreamRunner` + `ws_server.with_dream` + 阈值触发已就位，但 `lure-desktop` 目前装配的是 `EchoProvider`（离线安全、产出无意义）；换成 config 解析出的真实 provider 后 dream 产出才有质量。
3. **cron 表达式 + 工具化**：补 `croniter` 等价实现，让 cron job 真正可调度；暴露 cron/trigger 为 agent 可用工具。
4. **CLI slash commands**：交互模式内 `/help`、`/model`、`/session` 等命令（Phase 3 待补）。
