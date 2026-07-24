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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（CLI one-shot 纵向闭环）：
  - `lure-core::provider`：同步 `LlmProvider` trait + 契约类型（`CompletionRequest`/`LlmResponse`/
    `GenerationSettings`，默认对齐上游 `base.py` 0.7/4096）+ 结构化 `ProviderError` + `EchoProvider` 占位。
  - `lure-core::agent`：`ContextBuilder`（system + 历史 `{role,content}` 投影）、
    `AgentRunner`（单次补全）、`AgentLoop`（追加 user turn → build context → runner → 追加
    assistant turn → save → 返回 `TurnOutcome` + 结构化 `ProgressEvent`）、结构化 `AgentError`。
  - CLI `lure agent -m "..." [--workspace P]`：解析输入、选 workspace/session、跑闭环、stdout 输出回复。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（51 passed）。
  - 手动 smoke：`lure agent -m "你好，世界" --workspace <tmp>` 输出 `echo: 你好，世界`，
    JSONL 持久化 metadata + user + assistant turn，存储 key 为 `cli:direct` 的 base64url。
- 上游对照：
  - 已覆盖（`test_loop_runner_integration.py` 最小闭环 + `tests/cli/` one-shot）：
    输入进入 loop、fake provider 返回最终回复、user/assistant turn 保存、下一轮读历史、
    provider 失败形成结构化错误、CLI one-shot 可运行。
  - 暂未覆盖（记入 ledger）：
    - async/streaming provider（上游为 async + StreamedResponseEvent）：改用同步 trait，随 bus/API 落地再引入。
    - tool 执行循环、goal/subagent、consolidation：属 Phase 5/6。
    - 真实 provider（`get_default_model`/`chat_with_retry` 真实实现）：属 Phase 4，现以 `EchoProvider` 占位。
    - 完整 `InboundMessage`（sender/metadata/附件）与 bus：属 Phase 7。
    - 上游 prompt_toolkit 交互输入、stream/progress 渲染、slash commands：基础 REPL 已覆盖，高级交互待后续。
- 下一步：
  - 进入 Phase 4：provider registry、model runtime resolver、OpenAI-compatible 最小真实调用与 golden test。

### 进度记录 2026-07-23（CLI interactive 回补）

- 状态：partial
- 本次完成：
  - `lure agent` 无 `-m/--message` 时进入基础 interactive REPL。
  - 支持持续复用同一 session 多轮对话，空行忽略，EOF 正常退出。
  - 支持上游退出命令集合：`exit`、`quit`、`/exit`、`/quit`、`:q`。
  - `agent` 新增 `--session/-s`，无冒号时按上游规则映射为 `cli:<id>`；`--workspace` 补充 `-w` 别名。
  - one-shot 与 interactive 共用同一 `AgentLoop`/session/provider 构造路径。
- 验证：
  - `rtk cargo test -p lure-cli --test cli_one_shot -- --nocapture` 通过（7 passed）。
- 上游对照：
  - 已覆盖：`tests/cli/` 中 agent direct 的 one-shot、基础 interactive 输入/退出/session 复用语义。
  - 暂未覆盖：prompt_toolkit 历史/快捷键/粘贴体验、stream/progress 渲染、bus async 消费、slash commands。
- 下一步：
  - 收敛 Phase 4 stateful `ModelRuntimeResolver`，再逐步接上 streaming/progress 与真实 runtime。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（preset 解析 + OpenAI-compatible provider + 最小 registry）：
  - 扩展 config：`ModelPresetConfig`、`Config.model_presets`（camelCase `modelPresets` + snake 别名）、
    `AgentDefaults` 新增 `model_preset`/`context_window_tokens`/`reasoning_effort`。
  - `Config::resolve_default_preset`/`resolve_preset(name)`/`validate`：解析顺序对齐上游
    （None→defaults.model_preset；空/`default`→隐式默认；命名查表；缺失/保留名/未知 preset 报错）。
    `load_config` 在解析后调用 `validate`，非法 preset 走 `ConfigError::Validation`。
  - provider：`HttpTransport` 抽象 + `HttpRequest`/`HttpResponse`；`OpenAiCompatProvider`
    构建 `{model,messages,temperature,max_tokens}` 请求、POST `{base}/chat/completions`、
    解析 `choices[0].message.content`/`finish_reason`/`usage`；错误按状态分类为
    `Auth/RateLimited/Server/Api/Transport/Response`。
  - `provider::registry`：代表性 provider 子集 + `find_by_name`/`match_provider`
    （forced 按名；auto 前缀优先、再关键字匹配）。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（77 passed）。全部不触网。
- 上游对照：
  - 已覆盖：`test_model_presets.py`（config 层 preset 解析/校验/序列化）、OpenAI-compatible
    请求/响应 golden + 错误分类、provider 选择顺序参数化。
  - 暂未覆盖（记入 ledger）：
    - `test_model_runtime_resolver.py`：stateful resolver 生命周期（refresh/admit/invalidate/preset
      tracking、LLMRuntime/ProviderSnapshot 不可变捕获），本次只覆盖 config 层解析顺序。
    - config 驱动的 `_match_provider`/`get_provider_name`：依赖尚未建模的 `ProvidersConfig`
      （api_key/OAuth/local fallback/transcription 过滤）。
    - 真实 HTTP 传输与真实 provider smoke（opt-in）：传输 trait 已就位，真实实现待接入。
    - `max_completion_tokens`/模型专属覆盖、streaming、tool call、fallback provider。
- 下一步：
  - 进入 Phase 5：tool trait/schema/registry、文件与 shell 工具最小集、workspace 安全策略。

### 进度记录 2026-07-24（stateful ModelRuntimeResolver 回补）

- 状态：partial
- 本次完成：
  - `provider::runtime`：新增不可变 `ProviderSnapshot`（provider 名/api_base/model）与
    `LlmRuntime`（preset 名 + snapshot + `GenerationSettings` + `generation` 代次）。
  - `ModelRuntimeResolver`：
    - `admit(name)`：解析 canonical preset → 匹配 provider（复用 `registry::match_provider`）→
      构建不可变 runtime，按 preset 名缓存并选中；命中缓存复用同一快照（generation 不变）。
    - `refresh(name)`：强制重建并选中，`generation` 递增。
    - `invalidate(preset)`：丢弃缓存；若正是 active 则清空 active。
    - `active()`：当前 admitted runtime。
    - preset 归一沿用 config 规则（None→defaults.model_preset；空/`default`→隐式默认；命名查表）。
  - 结构化 `RuntimeError`：`Preset(PresetError)` 与 `ProviderNotFound { model, provider }`。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - `rtk cargo test -p lure-core --test model_runtime_resolver` 通过（10 passed，均不触网）；
    全量 `cargo test --all-targets --all-features` 通过。
- 上游对照：
  - 已覆盖：`test_model_runtime_resolver.py` 记录的 admit/refresh/invalidate + preset tracking +
    不可变 `LlmRuntime`/`ProviderSnapshot` 捕获语义（上游源码未 vendored，按 ledger 建立事实来源）。
  - 暂未覆盖：真实 runtime 探活/降级、config 驱动 `_match_provider`（api_key/OAuth/local fallback）。
- 下一步：
  - 将 resolver 接入 AgentLoop/CLI 的 provider 选择路径（已在下一条记录完成），或补 config 驱动的 `ProvidersConfig` 匹配。

### 进度记录 2026-07-24（resolver 接入 AgentLoop/CLI provider 选择路径）

- 状态：partial
- 本次完成：
  - `AgentLoop::with_runtime(&LlmRuntime)`：builder 式覆盖 loop 的 `model` 与生成参数
    （model 取 `provider.model`，settings 取 runtime 捕获的 `settings`），非破坏性、附加。
  - CLI provider 选择统一经 resolver：
    - `build_agent_loop`：无 `--model` → 离线 EchoProvider；有 `--model` → `resolve_runtime` → 真实 provider。
    - `resolve_runtime`：用 `--model` 覆盖默认 preset 的 model，`ModelRuntimeResolver::admit(None)` 解析出 runtime。
    - `build_provider_from_runtime`：由 runtime 的 `ProviderSnapshot` 读 `<PROVIDER>_API_KEY` 构造 `OpenAiCompatProvider`。
    - 删除旧 `build_provider`（直接 `match_provider`）；provider 身份/api_base/model/settings 均由 runtime 决定。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - lure-core `agent_loop`（新增 `with_runtime` 驱动 model/settings 用例，capturing provider 断言）7 passed；
    CLI `cli_one_shot`（新增 resolver 选择/缺 key/不可匹配 provider 用例）9 passed；全量测试通过。
- 上游对照：
  - 已覆盖：CLI/loop 的 provider 选择由 resolver 驱动，settings 从 config 默认 preset（temp 0.1 / max 8192）取值。
  - 暂未覆盖：`--preset <name>` 命名 preset 入口、config 文件加载后的 preset 集合（已在下一条记录完成）、真实 runtime 探活/降级。
- 下一步：
  - 补 config 文件加载 + `--preset` 命名入口（已完成），或推进 config 驱动的 `ProvidersConfig`（api_key/OAuth/local fallback）。

### 进度记录 2026-07-24（--preset 与 config 文件加载）

- 状态：partial
- 本次完成：
  - CLI 新增 `--config/-c <path>` 与 `--preset/-p <name>`：
    - `load_cli_config`：加载 `--config`（缺省 `default_config_path` = `~/.nanobot/config.json`）；文件不存在回落默认配置。
    - `resolve_runtime(config, preset, model)`：`--preset` 选中命名 preset（`admit(Some(name))`）；
      `--model` 覆盖默认 preset 的 model 并强制 `provider=auto`（`admit(None)`）；二者互斥，同时给出报错。
    - `build_agent_loop`：无 `--preset`/`--model` 仍走离线 EchoProvider；否则加载 config → resolver → 真实 provider。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - CLI `cli_one_shot` 新增用例（config 文件命名 preset 选中并报缺 key、未知 preset NotFound、`--preset`/`--model` 互斥）
    12 passed；全量测试通过（35 套件）。
- 上游对照：
  - 已覆盖：config 文件加载 + 命名 preset 选中入口，命名 preset 经 resolver 解析为不可变 runtime。
  - 暂未覆盖：`--model` 与 `--preset` 组合覆盖（当前互斥）、config 驱动的 `ProvidersConfig`
    （api_key/OAuth/local fallback）、真实 runtime 探活/降级。
- 下一步：
  - 推进 config 驱动的 `ProvidersConfig`（`_match_provider` 的 api_key/OAuth/local fallback），或进入 Phase 5 tool 运行时。

### 进度记录 2026-07-24（config 驱动 ProvidersConfig）

- 状态：partial
- 本次完成：
  - schema 新增 `ProviderConfig`（`apiKey`/`apiBase`/`enabled`，camelCase + snake 别名，`enabled` 默认 true）
    与 `Config.providers: BTreeMap<String, ProviderConfig>`；导出 `ProviderConfig`/`ResolvedProvider`。
  - `Config::resolve_provider(model, forced)`（config 驱动切片，复用 `registry::find_by_name`/`PROVIDERS`）：
    - forced/前缀为显式意图，禁用不影响；auto 关键字匹配时跳过 config 中被禁用的 provider。
    - 生效 `api_base` 优先 `providers.<name>.apiBase`，否则 registry 默认。
    - `provider_api_key(name)` 暴露 config 显式 key。
  - resolver：`build` 改用 `config.resolve_provider`，`ProviderSnapshot` 的 provider 名/api_base 走 config 覆盖。
  - CLI：`build_provider_from_runtime(config, runtime)` 的 api_key 解析改为 config 优先、env 回落。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - 新增 `config_providers.rs`（8 passed：api_base 覆盖/回落、auto 跳过禁用、forced 显式、api_key、serde 默认）；
    resolver 新增 2 用例（api_base 覆盖、禁用 provider → ProviderNotFound）；CLI 新增 config apiKey+apiBase 用例；
    全量测试通过（36 套件）。
- 上游对照：
  - 已覆盖：`_match_provider` 的 config 驱动核心切片（api_base 覆盖、enabled 过滤、api_key 解析）。
  - 暂未覆盖：provider 的 OAuth 凭据、local fallback、transcription-only 过滤；`--model`/`--preset` 组合覆盖。
- 下一步：
  - 进入 Phase 5 tool 运行时盘点，或继续补 provider 的 OAuth/local fallback。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（工具运行时 + 安全边界）：
  - `lure-core::security::workspace`：`resolve_path`（软解析，跟随已存在部分符号链接）、
    `is_path_within`（分量比较，避免前缀误判）、`resolve_allowed_path`（allowed root/extra
    roots/exact files）、`WorkspaceBoundaryError`。
  - `lure-core::tool`：`Tool` trait（name/description/parameters/execute，同步）、`ToolResult`、
    `truncate_result`、`ToolRegistry`（register/get_definitions[OpenAI function 格式]/execute +
    近似建议）、`validate_value`（JSON Schema 子集：type/required/enum/min-max/长度/递归）、
    结构化 `ToolError`（UnknownTool/InvalidArgs）。
  - `tool::shell`：`ExecPolicy.guard_command`（allow 优先、deny 在原始命令、allowlist-only；
    顶层分段切分保留 `2>&1` 等 fd 重定向）+ `ExecTool`（门禁后在 workspace 内执行）。
  - `tool::file`：`ReadFileTool`/`WriteFileTool` 强制路径落在 workspace 内，越界返回错误结果。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（107 passed）。
- 上游对照：
  - 已覆盖：`test_workspace_policy.py`（相对路径/穿越/前缀兄弟/符号链接逃逸/额外 root/精确文件）、
    `test_exec_allow_patterns.py`（allow/deny/allowlist/分段/fd 重定向）、`test_tool_registry.py`
    （定义/派发/近似建议/参数校验）、结果截断、文件工具 workspace 越界拒绝。
  - 暂未覆盖（记入 ledger）：
    - apply_patch/search/web/mcp/image/message 等工具（体量大，后续按需）。
    - exec 平台/env/session 隔离/reap（`test_exec_platform`/`test_exec_env`/`test_exec_session_*`）。
    - extra-file 符号链接逃逸精确拦截（跨平台稳健先用软解析对比，escape 边界待补）。
    - async 执行、tool 上下文变量注入完整链路、network SSRF（`security/network`）。
- 下一步：
  - 进入 Phase 6：memory store、history、dream consolidation（fake runner）、context 注入。

### 进度记录 2026-07-24（tool-call 循环接入 agent loop）

- 状态：partial
- 本次完成（把已就绪的 tool primitives 串成可用链路）：
  - provider：`LlmResponse` 新增 `tool_calls: Vec<ToolCall>`；`parse_chat_response` 解析
    `message.tool_calls`（id/function.name/function.arguments）。
  - `ContextBuilder::project_message`：透传 `tool_calls`（assistant）与 `tool_call_id`（tool），
    使多轮 tool 上下文能正确回放给 provider。
  - `AgentLoop`：新增 `with_tools(registry)` 与 `MAX_TOOL_ITERATIONS`；`process` 变为 tool-call 循环：
    无 tool_calls（或未挂 registry）为终态；否则持久化带 `tool_calls` 的 assistant turn、逐个执行工具、
    把结果作为 `tool` turn 回灌历史，至多 8 轮。参数非法/未知工具/工具错误统一转文本回灌供模型自纠。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - 新增 `agent_tool_loop.rs`（4：单轮执行+历史顺序+上下文回放、迭代上限、未知工具恢复、无 registry 终态）；
    provider 新增 2 解析用例；全量测试通过（37 套件）。
- 上游对照：
  - 已覆盖：provider tool_calls 解析 + agent tool-call 迭代闭环（fake provider + 内存 echo tool，不触网）。
  - 暂未覆盖：CLI 侧工具注册（file/shell 工具 + 来自 config 的 exec 策略绑定）、并行 tool、streaming、
    tool 上下文变量注入完整链路。
- 下一步：
  - CLI `build_agent_loop` 注册 workspace 绑定的 file/shell 工具（exec 策略来源待定），或进入 Phase 6。

### 进度记录 2026-07-24（CLI 工具注册）

- 状态：partial
- 本次完成：
  - schema 新增 `ToolsConfig`/`ExecToolConfig`（`tools.exec.enabled`/`allow`/`deny`，camelCase + 默认，exec 默认关闭）
    与 `Config.tools`；导出 `ToolsConfig`/`ExecToolConfig`。
  - `tool::registry_from_config(config, workspace)`（+ `ToolSetupError`）：默认注册 workspace 绑定的
    `read_file`/`write_file`；`tools.exec.enabled` 时按 `allow`/`deny` 正则构造 `ExecPolicy` 注册 `exec`，
    正则非法即结构化报错。CLI/WebUI 共用同一注册策略。
  - CLI `build_agent_loop`：真实 provider 分支先 `registry_from_config`（config 错误 fail-fast，先于出网/读 key），
    再 `.with_tools(tools)`。离线 EchoProvider 分支不挂工具（echo 不触发 tool_calls）。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - 新增 `tool_setup.rs`（4：默认文件工具/exec 开关/非法正则报错/serde 默认）；CLI 新增非法 exec 正则用例；
    全量测试通过（38 套件）。
- 上游对照：
  - 已覆盖：config 驱动的工具注册 + CLI 接线，tool-call 循环现可在真实 CLI 运行（文件工具默认可用，exec opt-in）。
  - 暂未覆盖：exec env/session 隔离、file edit/search、并行 tool、streaming、tool 上下文变量注入完整链路。
- 下一步：
  - 进入 Phase 6（memory/dream 缺口盘点），或补文件工具 edit/search 与 exec 平台细节。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（长期记忆存储 + dream 整合 + context 注入）：
  - `lure-core::memory::MemoryStore`：`MEMORY.md`/`SOUL.md`/`USER.md` 读写、`get_memory_context`
    （`## Long-term Memory` 块）、`history.jsonl` append（自增 cursor + `strip_think` + 硬上限）、
    `read_unprocessed_history`（cursor 过滤）、`read_recent_history_for_prompt`（session 过滤）、
    `.cursor`/`.dream_cursor` 持久化；损坏 cursor/字段的记录丢弃。
  - `memory::strip_think`：移除完整 think 块、开头未闭合前缀、`<channel|>` 标记、畸形开标签。
  - `memory::DreamRunner` + `MemoryStore::consolidate`：整合 dream cursor 之后的历史，写回
    MEMORY.md 并推进 cursor；无新历史返回 None。fake runner 覆盖，不接真实 LLM。
  - `agent::ContextBuilder`：新增 `with_memory`，注入顺序 system → memory → 历史。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（126 passed）。
- 上游对照：
  - 已覆盖：`test_memory_store.py`（memory/soul/user 读写、history cursor、strip、session 过滤、
    reopen 持久化）、dream 触发/写回/幂等/cursor 推进（fake runner）、context 注入顺序。
  - 暂未覆盖（记入 ledger）：
    - GitStore 版本化。
    - 真实 LLM dream、SOUL/USER 的整合、迭代/批次与压缩策略、autocompact（`test_dream.py` 等）。
    - unified session 内部会话过滤、`compact_history` 完整策略、并发 append 锁。
- 下一步：
  - 进入 Phase 7：message bus、InboundMessage/OutboundMessage、channel trait 与最小 gateway。

### 进度记录 2026-07-24（memory 接入 agent loop）

- 状态：partial
- 本次完成（把已就绪的 memory primitives 串进闭环）：
  - `AgentLoop::with_memory(MemoryStore)`：每轮把 `get_memory_context()` 注入 context（system→memory→历史，
    本轮内稳定），user 内容与最终 assistant 内容追加到 `history.jsonl`（按 session 归属，供 dream）。
  - `AgentLoop::consolidate(&runner)`：委托 `MemoryStore::consolidate`，未挂 memory 或无未处理历史返回 None。
  - `AgentError` 新增 `Memory(io::Error)` 变体；history 追加失败形成结构化错误。
  - 未挂 memory 时行为不变（无 history.jsonl、consolidate 返回 None），向后兼容。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - 新增 `agent_memory.rs`（4：记忆块注入、history.jsonl 追加、consolidate 更新 MEMORY.md、无 memory 终态）；
    全量测试通过（39 套件）。
- 上游对照：
  - 已覆盖：memory 注入 + history 记录 + dream 触发接口（fake runner），agent loop 现具长期记忆闭环。
  - 暂未覆盖：CLI 侧 memory 注册、dream 自动触发策略（阈值/定时）、真实 LLM dream、SOUL/USER 整合。
- 下一步：
  - CLI `build_agent_loop` 注册 `MemoryStore` 并定 dream 触发策略，或进入 Phase 7。

### 进度记录 2026-07-24（CLI memory 注册）

- 状态：partial
- 本次完成：
  - CLI `build_agent_loop`：两个分支（离线 EchoProvider + 真实 provider）都 `MemoryStore::new(workspace)`
    并 `.with_memory(memory)`，长期记忆作为核心能力常驻——注入记忆块 + 记录 `history.jsonl`。
  - memory 初始化失败形成 CLI 错误字符串（fail-fast）。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - CLI 新增用例：`agent -m hello`（离线 echo，可无网络跑完 turn）后 `history.jsonl` 记录 user/assistant；
    全量测试通过（39 套件）。
- 上游对照：
  - 已覆盖：CLI 挂载 memory，真实 CLI 现具长期记忆闭环（跨轮记忆注入 + history 记录）。
  - 暂未覆盖：dream 自动触发策略（需真实 LLM DreamRunner）、SOUL/USER 整合、memory 开关配置。
- 下一步：
  - 进入 Phase 7（bus/channel/gateway 盘点），或补真实 LLM dream + 触发策略。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（消息总线 + channel 契约 + 最小 gateway，同步内存实现）：
  - `lure-core::bus`：`InboundMessage`（channel/sender/chat/content/metadata/session_key_override
    + `session_key()`）、`OutboundMessage`（含 `reply` 构造）、同步内存 `MessageBus`（publish/consume/size）。
  - 收敛入口：`AgentLoop::process` 改为接受 `bus::InboundMessage`，移除 Phase 3 的占位
    `AgentInput`；CLI 与相关测试同步迁移。
  - `lure-core::channel`：`Channel` trait（name/validate/deliver）、结构化 `ChannelError`
    （MissingConfig/Delivery）、测试用 `RecordingChannel`（共享 `DeliveryLog`）。
  - `lure-core::gateway`：`Gateway`（注册 channel[先校验]、start/stop、submit、
    dispatch_pending[drain inbound → agent → outbound → 路由 channel]、health），结构化
    `GatewayError`（Agent/Channel/UnknownChannel）。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（135 passed）。
  - 手动 smoke：CLI one-shot 经收敛后的 InboundMessage 仍正常闭环。
- 上游对照：
  - 已覆盖：`bus` 消息契约与队列语义、InboundMessage→AgentLoop→OutboundMessage→channel 闭环、
    channel 配置校验、gateway 启停不丢任务、未知 channel 路由错误、health 状态。
  - 暂未覆盖（记入 ledger）：
    - WebSocket 最小消息往返：需真实 WebSocket，随 Phase 10 WebUI 落地。
    - 真实 HTTP health endpoint、进程管理 runtime、async 调度。
    - channel 热加载、delta coalescing、pairing、各具体平台 channel（telegram/discord/…）。
- 下一步：
  - 进入 Phase 8：cron job store、session-bound delivery、heartbeat、local trigger。

### 进度记录 2026-07-24（progress/outbound 事件传播）

- 状态：partial
- 本次完成：
  - `agent::ProgressEvent` 新增 `ToolInvoked { name }`，tool-call 循环每次执行工具时发出。
  - `bus::ProgressUpdate`（channel/chat_id/kind/content）+ `ProgressKind`（Started/ToolInvoked/Final），
    传输无关的 outbound 运行时事件。
  - `Channel` trait 新增 `deliver_progress`（默认 no-op）；`RecordingChannel` 记录 progress（共享 `ProgressLog`）。
  - `Gateway::dispatch_pending`：处理后把 `outcome.progress` 逐条映射为 `ProgressUpdate` 转发给目标 channel，
    再投递最终 outbound；未注册 channel 的 progress 也走结构化 `UnknownChannel`。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - 新增 `gateway_progress.rs`（2：echo Started/Final 转发 + 路由信息、tool 轮 ToolInvoked 转发）；
    `agent_tool_loop.rs` 新增 ToolInvoked 发出用例；全量测试通过（40 套件）。
- 上游对照：
  - 已覆盖：agent progress 事件流经 gateway 转发到 channel（Started/ToolInvoked/Final），Phase 10 WebUI 进度流可复用。
  - 暂未覆盖：async 订阅/真实流式传输、更细粒度 progress（token 级 streaming）、channel 侧 typing 指示等平台语义。
- 下一步：
  - 进入 Phase 8/9，或补真实流式传输与更细粒度 progress。

### 进度记录 2026-07-24（真实流式传输 + 细粒度 progress）

- 状态：partial
- 本次完成：
  - provider SSE 流式消费：`StreamChunk`/`ToolCallDelta` + `parse_sse_line` + `StreamAssembler`
    （内容/推理拼接、tool_calls 按 index 累积参数）；`LlmProvider::complete_streaming`（默认回退单块回调，
    所有 provider 可被流式路径统一驱动），`OpenAiCompatProvider` 覆盖为真 SSE（`stream:true` + 逐行解析 + 状态分类）。
  - transport：`HttpTransport::post_json_streaming`（默认按行回放；`UreqTransport` 覆盖为真·增量——
    拿到响应体 reader 后逐行边收边发）。
  - agent：`AgentRunner::run_streaming`，`AgentLoop::process` 改走流式驱动，每个内容增量转成
    `ProgressEvent::ContentDelta`；`bus::ProgressKind` 增 `ContentDelta`，gateway 一并转发。
- 验证：
  - `rtk cargo fmt --check` 通过；`clippy --all-targets --all-features -D warnings` 无问题。
  - 新增 `provider_stream.rs`（6：parse/顺序/内容组装/tool_calls 跨块组装/错误分类）、`agent_stream.rs`（1：ContentDelta 顺序）；
    全量测试通过（42 套件）。
- 上游对照：
  - 已覆盖：SSE 流式消费 + 增量组装 + 细粒度 ContentDelta progress 经 loop/gateway 传播；真实增量 IO（ureq）。
  - 暂未覆盖：token 级 usage/流式 usage、async 订阅、流式下的 tool 循环端到端网络验证（属 opt-in smoke）。
- 下一步：
  - 进入 Phase 8/9，或补流式 usage 与真实网络 opt-in smoke。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（cron store + session-bound 投递 + heartbeat + 本地 trigger）：
  - `lure-core::cron`：`CronSchedule`（at/every/cron，camelCase `atMs`/`everyMs`）、`CronJob`/
    `CronPayload`/`CronJobState`/`RunStatus`；`compute_next_run`（at 过期返回 None、every=now+间隔、
    cron 暂 None）；`CronStore`（`workspace/cron/jobs.json` 持久化、add 算 next_run、due_jobs、
    record_run 推进/一次性删除）；`heartbeat` 受保护 job（`remove` 返回 `Protected`，
    `is_heartbeat` 区别于普通 reminder）。
  - `cron::origin_delivery_context`：session-bound cron 返回 `(channel, chat_id, metadata)`，
    缺 origin 字段返回 `MissingOriginError`。
  - `lure-core::trigger::LocalTriggerQueue`：enqueue/claim/complete/recover 的 at-least-once
    语义（recover 把未完成投递重新入队，attempts 递增），claim 尊重 busy-session 等待与 limit。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（148 passed）。
- 上游对照：
  - 已覆盖：`test_cron_persistence.py`（持久化/next-run/camelCase）、`test_session_delivery.py`
    （origin 上下文）、`test_local_triggers.py`（at-least-once/忙等/limit）、heartbeat 保护。
  - 暂未覆盖（记入 ledger）：
    - cron 表达式调度（croniter）与时区、并发调度线程、run history 完整记录。
    - 真实文件 inbox/processing 目录布局与 gateway 消费循环、trigger 定义存储。
    - cron 工具（create/list）与 schema 契约（`test_cron_tool_*`）。
- 下一步：
  - 进入 Phase 9：OpenAI-compatible API server、`/v1/chat/completions`（非 streaming + streaming）、SDK facade。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（OpenAI-compatible API 表面，传输无关）：
  - `lure-core::api`：`parse_chat_request`（messages 必须恰好一条 user，支持多模态 text 抽取、
    stream 标志、可选 model）；`validate_model`（不匹配→400）；`authorize`（未配置 key 放行，
    配置后校验 `Bearer <key>`→401）；`api_session_key`（固定 `api:default` 或 `api:{id}`）。
  - 响应：`chat_completion_response`（`object:"chat.completion"`、choices、usage total 规则：
    provider total 优先，否则 prompt+completion）、`error_body`（`{error:{message,type,code}}`）、
    结构化 `ApiError`。
  - streaming：`sse_chunks` 产出有序事件（内容 chunk → finish=stop chunk → `data: [DONE]`）。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（157 passed）。
- 上游对照：
  - 已覆盖：`test_openai_api.py`（error json、chat completion 形状/usage、单条 user 校验、model
    不匹配、鉴权、固定 session）、`test_api_stream.py`（SSE 事件顺序）。
  - 暂未覆盖（记入 ledger）：
    - 真实 HTTP server（aiohttp 对应）、`/v1/models`、media 上传、并发 session lock、
      真实 agent 接线与响应回填。
    - SDK facade（`nanobot/sdk`）与其 streaming client。
    - API runtime 进程生命周期（与 gateway runtime 的隔离目前体现在固定 session key 命名空间）。
- 下一步：
  - 进入 Phase 10：WebUI 后端服务协议、session list/thread API、WebSocket stream 协议与前端测试映射。

### 进度记录 2026-07-24

- 状态：partial
- 本次完成（真实 HTTP server 接线）：
  - `lure-core::api::server`：最小**同步** HTTP server（`tiny_http`），把传输无关 api 表面接到真实端点。
    - `POST /v1/chat/completions`：读取 body → `parse_chat_request` → `validate_model` → 调注入的
      `ChatRunner` → 非流式 JSON 或 SSE 流（`stream:true` 时按 `sse_chunks` 发内容 chunk → finish chunk → `[DONE]`）。
    - `GET /health`：返回 `{"status":"ok"}`（不鉴权）。
    - 鉴权走 `authorize`（Bearer key，未配置放行，配置后 401）；错误响应走 `error_body`，状态码稳定（400/401/404/500）。
  - `ChatRunner` trait 抽象 runner，为 `AgentLoop` 提供实现（构造 `InboundMessage`，`channel="api"`、
    `chat_id="default"`、`session_key_override` 由 `api_session_key` 派生）。
  - 测试拓扑：`AgentLoop` 无 `Send` 界，server 留在测试主线程，`handle_next()` 逐条阻塞应答；
    HTTP 客户端跑子线程，请求/应答 ping-pong 天然串行。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk env -u DEEPSEEK_API_KEY cargo test --all-targets --all-features` 通过（163 passed，含新增 api_server 6 个）。
- 上游对照：
  - 已覆盖：`test_openai_api.py`（error json、chat completion 形状/usage、单条 user 校验、model
    不匹配、鉴权、固定 session）、`test_api_stream.py`（SSE 事件顺序）、真实 HTTP server 接线。
  - 暂未覆盖（记入 ledger）：
    - `/v1/models`、media 上传、并发 session lock、逐 token SSE（当前按 `sse_chunks` 发单条内容 chunk）。
    - SDK facade（`nanobot/sdk`）与其 streaming client。
    - API runtime 进程生命周期（与 gateway runtime 的隔离目前体现在固定 session key 命名空间）。
- 下一步：
  - Phase 9 剩余外围（`/v1/models`、multipart/media 上传、并发 session lock、逐 token SSE），
    或 Phase 8（cron/trigger）盘点。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（WebUI 后端服务协议，传输无关）：
  - `session::SessionManager::list_stored_keys`：枚举并反解 sessions 目录（补 Phase 2 缺口）。
  - `lure-core::webui`：`list_webui_sessions`（session 列表行 key/preview/计数/更新时间）、
    `thread_messages`（thread 的 `{key, messages:[{role,content}]}` 投影）、`webui_status`
    （bootstrap/status：status/version/sessions）。
  - WebSocket 协议：出站事件 `message`/`delta`/`status`/`error`（`{event, ...}` 形状）、
    入站 `parse_ws_inbound`（`{type,chat_id,content}` → InboundMessage，缺 chat_id/content 返回 detail）。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（163 passed）。
- 上游对照：
  - 已覆盖：`test_session_list_index.py`（preview/计数/枚举）、WebSocket 事件形状与入站校验、
    bootstrap/status。
  - 暂未覆盖（记入 ledger）：
    - 真实 HTTP/WebSocket 服务、连接生命周期（attach/detach）、SSL、媒体重写。
    - session_list_index 的索引缓存/增量重扫优化。
    - settings/transcript/token usage/mcp presets 等大表面 API。
    - 前端资源构建（`webui/src`）与前端测试：属外部前端构建资产，暂缓（可后续决定复用或改写）。
- 下一步：
  - 进入 Phase 11：Rust 发布产物、Dockerfile/compose、legacy config/session/memory 迁移、发布清单。

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

### 进度记录 2026-07-23

- 状态：partial
- 本次完成（发布产物 + Docker 骨架 + config 迁移 + 清单）：
  - `config::migrate_config`：原始 JSON 层 legacy 迁移（丢弃 `maxMessages`/`max_messages`、
    `tools.exec.restrictToWorkspace` → `tools.restrictToWorkspace`、`myEnabled`/`mySet` →
    `tools.my.{enable,allowSet}` 且已有子键优先）；`load_config` 在 typed 解析前调用。
  - CLI `--version`/`-V`：输出与包版本一致，供发布产物核对。
  - `Dockerfile`（多阶段 builder → runtime，非 root，workspace volume）、`docker-compose.yml`
    （构建 + 版本 smoke）。
  - `handbook/release-checklist.md`：构建/产物/Docker/迁移/文档/暂缓项完整清单。
- 验证：
  - `rtk cargo fmt --check` 通过；`rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk cargo test --all-targets --all-features` 通过（170 passed）。
  - `rtk cargo build --release --bin lure` 生成 `target/release/lure`（643K），`--version` 输出 `lure 0.0.0`。
- 上游对照：
  - 已覆盖：`test_config_migration.py`（maxMessages/exec.restrictToWorkspace/my tool keys 变换）、
    `test_package_version.py`（版本核对，等价为 CLI `--version`）。
  - 暂未覆盖（记入 ledger）：
    - 真实 `docker build`/`docker compose`（外部工具，属 CI；本地不作单测门禁）。
    - `tools` typed 建模后 my tool keys 的 typed 往返（当前仅原始 JSON 层迁移可测）。
- 说明：至此 11 个阶段均形成可运行纵向切片，Phase 1-11 以 partial 状态收敛，暂缓项均在
  `upstream-test-ledger.md` 与 `release-checklist.md` 记录。

### 进度记录 2026-07-23（legacy session/memory 迁移回补）

- 状态：partial
- 本次完成：
  - `SessionManager` 支持 workspace 内 legacy lossy stem（如 `telegram_12345.jsonl`）读取 metadata key，
    `list_stored_keys` 可在损坏行存在时保留 session，并迁移到 base64url canonical 文件名。
  - `get_or_create` 在 canonical 文件缺失时会从匹配 key 的 legacy lossy stem 迁移并加载原消息。
  - `MemoryStore::new` 增加一次性 `memory/HISTORY.md` → `memory/history.jsonl` 迁移：
    支持时间戳块、连续无空行条目、`[RAW]` 块保持单条、空 `history.jsonl` 仍迁移、非空
    `history.jsonl` 跳过、非法 UTF-8 以 lossless-enough 方式保留可读内容。
  - 迁移后写入 `.cursor` 与 `.dream_cursor` 到最后 cursor，并把原始 `HISTORY.md` 移为
    `HISTORY.md.bak`（已有备份时自动加序号）。
  - 修正 `handbook/README.md` 过期基线描述。
- 验证：
  - `rtk cargo fmt --check` 通过。
  - `rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
  - `rtk env -u DEEPSEEK_API_KEY cargo test --all-targets --all-features` 通过（177 passed）。
  - 直接运行 `rtk cargo test --all-targets --all-features` 时，因当前环境含 `DEEPSEEK_API_KEY`
    且沙箱禁止 DNS，opt-in `provider_deepseek_smoke` 触网失败；已按 opt-in 设计清除该变量后完成本地全量验证。
- 上游对照：
  - 已覆盖：`tests/session/test_session_list_repair_legacy.py`、`tests/agent/test_memory_store.py`
    中 legacy `HISTORY.md` 迁移核心场景。
  - 暂未覆盖：legacy 全局 sessions 目录（`~/.nanobot/sessions`）迁移、GitStore 版本化、
    `compact_history` 完整策略与并发 append 锁。
- 下一步：
  - 可继续收敛 Phase 4 的 stateful `ModelRuntimeResolver`，或 Phase 9/10 的真实 HTTP/WebSocket 服务。
