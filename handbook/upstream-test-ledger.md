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
| `tests/session/` | 2 | partial | 存储/clamp/cache/goal_state 已覆盖；list repair、turn continuation、weak-identity 暂缓，见下方明细 |
| `tests/agent/` | 2,3,4,6 | todo | 需按 session、loop、provider、memory 拆分 |
| `tests/cli/` | 3 | todo | 先覆盖 CLI one-shot，再覆盖 interactive |
| `tests/providers/` | 4 | todo | provider registry、runtime resolver、真实 provider opt-in |
| `tests/tools/` | 5 | todo | tool schema、文件、shell、web、MCP 分阶段覆盖 |
| `tests/security/` | 5 | todo | workspace、network、启动安全等边界测试 |
| `tests/bus/` | 7 | todo | message bus 事件和队列 |
| `tests/channels/` | 7 | todo | channel contract、manager、plugin、validation |
| `tests/gateway/` | 7,9 | todo | gateway service 与 API runtime 分开映射 |
| `tests/cron/` | 8 | todo | cron store、delivery、schema contract |
| `tests/triggers/` | 8 | todo | local trigger 与 session delivery |
| `tests/webui/` | 10 | todo | 后端 WebUI API 先于前端复刻 |
| `webui/src/tests/` | 10 | todo | 前端行为测试，后续决定复用或重写 |
| `tests/test_openai_api.py` | 9 | todo | OpenAI-compatible API 核心验收 |
| `tests/test_api_stream.py` | 9 | todo | streaming event 验收 |
| `tests/test_document_parsing.py` | 5 | todo | 文档读取可作为工具/文档能力子阶段 |
| `tests/test_docker.sh` | 11 | todo | Docker 验收，最后阶段处理 |
| `tests/test_package_version.py` | 11 | todo | 发布包版本语义，最后阶段处理 |

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
| `tests/agent/test_loop_runner_integration.py` | 3 | `crates/lure-core/tests/loop_runner_integration.rs` | mapped | agent loop 最小闭环 |
| `tests/agent/test_model_runtime_resolver.py` | 4 | `crates/lure-core/tests/model_runtime_resolver.rs` | mapped | provider/model 解析 |

## Phase 2 明细映射

| 上游测试 | 归属 phase | Rust 测试 | 状态 | 说明 |
|---|---:|---|---|---|
| `tests/session/test_goal_state.py` | 2 | `crates/lure-core/tests/session_goal_state.rs` | partial | 纯派生视图已覆盖；`runner_wall_llm_timeout_s`（需 SessionManager/runner）归 Phase 3 |
| `tests/session/test_consolidated_offset_clamp.py` | 2 | `crates/lure-core/tests/session_offset_clamp.rs` | covered | 内存与加载两条路径的 clamp 均覆盖 |
| `tests/session/test_session_cache.py` | 2 | `crates/lure-core/tests/session_cache.rs` | partial | bounded/LRU order/淘汰重载已覆盖；weak-overflow 身份保留两例需 `Rc`/`Weak`，后续回补 |
| `tests/session/test_session_fsync.py` | 2 | `crates/lure-core/tests/session_cache.rs` + `session_persistence.rs` | partial | durable reload / flush_all / 无 tmp 残留已覆盖；fsync 调用计数与 PermissionError 传播暂缓（Rust std 不便 mock `os.fsync`） |
| `tests/session/test_session_list_repair_legacy.py` | 2 | 待定 | deferred | legacy lossy stem 修复，属迁移边界，待 legacy 迁移子阶段回补 |
| `tests/session/test_turn_continuation.py` | 2→3/7 | 待定 | deferred | 耦合 `bus.InboundMessage` 与 agent runner，随 loop/bus 落地 |

> 注：以上上游文件名均已按 `nanobot/tests/` 现状核对存在。base64url 存储 key 可逆性与
> JSONL round-trip（含非 ASCII）由 `session_persistence.rs` 覆盖。
