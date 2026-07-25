# Lure Handbook

本目录是 Rust 版 `nanobot` 复刻推进的工作中枢。

## 当前基线

- Cargo workspace 含 `lure-core`、`lure-cli`、`lure-desktop` 三个 crate。
- 上游事实来源：`/Users/scottlee/workspace/github/nanobot`。
- 12 个阶段均已有可运行纵向切片；各阶段当前状态与缺口见 [phase-roadmap.md](./phase-roadmap.md)。
- 推进原则：先做窄而可运行的纵向闭环，再展开外围；每个能力先补测试，再补生产代码。

## 文档索引

- [phase-roadmap.md](./phase-roadmap.md)：各阶段状态、已完成项与缺口（一张表）。
- [phase-plans.md](./phase-plans.md)：每阶段 plan、验收标准与**下一步目标**（优先级排序）。
- [phase-execution.md](./phase-execution.md)：每个 phase 的固定推进流程、门禁与验证。
- [upstream-test-ledger.md](./upstream-test-ledger.md)：上游测试覆盖台账（逐文件记录覆盖/暂缓/缺口）。

## 推进守则

1. 每个 phase 开始前，先阅读上游相关源码和测试。
2. 每个能力先建立本项目测试，再实现 Rust 代码。
3. 上游测试无法等价复刻时，在 `upstream-test-ledger.md` 记录缺口和原因。
4. 阶段完成只能基于当前仓库验证结果，不基于推测或计划完成度。
5. 引入 Rust 代码后，阶段验收默认包含 `cargo fmt`、`cargo clippy`、`cargo test`。
