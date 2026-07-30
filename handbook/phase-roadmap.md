# Phase Roadmap

| 阶段 | 状态 | 核心已完成 | 主要缺口 |
|---|---|---|---|
| Phase 0 骨架 | done | Cargo workspace、CLI/core crate、台账格式 | — |
| Phase 1 配置 | partial | config schema/别名/默认值、路径解析、原子读写、migration、preset 解析 | onboard 交互式初始化、env 插值 |
| Phase 2 Session | partial | session key/base64url 命名、JSONL 存储/修复、历史/offset/goal 派生、legacy 迁移 | turn continuation、weak-identity |
| Phase 3 Agent Loop | partial | AgentLoop/AgentRunner/ContextBuilder、EchoProvider、CLI one-shot + 完整交互 UX（流式/tool 行/reasoning 流/spinner 定时动画/阶段标签/Ctrl-C 中断/单轮容错）、**CommandRouter 命令路由核心**（@bot 归一 + priority/exact/prefix 三层 + is_priority/is_dispatchable 谓词 + 内置命令全表登记 + /help//pairing//skill 完整处理器）、**SkillsLoader**（两源枚举/需求过滤/YAML frontmatter/摘要）、**Subagent 状态/簿记核心**（SubagentStatus/registry/cancel_by_session/partial-progress） | 运行时命令处理器（/new /goal /history /dream* /stop… 待各子系统接线）、subagent 真实后台执行（需异步运行时）、CLI REPL 接入统一 router、**subagent**、goal 状态机 |
| Phase 4 Provider | partial | ModelRuntimeResolver、OpenAI-compatible 线协议+SSE 流式、config 驱动 provider 匹配、CLI --config/--preset/--model | OAuth 凭据、重试策略、image 生成、audio 转写（**范围决定：仅做 OpenAI-compat 一种线协议，Anthropic/Bedrock/Azure/Copilot/Codex 原生协议不做**） |
| Phase 5 Tools | partial | Tool trait/registry、文件读写搜索（edit+grep+list）、shell 执行策略、tool-call 循环、**MCP 纯变换核心**（工具名净化/限长 + OpenAI schema 归一 + 畸形进度检测） | MCP 真实连接/会话/传输（需 MCP SDK + 异步）、apply_patch、web_search/find_files、文档解析、并行 tool、network SSRF 边界 |
| Phase 6 Memory | partial | MemoryStore（读写/历史/迁移）、dream consolidation（FakeRunner + ProviderDreamRunner）、阈值自动触发、AgentLoop/CLI 记忆接入 | **GitStore 版本化**、**autocompact / context governance**、SOUL/USER 整合 |
| Phase 7 Gateway | partial | Inbound/OutboundMessage、MessageBus、Channel trait + 访问控制、Gateway 编排、progress 事件传播、**pairing 配对 store**（生成/审批/撤销/TTL/命令派发，对齐上游 test_store 全覆盖） | **真实 channel 平台（telegram/discord/slack…）**、**channel manager 热加载 / plugin**、`/pairing` 命令 UI 接入（随命令路由） |
| Phase 8 Cron | partial | cron store/next-run(at/every/**cron 表达式**)/session delivery/heartbeat、trigger at-least-once、5 字段 cron + UTC/本地时区、**`cron` agent 工具**、**cron service 定时执行**、**接入 lure-desktop 长驻宿主**(常驻 CronScheduler，产出写 transcript，headless 端到端验证)、**cron 实时推送**(WsHub 在线连接注册表 + 连接读循环 drain 推送队列，cron 产出实时推给在线连接，headless 端到端验证)、**IANA 具名时区**(chrono-tz，含 DST 跳变正确处理；cron 工具接受任意合法 IANA 名) | run history、jobs.json 并发加锁 |
| Phase 9 API | partial | ChatServer(tiny_http)、/v1/models、逐 token SSE、session 并发锁 | **SDK facade（clients/streaming/types）**、media 上传、锁接入 handler |
| Phase 10 WebUI | partial | **desktop 闭环**——vendor React 前端、wry 窗口、loopback HTTP/WS、agent loop 每连接独立、transcript/webui-thread 持久化、/api 读表面全覆盖、**settings 写入大表面**(agent/provider/model-preset GET-style 写、原子落盘)、dream 接真实 provider、`--headless` + Playwright E2E 契约测试(**settings 写入面 + hermetic 种子 config 隔离**) | media 代理、非 macOS、E2E 扩面(跨会话/new-chat)+CI |
| Phase 11 打包 | partial | config/Docker/legacy 迁移骨架、release checklist | 完整 release pipeline |

## 跨阶段未建模的上游子系统

以下为上游 `nanobot/` 中已成规模、但当前 lure **完全或几乎未建模**的能力域（不干净归属单一 phase，按上游模块登记，防止在阶段表中被遗漏）：

| 上游子系统 | 上游模块（规模） | lure 现状 | 影响 |
|---|---|---|---|
| ~~MCP 集成~~ | `webui/mcp_presets_api.py`(45KB)、MCP tool | **纯变换核心已复刻**（`lure_core::tool::mcp`：工具名净化/限长、OpenAI schema 归一、畸形进度检测，14 例） | 真实连接/会话/传输（stdio/HTTP/SSE，需 MCP SDK + 异步）、webui 预设表面（Phase 10） |
| ~~原生多协议 provider~~ | `providers/anthropic`/`bedrock`/`azure`/`github_copilot`/`openai_codex`/`fallback` | 仅 OpenAI-compat 线协议 | **范围决定：不做**。lure 只对接 OpenAI 兼容端点；原生 Anthropic Messages/Bedrock 等协议与 fallback 链明确不复刻 |
| **图像生成 / 音频转写** | `providers/image_generation.py`(64KB)、`providers/transcription.py`(28KB)、`audio/`、`webui/transcription_ws.py` | 无 | 多模态生成与语音输入整条链缺失（走 OpenAI-compat 端点实现，不涉原生协议） |
| ~~Skills 系统~~ | `agent/skills.py`、`webui/skills_api.py`、`/skill` 命令 | **loader 已复刻**（`lure_core::agent::skills`，21 例对齐 test_skills_loader；`/skill` 已接入路由） | webui skills_api（Phase 10）、bundled skills vendored 资产 |
| ~~Subagent~~ | `agent/subagent.py`(19KB)、级联 exec 终止 | **状态/簿记核心已复刻**（`lure_core::agent::subagent`：SubagentStatus/registry/cancel_by_session/partial-progress，21 例） | 真实后台执行（spawn 起 agent turn、announce 回灌、exec 级联终止）需异步运行时 |
| **命令路由全量** | `command/builtin.py`(106KB)、`command/router.py` | **router 核心 + 全表登记已复刻**（`lure_core::command`，/help//pairing 完整，谓词对齐 test_router_dispatchable） | 运行时命令处理器（/new /goal /dream* /skill /stop…）随各子系统接线；CLI REPL 接入统一 router |
| ~~Pairing 配对~~ | `pairing/store.py`(9.6KB) | **store 已复刻**（`lure_core::pairing`，31 例对齐 test_store） | 仅剩 `/pairing` 命令 UI 接入（随命令路由） |
| **GitStore 记忆版本化** | `utils/gitstore.py`(20KB) | 无 | 记忆快照/回滚（/dream-restore）缺失 |
| **Context governance / autocompact** | `agent/context_governance.py`(19KB)、`agent/autocompact.py` | 仅 `compact_history` 容量裁剪 | 大上下文治理策略缺失 |
| **Turn continuation / goal 状态机** | `session/turn_continuation.py`、`session/goal_state.py` | 仅 goal 派生视图 | 多轮续跑与目标追踪缺失 |
| **WebUI 大表面** | `media_api`/`attachment_ingress`/`token_usage`/`forking`/`workspaces`/`cli_apps_api` | 仅读表面 + settings 写 | 媒体上传/附件/用量面板/会话分叉/工作区管理缺失 |
| **SDK facade** | `sdk/`(clients/streaming/types) | 无 | 程序化嵌入接口缺失 |
| **network SSRF 安全** | `security/network.py`(10KB)、`security/workspace_access.py`(13KB) | 仅 workspace policy | 出网访问控制未建模 |
