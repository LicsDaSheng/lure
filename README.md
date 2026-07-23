# Lure

用 Rust 完整重新实现 [`nanobot`](https://github.com/) 的项目。

上游 Python 实现是**事实来源**：行为、契约、数据流和运行时边界以上游为准；未验证上游行为之前不自行发明产品行为或协议契约。推进方式采用 TDD——每个能力先补测试再写生产代码，并在 [`handbook/`](handbook/) 中逐阶段记录进度与上游测试映射。

## 当前状态

按阶段推进，每个阶段先形成窄而可运行的纵向切片，再逐步展开。`partial` 表示核心切片已落地、部分外围能力按台账明确暂缓。

| 阶段 | 内容 | 状态 |
|---|---|---|
| Phase 0 | 项目骨架与复刻边界（Cargo workspace） | `done` |
| Phase 1 | 配置 schema、路径解析与读写 | `partial` |
| Phase 2 | Session 存储、缓存与 goal 派生视图 | `partial` |
| Phase 3 | Agent Loop 最小纵向闭环（CLI one-shot） | `partial` |
| Phase 4 | Provider preset 解析与 OpenAI-compatible provider | `partial` |
| Phase 5 | Tool 运行时与 workspace 安全边界 | `partial` |
| Phase 6–11 | Memory / Bus / Automations / API / WebUI / 打包 | `todo` |

详细阶段计划、验收标准与上游测试映射见 [handbook/](handbook/)：
- [phase-roadmap.md](handbook/phase-roadmap.md)：阶段拆分与状态
- [phase-plans.md](handbook/phase-plans.md)：每阶段 plan、验收标准与进度记录
- [phase-execution.md](handbook/phase-execution.md)：固定推进流程与门禁
- [upstream-test-ledger.md](handbook/upstream-test-ledger.md)：上游测试覆盖台账

## 架构

Cargo workspace，以 crate 边界作为主要模块化机制：

```
lure/
├── Cargo.toml                     # workspace 根
└── crates/
    ├── lure-core/                 # 核心领域库
    │   └── src/
    │       ├── config/            # 配置 schema / 路径 / 读写 / preset 解析
    │       ├── session/           # session key / 存储 / 缓存 / goal 派生视图
    │       ├── provider/          # LLM provider 契约 / OpenAI-compatible / registry
    │       ├── agent/             # 最小 loop / runner / context 闭环
    │       ├── security/          # workspace 路径边界
    │       └── tool/              # tool trait / registry / 文件与 shell 工具
    └── lure-cli/                  # 命令行入口（二进制 `lure`）
```

设计原则：优先类型化 API、枚举、trait 与结构化错误；保持解析、领域逻辑、运行时执行、存储与传输之间的清晰边界；避免大型万能 crate。

## 构建与测试

需要 Rust（edition 2021，`rust-version = 1.85`）。

```bash
cargo build
cargo test --all-targets --all-features
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

> 本仓库约定所有 shell 命令通过 `rtk` 执行（在原始命令前加 `rtk`），例如 `rtk cargo test`。

## CLI 用法

Phase 3 已打通 CLI 到 session 的最小闭环。当前使用占位 `EchoProvider`（回显最近一条用户消息），真实 provider 属 Phase 4 后续接入：

```bash
# 一次性对话：跑完 agent loop，保存 user/assistant turn 并输出回复
cargo run --bin lure -- agent -m "你好" --workspace /path/to/workspace

# 无子命令：打印版本
cargo run --bin lure
```

`--workspace` 缺省时使用 `~/.nanobot/workspace`（对齐上游默认路径）。会话以 JSONL 持久化在 `<workspace>/sessions/` 下，文件名为 session key 的 base64url 编码。

## 贡献约定

- 默认使用中文沟通、提交信息、文档与注释。
- 修改前后检查 git 状态，只显式暂存本次需要提交的路径。
- 引入 Rust 代码后，阶段验收默认包含 `cargo fmt`、`cargo clippy`、`cargo test`。
- 详见 [AGENTS.md](AGENTS.md)。
