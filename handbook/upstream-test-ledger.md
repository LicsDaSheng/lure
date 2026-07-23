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
| `tests/config/` | 1 | todo | 配置 schema、loader、路径解析优先映射 |
| `tests/session/` | 2 | todo | session JSONL、fsync、cache、goal state、turn continuation |
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
| `tests/agent/test_onboard_logic.py` | 1 | `crates/lure-core/tests/onboard_logic.rs` | mapped | onboard 初始化和默认配置 |
| `tests/config/test_config_load_errors.py` | 1 | `crates/lure-core/tests/config_load.rs` | mapped | 无效配置错误路径 |
| `tests/config/test_config_atomic_save.py` | 1 | `crates/lure-core/tests/config_save.rs` | mapped | 配置原子保存格式 |
| `tests/session/test_goal_state.py` | 2 | `crates/lure-core/tests/session_goal_state.rs` | mapped | goal state 持久化 |
| `tests/session/test_session_fsync.py` | 2 | `crates/lure-core/tests/session_fsync.rs` | mapped | session 写入安全 |
| `tests/agent/test_loop_runner_integration.py` | 3 | `crates/lure-core/tests/loop_runner_integration.rs` | mapped | agent loop 最小闭环 |
| `tests/agent/test_model_runtime_resolver.py` | 4 | `crates/lure-core/tests/model_runtime_resolver.rs` | mapped | provider/model 解析 |

> 注：以上上游文件名均已按 `nanobot/tests/` 现状核对存在。`tests/session/` 另有
> `test_consolidated_offset_clamp.py`、`test_session_cache.py`、`test_turn_continuation.py`、
> `test_session_list_repair_legacy.py`，进入 Phase 2 时一并映射。
