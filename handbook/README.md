# Lure 复刻推进 Handbook

本目录是后续持续推进 Rust 版 `nanobot` 复刻的工作中枢。

## 当前基线

- 当前仓库已建立 Cargo workspace，包含 `lure-core` 与 `lure-cli` 两个 crate。
- Phase 0 已完成；Phase 1-11 均已有可运行纵向切片，当前以 `partial` 状态继续回补上游测试缺口。
- 上游事实来源：`/Users/scottlee/workspace/github/nanobot`。
- 上游当前可见形态：Python 后端核心、CLI/API/Gateway/WebUI、多渠道、工具、provider、session、memory、cron 等能力。
- 本项目推进原则：先做窄而可运行的纵向闭环，再扩展到外围能力；每个阶段先补测试，再补生产代码。

## 文档索引

- [phase-roadmap.md](./phase-roadmap.md)：阶段拆分、阶段目标、依赖关系和状态。
- [phase-execution.md](./phase-execution.md)：阶段推进方式、每次推进的固定流程和状态更新规则。
- [phase-plans.md](./phase-plans.md)：每个阶段的 plan、验收标准和完成证据要求。
- [upstream-test-ledger.md](./upstream-test-ledger.md)：上游测试映射台账，用来管理哪些测试已复刻、哪些暂缓、为什么暂缓。

## 状态标记

- `todo`：尚未开始。
- `in_progress`：正在实现或验收中。
- `blocked`：被明确外部条件阻塞，必须记录阻塞原因和下一步。
- `partial`：已有可运行切片，但验收标准未全部满足。
- `done`：验收标准已通过，并记录了命令或人工核对证据。

## 推进守则

1. 每个 phase 开始前，先阅读上游相关源码和测试。
2. 每个能力先建立本项目测试，再实现 Rust 代码。
3. 上游测试无法等价复刻时，在 `upstream-test-ledger.md` 记录缺口和原因。
4. 阶段完成只能基于当前仓库验证结果，不基于推测或计划完成度。
5. 引入 Rust 代码后，阶段验收默认包含 `cargo fmt`、`cargo clippy`、`cargo test`。
