# Phase Plans 与验收标准

## Phase 0: 项目骨架与复刻边界

### Plan

- 建立 Cargo workspace。
- 创建最小 crate：
  - `lure-core`：配置、session、agent loop 等核心领域逻辑的落点。
  - `lure-cli`：命令行入口。
- 建立 `tests/` 或 crate 内 integration tests 的组织方式。
- 建立上游测试分类台账。
- 写入第一条纵向闭环的明确边界：CLI one-shot fake provider。

### 验收标准

- `Cargo.toml` workspace 可被 cargo 识别。
- `rtk cargo test --all-targets --all-features` 至少能跑通空实现或最小实现。
- `rtk cargo fmt --check` 通过。
- handbook 中记录 phase 1 到 phase 3 的上游测试入口。
- 不引入真实 provider、channel 或 gateway 行为。

### 进度记录 2026-07-23

- 状态：done
- 本次完成：
  - 初始化 Cargo workspace（`resolver = "2"`，共享 `workspace.package`）。
  - 建立最小 crate：`crates/lure-core`（核心领域库）与 `crates/lure-cli`（`lure` 二进制入口）。
  - 建立 crate 内集成测试组织方式：`crates/lure-core/tests/skeleton.rs`。
  - `lure-cli` 通过 `lure_core::version()` 建立跨 crate 依赖边界；`lure` 可运行并输出版本。
  - 公开 API 保持最小：仅暴露 `version()`，未提前泛化。
- 验证：
  - `rtk cargo fmt --check` 通过。
  - `rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（2 passed）。
  - `rtk cargo run --bin lure` 输出 `lure 0.0.0`。
- 上游对照：
  - 已确认 `tests/config/`、`tests/session/`、`tests/cli/`、`tests/agent/` 下 phase 1-3 目标测试文件存在（见 `upstream-test-ledger.md`）。
  - 本阶段不复刻任何上游行为，仅盘点入口。
- 下一步：
  - 进入 Phase 1：从上游 `nanobot/config/` 提取字段/默认值/别名策略，先补 config loader 测试。

## Phase 1: 配置、路径与工作区基础

### Plan

- 从上游 `nanobot/config/` 和配置测试提取字段、默认值、别名策略。
- 设计 Rust typed config：
  - 顶层配置结构。
  - providers/modelPresets/agents defaults 的最小字段。
  - gateway/tools/security 的占位策略仅在上游确认后加入。
- 实现 config load/save。
- 实现 workspace/config path resolution。
- 实现 onboard 最小初始化。

### 验收标准

- Rust 测试覆盖：
  - 默认 config 路径。
  - 自定义 config/workspace 路径。
  - camelCase/snake_case 读取兼容。
  - 保存时输出约定格式。
  - 无效配置错误。
- 上游 `tests/config/` 和 `test_onboard_logic.py` 相关场景已映射。
- `rtk cargo fmt --check`、`rtk cargo clippy --all-targets --all-features -- -D warnings`、`rtk cargo test --all-targets --all-features` 通过。

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（config 核心纵向切片）：
  - `lure-core::config` 子模块落地：`schema` / `paths` / `loader`。
  - typed config：`Config → AgentsConfig → AgentDefaults`，字段与默认值对齐上游
    `nanobot/config/schema.py`（model=`anthropic/claude-opus-4-5`、workspace=`~/.nanobot/workspace`、
    provider=`auto`、max_tokens=8192、temperature=0.1）。
  - 别名策略对齐上游 `config_base.Base`：序列化 camelCase；反序列化兼容 camelCase 与
    snake_case；缺失字段回落默认；未知字段忽略。
  - `load_config`：文件不存在返回默认；解析失败快速失败并带路径上下文。
  - `save_config`：camelCase + 缩进 2 + 保留非 ASCII；temp + rename 原子写；unix 保留既有权限位。
  - 路径解析：`default_config_path`、`default_workspace`、`expand_user`、`resolve_workspace`、
    `is_default_workspace`（纯解析，不创建目录）。
  - 结构化错误 `ConfigError`（Read/Parse/Serialize/Write），实现 `Display` + `Error::source`。
- 验证：
  - `rtk cargo fmt --check` 通过。
  - `rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（17 passed）。
- 上游对照：
  - 已覆盖：`tests/config/test_config_paths.py`（workspace 部分）、`test_config_load_errors.py`
    （missing→default / invalid json / 类型不匹配）、`test_config_atomic_save.py`
    （round trip / camelCase 格式 / 父目录创建 / unix 权限位保留）。
  - 暂未覆盖（记入 ledger，属尚未进入范围的上游子系统）：
    - onboard 最小初始化：上游 `cli/onboard.py` 为 ~1900 行交互式向导，耦合 providers/plugins，
      待 provider 配置落地后回补。
    - invalid schema fails fast（`tools.exec.timeout=-1`）：tools 字段 Phase 5 建模。
    - ApiConfig wildcard host 需 api_key：gateway/api 配置 Phase 7/9 建模。
    - `_migrate_config` / env 变量插值：依赖尚未建模字段，后续回补。
- 下一步：
  - 进入 Phase 2：提取上游 `nanobot/session/` 数据结构与文件语义，先补 session key / JSONL 测试。
  - 回补项在对应 phase 落地时消账。

## Phase 2: Session 与近程历史

### Plan

- 提取上游 `nanobot/session/` 的核心数据结构和文件语义。
- 先复刻 session key、JSONL line format 和历史读取。
- 再补 fsync、cache repair、turn continuation、goal state。
- 对 automation/webui turn 先做数据模型兼容，暂不接 gateway。

### 验收标准

- 覆盖上游 `tests/session/` 的全部核心场景。
- Rust 测试证明：
  - session key 安全、稳定、可逆边界明确。
  - JSONL 写入后可恢复。
  - fsync/原子写行为按当前 Rust 实现可验证。
  - legacy list repair 有测试。
  - consolidated offset clamp 有测试。
  - goal state 持久化和读取有测试。
- 未覆盖场景已写入 `upstream-test-ledger.md`。
- 全量 Rust 验证通过。

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（session 存储核心纵向切片）：
  - `lure-core::session` 落地：`keys` / `goal_state` / `model`(Session) / `store`(SessionManager)。
  - session key：`session_key_for_channel` 与 `unified:default`，对齐上游 `keys.py`。
  - 存储 key：base64url（无 padding）编码，`storage_key`/`decode_storage_key` 可逆、抗冲突、文件名安全。
  - JSONL：首行 metadata + 每行一条消息；容错读取（跳过损坏行，合并上游 `_load`+`_repair`）。
  - `Session`：`last_consolidated` clamp（越界/负数/浮点/布尔/字符串/空 → 0，不丢消息）；
    `get_history` 按条数切片（富回放逻辑留待 context builder）。
  - `SessionManager`：`get_or_create`、`save`(temp+rename 原子写，`fsync` 刷 file+目录)、
    `flush_all`、有界 LRU cache（`SESSION_CACHE_MAX_SIZE=128`）、`invalidate`。
  - `goal_state` 纯派生视图：`goal_state_raw`/`parse_goal_state`/`sustained_goal_active`/
    `goal_state_runtime_lines`/`goal_state_ws_blob`/`discard_legacy_goal_state_key`/
    `explicit_goal_requested`/`sustained_goal_turn`，兼容 legacy `thread_goal` key。
  - 结构化错误 `SessionError`（Io/NotCached）。
- 验证：
  - `rtk cargo fmt --check` 通过。
  - `rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（43 passed）。
- 上游对照：
  - 已覆盖：`test_goal_state.py`（纯函数）、`test_consolidated_offset_clamp.py`、
    `test_session_cache.py`（bounded + LRU order + 淘汰后重载）、
    `test_session_fsync.py`（durable reload + 无 tmp 残留；fsync=true 路径执行）、
    session key 可逆与 JSONL round-trip（含非 ASCII）。
  - 暂未覆盖（记入 ledger）：
    - `test_session_list_repair_legacy.py`：legacy lossy stem 修复，属迁移边界，后续回补。
    - `test_turn_continuation.py`：耦合 `bus.InboundMessage` 与 agent runner，归 Phase 3/7。
    - weak-overflow 身份保留（cache 的 `is` 身份两例）：需 `Rc`/`Weak` 语义，后续回补。
    - `runner_wall_llm_timeout_s`：runner 关注点，归 Phase 3。
    - fsync 调用计数断言：Rust std 不便 mock `os.fsync`，改以 durable reload 行为验证。
- 下一步：
  - 进入 Phase 3：定义 `AgentLoop`/`AgentRunner` 边界与 fake provider，CLI one-shot 闭环。

## Phase 3: Agent Loop 最小纵向闭环

### Plan

- 定义 `AgentLoop` 和 `AgentRunner` 的 Rust 边界。
- 建立 fake provider，使测试不依赖真实 LLM。
- 实现 CLI `agent -m "..."`：
  - 解析输入。
  - 选择 workspace/session。
  - 构建最小 context。
  - 调用 runner/provider。
  - 保存 turn。
  - stdout 输出最终回复。
- 保持 progress/event 先结构化，不急于做完整 streaming。

### 验收标准

- CLI one-shot 可运行。
- 测试覆盖：
  - 输入消息进入 agent loop。
  - fake provider 返回最终回复。
  - user/assistant turn 被保存。
  - 下一轮可读取历史。
  - provider 失败会形成结构化错误。
- 与上游 `tests/agent/test_loop_runner_integration.py`、`tests/cli/` 相关测试建立映射。
- 全量 Rust 验证通过。

## Phase 4: Provider 与模型运行时

### Plan

- 复刻 provider registry 和 model runtime resolver。
- 先支持 fake provider 与 OpenAI-compatible provider。
- 对 provider selection 顺序建立参数化测试。
- 将真实网络调用与单元测试隔离。

### 验收标准

- provider/model preset 解析顺序与上游文档和测试一致。
- OpenAI-compatible 请求和响应映射有 golden test。
- 网络错误、认证错误、响应解析错误有结构化错误。
- 不需要真实 API key 的测试全量通过。
- 真实 provider smoke test 标记为显式 opt-in。

## Phase 5: Tool Runtime 与安全边界

### Plan

- 设计 tool trait、tool schema 和 registry。
- 复刻文件工具最小集。
- 复刻 shell 工具最小集。
- 引入 workspace access policy。
- 加入工具结果截断和上下文变量传递。

### 验收标准

- 文件工具不能越过允许 workspace。
- shell 工具按配置允许或拒绝。
- tool schema 与上游等价场景有测试。
- 工具错误不会被字符串控制流程吞掉。
- 相关 `tests/tools/`、`tests/security/` 映射完成。

## Phase 6: Memory、Dream 与长期上下文

### Plan

- 实现 memory store。
- 实现 history append 和读取。
- 实现 dream consolidation 的可替换 runner。
- 接入 context builder。

### 验收标准

- memory 文件初始化、读取、写入有测试。
- dream 触发和结果写回有测试。
- context builder 注入 memory 的顺序有测试。
- 未接真实 LLM 的 consolidation 使用 fake runner 覆盖。

## Phase 7: Bus、Channels 与 Gateway

### Plan

- 建立 message bus 事件和队列。
- 把 CLI 和 WebSocket 的共同入口收敛到 InboundMessage。
- 实现 channel trait、生命周期和最小 gateway。
- 实现 health endpoint。

### 验收标准

- InboundMessage 到 AgentLoop 到 OutboundMessage 闭环可测。
- gateway 启停不会丢任务。
- channel 配置校验有测试。
- WebSocket 最小消息往返有测试。

## Phase 8: Automations、Cron 与 Trigger

### Plan

- 实现 cron job store。
- 实现 session-bound delivery。
- 实现 heartbeat protected job。
- 实现 local trigger delivery。

### 验收标准

- cron job 持久化和 next run 计算有测试。
- trigger busy session 等待语义有测试。
- at-least-once delivery 语义有测试。
- heartbeat 与普通 reminder 区分清楚。

## Phase 9: OpenAI-compatible API 与 SDK 表面

### Plan

- 实现 API server 基础。
- 实现 `/v1/chat/completions` 非 streaming。
- 实现 streaming。
- 实现最小 SDK facade。

### 验收标准

- OpenAI-compatible 请求/响应测试通过。
- streaming event 顺序可测。
- 错误响应状态码和 body 稳定。
- API runtime 与 gateway 生命周期互不混淆。

## Phase 10: WebUI 与前端集成

### Plan

- 梳理上游 WebUI API 和 WebSocket 事件。
- 先实现后端服务协议。
- 再决定是否复刻或改写前端。
- 建立前端测试映射。

### 验收标准

- WebUI bootstrap/status API 可用。
- session list 和 thread message API 可用。
- WebSocket stream 与前端期望事件兼容。
- 前端测试有等价验证或明确暂缓原因。

## Phase 11: 打包、部署与迁移兼容

### Plan

- 设计 Rust 发布产物。
- 实现 Dockerfile 和 compose。
- 实现 config/session/memory legacy migration。
- 建立 release checklist。

### 验收标准

- release build 可生成。
- Docker smoke test 可运行。
- legacy workspace fixture 可迁移。
- 完整验证清单有记录。
