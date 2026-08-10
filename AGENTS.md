# Project Instructions

## Important Principles

- 默认使用中文进行沟通、计划、总结、文档、注释和提交信息编写。
- 保持 git 历史清晰、可追踪：
  - 修改前后都要检查状态。
  - 只显式暂存本次需要提交的路径。
  - 除非用户明确要求，否则不要回滚用户已有改动。
  - 除非用户明确要求，否则不要使用破坏性的 git 命令。

## Shell

- 所有 shell 命令都必须通过 `rtk` 执行，也就是在原始命令前加上 `rtk`。
  - 示例：使用 `rtk cargo test`、`rtk git status`、`rtk rg --files`。

## Project Context

- 这是一个 Rust 项目。
- 项目目标是使用 Rust 完整重新实现 `nanobot` 的**后端与领域行为**（不做功能删减或额外新增）。
- 原始 `nanobot` 代码路径为 `/Users/scottlee/workspace/github/nanobot`。
- 当原始 `nanobot` 的行为、契约、数据流和运行时边界可用时，以上游实现为事实来源。
- 未验证上游行为之前，不要自行发明产品行为或协议契约。

### 前端与桌面（已偏离 vendored）

- **WebUI 前端已改为自有重写**：React + shadcn/ui + Tailwind v4，位于 `frontend/app`，构建产物输出到 `frontend/dist`（被 `lure-desktop` 内嵌）。它对接的是 lure 自己的 webui 后端契约（`lure-core::webui` 的 HTTP `/api/*`、`/webui/bootstrap` 与 WS 复用协议），**不再原样 vendored nanobot 前端**。
- 旧的 vendored nanobot 前端（`frontend/webui`、`frontend/nanobot`、`UPSTREAM_COMMIT`）**已全部移除**。
- **桌面外壳为 Tauri V2**（`lure-desktop`）：仅作窗口/打包外壳，加载进程内 loopback WebUI；能力仍走进程内 HTTP/WS，**不迁移到 Tauri IPC**。
- 因此「以 nanobot 为事实来源、不增删」约束**适用于后端/领域/协议契约**；前端交互与视觉是有意的自有实现，不要求与上游 nanobot WebUI 一致。

## Architecture Principles

- 以 Rust crate 边界作为主要模块化机制组织代码。
- 一旦引入多个 crate，优先使用 Cargo workspace。
- 模块应保持小而明确，并与 crate 的职责边界一致。
- 只有存在真实跨 crate 复用需求时，才把共享领域类型和契约放入专门 crate。
- 避免大型万能 crate 或泛化的工具模块。
- 优先使用类型化 API、枚举、trait 和结构化错误，避免依赖字符串控制流程。
- 保持解析、领域逻辑、运行时执行、存储和传输之间的清晰边界。

## Implementation Guidance

- 修改架构前，先阅读现有代码和上游参考实现。
- 采用 TDD 测试驱动开发；所有复刻实现都必须优先实现 tests，再实现生产代码。
- 新增复刻功能前，先梳理上游 `/Users/scottlee/workspace/github/nanobot/tests` 中相关测试场景。
- 本项目 tests 必须包含上游所有测试场景；如果暂时无法等价覆盖，必须在测试或任务说明中明确记录缺口和原因。
- 先实现窄而可运行的纵向闭环，再展开大范围重写。
- 上游契约确认之前，保持公开 crate API 最小化。
- 在引入行为或契约的 crate 边界补充测试。
- 当项目中存在 Rust 代码时，使用 `cargo fmt`、`cargo clippy` 和 `cargo test` 做验证。

## Git Hygiene

- 修改前后都要执行 `rtk git status --short --branch`。
- 只显式暂存需要提交的路径。
- 除非用户明确要求，否则不要回滚用户改动。
- 默认使用中文提交信息。

## Git Commit Format

- 提交信息中文优先。
- 使用以下格式：

```text
feat: 增加 runtime session 显式创建入口
- 新增工具 create_runtime_session
- 创建 SessionSpec 追加到 SESSIONS.md，默认状态为 pending
```
