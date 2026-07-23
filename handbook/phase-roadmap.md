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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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

- 状态：`todo`
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
