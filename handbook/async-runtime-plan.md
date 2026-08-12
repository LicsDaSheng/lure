# 异步运行时对齐计划（Async Runtime Alignment Plan）

> 目标：对齐上游 nanobot 的 asyncio 运行时模型，把 lure 从同步架构迁移到 tokio 异步架构，
> 解锁被同步模型卡住的子系统（subagent 真实执行、MCP 真实连接、真实 channel 平台）。
> 本文档是跨会话工程的执行依据；每阶段按 phase-execution.md 的推进循环落地。

## 1. 上游异步模型综述（调研结论，2026-08）

### 1.1 核心：单 event loop 的 AgentLoop

上游 `nanobot/agent/loop.py`（2351 行）是异步心脏，`AgentLoop.run()`（loop.py:1161）常驻消费消息总线：

```text
run():
  await _connect_mcp()                          # MCP 生命周期挂在 loop 上
  while running:
    msg = await wait_for(bus.consume_inbound(), 1.0)
      ├─ TimeoutError → _check_expired_sessions_if_due()   # 超时即心跳
      ├─ CancelledError → shutdown 语义处理
      └─ 拿到消息：
           ├─ handle_runtime_control(...)       # 运行时控制消息
           ├─ require_existing_session 检查
           ├─ commands.is_priority → _dispatch_command_inline   # 优先级命令内联
           ├─ automation coordinator.defer_if_active(...)       # cron/trigger 与聊天 turn 协调
           ├─ session 已有活跃任务 → _pending_queues.put_nowait  # mid-turn 注入
           └─ 否则 asyncio.create_task(_dispatch(msg))           # 每消息一个 task
                _active_tasks[session_key].add(task)             # /stop 按 session 取消
```

关键调度语义（lure 必须对齐）：
1. **bus 异步消费**：channel → MessageBus → loop，`consume_inbound()` 是 async 队列消费。
2. **每消息一个 task**：并发调度，互不阻塞；/stop 通过 `_active_tasks` 按 session 取消（`_cancel_active_tasks`）。
3. **同 session 串行注入**：活跃 session 的后续消息进 `_pending_queues`（asyncio.Queue，有界），不另起竞争任务。
4. **automation 协调**：cron/trigger turn 通过 coordinator `defer_if_active` 判断是否让位于聊天 turn。
5. **MCP 生命周期**：loop 启动时连接、退出时关闭（AnyIO cancel scope 管理 stdio 传输）。
6. **process_direct**（loop.py:2279）：同步语义的异步入口，绕过 bus 直接跑一轮 turn（SDK/CLI 用）。

### 1.2 全 IO 面都是 async

| 域 | 规模 | 形态 |
|---|---|---|
| channels/ | 1558 async def / 2345 await | BaseChannel（base.py:341 行）契约：`send_delta`/`send_reasoning`/`login` 全 async；16 个平台子包 |
| agent/ | 220 async def | loop/runner/subagent/skills 全异步 |
| providers/ | 102 async def | 异步客户端（openai/aiohttp 系） |
| webui/ | 69 async def | aiohttp server |
| command/ | 20 async def | 内置命令全 async（cmd_stop 可取消任务） |
| cron/triggers | 9 async | `submit_cron_turn`/`submit_local_trigger_turn` 投进 loop |
| session/ | 13 async | turn continuation 等 |

### 1.3 进程模型

gateway 是**独立子进程**（`process_runtime.py` ManagedProcessRuntime + `gateway/runtime.py` GatewayRuntime，multiprocessing）。
channel 平台同样可独立进程托管（ChannelManager，39.4KB）。

### 1.4 WebUI 后端即 websocket channel

上游 WebUI 的后端**本身就是 channel**：`channels/websocket/runtime.py`（67.8KB / 1779 行）的
`WebSocketChannel(BaseChannel)`，docstring 原话："Run a local WebSocket server; forward
text/JSON messages to the message bus."。

- 消息流：浏览器 WS → `WebSocketChannel` → `MessageBus`(InboundMessage) → `AgentLoop` 单实例消费调度。
- WebUI 的 HTTP 路由（`/webui/bootstrap`、sessions、settings、skills、media、mcp presets、
  cli_apps、forking、channel 配置热加载）全部由该 channel 借助 `GatewayServices` 提供。
- 订阅簿记：`_subs`（chat_id → connections fan-out）/`_conn_chats`/`_webui_connections`
  （bootstrap token 认证）——与 lure 现有 `WsHub` 同构。
- 契约测试规模：websocket 相关测试 9000+ 行（test_websocket_channel.py 5074 行 +
  test_websocket_http_routes.py 3598 行），即上游 WebUI 全部契约。

结论：上游「多入口共享一个 agent」的形态是**所有入口（浏览器/Telegram/Discord）都是
BaseChannel 子类，统一进 bus，单实例 AgentLoop 常驻调度**。

## 2. lure 现状（同步架构）

| 域 | 现状 | 与上游差距 |
|---|---|---|
| AgentLoop | `process()` 一次性同步调用（loop_run.rs），无常驻循环 | 无 run() 循环、无任务表/取消、无 mid-turn 注入、无 automation 协调 |
| bus | 同步 FIFO 队列 | 无 async 消费 |
| provider | `LlmProvider` 同步 trait，UreqTransport（阻塞） | 无异步客户端 |
| webui | tiny_http + 每连接一线程（WS），**直连模式**：每连接新建独立 AgentLoop 实例，不走 bus；cron 另起独立实例；`WsHub` 订阅簿记与上游 `_subs` 同构但未 channel 化 | 无异步 server；WebUI 未纳入 channel/bus 体系 |
| cron | 轮询线程（30s interval） | 无 submit_cron_turn 语义 |
| gateway | 最小同步编排 | 无 async 调度、无真实 channel |
| subagent/MCP | 状态簿记 / schema 纯变换 | 被同步模型卡住 |

## 3. 迁移设计决策

1. **运行时**：tokio（multi-thread）。tauri v2 已依赖 tokio，desktop 外壳天然兼容；单 loop 语义用 task 调度等价实现。
2. **Provider trait 直接异步化**：async-trait（对象安全无坑，现有 `Box<dyn LlmProvider>` 模式保持），transport 换 reqwest。
   不做双 trait——调用点只有 agent loop/CLI/desktop，可控。
3. **双轨过渡**：过渡期同步入口（CLI 部分路径、旧测试）保留；核心 turn 执行异步化后，同步 `process()` 改为 runtime 内 `block_on` 薄层，最终态删除。
   现有 573 个测试在双轨期保持通过，不重写；新增契约测试用 `#[tokio::test]`。
4. **进程模型取舍**：desktop 形态内**单进程多 task**，不复刻上游 multiprocessing（进程隔离在桌面场景无必要，且与内嵌 loopback 架构冲突）。
   gateway 作为 async 对象运行在同一 runtime 内；`process_runtime.py` 的子进程托管**明确不建模**，台账记录范围决定。
5. **webui 契约不变**：axum（hyper）替换 tiny_http，`/api/*`、`/webui/bootstrap`、WS 复用协议保持字面对齐；E2E（Playwright）是守护网。
6. **范围决定（用户确认，2026-08）**：channel 体系只做 **websocket 渠道**（WebUI channel 化）；
   外部平台 channel（telegram/discord/slack 等）与 `lure_core::sdk`（SDK facade）**明确不实现**。

## 4. 分阶段执行计划

### Stage 0：tokio 基础设施
- **契约**：lure-core 可编译运行于 tokio runtime；desktop `--headless` 行为不变（E2E 绿）。
- **改动**：
  - lure-core Cargo.toml：`tokio`（rt-multi-thread/sync/time/net/macros/signal）、`async-trait`、`futures`、`reqwest`（json/stream）。
  - lure-desktop main 入口切 `#[tokio::main]`（Tauri setup 前建 runtime；headless park 改为 runtime 内 park）。
- **测试**：现有全量测试保持绿；新增 1 个 runtime smoke（tokio runtime 内跑现有 AgentLoop 同步 process）。

### Stage 1：异步 bus + AgentLoop.run 调度核心（本阶段是契约重点，测试先行）
- **契约**（对齐 loop.py 调度语义，先写 `#[tokio::test]`）：
  1. 消息经 async bus 消费，顺序 FIFO。
  2. 每消息 spawn 独立 dispatch task；同 session 并发消息串行处理（活跃任务存在时入 pending 队列）。
  3. `/stop`（或 cancel 信号）按 session 取消活跃 task；取消后 pending 队列语义正确。
  4. 消费超时（1s）执行心跳钩子。
  5. automation（cron/trigger）defer 协调：活跃聊天 turn 时 cron turn 让位。
- **改动**：
  - `bus`：新增 async 消费路径（tokio mpsc），保持现有同步 API。
  - `agent`：新增 `AgentLoop::run()`（async）+ `_dispatch` 等私有调度；`_active_tasks`/`_pending_queues` 按 session 索引。
  - 新文件 `agent/async_loop.rs`（或 loop_run.rs 内新增模块），同步 `process()` 不动。
- **验证**：新契约测试全绿 + 既有测试全绿。

### Stage 2：turn 执行异步化 + Provider async
- **契约**：`LlmProvider` 全 async（async-trait）；EchoProvider/OpenAI transport 异步；turn 构建/执行/持久化（_build_turn/_run_turn/_persist_turn 对应物）异步化；`AgentLoop::process_direct`（async 一次性入口）可用；旧同步 `process()` 变 block_on 薄层。
- **改动**：
  - `provider`：trait async 化（`async fn complete/stream` 等）、`UreqTransport` → `ReqwestTransport`（错误分类/超时语义以现有 transport 测试守护）、usage 归一不变。
  - `agent`：runner/loop 核心改 async；ContextBuilder 若含同步 IO 保持（同步 IO 可留在 task 内，非阻塞耗时点才需 async）。
  - `cli`：main 迁 async（one-shot 与 REPL await process_direct）；交互 UX（流式渲染/spinner）保持。
- **验证**：全量测试（同步 process 薄层仍绿）+ 新增异步 provider 测试（echo 往返、SSE 流式、错误分类）。

### Stage 3：webui server axum 化
- **契约**：`/api/*`、`/webui/bootstrap`、`/ws`（含 mux/transcript/实时推送）响应形状与方法语义不变；每连接独立 agent 实例语义保留（factory 闭包不变）；E2E 全绿。
- **改动**：
  - `webui`：`WebuiServer`/`WsServer` 改 axum（Router + `axum::extract::ws`）；静态资源/SSE/bootstrap 路由平移。
  - 每连接线程 → `tokio::spawn` 连接 task；WsHub 推送语义保持。
  - `api`（Phase 9 ChatServer，/v1/chat/completions）一并平移或保留 tiny_http 至最终删除。
- **验证**：Rust 集成测试（HTTP/WS 契约）+ E2E（Playwright 7 用例）全绿。

### Stage 4：cron/trigger 异步化 + automation 协调
- **契约**：`CronScheduler` 为 loop 内 tokio interval task；cron turn 经 submit 语义投递（与聊天 turn 共享调度核心）；defer 协调生效；触发语义 at-least-once 保持。
- **改动**：cron/service.rs 线程轮询 → interval task；trigger 队列异步化；desktop 装配改 runtime 内 task。
- **验证**：cron 相关既有测试（含实时推送端到端）+ 新增 defer 契约测试。

### Stage 5：channel 异步化 + WebUI/websocket channel 化（唯一渠道）
- **契约**：
  - `Channel` trait async（send/progress/deliver）；gateway async 编排。
  - **WebUI channel 化**（对齐 1.4 上游形态）：现有 webui 直连模式重构为
    `WebSocketChannel` 等价物——浏览器 WS 消息经 `MessageBus` 进单实例 AgentLoop；
    `WsHub` 订阅簿记平移为 channel 内订阅表；`/webui/bootstrap` 与 `/api/*` 路由
    语义保持字面不变（E2E 守护）；每连接独立 AgentLoop 的隔离语义由「单实例 +
    按 session 调度」替代（隔离性不变、资源复用）。
- **范围决定**：websocket 是**唯一**真实 channel 渠道——外部平台 channel
  （telegram/discord/slack 等上游 60K 行）**明确不实现**（见 roadmap 范围决定）。
- **改动**：channel trait async-trait；webui 模块拆分出 `channel/websocket.rs`
  （复用现有 WsHub/transcript/token/静态资源逻辑）；Gateway 改 async 编排；
  desktop 装配从「每连接 factory + cron 独立实例」改为
  「单实例 AgentLoop.run() + websocket channel 注册进 bus」。
- **依赖**：本阶段以 Stage 1 的 bus 消费 + 单实例调度为前提——channel 化的落点
  就是异步 AgentLoop 的 run() 循环；若 Stage 1/3 已先行，此处仅剩 trait 化与接线。
- **验证**：channel 契约测试（mock 传输）+ gateway async 编排测试 + 既有 E2E 7 用例全绿
  （WebUI 契约不变的守护网）。

### Stage 6：subagent 后台执行 + MCP 异步客户端
- **契约**：subagent spawn 起后台 agent turn（async task）、announce 经 bus 回灌、exec 级联终止（对齐 test_subagent.py 语义）；MCP stdio/HTTP/SSE 客户端 + enabled-tools 过滤 + 重连/重试（对齐 test_mcp_*）。
- **改动**：agent/subagent.rs 簿记核心接入执行；新 `mcp` 客户端模块。
- **验证**：上游 test_subagent/test_mcp_* 等价 Rust 测试；台账 deferred → covered。

### Stage 7：收尾（SDK facade 明确不做）
- **范围决定**：`lure_core::sdk`（Nanobot 等价物 from_config/run/run_streamed）**不实现**——
  lure 的消费面是 CLI + desktop WebUI 两条自有入口，程序化嵌入无真实需求；上游 `sdk/`（679 行）不入范围。
- **收尾**：同步死代码删除（同步 `process()` 薄层、tiny_http 残留、线程模型遗留）；旧 573 测试适配/重写收尾；README/roadmap 更新；台账最终核销。

## 5. 验证与门禁

- 每 stage：新增契约测试先行 → `cargo fmt` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test --all-targets --all-features`。
- 涉及 webui/desktop/前端契约的 stage（3、4）必须跑 `make e2e`。
- `upstream-test-ledger.md`：deferred 项（MCP 连接、subagent 执行）随 stage 6 转 covered；每 stage 记录映射。
- 最终态：`phase-roadmap.md` 跨阶段表中「异步运行时」相关缺口勾销。

## 6. 风险与决策记录

| 风险 | 缓解 |
|---|---|
| 双轨期复杂度膨胀 | 每 stage 只动一条链；同步 API 保持到该链迁移完成即删 |
| reqwest 与 ureq 行为差异（超时/错误分类） | transport 语义测试守护；现有错误分类测试先于实现迁移 |
| async-trait 性能/对象安全 | 使用 async-trait（成熟方案）；trait 对象模式保持 |
| E2E 脆弱（webServer 启动方式） | `--headless` 行为与输出格式（LURE_HTTP_URL）保持不动 |
| 上游进程模型未复刻 | 明确范围决定（desktop 单进程多 task），台账记录 |
