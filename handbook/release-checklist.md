# 发布前验收清单

用于每次发布前的完整核对。以当前仓库验证结果为准，不基于推测。
异步运行时对齐（Stage 0-7，见 `async-runtime-plan.md`）已全部完成。

## 构建与质量门禁

- [ ] `rtk cargo fmt --check` 通过。
- [ ] `rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
- [ ] `rtk cargo test --all-targets --all-features` 全量通过，并记录通过数。
- [ ] `rtk cargo build --release` 生成 `target/release/lure`。
- [ ] `target/release/lure --version` 输出与包版本一致。

## 异步运行时架构（Stage 0-7 收尾后）

- [ ] 全量 Rust 测试（含 `channel`/`cron_async`/`subagent_run`/`mcp_client` 契约测试）通过。
- [ ] `make e2e`（Playwright 7 用例）通过——真实浏览器驱动 headless 后端，守护 WebUI 契约不变。
- [ ] `cargo test -p lure-desktop --test headless` 通过——守护 cron → transcript → 在线 WS 推送。
- [ ] 单实例调度核心：WS 渠道与 cron 共用 bus，`/stop` 按 session 取消（`agent_loop_async.rs`）。
- [ ] 同步死代码已清理：无 `tiny_http`、无线程版 cron 调度器（`cargo tree`/grep 复核）。

## 发布产物

- [ ] workspace 版本号在 `Cargo.toml` 中已更新（`workspace.package.version`）。
- [ ] `Cargo.lock` 已提交且与依赖一致。
- [ ] 二进制 `lure` 可在目标平台运行（至少 `--version` 与 `agent -m` one-shot）。

## Docker

- [ ] `docker build -t lure:local .` 成功（多阶段：builder → runtime）。
- [ ] `docker run --rm lure:local` 打印版本（构建 smoke）。
- [ ] `docker compose build` 与 `docker compose run --rm lure` 正常。
- 说明：真实镜像构建/推送属外部工具，通常在 CI 执行；本地不作为单测门禁。

## 迁移兼容

- [ ] legacy config：含 `maxMessages` / `tools.myEnabled|mySet` / `tools.exec.restrictToWorkspace`
      的旧 `config.json` 经 `load_config` 迁移后可正常加载（见 `config_migration.rs`）。
- [ ] legacy session/memory fixture 迁移：基础闭环已覆盖（workspace legacy lossy stem →
      canonical session 文件、`HISTORY.md` → `history.jsonl`，见 `session_persistence.rs`
      与 `memory_store.rs`）；legacy 全局 sessions 目录迁移待补。

## 文档

- [ ] `README.md` 架构树（含 `mcp/`）与异步运行时说明与当前进度一致。
- [ ] `handbook/phase-roadmap.md` 各阶段状态准确（MCP/subagent 缺口已随 Stage 6 勾销）。
- [ ] `handbook/upstream-test-ledger.md` 覆盖状态更新到位（Stage 7 删除项已记最终状态）。

## 暂缓项（发布说明中显式列出）

- 真实 provider 网络调用（Phase 4 opt-in，需真实 API key；`provider_deepseek_smoke` 为显式 opt-in）。
- MCP HTTP/SSE 传输（Stage 6 已落地 stdio；HTTP/SSE 需真实协议服务器，按台账记 partial）。
- 前端资源构建与前端行为测试（Phase 10，属外部构建资产，`frontend/dist` 构建前需 `bun run build`）。
- legacy 全局 sessions 目录迁移、run history、精确睡到 next_wake 等按台账记录。
- SDK facade（`lure_core::sdk`）：范围决定不做（消费面为 CLI + desktop WebUI）。
