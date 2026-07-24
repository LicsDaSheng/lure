# Lure

用 Rust 完整重新实现 [`nanobot`](https://github.com/) 的项目。

上游 Python 实现是**事实来源**：行为、契约、数据流和运行时边界以上游为准；未验证上游行为之前不自行发明产品行为或协议契约。推进方式采用 TDD——每个能力先补测试再写生产代码，并在 [`handbook/`](handbook/) 中逐阶段记录进度与上游测试映射。

## 当前状态

按阶段推进，每个阶段先形成窄而可运行的纵向切片，再逐步展开。`partial` 表示核心切片已落地、部分外围能力按台账明确暂缓。

当前 11 个阶段均已形成可运行切片；最近完成了 legacy session/memory 迁移回补：
workspace 内旧版有损 session 文件名会迁移到 canonical base64url 文件，`memory/HISTORY.md`
会一次性迁移为 `memory/history.jsonl` 并备份原文件。下一步优先收敛 Phase 4 的
stateful `ModelRuntimeResolver`，为真实 provider/API/Gateway 接线打稳基础。

| 阶段 | 内容 | 状态 |
|---|---|---|
| Phase 0 | 项目骨架与复刻边界（Cargo workspace） | `done` |
| Phase 1 | 配置 schema、路径解析与读写 | `partial` |
| Phase 2 | Session 存储、缓存、goal 派生视图与 legacy stem 迁移 | `partial` |
| Phase 3 | Agent Loop 最小纵向闭环（CLI one-shot + 基础 interactive） | `partial` |
| Phase 4 | Provider preset 解析与 OpenAI-compatible provider | `partial` |
| Phase 5 | Tool 运行时与 workspace 安全边界 | `partial` |
| Phase 6 | Memory 存储、history、legacy HISTORY.md 迁移与 dream consolidation | `partial` |
| Phase 7 | Bus、channel 契约与最小 gateway | `partial` |
| Phase 8 | Cron store、session 投递、heartbeat 与 trigger | `partial` |
| Phase 9 | OpenAI-compatible API 表面 | `partial` |
| Phase 10 | WebUI 后端服务协议 | `partial` |
| Phase 11 | 打包、Docker 骨架、config 与基础 legacy fixture 迁移 | `partial` |

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
    │       ├── config/            # 配置 schema / 路径 / 读写 / preset / 迁移
    │       ├── session/           # session key / 存储 / 缓存 / goal 派生视图 / legacy 迁移
    │       ├── provider/          # LLM provider 契约 / OpenAI-compatible / registry
    │       ├── agent/             # 最小 loop / runner / context 闭环
    │       ├── security/          # workspace 路径边界
    │       ├── tool/              # tool trait / registry / 文件与 shell 工具
    │       ├── memory/            # 长期记忆 / history / legacy HISTORY.md 迁移 / dream consolidation
    │       ├── bus/               # InboundMessage / OutboundMessage / 消息总线
    │       ├── channel/           # channel 契约
    │       ├── gateway/           # 最小 gateway 编排
    │       ├── cron/              # cron 调度 / 持久化 / session 投递
    │       ├── trigger/           # 本地 trigger at-least-once 队列
    │       ├── api/               # OpenAI-compatible API 表面（传输无关）
    │       └── webui/             # WebUI 后端服务协议（传输无关）
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
> `provider_deepseek_smoke` 是显式 opt-in 真实网络测试；如果环境中设置了
> `DEEPSEEK_API_KEY`，全量测试会尝试出网。做本地无网络验证时可使用：
> `rtk env -u DEEPSEEK_API_KEY cargo test --all-targets --all-features`。

## CLI 用法

Phase 3 已打通 CLI 到 session 的最小闭环。当前使用占位 `EchoProvider`（回显最近一条用户消息），真实 provider 属 Phase 4 后续接入：

```bash
# 一次性对话：跑完 agent loop，保存 user/assistant turn 并输出回复
cargo run --bin lure -- agent -m "你好" --workspace /path/to/workspace

# 交互模式：持续复用同一 session；输入 exit、quit、/exit、/quit 或 :q 退出
cargo run --bin lure -- agent --workspace /path/to/workspace --session cli:direct

# 真实 provider：provider 选择经 ModelRuntimeResolver
#   --model 覆盖默认 preset 的 model；--preset 从 --config 加载的 config 选中命名 preset（二者互斥）
cargo run --bin lure -- agent -m "你好" --model deepseek-v4-pro          # 需 DEEPSEEK_API_KEY
cargo run --bin lure -- agent -m "你好" --config ~/.nanobot/config.json --preset fast

# 无子命令：打印版本
cargo run --bin lure
```

`--workspace` 缺省时使用 `~/.nanobot/workspace`，`--config` 缺省时使用 `~/.nanobot/config.json`（对齐上游默认路径）。会话以 JSONL 持久化在 `<workspace>/sessions/` 下，文件名为 session key 的 base64url 编码。指定 `--model`/`--preset` 时，provider 身份、api_base、model 与生成参数均由 resolver 解析出的不可变 runtime 决定，并挂载工具运行时（tool-call 循环）：workspace 绑定的 `read_file`/`write_file` 默认可用；`exec` shell 工具需在 config 中显式开启并配置 allow 模式：

```jsonc
{
  "tools": {
    "exec": { "enabled": true, "allow": ["^ls\\b", "^cat\\b"], "deny": [] }
  }
}
```

CLI 默认挂载 workspace 绑定的长期记忆：每轮把 `MEMORY.md` 的记忆块注入上下文，并把 user/assistant 内容记入 `history.jsonl`（供后续 dream consolidation）。

## 贡献约定

- 默认使用中文沟通、提交信息、文档与注释。
- 修改前后检查 git 状态，只显式暂存本次需要提交的路径。
- 引入 Rust 代码后，阶段验收默认包含 `cargo fmt`、`cargo clippy`、`cargo test`。
- 详见 [AGENTS.md](AGENTS.md)。
