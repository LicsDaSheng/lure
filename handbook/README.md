# Lure Handbook（TypeScript 版）

本目录是 lure TypeScript 重写（Rust → TS 改道）的工作中枢。

## 当前基线

- 分支 `refactor/typescript-rewrite`；`main` 仍是 Rust 基线（保留完整历史作参照）。
- 上游事实来源：`/Users/scottlee/workspace/github/nanobot`（Python）。
- 迁移的**可执行规格**分两层：领域行为以原 Rust 测试为规格（`main` 分支 `crates/lure-core/tests`）；前端协议契约适配 **assistant-ui 最佳实践**（`ThreadMessage`/part 模型 + 自定义 runtime 适配器）。
- 方案定稿见 [typescript-migration-plan.md](./typescript-migration-plan.md)。

## 文档索引

- [typescript-migration-plan.md](./typescript-migration-plan.md)：技术选型、目录结构、模块映射、Phase 0-5 迁移路线。

## 推进守则

1. 每个能力先移植测试（vitest），再实现生产代码。
2. 上游/原 Rust 测试无法等价复刻时，记录缺口与原因。
3. 阶段完成只基于当前仓库验证结果，不基于推测。
4. 门禁：`pnpm typecheck`、`pnpm lint`、`pnpm test`、`pnpm build`。
