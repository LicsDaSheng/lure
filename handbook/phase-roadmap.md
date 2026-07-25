# Phase Roadmap

## Phase 0: 项目骨架与复刻边界

- 状态：`done`
- 目标：建立 Rust 项目最小骨架、workspace/crate 边界、测试入口和上游行为盘点方式。
- 范围：
  - 初始化 Cargo workspace。
  - 建立最小 CLI crate 和核心库 crate。
  - 建立上游测试映射台账格式。
  - 明确第一条纵向闭环的输入、输出和不做事项。
- 不做：
  - 不实现 provider 调用。
  - 不实现 channel/gateway/webui。
  - 不承诺完整兼容上游所有配置。

## Phase 1: 配置、路径与工作区基础

- 状态：`partial`
- 已落地：config schema + camelCase/snake 别名 + 默认值、config/workspace 路径解析（`~` 展开）、原子读写、结构化 `ConfigError`、config migration、model preset 解析/校验、`ProvidersConfig` 与 `ToolsConfig`。
- 待补：onboard 交互式初始化、env 插值、gateway/security 等更完整字段。
- 目标：复刻 `nanobot` 的实例配置、workspace 路径和基础文件布局，为 session/memory/agent loop 提供稳定地基。
- 上游参考：
  - `nanobot/config/`
  - `docs/concepts.md`
  - `docs/configuration.md`
  - `tests/config/`
  - `tests/agent/test_onboard_logic.py`
- 范围：
  - config 读取、默认值、camelCase/snake_case 兼容策略。
  - config path 与 workspace path 解析。
  - onboard 初始化最小文件。
  - 错误类型和配置校验。

## Phase 2: Session 与近程历史

- 状态：`partial`
- 已落地：session key + base64url 文件命名、JSONL 原子存储、缓存与修复、历史读取与 offset clamp、goal 派生视图、list repair、legacy 有损 session 文件名迁移。
- 待补：turn continuation、weak-identity、automation/webui turn 兼容模型。
- 目标：复刻 session key、JSONL 存储、历史读取、turn continuation、goal state、压缩偏移等核心状态语义。
- 上游参考：
  - `nanobot/session/`
  - `tests/session/`
  - `tests/agent/test_loop_save_turn.py`
  - `tests/agent/test_consolidate_offset.py`
- 范围：
  - session 文件命名和 key 安全规则。
  - 原子写入、fsync、缓存修复。
  - 历史可见性、offset clamp、goal state。
  - automation/webui turn 的最小兼容模型。

## Phase 3: Agent Loop 最小纵向闭环

- 状态：`partial`
- 已落地：`AgentLoop`/`AgentRunner`/`ContextBuilder` 纵向闭环、`LlmProvider` trait + `EchoProvider`、turn 保存与历史可读、结构化 `AgentError`、progress 事件、CLI `agent -m` one-shot + 基础 interactive REPL + `--session`/`--show-reasoning`。
- 待补：prompt_toolkit 富交互、stream/progress 渲染、slash commands。
- 目标：实现 CLI one-shot 到 agent loop，再到可替换 provider stub，最终保存 session 并输出回复的最小闭环。
- 上游参考：
  - `nanobot/agent/loop.py`
  - `nanobot/agent/runner.py`
  - `nanobot/agent/context.py`
  - `tests/agent/test_loop_runner_integration.py`
  - `tests/cli/`
- 范围：
  - CLI `agent -m` 最小入口。
  - context builder 最小可测结构。
  - provider trait 和 fake provider。
  - turn 保存、progress 事件和最终输出。
- 不做：
  - 不接真实 LLM。
  - 不实现复杂 streaming/tool call。
  - 不做 gateway/channel。

## Phase 4: Provider 与模型运行时

- 状态：`partial`
- 已落地：preset 解析顺序、OpenAI-compatible 请求/响应/错误分类、SSE 流式消费（增量回调 + 内容/tool_calls 组装）、provider registry 选择顺序、`UreqTransport` 真实同步/增量传输、stateful `ModelRuntimeResolver`（admit/refresh/invalidate + 不可变快照）接入 AgentLoop/CLI、config 驱动 `ProvidersConfig`（api_base 覆盖 / enabled 过滤 / api_key 解析）、CLI `--config`/`--preset`/`--model`。
- 待补：真实 provider smoke（opt-in）、OAuth/local fallback、`max_completion_tokens`/流式 usage/重试。
- 目标：建立 provider registry、model preset、运行时解析和 OpenAI-compatible 的最小真实调用边界。
- 上游参考：
  - `nanobot/providers/`
  - `nanobot/agent/model_runtime.py`
  - `tests/providers/`
  - `tests/agent/test_model_runtime_resolver.py`
- 范围：
  - provider/model preset 解析顺序。
  - OpenAI-compatible 请求/响应结构。
  - provider 错误分类、超时和重试边界。
  - fake provider 与真实 provider 测试分层。

## Phase 5: Tool Runtime 与安全边界

- 状态：`partial`
- 已落地：`Tool` trait / `ToolRegistry` / JSON Schema 子集校验 / 结果截断、`ReadFileTool`/`WriteFileTool`（workspace 越界拒绝）、`ExecPolicy`+`ExecTool`（allow/deny/分段/fd 重定向）、workspace 路径边界、AgentLoop tool-call 循环（tool_calls → 执行 → tool turn 回灌，至多 8 轮）、config 驱动 `registry_from_config` + CLI 工具注册（文件工具默认、exec opt-in）。
- 待补：apply_patch/search/web/mcp、file edit/search、exec env/session 隔离、并行 tool、network SSRF。
- 目标：复刻工具注册、schema、文件和 shell 等基础工具，并把 workspace 安全策略作为功能契约实现。
- 上游参考：
  - `nanobot/agent/tools/`
  - `nanobot/security/`
  - `tests/tools/`
  - `tests/security/`
  - `tests/test_file_tool_toggle.py`
- 范围：
  - tool trait、schema、registry。
  - 文件读写/搜索/编辑的最小安全实现。
  - shell 执行策略与错误映射。
  - 工具调用结果截断和上下文传递。

## Phase 6: Memory、Dream 与长期上下文

- 状态：`partial`
- 已落地：`MemoryStore`（MEMORY/SOUL/USER 读写、`history.jsonl` append + cursor、session 过滤、legacy `HISTORY.md` 迁移）、`strip_think`、可替换 `DreamRunner` + `consolidate`（fake runner）、`ContextBuilder::with_memory` 注入顺序、AgentLoop 记忆接入（记忆块注入 + history 记录 + `consolidate` 接口）、CLI 常驻挂载 memory。
- 待补：真实 LLM dream、dream 自动触发策略（阈值/定时）、SOUL/USER 整合、compact/并发锁、GitStore 版本化。
- 目标：复刻 memory store、history、dream consolidation 触发和上下文注入规则。
- 上游参考：
  - `nanobot/agent/memory.py`
  - `nanobot/templates/memory/`
  - `tests/agent/test_memory_store.py`
  - `tests/agent/test_dream.py`
  - `docs/memory.md`
- 范围：
  - `memory/MEMORY.md` 和 history 存储。
  - session history 到 memory 的整合流程。
  - 上下文构建时的 memory 注入。

## Phase 7: Bus、Channels 与 Gateway

- 状态：`partial`
- 已落地：`InboundMessage`/`OutboundMessage` 契约、`MessageBus`（FIFO inbound/outbound 队列）、`ProgressUpdate`/`ProgressKind`（Started/ContentDelta/ToolInvoked/Final）运行时事件、`Channel` trait（含 `deliver_progress`）+ 配置校验、`Gateway` 同步编排闭环（注册/启停不丢任务/inbound→AgentLoop→outbound→路由/progress 转发/未知 channel/health）。
- 待补：channel 热加载与具体平台、async 事件订阅、真实 HTTP health endpoint、进程管理 runtime（后二者属 Phase 9/10 transport）。
- 目标：把 CLI 之外的入口抽象到 message bus/channel contract，并实现 gateway 最小常驻进程。
- 上游参考：
  - `nanobot/bus/`
  - `nanobot/channels/`
  - `nanobot/gateway/`
  - `tests/bus/`
  - `tests/channels/`
  - `tests/gateway/`
- 范围：
  - Inbound/Outbound message contract。
  - channel 生命周期、热加载边界、基础校验。
  - gateway health endpoint 和服务编排。
  - WebSocket channel 的最小可用路径。

## Phase 8: Automations、Cron 与 Trigger

- 状态：`partial`
- 已落地：cron store 持久化、next-run 计算、session-bound delivery、heartbeat protected job、trigger at-least-once 投递队列（忙等/limit）。
- 待补：完整 cron 表达式、工具 schema、文件 inbox 布局、trigger 定义存储。
- 目标：复刻提醒、定时任务、heartbeat、trigger delivery 这些后台任务能力。
- 上游参考：
  - `nanobot/cron/`
  - `nanobot/triggers/`
  - `tests/cron/`
  - `tests/triggers/`
  - `docs/automations.md`
- 范围：
  - cron job 存储和调度。
  - session-bound delivery。
  - heartbeat protected job。
  - trigger at-least-once 语义。

## Phase 9: OpenAI-compatible API 与 SDK 表面

- 状态：`partial`
- 已落地：传输无关的 chat completions 请求/响应/校验/鉴权/session 表面、SSE 事件顺序；
  真实 HTTP server 接线（`ChatServer` / `ChatRunner` / `ServerConfig`），含 `GET /v1/models`
  与 `GET /health`；**逐 token SSE**（`run_streaming` 驱动内容增量、跨 tool 轮不关流、共享单 id）；
  per-session 并发锁原语 `SessionLocks`（同 key 串行、不同 key 独立、RAII 自释放）。
- 待补：media、SDK facade；`SessionLocks` 接入 HTTP handler 待多线程 runner。
- 目标：实现 `/v1/chat/completions` 等 API 兼容层和 SDK 可调用表面。
- 上游参考：
  - `nanobot/api/`
  - `nanobot/sdk/`
  - `tests/test_openai_api.py`
  - `tests/test_api_stream.py`
  - `tests/gateway/test_api_runtime.py`
- 范围：
  - chat completions 请求/响应。
  - streaming 事件。
  - API runtime lifecycle。
  - 鉴权和 token 最小策略。

## Phase 10: WebUI 与前端集成

- 状态：`partial`
- 已落地：传输无关的 WebUI 后端服务协议——session list/thread/status、WS inbound/outbound 事件形状。
- 待补：真实 HTTP/WebSocket 传输、settings/transcript/token usage/媒体等大表面、前端资产复用或重写。
- 目标：在 Rust 后端稳定后，复刻 WebUI 服务、WebSocket 协议和前端集成。
- 上游参考：
  - `webui/`
  - `nanobot/webui/`
  - `nanobot/channels/websocket/`
  - `tests/webui/`
  - `webui/src/tests/`
- 范围：
  - WebUI runtime/build/status。
  - session list/index API。
  - WebSocket stream 协议。
  - 前端测试映射。

## Phase 11: 打包、部署与迁移兼容

- 状态：`partial`
- 已落地：config 迁移、基础 legacy fixture 迁移、Docker 骨架。
- 待补：完整 Rust crate/package 发布结构、Docker 镜像与 compose、发布前完整验收清单。
- 目标：完成跨平台安装、Docker、发布包、已有 workspace/config/session 的兼容迁移。
- 上游参考：
  - `Dockerfile`
  - `docker-compose.yml`
  - `scripts/`
  - `hatch_build.py`
  - `tests/test_package_version.py`
  - `tests/test_docker.sh`
- 范围：
  - Rust crate/package 发布结构。
  - Docker 镜像和 compose。
  - legacy 数据修复与迁移。
  - 发布前完整验收清单。
