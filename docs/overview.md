# Lure 工程结构概览

## 1. 项目定位

Lure 是已安装 Pi 的原生桌面 GUI 外壳。桌面端负责进程与会话生命周期、RPC 转发、执行过程展示、文件选择、扩展 UI 请求和本地界面偏好；模型、认证、Agent 推理、工具、命令、会话持久化与上下文管理均由 Pi 提供。

Lure 与 Pi 的唯一集成边界是：在用户选择的工作目录中启动 `pi --mode rpc`，并通过子进程的 stdin/stdout 交换 JSONL 消息。

首期技术栈：

- 桌面框架：Tauri 2
- 前端：React 19 + TypeScript + Vite
- UI：AI Elements + shadcn/ui + Tailwind CSS 4
- Rust：Cargo workspace（edition 2024）
- 包管理器：pnpm
- 初始平台：macOS，工程结构预留其他桌面平台适配空间

## 2. 目录结构

```text
lure/
├── Cargo.toml                 # Rust workspace、公共依赖、lint 与构建配置
├── package.json               # 前端依赖及开发、测试、构建脚本
├── pnpm-lock.yaml             # 前端依赖锁文件
├── components.json            # shadcn/ui 组件生成配置
├── index.html                 # Vite HTML 入口
├── vite.config.ts             # Vite、Vitest 与 Tauri 开发服务器配置
├── tsconfig.json              # 前端 TypeScript 配置
├── tsconfig.node.json         # 构建工具 TypeScript 配置
├── crates/
│   ├── lure-core/             # 与 GUI/进程实现无关的领域模型和应用用例
│   │   └── src/lib.rs
│   └── lure-rpc/              # Pi RPC 子进程、JSONL 编解码与协议适配
│       └── src/lib.rs
├── src/                       # React 前端
│   ├── App.tsx                # 当前应用壳与前端组合入口
│   ├── App.test.tsx           # 前端可观察行为测试
│   ├── components/
│   │   ├── ai-elements/       # 流式对话、消息、推理与工具展示组件
│   │   └── ui/                # 纳入源码管理的 shadcn/ui 基础组件
│   ├── lib/utils.ts           # className 合并工具
│   ├── index.css              # Tailwind CSS 与设计令牌
│   ├── main.tsx               # React 挂载入口
│   └── test/setup.ts          # Vitest 测试环境初始化
├── src-tauri/                 # Tauri 桌面应用 crate
│   ├── build.rs               # Tauri 构建脚本
│   ├── Cargo.toml             # 桌面入口依赖
│   ├── capabilities/          # Tauri 窗口权限声明
│   ├── icons/                 # 应用打包图标
│   ├── src/
│   │   ├── lib.rs             # Tauri Builder 组合根
│   │   └── main.rs            # 桌面进程入口
│   └── tauri.conf.json        # 窗口、开发服务与打包配置
├── public/                    # 直接复制到前端产物的静态资源
└── docs/
    └── overview.md            # 本文档
```

## 3. Rust crate 职责

### `lure-core`

承载稳定的业务概念和应用用例，例如会话生命周期、运行状态、工作目录与用户意图。该 crate 不依赖 Tauri、React 或具体子进程实现，便于独立测试。

### `lure-rpc`

封装 Pi RPC 集成：

- 检测并启动 `pi --mode rpc`
- 管理 stdin/stdout JSONL 通道和子进程退出
- 定义请求、响应、事件及协议错误类型
- 将协议消息转换为 `lure-core` 可消费的边界类型

该 crate 不通过 PTY 解析 Pi TUI，也不实现 Pi Agent 的任何能力。

### `lure-desktop`（`src-tauri`）

作为系统组合根，负责装配 Tauri 插件、应用用例与平台能力，并向 React 前端暴露窄而稳定的命令/事件接口。业务规则应下沉到 `lure-core`，Pi 协议细节应留在 `lure-rpc`。

依赖方向保持为：

```text
React UI ⇄ Tauri command/event ⇄ lure-desktop
                                  ├── lure-core
                                  └── lure-rpc ── JSONL ── pi --mode rpc
```

`lure-core` 位于依赖关系中心，不反向依赖 `lure-rpc` 或 `lure-desktop`。

## 4. 前端组织约定

随着功能增加，`src/` 按产品能力拆分，而不是按通用技术层堆叠：

```text
src/
├── app/                       # 路由、全局 Provider、窗口级状态
├── features/
│   ├── sessions/              # 会话选择与生命周期界面
│   ├── conversation/          # 消息与流式更新展示
│   ├── execution/             # 工具调用和运行状态展示
│   ├── models/                # 模型与 thinking level 选择
│   └── extension-ui/          # extension_ui_request 原生交互映射
├── components/                # 跨 feature 复用的无业务组件
└── lib/                       # Tauri IPC 客户端、通用类型与小型工具
```

Feature 内聚自己的组件、状态与测试；只有形成稳定复用需求后才上移到 `components/` 或 `lib/`。通用界面原语优先通过 shadcn/ui CLI 写入 `components/ui/`，样式使用 Tailwind CSS 和 `src/index.css` 中的语义设计令牌。

流式对话界面统一采用 AI Elements。`components/ai-elements/` 当前包含会话滚动、消息与 Markdown、提示输入、推理过程、来源和工具调用组件；组件源码纳入项目维护，并通过 Streamdown 渲染流式 Markdown。AI Elements 只承担展示与交互，消息事实来源和运行控制仍由 Pi RPC 适配层提供。

```bash
# 按需引入 shadcn/ui 组件
pnpm dlx shadcn@latest add button

# 按需引入 AI Elements 组件
pnpm dlx ai-elements@latest add conversation
```

## 5. 开发命令

```bash
# 安装前端依赖
pnpm install

# 浏览器中开发前端
pnpm dev

# 启动 Tauri 桌面开发应用
pnpm tauri dev

# 前端测试与类型检查
pnpm test
pnpm check

# 前端生产构建
pnpm build

# Rust 格式、静态检查与测试
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## 6. 配置与生成文件

- 前端依赖版本由 `pnpm-lock.yaml` 固定。
- Rust 依赖版本由根目录 `Cargo.lock` 固定。
- Cargo 构建产物写入根目录 `target/`。
- Tauri 自动生成的 capability schema 位于 `src-tauri/gen/schemas/`，不提交版本库。
- `src-tauri/tauri.conf.json` 中的 CSP 在接入真实 IPC 和资源协议时应按实际资源来源收紧。

## 7. 后续扩展原则

1. 先在 `lure-core` 定义可测试的领域行为，再由 `lure-rpc` 和 `lure-desktop` 提供外部实现。
2. Tauri 命令只承担输入校验、用例调用和错误映射，避免直接堆叠业务逻辑。
3. 前端以 Pi 返回的 RPC 数据为事实来源，不在桌面端复制模型、命令或会话规则。
4. 新增 Tauri 权限按最小权限原则加入 `capabilities/`。
5. 每个新增行为先添加对应层级的测试，再完成实现。
