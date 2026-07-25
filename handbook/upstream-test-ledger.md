# 上游测试映射台账

本文件用于跟踪 `/Users/scottlee/workspace/github/nanobot/tests` 与本项目 Rust 测试之间的覆盖关系。

## 记录格式

```text
| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| tests/session/test_goal_state.py | 2 | crates/lure-core/tests/session_goal_state.rs | todo | 等待 session store 实现 |
```

## 状态定义

- `todo`：尚未映射。
- `mapped`：已确定 Rust 测试位置，但尚未实现。
- `covered`：已有 Rust 等价测试并通过。
- `partial`：覆盖了主要路径，但存在明确缺口。
- `deferred`：暂缓，必须说明原因和回补 phase。
- `not_applicable`：上游测试仅适用于 Python 打包、特定前端或不再保留的实现细节，必须说明替代验收。

## 初始分类

| 上游测试区域 | 归属 phase | 状态 | 说明 |
|---|---:|---|---|
| `tests/config/` | 1 | partial | 核心 loader/paths/save 已覆盖；migration/env/gateway 相关暂缓，见下方明细 |
| `tests/session/` | 2 | partial | 存储/clamp/cache/goal_state/list repair 已覆盖；turn continuation、weak-identity 暂缓，见下方明细 |
| `tests/agent/` | 2,3,4,6 | partial | session/loop、stateful model runtime resolver、tool-call 循环、SSE 流式驱动、memory 接入 loop、legacy history migration 已覆盖；goal/subagent、真实 LLM dream 待后续 |
| `tests/cli/` | 3 | partial | one-shot 与基础 interactive 已覆盖；上游 prompt_toolkit/progress/commands 待后续 |
| `tests/providers/` | 4 | partial | OpenAI-compatible 请求/响应/错误、选择顺序、SSE 流式消费（含末帧 usage 捕获 + `stream_options.include_usage`）、stateful resolver、config 驱动 provider 匹配（api_base/enabled/api_key）已覆盖；真实 provider opt-in、OAuth/local fallback 待补 |
| `tests/tools/` | 5 | partial | registry/schema/文件读写+edit/list_dir+grep/shell allow-deny、tool-call 循环、config 驱动工具注册已覆盖；apply_patch/find_files/web/mcp/exec 平台细节待补 |
| `tests/security/` | 5 | partial | workspace 边界已覆盖；network SSRF、启动安全待补 |
| `tests/bus/` | 7 | partial | InboundMessage/OutboundMessage、内存队列、outbound 运行时事件（`ProgressUpdate`）已覆盖；async 队列待补 |
| `tests/channels/` | 7 | partial | channel 契约、配置校验、发送方访问控制（allowlist + pairing 兜底）已覆盖；manager 热加载、plugin、具体平台待补 |
| `tests/gateway/` | 7,9 | partial | 编排闭环/启停/health 状态已覆盖；真实 HTTP endpoint、进程 runtime、API runtime 待补 |
| `tests/cron/` | 8 | partial | store 持久化/next-run/session delivery/heartbeat 已覆盖；cron 表达式、工具 schema 待补 |
| `tests/triggers/` | 8 | partial | at-least-once/忙等/limit 已覆盖；文件 inbox 布局、trigger 定义存储待补 |
| `tests/webui/` | 10 | partial | session list/thread/status/WS 事件已覆盖；settings/transcript/token usage/媒体等大表面待补 |
| `webui/src/tests/` | 10 | deferred | 前端行为测试，属外部前端构建资产，后续决定复用或重写 |
|| `tests/test_openai_api.py` | 9 | partial | 请求/响应/校验/鉴权/session、真实 HTTP server、`/v1/models` 已覆盖；per-session 并发锁原语（`SessionLocks`）已落地；media、锁接入 handler 待补 |
| `tests/test_api_stream.py` | 9 | partial | 逐 token SSE 已接线（逐 delta chunk、跨 tool 轮不关流、单 id、默认回退）；token 级 usage 待补 |
| `tests/test_api_lock*.py` | 9 | partial | per-session 锁原语 `SessionLocks`（同 key 互斥、不同 key 独立、RAII 释放，真多线程验证）已覆盖；接入 HTTP handler 待多线程 runner |
| `tests/test_document_parsing.py` | 5 | todo | 文档读取可作为工具/文档能力子阶段 |
| `tests/test_docker.sh` | 11 | partial | Dockerfile/compose 骨架已提供；真实 docker build 属 CI 外部工具 |
| `tests/test_package_version.py` | 11 | partial | CLI `--version` 版本核对已覆盖；发布包元数据待补 |

## Phase 0 待补映射

Phase 0 已完成，确定 phase 1-4 关键上游测试的 Rust 测试落点（文件尚未实现，标记 `mapped`）：

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/agent/test_onboard_logic.py` | 1 | `crates/lure-core/tests/onboard_logic.rs` | deferred | 上游为 ~1900 行交互式向导，耦合 providers/plugins；待 provider 配置落地后回补 |
| `tests/config/test_config_paths.py` | 1 | `crates/lure-core/tests/config_paths.rs` | partial | workspace 路径已覆盖；`get_data_dir`/`get_cron_dir` 等运行时子目录待引入 |
| `tests/config/test_config_load_errors.py` | 1 | `crates/lure-core/tests/config_load.rs` | partial | missing/invalid-json/类型不匹配已覆盖；invalid-schema(`tools.exec.timeout=-1`) 待 Phase 5；ApiConfig wildcard 待 Phase 7/9 |
| `tests/config/test_config_atomic_save.py` | 1 | `crates/lure-core/tests/config_save.rs` | partial | round-trip/camelCase/父目录/unix 权限位已覆盖；`preserves_existing_file_when_write_fails`(mock `Path.replace`) 由 temp+rename 设计保证，未单独 mock `fs::rename` |
| `tests/config/test_config_migration.py` | 1 | 待定 | deferred | `_migrate_config` 依赖尚未建模字段（maxMessages/tools 迁移） |
| `tests/config/test_env_interpolation.py` | 1 | 待定 | deferred | `${VAR}` 插值依赖后续字段与运行时上下文 |
## Phase 11 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/config/test_config_migration.py` | 11 | `crates/lure-core/tests/config_migration.rs` | partial | maxMessages/exec.restrictToWorkspace/my tool keys 原始 JSON 迁移已覆盖；tools typed 往返待 ToolsConfig |
| `tests/test_package_version.py` | 11 | `crates/lure-cli/tests/version.rs` | partial | CLI `--version` 与包版本一致已覆盖 |
| `tests/test_docker.sh` | 11 | Dockerfile/docker-compose.yml | partial | 发布产物结构已提供；真实 docker build 属 CI |
| legacy session/memory fixture 迁移 | 2,6,11 | `crates/lure-core/tests/session_persistence.rs` + `crates/lure-core/tests/memory_store.rs` | partial | workspace legacy lossy stem 与 `HISTORY.md` → `history.jsonl` 已覆盖；legacy 全局 sessions 目录待补 |

> 注：Phase 11 提供发布结构与 config 迁移逻辑；真实镜像构建留待 CI/发布验收。

## Phase 10 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/webui/test_session_list_index.py` | 10 | `crates/lure-core/tests/webui_protocol.rs` | partial | session 列表 preview/计数/枚举已覆盖；索引缓存/增量重扫优化待补 |
| `nanobot/channels/websocket/runtime.py`（事件协议） | 10 | `crates/lure-core/tests/webui_protocol.rs` | partial | message/delta/status/error 事件形状与入站校验已覆盖；连接生命周期、SSL、媒体待补 |
| `webui/src/tests/` | 10 | 待定 | deferred | 前端行为测试，属外部前端构建资产 |

> 注：Phase 10 为传输无关的 WebUI 后端协议；真实 HTTP/WebSocket 服务、前端资源构建、
> settings/transcript 等大表面留待后续。

## Phase 9 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
|| `tests/test_openai_api.py` | 9 | `crates/lure-core/tests/api_openai.rs` `api_server.rs` | partial | error json、chat completion 形状/usage（真实回填 loop 累加 usage：prompt/completion/total）、单条 user 校验、model 不匹配、鉴权、固定 session、真实 HTTP server、`/v1/models`（形状+鉴权）、per-session 锁原语已覆盖；media、锁接入 handler 待补 |
| `tests/test_api_stream.py` | 9 | `crates/lure-core/tests/api_openai.rs` `api_server.rs` | partial | SSE 事件顺序（内容→finish→[DONE]）、逐 token 流式（逐 delta chunk、跨 tool 轮不关流、单 id、默认回退单 chunk）已覆盖；token 级 usage 待补 |
| `tests/test_api_lock*.py` | 9 | `crates/lure-core/tests/session_lock.rs` | partial | per-session 锁原语 `SessionLocks`（同 key 互斥/串行、不同 key 独立、RAII 释放，真多线程验证）已覆盖；接入 HTTP handler 待多线程 runner |

> 注：Phase 9 传输无关表面、真实 HTTP server（含 `/v1/models`、`/health`）、逐 token SSE 与
> per-session 并发锁原语均已落地；`SessionLocks` 接入 HTTP handler、SDK facade、API runtime
> 进程生命周期、token 级流式 usage 留待后续。

## Phase 8 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/cron/test_cron_persistence.py` | 8 | `crates/lure-core/tests/cron_store.rs` | partial | 持久化/next-run(at,every)/camelCase/due/record_run/一次性删除/heartbeat 保护已覆盖；cron 表达式、run history 待补 |
| `tests/cron/test_session_delivery.py` | 8 | `crates/lure-core/tests/cron_delivery.rs` | covered | origin delivery 上下文与缺失校验均覆盖 |
| `tests/triggers/test_local_triggers.py` | 8 | `crates/lure-core/tests/trigger_queue.rs` | partial | enqueue/claim/complete/recover(at-least-once)/忙等/limit 已覆盖；文件 inbox 布局、trigger 定义存储待补 |

> 注：Phase 8 用内存队列建模 trigger at-least-once 语义（上游为文件 inbox）；cron 表达式调度
> （croniter）与并发调度线程留待后续。

## Phase 7 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `nanobot/bus`（events/queue） | 7 | `crates/lure-core/tests/bus_events.rs` | partial | InboundMessage session_key、OutboundMessage reply、FIFO 队列已覆盖 |
| `nanobot/bus`（progress 事件） | 7 | `crates/lure-core/tests/gateway_progress.rs` `agent_stream.rs` | partial | `ProgressUpdate`/`ProgressKind`（含 ContentDelta）+ gateway 转发 Started/ContentDelta/ToolInvoked/Final、streaming provider 逐增量 ContentDelta 已覆盖；async 订阅待补 |
| `tests/channels/test_channel_validation.py` 等 | 7 | `crates/lure-core/tests/gateway_dispatch.rs` `channel_access.rs` | partial | channel 配置校验（缺字段）、发送方访问控制 `AccessPolicy::is_allowed`（star>精确 allowlist>pairing>deny，`allowFrom` 别名/null，对齐 `test_base_channel::TestIsAllowed`）已覆盖；TCP probe/热加载/plugin 待补 |
| `tests/gateway/`（service 编排） | 7 | `crates/lure-core/tests/gateway_dispatch.rs` `gateway_progress.rs` | partial | InboundMessage→AgentLoop→OutboundMessage→channel 闭环、启停不丢任务、未知 channel、health、progress 事件转发已覆盖；HTTP endpoint/进程 runtime 待补 |

> 注：Phase 7 为同步内存实现（上游 asyncio）；WebSocket 最小往返随 Phase 10 WebUI，
> 真实 HTTP health endpoint 与进程管理 runtime 留待后续。

## Phase 6 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/agent/test_memory_store.py` | 6 | `crates/lure-core/tests/memory_store.rs` | partial | memory/soul/user 读写、history cursor、strip、session 过滤、reopen、legacy `HISTORY.md` 迁移、compact_history（`max_history_entries` 保留最新 N、保留 session_key）、硬上限截断带 `... (truncated)` 标记已覆盖；per-call `max_chars`、并发游标锁、compact 接入 autocompact 待补 |
| `tests/agent/test_dream.py` | 6 | `crates/lure-core/tests/memory_dream.rs` | partial | dream 触发/写回/幂等/cursor 推进用 fake runner 覆盖；真实 LLM dream、SOUL/USER 整合、批次策略暂缓 |
| `tests/agent/test_context_builder.py` | 6 | `crates/lure-core/tests/memory_context.rs` | partial | memory 注入顺序（system→memory→历史）已覆盖；runtime context 块、富历史处理待补 |
| `tests/agent/` (memory 接入 loop) | 6 | `crates/lure-core/tests/agent_memory.rs` | partial | `AgentLoop::with_memory`（记忆块注入、user/assistant 追加 history.jsonl）+ `consolidate`（fake runner）已覆盖；CLI 已挂载 memory（`cli_one_shot.rs` 验证 history 记录）；dream 触发策略、真实 LLM dream 待后续 |

> 注：Phase 6 dream 把 LLM 抽象为可替换 `DreamRunner`，测试不接真实 LLM；GitStore 版本化、
> autocompact、unified session 内部会话过滤留待后续。

## Phase 5 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/security/test_workspace_policy.py` | 5 | `crates/lure-core/tests/security_workspace.rs` | partial | 相对/穿越/前缀兄弟/符号链接逃逸/额外 root/精确文件已覆盖；extra-file 符号链接逃逸精确拦截待补 |
| `tests/tools/test_exec_allow_patterns.py` | 5 | `crates/lure-core/tests/tool_shell_policy.rs` | covered | allow/deny/allowlist/分段/fd 重定向均覆盖 |
| `tests/tools/` (config 驱动注册) | 5 | `crates/lure-core/tests/tool_setup.rs` | partial | `registry_from_config`（默认注册文件工具、exec 按 `tools.exec` 开关+正则门禁、非法正则报错）已覆盖；CLI 已接入 `build_agent_loop` |
| `tests/tools/test_tool_registry.py` | 5 | `crates/lure-core/tests/tool_registry.rs` | partial | 定义顺序/派发/近似建议/参数校验已覆盖；MCP 排序、prepare_call 全貌待补 |
| `tests/tools/test_filesystem_tools.py` | 5 | `crates/lure-core/tests/tool_file.rs` `tool_edit.rs` `tool_search.rs` | partial | read/write + workspace 越界拒绝、edit（`find_match` 精确+行 trim 回退、CRLF 保留、replace_all、歧义告警、not-found/缺 new_text 错误）、list_dir（基础/递归/忽略噪声目录/max_entries 截断/空目录/not-found/缺 path，对齐 `TestListDirTool`）已覆盖；高级读增强、apply_patch 待补 |
| `tests/tools/test_search_tools.py` | 5 | `crates/lure-core/tests/tool_search.rs` | partial | grep（files_with_matches 默认+mtime 降序、content 带上下文、case_insensitive、fixed_strings、glob/type 过滤、head_limit/offset 分页，对齐 `GrepTool`）已覆盖；count 模式、二进制/大文件跳过、size 截断、find_files、web_search 待补 |
| `tests/tools/test_tool_validation.py` | 5 | `crates/lure-core/tests/tool_registry.rs` | partial | type/required/enum/数值/长度校验已覆盖；组合校验待补 |
| `tests/test_truncate_text_shadowing.py` | 5 | `crates/lure-core/tests/tool_registry.rs` | partial | 结果截断行为已覆盖（`truncate_result`）；上游具体 shadowing 回归 N/A |

> 注：Phase 5 tool `execute` 采用同步（上游 async）；apply_patch/search/web/mcp/exec 平台细节、
> network SSRF、tool 上下文变量注入完整链路留待后续 phase。

## Phase 4 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/config/test_model_presets.py` | 4 | `crates/lure-core/tests/config_model_presets.rs` | partial | config 层 preset 解析/校验/序列化已覆盖；`get_provider_name` 的 OAuth/local fallback 暂缓 |
| `tests/config/` (ProvidersConfig) | 4 | `crates/lure-core/tests/config_providers.rs` | partial | config 驱动 `resolve_provider`（api_base 覆盖、auto 跳过禁用、forced 显式、api_key 解析）已覆盖；OAuth/local fallback 暂缓 |
| `tests/providers/` (OpenAI-compatible) | 4 | `crates/lure-core/tests/provider_openai.rs` `provider_stream.rs` `provider_usage.rs` | partial | 请求 golden + 响应解析 + 错误分类、SSE 流式（增量回调/内容+tool_calls 组装/错误分类）、流式 usage 捕获（`stream_options.include_usage` + 末帧折入）、usage 归一（cached_tokens 按 `prompt_tokens_details.cached_tokens`→`cached_tokens`→`prompt_cache_hit_tokens` 优先级链提取，对齐 upstream `_extract_usage`/`_get_nested_int`）已覆盖；`max_completion_tokens`/重试暂缓 |
| `tests/providers/` (selection order) | 4 | `crates/lure-core/tests/provider_registry.rs` | partial | forced/前缀/关键字选择顺序已覆盖；config 驱动 api_base/enabled/api_key 已由 `config_providers.rs` 覆盖，OAuth/local fallback 暂缓 |
| `tests/agent/test_model_runtime_resolver.py` | 4 | `crates/lure-core/tests/model_runtime_resolver.rs` | partial | stateful resolver 生命周期（admit/refresh/invalidate + preset tracking + 不可变 `LlmRuntime`/`ProviderSnapshot`）已覆盖；上游源码未 vendored，按本 ledger 记录语义建立事实来源，真实 runtime 探活/降级仍待补 |

> 注：Phase 4 provider 通过 `HttpTransport` 抽象，单测用假传输不触网。真实同步 HTTP 传输
> 已由 `UreqTransport`（ureq）落地；真实 provider smoke 为 opt-in（`provider_deepseek_smoke.rs`，
> 仅设置 `DEEPSEEK_API_KEY` 时出网），CLI `--model <model>` 经 registry 匹配 provider 并从
> `<PROVIDER>_API_KEY` 读取 key。stateful `ModelRuntimeResolver` 已落地
> （preset → 不可变 runtime/snapshot + admit/refresh/invalidate 缓存），并已接入 CLI/AgentLoop
> 的 provider 选择路径（`AgentLoop::with_runtime` 注入 model/settings），且已补 `--preset`
> 命名入口与 config 驱动 `resolve_provider`（api_base 覆盖、auto 跳过禁用 provider、api_key
> config 优先 env 回落）；provider 的 OAuth 凭据与 local fallback 仍待后续。

## Phase 3 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/agent/test_loop_runner_integration.py` | 3 | `crates/lure-core/tests/agent_loop.rs` | partial | 最小闭环（输入/最终回复/turn 保存/历史可读/结构化错误）已覆盖；streaming、goal/subagent、consolidation 属 Phase 4/6 |
| `tests/agent/` (tool-call loop) | 5 | `crates/lure-core/tests/agent_tool_loop.rs` | partial | provider tool_calls 解析 + `AgentLoop` tool-call 迭代（执行/tool turn 回灌/上限/未知工具恢复/无 registry 终态）、空工具结果替换标记、空终响应静默重试+finalization（`stop_reason` + `EMPTY_FINAL_RESPONSE_MESSAGE`）、usage 跨轮累加（含 cached_tokens，对齐 `test_runner_core::{replaces_empty_tool_result_with_marker,retries_empty_final_response,uses_specific_message_after_empty_finalization,accumulates_usage}`）已覆盖；并行 tool、token 估算兜底待后续 |
| `tests/cli/` (agent direct) | 3 | `crates/lure-cli/tests/cli_one_shot.rs` | partial | `agent -m` one-shot、interactive REPL（经 `process_streaming` 逐增量实时输出 + 空内容回退，`StreamRenderer` 单测）、`--session`、`--config`/`--preset`/`--model`（经 resolver 选 provider）、EOF/退出命令已覆盖；prompt_toolkit、多行输入、tool/progress 富渲染、slash commands 待后续 |

> 注：Phase 3 provider 采用同步 trait + `EchoProvider` 占位；上游 async provider 与
> 真实 OpenAI-compatible 调用归 Phase 4，届时收敛 provider 契约。

## Phase 2 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/session/test_goal_state.py` | 2 | `crates/lure-core/tests/session_goal_state.rs` | partial | 纯派生视图已覆盖；`runner_wall_llm_timeout_s`（需 SessionManager/runner）归 Phase 3 |
| `tests/session/test_consolidated_offset_clamp.py` | 2 | `crates/lure-core/tests/session_offset_clamp.rs` | covered | 内存与加载两条路径的 clamp 均覆盖 |
| `tests/session/test_session_cache.py` | 2 | `crates/lure-core/tests/session_cache.rs` | partial | bounded/LRU order/淘汰重载已覆盖；weak-overflow 身份保留两例需 `Rc`/`Weak`，后续回补 |
| `tests/session/test_session_fsync.py` | 2 | `crates/lure-core/tests/session_cache.rs` + `session_persistence.rs` | partial | durable reload / flush_all / 无 tmp 残留已覆盖；fsync 调用计数与 PermissionError 传播暂缓（Rust std 不便 mock `os.fsync`） |
| `tests/session/test_session_list_repair_legacy.py` | 2 | `crates/lure-core/tests/session_persistence.rs` | covered | legacy lossy stem 在 list 和 get_or_create 路径均迁移到 canonical base64url 文件 |
| `tests/session/test_turn_continuation.py` | 2→3/7 | 待定 | deferred | 耦合 `bus.InboundMessage` 与 agent runner，随 loop/bus 落地 |

> 注：以上上游文件名均已按 `nanobot/tests/` 现状核对存在。base64url 存储 key 可逆性与
> JSONL round-trip（含非 ASCII）由 `session_persistence.rs` 覆盖。
