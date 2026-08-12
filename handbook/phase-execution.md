# Phase 推进方式

## 固定推进循环

每个 phase 都按同一个循环推进：

1. 读取本阶段文档和 `upstream-test-ledger.md`。
2. 搜索并阅读上游相关源码、文档和测试。
3. 写下本次要复刻的最小行为契约：
   - 输入是什么。
   - 输出是什么。
   - 状态如何变化。
   - 错误如何暴露。
   - 本次明确不做什么。
4. 在 Rust 项目中先补测试。
5. 实现最小生产代码。
6. 运行本阶段验证命令。
7. 更新 phase 状态、验收证据、上游测试映射和缺口。

## 阶段推进门禁

### 进入下一阶段前必须满足

- 当前阶段至少形成一个可运行的纵向切片。
- 当前阶段 plan 中的必做验收标准已通过，或明确标记为 `partial` 并记录原因。
- `upstream-test-ledger.md` 已记录本阶段涉及的上游测试覆盖状态。
- 当前仓库状态已检查，且没有混入与本阶段无关的改动。

### 允许 `partial` 推进的条件

只有满足以下条件时，允许带着 `partial` 进入下一阶段：

- 缺口来自外部依赖、真实网络服务、平台账号、前端构建资产或尚未进入范围的上游子系统。
- 缺口不会破坏下一阶段的最小纵向闭环。
- 文档记录了明确的回补 phase 或回补任务。

### 不允许推进的情况

- 没有测试，只靠手工运行判断完成。
- 没有上游对照，直接发明行为。
- 阶段目标过大，无法在本阶段形成可运行切片。
- 公开 API 被提前泛化，导致后续难以按上游契约收敛。

## 验证命令原则

仓库引入 Rust 代码前：

- 至少执行 `rtk git status --short --branch`。
- 文档类变更读回确认格式。

仓库引入 Rust 代码后：

- 默认执行 `rtk cargo fmt --check`。
- 默认执行 `rtk cargo clippy --all-targets --all-features -- -D warnings`。
- 默认执行 `rtk cargo test --all-targets --all-features`。
- 如阶段只触及单 crate，可先跑 scoped tests，但阶段完成前必须跑全量 Rust 验证。

涉及上游行为确认时：

- 先阅读上游测试，不直接运行上游测试作为本项目完成证据。
- 本项目必须有等价 Rust 测试或在台账中记录暂缓原因。

## WebUI 契约门禁（真实浏览器 E2E）

Rust 集成测试用字面参数、无浏览器，**照不出前后端契约错配**（URL 编码、响应形状、
method、WS、bootstrap）。典型：前端 `encodeURIComponent` 把 session key 的 `:` 编码为
`%3A`，服务端未解码导致删除 404——Rust 测试全绿仍漏。

因此，凡改动满足以下任一，除 Rust 全量门禁外**必须过 `e2e/` 的 Playwright smoke**：

- `webui` HTTP/WS 路由、响应形状或 key/路径编码；
- 前端（`frontend/app`）改动或 `frontend/dist` 构建产物；
- `lure-desktop` server 装配（provider、dream、transcript 接线）。

运行：`make e2e`（`webServer` 自动构建 dist + 拉起 `lure-desktop --headless
--model echo`，离线确定性）。首次先 `make e2e-setup` 装依赖。详见
[../e2e/README.md](../e2e/README.md)。

**本地统一门禁入口**（见根目录 `Makefile`）：`make rust`（fmt+clippy+test）、
`make e2e`（浏览器契约）、`make check`（两者全跑，提交前用）。远程 CI 由
GitHub Actions 承担（`.github/workflows/ci.yml`：push main / PR 触发 Rust 门禁 +
前端构建 + E2E；`.github/workflows/release.yml`：v* tag 触发三平台安装包 + Release）。

## Git 卫生

- 修改前后执行 `rtk git status --short --branch`。
- 只显式暂存本次需要提交的路径。
- 不回滚用户已有改动。
- 提交信息中文优先，并保持可追踪。
