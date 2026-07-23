# 发布前验收清单

用于 Phase 11 及后续每次发布前的完整核对。以当前仓库验证结果为准，不基于推测。

## 构建与质量门禁

- [ ] `rtk cargo fmt --check` 通过。
- [ ] `rtk cargo clippy --all-targets --all-features -- -D warnings` 无问题。
- [ ] `rtk cargo test --all-targets --all-features` 全量通过，并记录通过数。
- [ ] `rtk cargo build --release` 生成 `target/release/lure`。
- [ ] `target/release/lure --version` 输出与包版本一致。

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

- [ ] `README.md` 阶段状态表与当前进度一致。
- [ ] `handbook/phase-roadmap.md` 各阶段状态准确。
- [ ] `handbook/upstream-test-ledger.md` 覆盖状态更新到位。

## 暂缓项（发布说明中显式列出）

- 真实 provider 网络调用（Phase 4 opt-in）、真实 HTTP/WebSocket 服务（Phase 7/9/10）、
  cron 表达式调度（Phase 8）、前端资源构建（Phase 10）等按台账记录，发布说明需说明其状态。
