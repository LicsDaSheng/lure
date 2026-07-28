# Phase Roadmap

| 阶段 | 状态 | 核心已完成 | 主要缺口 |
|---|---|---|---|
| Phase 0 骨架 | done | Cargo workspace、CLI/core crate、台账格式 | — |
| Phase 1 配置 | partial | config schema/别名/默认值、路径解析、原子读写、migration、preset 解析 | onboard 交互式初始化、env 插值 |
| Phase 2 Session | partial | session key/base64url 命名、JSONL 存储/修复、历史/offset/goal 派生、legacy 迁移 | turn continuation、weak-identity |
| Phase 3 Agent Loop | partial | AgentLoop/AgentRunner/ContextBuilder、EchoProvider、CLI one-shot + 完整交互 UX（流式/tool 行/reasoning 流/spinner 定时动画/阶段标签/Ctrl-C 中断/单轮容错） | slash commands |
| Phase 4 Provider | partial | ModelRuntimeResolver、OpenAI-compatible+SSE 流式、config 驱动 provider 匹配、CLI --config/--preset/--model | OAuth、重试策略 |
| Phase 5 Tools | partial | Tool trait/registry、文件读写搜索（edit+grep+list）、shell 执行策略、tool-call 循环 | apply_patch、web/mcp |
| Phase 6 Memory | partial | MemoryStore（读写/历史/迁移）、dream consolidation（FakeRunner + ProviderDreamRunner）、阈值自动触发、AgentLoop/CLI 记忆接入 | dream 运行时接真实 provider（现装配 Echo）、SOUL/USER 整合 |
| Phase 7 Gateway | partial | Inbound/OutboundMessage、MessageBus、Channel trait、Gateway 编排、progress 事件传播 | 真实 channel 平台 |
| Phase 8 Cron | partial | cron store 持久化/next-run/session delivery/heartbeat、trigger at-least-once | 完整 cron 表达式 |
| Phase 9 API | partial | ChatServer(tiny_http)、/v1/models、逐 token SSE、session 并发锁 | media 上传、SDK facade |
| Phase 10 WebUI | partial | **desktop 闭环**——vendor React 前端、wry 窗口、loopback HTTP/WS、agent loop 每连接独立、transcript/webui-thread 持久化、/api 读表面全覆盖、dream 接真实 provider、`--headless` + Playwright E2E 契约测试 | settings 变更大表面、media 代理、非 macOS、E2E 扩面+CI |
| Phase 11 打包 | partial | config/Docker/legacy 迁移骨架、release checklist | 完整 release pipeline |
