# 项目协作规则

- 当任务涉及读取或分析 pi 本地源码（`/Users/scottlee/workspace/github/pi`）时，仅讨论架构设计，不进行具体实现或代码改动。

## 产品边界：Pi 桌面客户端

- 桌面客户端是已安装 Pi 的原生 GUI 外壳，不实现或复制 Pi 的核心能力；默认用户已按 Pi 官方方式完成安装。
- 桌面客户端与 Pi 仅通过 `pi --mode rpc` 的 stdin/stdout JSONL 协议通信；不得通过 PTY 解析 TUI 的 ANSI 输出，也不得嵌入或重实现 Pi 的 Agent。
- 桌面客户端负责：Pi 可执行文件检测、工作目录选择、Pi 子进程及会话生命周期、RPC 请求/事件转发与故障呈现、会话和执行过程可视化、文件/图片选择、扩展 UI 请求的原生对话框，以及本地 UI 偏好。
- Pi 负责：模型与认证、Agent 推理、工具、skills、extensions、slash commands、会话持久化与分支、上下文压缩、重试、队列和 `.pi` 配置加载。
- 一个桌面会话默认对应一个在所选工作目录运行的 Pi RPC 子进程；对话、模型、会话及命令状态以 Pi 返回的 RPC 数据为准。
- 桌面端通过 `prompt`、`steer`、`follow_up`、`abort`、`clear_queue` 调用对话与运行控制；通过 `get_available_models`、`set_model`、`set_thinking_level` 管理模型；通过 `get_commands` 发现可用扩展命令、提示词和 skills，并以 `prompt` 发送 `/命令`。
- Pi 内置 TUI 专属命令不应通过 RPC 调用；对应功能由桌面端提供图形界面。桌面端应依据 `message_update`、`tool_execution_*`、`agent_settled`、`compaction_*`、`auto_retry_*` 等 RPC 事件展示执行过程，并处理 `extension_ui_request`/`extension_ui_response` 交互。

## Rust crate 项目开发规约

- 按 Rust crate 的边界组织代码：每个 crate 应有清晰、单一的职责，公共 API 通过 `lib.rs`/模块显式暴露，避免跨 crate 依赖内部实现细节。
- 优先使用 workspace 管理多 crate 项目：公共依赖、版本、lint、profile 等配置尽量集中在根 `Cargo.toml`，减少重复与漂移。
- 模块划分遵循领域语义而非技术分层堆叠：模块名表达业务/能力边界，文件结构与 `mod` 层级保持可读、可导航。
- 公共 API 保持小而稳定：默认私有，必要时才 `pub`；跨 crate 暴露优先提供抽象、类型和函数，而不是泄露内部数据结构。
- 错误处理使用类型化错误：库代码避免 `unwrap`/`expect`/`panic!` 作为常规控制流；应用入口可负责错误展示与退出码。
- 依赖保持克制：新增依赖前确认必要性、维护状态、许可证与编译成本；公共库 API 避免无必要地暴露第三方类型。
- 类型优先表达约束：使用 newtype、枚举、trait、生命周期和所有权模型表达不变量，减少运行时非法状态。
- 并发与异步边界清晰：不要无故引入 async；若使用 async，应明确 runtime 边界，库代码避免强绑定具体 runtime，除非 crate 职责要求。
- 测试按层次组织：单元测试靠近实现，集成测试放在 `tests/`，公共行为优先通过集成测试验证；修复缺陷时先补回归测试。
- 文档面向使用者：公共项补充必要 rustdoc；crate 根文档说明用途、核心概念、示例和安全/并发假设。
- 格式化与静态检查作为提交前门禁：保持 `cargo fmt`、`cargo clippy`、`cargo test` 可通过；若暂时不能通过，需在交付说明中明确原因。
- Feature flags 应可组合、命名清晰，并避免默认启用重量级能力；新增 feature 需考虑最小依赖和向后兼容。
- 性能优化以测量为依据：先保证正确性和清晰性，必要时通过 benchmark/profile 定位瓶颈，再进行局部优化。
- 不安全代码最小化：默认禁止 `unsafe`；确需使用时应隔离、注释安全前提，并提供测试覆盖。
- 提交与变更保持原子性：一个变更聚焦一个目的，避免把重构、格式化和行为变化混在一起。
