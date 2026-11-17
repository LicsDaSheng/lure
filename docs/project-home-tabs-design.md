# Lure 首页项目列表 + 项目 Tab + 多后台 Pi 进程 实现设计方案

本文档描述「首页项目卡片 + 终端 Tab 多会话」功能的实现设计。仅做设计，不含代码改动。
依据：根 `AGENTS.md` 产品边界、`docs/ui-design.md` 现行 UI 规约、探索报告及当前源码（引用格式为 `文件:行号`）。

## 0. 需求与边界确认

需求逐条对应：

1. 启动后进入 Lure 首页，读取 `$HOME/.pi/projects.yaml`，以卡片展示项目列表。
2. 创建项目：选择本地文件夹，默认名为文件夹最后一层，可改为中文，写入 projects.yaml；yaml 保存项目名称（未设置则为空）与项目文件地址。
3. 点击卡片用项目地址进入终端页；桌面上方以 tab 展示项目名称；tab 条有新增按钮，弹出项目卡片列表，可选已有项目开新 tab，也可创建新项目（创建后立即开 tab 并建立 Pi 连接）。
   - **已确认决策 D2**：同一项目（按 canonical 路径）只允许一个 tab，重复打开时切到已有 tab，不创建第二个 Pi 进程。
4. Tab 切换不停止后台 Pi 进程。

边界（不可逾越）：

- 不解析或重建 Pi 语义；PTY 输出仍以原始字节进入 Alacritty 内核（`crates/lure-desktop/src/terminal.rs:161` 的 `TerminalGrid`），GPUI 按网格绘制。
- 一个 tab 对应一个在所选目录运行的 `pi` 交互子进程；不与同一会话并发启动 RPC 子进程，不读写 Pi 会话内部状态。
- `lure-pty`  crate 零改动：其 `PtyProcess`（`crates/lure-pty/src/lib.rs:139`）无全局状态、实例间独立，reader/waiter 线程按实例创建（`crates/lure-pty/src/lib.rs:200`、`crates/lure-pty/src/lib.rs:205`），天然支持多实例并存。
- Lure 只在 `$HOME/.pi/` 下新增并独占 `projects.yaml` 一个文件；不读取、不修改 `.pi` 目录下 Pi 自有的任何配置。

---

## a) 数据模型与 projects.yaml 读写

### 路径与文件归属

- 固定路径：`$HOME/.pi/projects.yaml`。`$HOME` 经 `std::env::var_os("HOME")` 获取，与现有 `default_workspace`（`crates/lure-desktop/src/main.rs:1093`）同一方式。
- Lure 是该文件的唯一写方与读方；**不读取、不创建、不修改 `.pi` 目录下的任何其他内容**。
- **`.pi` 目录不存在时不由 Lure 代建**：读取时按「Pi 未初始化、项目列表为空」处理并在首页给出低干扰提示；创建项目写盘时若目录仍不存在，展示「未检测到 Pi 配置目录，请先完成 Pi 安装与初始化」并中止写盘。
- **已确认决策 D1**：固定使用 `$HOME/.pi/projects.yaml`。该落点与根 `AGENTS.md` 的职责清单存在张力，因此必须落实边界缓解措施：「Lure 独占 `projects.yaml` 单一文件 + 归属契约写入 `docs/ui-design.md` 边界段（j 节）+ 不代建 `.pi` 目录」。

### Schema

```yaml
# $HOME/.pi/projects.yaml
projects:
  - name: "我的项目"            # 项目名称；未设置时为空字符串 ""
    path: "/Users/scott/work/demo"  # 项目文件夹的规范化绝对路径
  - name: ""
    path: "/Users/scott/work/another"
```

Rust 模型（新增 `crates/lure-desktop/src/projects.rs`，挂在 lib 上以便脱离 GPUI 做单元测试）：

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEntry {
    #[serde(default)]
    pub name: String,      // 空字符串 = 未设置名称
    pub path: PathBuf,     // 写入前经 canonicalize 的绝对路径
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProjectList {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
}
```

### 空名称语义

- `name == ""` 表示用户未设置名称；展示名（卡片标题、tab 标题）回退为 `path` 最后一层目录名（`Path::file_name`，取不到时用完整路径字符串）。
- 回退只发生在展示层，**不反写回 yaml**：yaml 中保持空字符串，用户后续改名才落盘。
- 中文名称直接以 UTF-8 存储；名称编辑走与终端一致的输入法链路（见 f 节），允许任意 Unicode。

### 读写时机

| 时机 | 操作 |
| --- | --- |
| 应用启动（进入首页前） | 读取并解析 projects.yaml；解析结果存入 App 壳的内存模型 |
| 返回首页时 | 使用内存模型渲染；不重复读盘（单实例应用，无外部并发写场景，见下）。注：tab 存活期间不提供返回首页的入口，**关闭最后一个 tab 是回首页的唯一路径**（见 d 节），因此「返回首页」总是伴随会话状态变化、内存模型已是最新 |
| 创建项目确认时 | 内存模型追加条目 → 立即整体写盘 |
| 重命名/删除（若后续版本加入） | 同样「改内存 → 整体写盘」 |

每次写盘都是**全量序列化整个 `ProjectList`**，文件小（条目级），无需增量更新。

### 并发与损坏容错

- **并发**：应用为单窗口单实例；所有读写发生在 UI 线程（创建项目确认的处理闭包内），无跨线程写。多实例同时运行的场景不做互斥（后写覆盖先写，可接受，记入残余风险）。
- **写盘原子性**：先写 `projects.yaml.tmp` 并 `fsync`，再 `std::fs::rename` 覆盖正式文件，避免中途崩溃产生半截文件。
- **损坏容错**：
  - 文件不存在 → 视为空列表（首次使用场景）。
  - YAML 解析失败 → 视为空列表进入首页并展示一条低干扰错误提示（样式沿用 `status_panel` 的 Gruvbox 卡片语言，`crates/lure-desktop/src/main.rs:478`）；**不立即覆盖原文件**；待用户首次成功创建项目写盘前，把损坏原文件复制为 `projects.yaml.bak-<时间戳>` 再覆盖。
  - 单条目缺字段：`name` 缺省为空串（`serde(default)`）；`path` 缺失或指向不存在的目录 → 该条目在加载时丢弃（记录到内存中的告警列表，卡片不渲染）。
  - 路径去重：加载时按规范化路径去重，保留先出现者。
- **路径规范化**：创建项目时用现有 `canonical_directory`（`crates/lure-desktop/src/main.rs:1101`）校验「存在且为目录」并取 canonical 路径后再入库；`lure-pty` spawn 时还会再 canonicalize 一次（`crates/lure-pty/src/lib.rs` spawn 内部），两层一致。

### 依赖论证（serde / serde_yaml）

当前 workspace 无任何序列化依赖（根 `Cargo.toml:11-17`），需新增：

- `serde`（`derive` feature）：Rust 结构体序列化事实标准，MIT/Apache-2.0，维护活跃。
- `serde_yaml`：需求明确要求 YAML 格式，且该文件位于 `.pi` 目录、可能被用户手工编辑，YAML 的可读性优于 JSON/TOML。`serde_yaml 0.9` 当前为维护归档状态（API 冻结、无已知安全公告），对本项目「扁平字符串列表」这一极简 schema 完全够用；若评审不接受归档依赖，备选为 `serde_yml`（fork，供应链风险更高，**不推荐**）或手写最小 YAML 读写（转义规则易错，**不推荐**）。两者共同满足根 `AGENTS.md`「新增依赖前确认必要性、维护状态、许可证」的要求。
- 不加 `directories`/`dirs` 类路径库：需求固定 `$HOME/.pi`，现有代码已直接用 `HOME` 环境变量（`crates/lure-desktop/src/main.rs:1094`）。

---

## b) 架构拆分：App 壳 + 每 tab 会话实体

现状：`crates/lure-desktop/src/main.rs` 为 1134 行单体，`LureApp`（`crates/lure-desktop/src/main.rs:60-76`）把「窗口壳」与「单会话」焊死，`Render`（`crates/lure-desktop/src/main.rs:651`）直接产出「标题栏 + 终端区」，无页面概念。

### 目标模块划分（二进制 crate 内部分模块，保持单窗口）

```
crates/lure-desktop/src/
├── main.rs        # 仅入口：窗口创建（现 main() 主体，main.rs:1111-1134）、挂载 LureApp 壳
├── app.rs         # App 壳 LureApp：路由、tab 列表、消息路由、标题栏/tab 条渲染
├── session.rs     # TerminalSession：每 tab 一个会话实体（现有 LureApp 字段集整体下沉）
├── home.rs        # 首页：项目卡片列表、创建项目流程
└── (lib 侧) crates/lure-desktop/src/projects.rs  # 数据模型与读写，lib.rs 增加 pub mod projects
```

### 路由模型

```rust
enum Route {
    Home,                       // 首页：项目卡片列表
    Terminal,                   // 终端页：展示 active 会话
}

struct LureApp {
    route: Route,
    sessions: Vec<Entity<TerminalSession>>,  // 打开中的 tab，顺序即 tab 条顺序
    active: Option<usize>,                   // 活动 tab 下标；None 时 route 必为 Home
    next_session_id: u64,
    projects: ProjectList,                   // 内存项目模型
    projects_error: Option<String>,          // 加载/写盘失败的低干扰提示
    messages: Sender<AppMessage>,            // 全局唯一消息通道（见下）
}
```

### 会话实体（现有字段集原样下沉）

`TerminalSession` 逐字段继承现 `LureApp`（`crates/lure-desktop/src/main.rs:60-76`）：

| 字段（现状行号） | 下沉后归属 | 说明 |
| --- | --- | --- |
| `focus: FocusHandle`（:61） | TerminalSession | 每会话一个焦点句柄，见 f 节 |
| `directory: PathBuf`（:62） | TerminalSession | 即项目 path；来源从「选择目录」改为「项目条目」 |
| `phase / error / exit_code`（:63-65） | TerminalSession | 每 tab 独立状态机，`Phase` 枚举（:29-34）不变 |
| `terminal: TerminalGrid`（:66） | TerminalSession | 每 tab 一个 Alacritty Term 常驻内存 |
| `process: Option<PtyProcess>`（:67） | TerminalSession | 每 tab 独立 Pi 子进程 |
| `dimensions`（:68） | TerminalSession | 各会话独立记录；切换 tab 时按窗口尺寸重新 `sync_size` |
| `generation: Arc<AtomicU64>`（:69） | TerminalSession | 防串流语义不变，见下 |
| `messages: Sender<AppMessage>`（:70） | TerminalSession 持有壳通道的 clone | 发送端，通道本体在壳 |
| `env_resolver: Arc<SpawnEnvResolver>`（:71） | TerminalSession | 每会话自建；`SpawnEnvResolver::new()`（`crates/lure-pty/src/env.rs:32`）无共享状态 |
| `marked_text`（:72） | TerminalSession | 每会话独立预编辑状态 |
| `scroll_rows_accumulator / selection / selection_dragged`（:73-75） | TerminalSession | 每会话独立滚动与选择状态 |

新增字段：`id: SessionId`（u64）、`name: String`（展示名，创建 tab 时按 a 节规则解析并缓存）。

### AppMessage 增加会话 id 路由

现 `AppMessage`（`crates/lure-desktop/src/main.rs:36-58`）全部变体仅按 `generation` 过滤。改造：

```rust
enum AppMessage {
    Started    { session: SessionId, generation: u64, process: PtyProcess, directory: PathBuf },
    StartFailed{ session: SessionId, generation: u64, message: String },
    Output     { session: SessionId, generation: u64, data: Vec<u8> },
    Exited     { session: SessionId, generation: u64, code: u32 },
    ReadFailed { session: SessionId, generation: u64, message: String },
}
```

- **通道**：保留一对 `async_channel::unbounded`，但创建者从会话上移到 App 壳；壳的 `receive_messages`（现 `crates/lure-desktop/src/main.rs:106-118`）循环收到消息后按 `session` 字段查 `sessions`，再 `entity.update(cx, |s, cx| s.handle_message(...))` 投递。找不到 session（tab 已关闭）→ 丢弃；若为 `Started`，需先 `process.stop()` 回收孤儿进程。
- **generation 防串流迁移**：语义完全保留。generation 仍是**会话内**重启序号：`TerminalSession::start`（现 `crates/lure-desktop/src/main.rs:186-239` 原样下沉）内 `fetch_add`，`is_current`（现 :182）过滤。`session id + generation` 构成二级路由：tab 关闭后迟到的消息被 session id 丢弃；同 tab 重启后旧进程的迟到消息被 generation 丢弃。`forward_pty_events`（现 `crates/lure-desktop/src/main.rs:1059`）签名加 `session: SessionId`，在 start 线程内与 generation 一起随消息发出。
- **派发差异**：非活动会话的 `Output` 消息照常 `terminal.process(&data)` 并 `cx.notify()`，后台 tab 的终端模型持续跟进输出，切回时即为最新画面。
- **活动会话的重绘链路（必须）**：渲染由壳的 `Render` 完成（绘制活动会话的网格快照），而现状的活输出刷新机制是 `handle_message` 里对**渲染实体自身**的 `cx.notify()`（`crates/lure-desktop/src/main.rs:120-180` 各分支）。拆分后会话 entity 的 `notify` 不会触发壳重绘，因此壳在创建每个会话时注册观察：`cx.observe(&session, |_shell, _, cx| cx.notify()).detach()`——任一会话状态变化（含活动会话收到 `Output`）都使壳标脏重绘；非活动会话的 notify 同样触发壳重绘但绘制内容不变，成本仅为一次同帧重绘，可接受。备选方案：`TerminalSession` 自实现 `Render` 并由壳作为子实体挂载；本设计采用 observe，因为标题栏/tab 条本就由壳渲染、壳已持有会话列表。

### 生命周期

- 打开 tab（首页点卡片 / tab 条新增）→ `cx.new(|cx| TerminalSession::new(...))` → push 进 `sessions` → `active = Some(last)` → `route = Terminal` → 调 `session.start(cx)` 建立 Pi 连接。
- 关闭 tab → 从 `sessions` 移除该 entity；GPUI 释放 entity 时触发 `TerminalSession::drop`（现 `crates/lure-desktop/src/main.rs:644-649` 下沉），停止进程并 bump generation。
- 关闭窗口 → 壳 entity 释放 → `sessions` 内所有 entity 级联释放 → 每个会话各自 `Drop` 回收进程、reader/waiter 线程随进程退出而结束（`crates/lure-pty/src/lib.rs:205` 的 waiter 在进程退出后 join reader 并发 `Exited`，通道随壳销毁，`send_blocking` 失败即退出）。

---

## c) 首页页面

### 布局

```
┌──────────── 标题栏（48px，含窗口控制）────────────┐
│ ●●●   Lure                                       │
├───────────────────────────────────────────────────┤
│  项目                              [+ 新建项目]  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐         │
│  │ 卡片     │ │ 卡片     │ │ 卡片     │  …      │
│  │ 名称     │ │          │ │          │         │
│  │ 路径     │ │          │ │          │         │
│  └──────────┘ └──────────┘ └──────────┘         │
└───────────────────────────────────────────────────┘
```

- 视觉沿用 Gruvbox Light：背景 `BACKGROUND`、卡片用 `TITLEBAR` 底色 + `BORDER` 细边框 + 12px 圆角（与状态面板卡片语言一致，`crates/lure-desktop/src/main.rs:495-510`），主题常量来自 `crates/lure-desktop/src/theme.rs`。
- 卡片内容：第一行展示名（name 为空时回退目录 basename，见 a 节），第二行完整路径（`MUTED` 色、超长 `text_ellipsis`，复用现标题栏路径截断手法，`crates/lure-desktop/src/main.rs:719-728`）。
- 空列表时展示引导文案（「还没有项目，点击右上角新建项目」）+ 居中的新建项目卡片。
- 若 `.pi` 目录不存在（a 节），在空态文案处附加「未检测到 Pi 配置目录」提示。
- 加载/写盘失败（a 节）以页面顶部低干扰横条展示错误文字，不弹模态。

### 创建项目流程（首页入口）

1. 点击「新建项目」→ 系统原生目录选择器：复用现 `choose_directory` 的 `cx.prompt_for_paths(PathPromptOptions{ files:false, directories:true, multiple:false })`（`crates/lure-desktop/src/main.rs:259-277`）。**取消选择不改变任何状态**（沿用现行语义）。
2. 选中目录 → 经 `canonical_directory`（`crates/lure-desktop/src/main.rs:1101`）校验 → 进入「确认名称」内联表单（不弹新窗口）：卡片位展示路径 + 名称输入框。
3. 名称输入框默认填入目录最后一层名称；用户可清空（空 = 未设置，见 a 节）或改为中文（输入框实现见 f 节的 IME 方案）。
4. 「创建」→ 追加 `ProjectEntry` → 全量写盘（含 .bak 容错，a 节）→ 关闭表单，新卡片出现在列表中（首页创建**不自动进入终端页**，与需求 2 一致；需求 3 的「创建后立即开 tab」只适用于 tab 条弹层入口，见 d 节）。
5. 「取消」→ 放弃，不写盘。
6. 目录已存在（同 canonical 路径）→ 表单内联报错「该项目已存在」，不允许重复创建。

### 点击卡片进入终端页

- 若该项目已有打开的 tab（按 canonical path 匹配 `sessions`）→ 直接切换 `active` 到该 tab，`route = Terminal`，**不新建进程**。
- 否则按 b 节生命周期新建 `TerminalSession` 并 `start`。
- 进入终端页前从 yaml 读的 `path` 需二次校验目录仍存在；已被外部删除 → 首页卡片旁 inline 报错，不创建 tab（不污染会话列表）。

---

## d) Tab 条 UI 与标题栏 48px 布局的关系

### 决策：tab 条内嵌于现有 48px 标题栏，不新增行

```
┌──────────────────── 标题栏（48px）───────────────────────────┐
│ ●●● │ [项目A] [我的项目×] [长名称项目…×] [+] │   ↻  ■       │
└──────────────────────────────────────────────────────────────┘
```

- `TITLEBAR_HEIGHT`（`crates/lure-desktop/src/terminal.rs:14`）保持 48px 不变，终端区垂直偏移完全不动（见 g 节）。这是选择内嵌方案而非「标题栏 + 独立 tab 行」的核心理由：避免改动 `terminal_cell_at` 与 `terminal_size_for_viewport` 的几何假设。
- 标题栏三段重排：
  - **左**：窗口控制不变（`window_controls`，`crates/lure-desktop/src/main.rs:996/1001`；macOS 72px 占位）。
  - **中**：tab 条（`flex_1` + `min_w_0`，横向排布；tab 过多时截断单个 tab 标题，必要时允许横向滚动）。tab 宽度自适应内容、设最大宽（如 180px），标题超长 `text_ellipsis`。
  - **右**：窗口级/会话级操作。保留「重新启动 Pi」「停止 Pi」两个按钮（现 `crates/lure-desktop/src/main.rs:740-753`），语义改为**作用于活动 tab 的会话**；「选择目录」按钮（现 :733-739）**移除**——目录选择迁移到创建项目流程，避免与「项目地址即工作目录」的模型冲突。**首页路由下（`active = None`）不渲染这两个按钮**（无会话可操作，渲染出来只能是空操作）。
- 标题栏空白区保留窗口拖动与双击缩放：`on_mouse_down` 的 `titlebar_double_click` / `start_window_move` 处理（现 `crates/lure-desktop/src/main.rs:708-716`，`titlebar_double_click` 在 :712）保留在标题栏容器与各 tab 之间的空隙元素上；tab 本体与按钮不传播拖动。
- 首页路由下标题栏中段不渲染 tab 条，渲染「Lure」标识；终端页路由渲染 tab 条。
- 窗口标题（`TitlebarOptions` 的 title，`crates/lure-desktop/src/main.rs:1121`）在终端页更新为「项目名 — Lure」，首页为「Lure」。**注**：运行时改窗口标题依赖 gpui-ce 是否暴露相应 API（`TitlebarOptions` 仅在 `open_window` 时设置），未验证，记入残余风险；不可行时回退为静态标题「Lure」。

### Tab 本体

- 内容：展示名（a 节规则）；活动 tab 高亮（`TITLEBAR` 反色/底部 2px `ACCENT` 指示条），非活动 tab 使用 `MUTED` 前景。
- 状态点：会话 `Phase` 非 Running 时在 tab 名左侧显示小圆点（Starting 黄色系 / Exited·Error 红灰色系，颜色取 `theme.rs` 现有常量，不引入新色）。
- 关闭按钮「×」：hover 显示；点击 → b 节的关闭流程（停进程、回收、释放 entity）。关闭活动 tab → 切到相邻 tab；关闭最后一个 tab → `route = Home`、`active = None`。
- 点击 tab → 仅切换 `active`（e 节：不停进程）。

### 新增按钮与项目选择弹层

- tab 条右端固定「+」按钮（样式复用 `title_button`，`crates/lure-desktop/src/main.rs:449-476`）。
- 点击「+」→ 在按钮下方弹出浮层（absolute 定位、Gruvbox 卡片、细边框、圆角，与状态面板同一视觉语言）：
  - 列表项 = projects.yaml 中的项目卡片精简版（名称 + 路径）；已打开的项目标注「已打开」，点击执行「切到已有 tab」而非开重复 tab（设计决策 D2，见 0 节与「评审处置」）。
  - 列表底部固定「新建项目…」项 → 进入与 c 节相同的目录选择 + 名称确认流程；**确认后立即创建 tab、切到该 tab 并 `start` 建立 Pi 连接**（需求 3）。
  - 点击浮层外部或 Esc 关闭浮层；浮层不抢占终端焦点语义，关闭后焦点回到活动会话（f 节）。

---

## e) 多进程并存与切换语义

- **每会话完全独立**：`PtyProcess`（含 reader/waiter 线程）、`TerminalGrid`、`generation`、消息发送端全部按 tab 持有（b 节字段表）。`lure-pty` 无共享状态，多实例并存已由其实现保证（`crates/lure-pty/src/lib.rs:139-205`）。
- **切换 tab 只做三件事**：
  1. 更新壳的 `active` 下标；
  2. 渲染层改绘活动会话的 `TerminalGrid` 快照（`lines()`，`crates/lure-desktop/src/terminal.rs:406`）；非活动会话不渲染但继续 `process` 输出（b 节），活动会话的输出重绘由壳的 `cx.observe` 链路保证（b 节「活动会话的重绘链路」）；
  3. 输入路由改指活动会话（f 节）：`track_focus` 换绑活动会话的 `FocusHandle` 并 `focus(window, cx)`。
- **明确不做**：不 `stop`、不 bump 后台会话 generation、不暂停其消息消费。后台会话 `Exited` 到达时只更新自己的 `phase`（tab 上显示状态点，d 节），不弹全局面板。
- **尺寸同步**：窗口 resize 时壳遍历所有会话执行 `sync_size`（现 `crates/lure-desktop/src/main.rs:436-447` 下沉后由壳广播），保证每个 PTY 的行列数始终跟随窗口；非活动会话的 `TerminalGrid.resize` 同样执行，切回时画面不错位。
- **窗口关闭**：壳释放 → 所有会话 entity 级联 `Drop`（现 `crates/lure-desktop/src/main.rs:644-649` 下沉）→ 每个 `PtyProcess::stop`（`crates/lure-pty/src/lib.rs:281`）+ Drop 自动 kill（`crates/lure-pty/src/lib.rs` 的 `impl Drop for PtyProcess`）。满足需求 4 的互补面：切换不杀进程，关窗全回收。
- **资源量级**：每个后台会话的内存占用 ≈ 一个 Alacritty Term（含 scrollback，上限 10,000 行，现行规约）。不设 tab 数量硬上限；实施时如实测 GPUI 重绘或内存异常再评估（记入残余风险）。

---

## f) 输入路由：绑定活动会话

现状所有输入状态与处理都挂在单例 `LureApp` 上；拆分后按下表迁移：

| 能力（现状行号） | 迁移方案 |
| --- | --- |
| `FocusHandle`（`main.rs:61`、`new` 中 :80-81） | 每 `TerminalSession` 一个 `cx.focus_handle()`。壳 `Render` 根节点 `track_focus` 绑定**活动会话**的句柄；切 tab 时调用活动句柄的 `focus(window, cx)`。首页表单输入框用独立句柄（首页与会话互不干扰）。 |
| `EntityInputHandler`（`main.rs:550-642`） | 改为 `impl EntityInputHandler for TerminalSession`。`marked_text`（:72）、`marked_text_range`（:573）、`replace_text_in_range`（:588）、`replace_and_mark_text_in_range`（:602）整体下沉；`bounds_for_range`（:615）继续用**本会话** `terminal.cursor()`（`crates/lure-desktop/src/terminal.rs:352`）锚定候选窗。canvas 的 `window.handle_input(&input_focus, ElementInputHandler::new(bounds, app), cx)`（`main.rs:863-865`）中 `app` 改为活动会话 entity。 |
| IME 与切 tab 的交互 | 平台 IME 组合态的取消依赖焦点切换机制（切 tab 时 `focus` 换绑会注销原输入上下文）；代码侧同时对原活动会话调用 `unmark_text`（:582 区域）清空本地 `marked_text` 作为兜底——注意 `unmark_text` 是 `EntityInputHandler` 回调，手动调用只清本地状态、不取消平台组合态，**须在阶段 4 实测验证预编辑不串 tab**（记入残余风险）。确认中的 IME 文本不会被提前写入 PTY 的现行保证（预编辑只进 `marked_text`，确认才 `write`）不变。 |
| 首页名称输入框的 IME | 复用同一模式：为首页表单实现一个轻量 `EntityInputHandler`（或把表单做成独立小 entity），中文预编辑与候选窗锚定行为与终端一致；不引入新文本组件依赖。 |
| 键盘 `handle_key`（`main.rs:279-300`） | 下沉到 `TerminalSession`；根节点 `on_key_down`（`main.rs:678`）的 listener 改为壳转发给活动会话。`Cmd/Ctrl+V` 分支（:281-289）随之下沉：读剪贴板后 `write` 进**活动会话**的 `process`。 |
| 选择/复制（`start_selection` :330、`update_selection` :348、`finish_selection` :358） | 下沉到会话；鼠标事件 listener 只挂在终端页的活动会话渲染区。`finish_selection` 内的 `cx.write_to_clipboard`（:370-373）语义不变：松开后自动复制、单击不覆盖剪贴板。 |
| 滚轮 `handle_scroll`（`main.rs:379-421`） | 下沉到会话；`scroll_rows_accumulator` 为每会话状态，切 tab 后互不累加。alternate screen 回退方向键、鼠标协议转发等逻辑（`terminal.rs:326` 的 `mouse_wheel_sequence`）原样。 |
| `Back to bottom` 按钮（`main.rs:966` 区域） | 每会话独立：按活动会话的 `scrollback()`（`terminal.rs:244`）显隐，点击只滚活动会话。 |
| 状态面板 `status_panel`（`main.rs:478-548`） | 改为按活动会话的 `Phase` 渲染，仍只覆盖终端区、不遮标题栏/tab 条；「重新启动」作用于活动会话。 |

原则：**键盘、IME、剪贴板、选择、滚轮任一时刻只服务活动 tab**；后台 tab 不接收任何输入事件。

---

## g) 布局常量与坐标偏移

- `TITLEBAR_HEIGHT = 48.0`（`crates/lure-desktop/src/terminal.rs:14`，常量块 :12-15）**保持不变**（d 节决策：tab 条内嵌标题栏）。
- 因此：
  - `terminal_cell_at`（`crates/lure-desktop/src/main.rs:318-328`，其中 `:323` 使用 `TITLEBAR_HEIGHT`）的垂直偏移公式不变，仅需随「终端区只在终端页渲染」而保持调用时机正确（首页无终端区，不调用）。
  - `terminal_size_for_viewport`（`crates/lure-desktop/src/terminal.rs:483`）的行数计算不变。
- 需要调整的只有「归属」而非「数值」：这两个函数从单例上下文迁到活动会话上下文后，偏移仍以窗口原点为基准（标题栏 48px + 终端区内边距），多 tab 不引入额外垂直结构。
- 若实施阶段评审推翻内嵌方案、改为独立 36px tab 行，则需同步修改：`TITLEBAR_HEIGHT` 语义拆为 `TITLEBAR_HEIGHT`(48) + `TABSTRIP_HEIGHT`(36)，并把 `terminal_cell_at`（`main.rs:323`）与 `terminal_size_for_viewport`（`terminal.rs:483`，行数计算在函数体内）的偏移改为两者之和。本设计**不采用**该方案，仅记录影响面。

---

## h) 启动流程变化

现状（`crates/lure-desktop/src/main.rs`）：

- `LureApp::new` 中 `default_workspace()`（:82 → :1093-1099）**会创建 `$HOME/lure` 目录**并以其为工作目录。
- `main()` 在 `open_window` 后立即 `app.start(cx)`（:1131）启动 Pi。

改造：

1. **删除 `default_workspace` 函数**（:1093-1099）：不再自动创建 `$HOME/lure`；`canonical_directory`（:1101-1109）保留，迁移后供创建项目流程复用（a/c 节）。
2. **`main()` 不再调用 `start`**（:1131 删除）：窗口创建后壳加载 projects.yaml（a 节）→ `route = Home` → 渲染首页。
3. 启动后**零进程**：直到用户点击卡片或从 tab 条新增项目才创建首个 `TerminalSession` 并 `start`。
4. 首页路由下 `sync_size` 无需执行；首次进入终端页时按当前窗口尺寸初始化会话 `dimensions` 并 `process.resize`（沿用 `Started` 消息处理中的 resize，现 :123-130）。

---

## i) `tests/source_audit.rs` 影响与测试计划

`tests/source_audit.rs` 对 `main.rs` 做字符串级断言，拆文件后必须同步重写断言目标：

| 测试（`crates/lure-desktop/tests/source_audit.rs`，全文 90 行） | 影响 | 处理 |
| --- | --- | --- |
| `terminal_text_uses_an_explicit_canvas_line_height_without_clipping`（:16-31） | canvas 绘制代码迁往 `session.rs` | 读取目标改为 `session.rs`；断言串不变 |
| `terminal_registers_gpui_text_input_and_uses_canvas_shaping`（:33-56） | `impl EntityInputHandler for LureApp` → `for TerminalSession`；`ElementInputHandler::new` / `window.handle_input(` / `.on_scroll_wheel(` / `back-to-bottom` 等均迁往 `session.rs` | 断言目标文件改为 `session.rs`，`for LureApp` 改为 `for TerminalSession` |
| `native_window_remains_manageable_by_the_operating_system`（:57-69） | `open_window`/titlebar/app_id 留在 `main.rs`；`titlebar_double_click`/`minimize_window` 等散布到 `app.rs`（标题栏）与 `main.rs`（window_controls） | 断言按文件归属拆分：窗口选项断言 `main.rs`，标题栏手势断言 `app.rs` |
| 其余三个测试（:4-14、:70-82、:84-90） | 不涉及 | 不变 |

新增测试（按阶段拆分，避免阶段 2 全量落地时失败）：

- **阶段 1**：`projects.rs` 单元测试（lib 内 `#[cfg(test)]`）：schema 序列化/反序列化往返、空 name 语义、缺 `name` 字段兼容、损坏 YAML → 容错路径、路径去重、原子写（tmp+rename）。
- **阶段 2**：`source_audit.rs` 仅按上表改写「断言目标文件归属」（拆文件引起的改动），并新增 `session.rs` 含 `session: SessionId` 路由字段的断言；**不断言** tab 条、`default_workspace` 移除等阶段 3/4 才成立的内容。
- **阶段 3**：新增断言——`main.rs` 不再含 `default_workspace`、不再含启动直启 `app.start(cx)`。
- **阶段 4**：新增断言——`app.rs` 含 tab 条与「+」入口、`cx.observe` 重绘链路存在。
- 手动验收脚本化部分见 k 节。

门禁（根 `AGENTS.md` 要求）：每阶段完成后 `cargo fmt --check`、`cargo clippy --workspace --all-targets`、`cargo test --workspace` 全绿（lint 为 `all/pedantic = warn`、`unsafe_code = forbid`，根 `Cargo.toml:20-27`）。

---

## j) `docs/ui-design.md` 需修订的条款清单

本设计落地前需先修订规约（AGENTS.md 强制要求「界面设计变化必须同步更新规约」）。以下只列条款与新表述方向，不直接改该文件（行号以现行 `docs/ui-design.md` 为准）：

1. **「产品与界面边界」职责清单**（:5-10）→ 补充 Lure 新增职责「项目列表的持久化与展示」，并写明归属契约：「`$HOME/.pi/projects.yaml` 由 Lure 独占创建与读写，Pi 不感知该文件；Lure 不读取、不修改 `.pi` 目录下的任何其他内容，也不代建 `.pi` 目录」。这是承接决策点 D1 的正确位置。
2. **页面布局 ASCII 图**（:15-23）→ 随条款 3 一并重绘：首页（卡片列表）与终端页（标题栏内嵌 tab 条 + 字符网格）两张图。
3. **「应用采用单一终端工作区」**（:25）→ 改为「应用包含首页与终端页两个页面；终端页以 tab 管理多个并行会话，每个 tab 是一个在所选目录运行的 Pi 交互进程」。
4. **标题栏布局条款**（:26 中部/右侧描述）→ 中部改为「tab 条（展示项目名称、活动态高亮、关闭按钮、新增按钮）」，右侧操作改为「作用于活动 tab 的重新启动与停止」；删除「选择目录」按钮的表述，目录选择迁入首页/弹层的创建项目流程。
5. **同条（:26）中的窗口回收语义** → 「最小化保留当前 Pi 进程与终端状态」改为「最小化保留所有 tab 的 Pi 进程与终端状态」；「关闭窗口则回收进程」改为「关闭窗口回收所有 tab 的后台 Pi 进程、PTY reader 与消息任务；关闭单个 tab 只回收该 tab 的进程」。
6. **「工作目录过长时截断显示」**（:27）→ 标题栏中部不再常驻显示工作目录，改为「tab 标题过长时截断显示」；后半句「目录选择使用系统原生文件夹选择器」保留（迁入创建项目流程）。
7. **「选择新目录时先停止当前 Pi，再在新目录启动」**（:42）→ 删除；替换为「打开或新建项目 tab 时在该项目目录启动新 Pi 进程；切换 tab 不停止后台进程」。
8. **「状态面板覆盖终端区域」**（:31）→ 补充「状态面板按活动 tab 的会话状态呈现，不影响其他 tab 的后台运行」。
9. **「启动成功后终端保持焦点；目录选择结束后可继续输入」**（:41）→ 整句改写为「创建/打开 tab 后终端保持焦点；切换 tab 后焦点移动到目标会话；创建项目流程的目录选择结束后焦点回到表单」。
10. **generation 条款**（:45）→ 保留并扩展为「会话 id 加 generation 二级隔离：tab 关闭后丢弃其迟到消息，tab 内重启隔离旧进程输出」。
11. **新增首页条款**：页面布局（卡片列表、新建入口、空态与错误提示）、projects.yaml 的位置与归属（同条款 1 的契约）、空名称回退目录名、卡片点击进入终端页。
12. **「启动中显示低干扰状态」等故障恢复条款**（:49-54）→ 保留，明确其作用域为「每个 tab 各自的生命周期」。

---

## k) 分阶段实施顺序与验收标准

| 阶段 | 内容 | 验收标准 |
| --- | --- | --- |
| 1. 数据层 | 新增 `serde`/`serde_yaml` 依赖（根 `Cargo.toml` workspace 依赖集中声明）；新增 `crates/lure-desktop/src/projects.rs` + 单元测试；`lib.rs` 暴露模块；固定读写 `$HOME/.pi/projects.yaml`，并落实「不代建 `.pi` 目录、不触碰其他 `.pi` 内容」的边界约束 | `cargo test -p lure-desktop` 通过；单元测试覆盖 a 节列出的容错与空名称语义；`cargo clippy` 无警告 |
| 2. 架构拆分（行为不变重构） | `main.rs` 拆为 `app.rs`/`session.rs`；`AppMessage` 加 `session: SessionId`；单 tab 下行为与现状完全一致（启动仍直开终端，本阶段暂保留） | 全部现有功能手工回归（启动、输入、IME、选择复制、滚轮、重启/停止、状态面板）；`source_audit.rs` 按 i 节改写后通过；三门禁绿 |
| 3. 首页 | 路由 `Route` 引入；首页卡片列表 + 创建项目流程（目录选择、默认名、中文改名、写盘）；启动流程改造（h 节：删除 `default_workspace` 与启动直启） | 冷启动进入首页且 `$HOME/lure` 不再被创建；创建项目后 yaml 内容符合 schema（含空名称）；损坏 yaml 容错生效；点击卡片进入终端页且 Pi 在项目目录运行（终端内 `pwd` 验证） |
| 4. Tab 条与多会话 | 标题栏嵌入 tab 条、新增按钮与弹层；tab 切换、关闭；多会话并存与输入路由（f 节）；壳广播 `sync_size`；壳对每个会话注册 `cx.observe` 重绘链路（b 节） | 同时打开 ≥3 个项目：各自 Pi 独立运行、切换 tab 进程不中断（后台 tab 的 `sleep`/`htop` 类输出持续推进，切回即最新）；**活动 tab 的输出实时刷新**（不依赖切 tab/resize 等偶发触发）；关闭 tab 只回收对应进程；关闭窗口全部回收；IME 预编辑不串 tab（实测验证，f 节）；`Cmd+V`、选择复制、滚轮只作用于活动 tab |
| 5. 规约与收尾 | 按 j 节修订 `docs/ui-design.md`；README/注释同步；最终门禁 | `docs/ui-design.md` 只描述最终状态；`cargo fmt --check && cargo clippy --workspace --all-targets && cargo test --workspace` 全绿 |

各阶段独立可交付、可回滚；阶段 2 是纯重构，阶段 3/4 是功能增量。

---

## 附：关键现状引用索引

- 单体应用：`crates/lure-desktop/src/main.rs:60-76`（`LureApp`）、`:36-58`（`AppMessage`）、`:651`（`Render`）
- 进程生命周期：`:186-239`（`start`）、`:245`（`stop`）、`:644-649`（`Drop`）、`:1059`（`forward_pty_events`）
- 目录与启动：`:259-277`（`choose_directory`）、`:1093-1099`（`default_workspace`）、`:1101-1109`（`canonical_directory`）、`:1131`（启动直启）
- 输入：`:279-300`（键盘/Cmd+V）、`:330-376`（选择复制）、`:379-421`（滚轮）、`:550-642`（IME）
- 终端模型：`crates/lure-desktop/src/terminal.rs:161`（`TerminalGrid`）、`:12-15`（布局常量，`TITLEBAR_HEIGHT` 在 :14）、`:483`（尺寸计算）
- PTY：`crates/lure-pty/src/lib.rs:139`（`PtyProcess`）、`:157`（`spawn`）、`:281`（`stop`）、`crates/lure-pty/src/env.rs:26`（`SpawnEnvResolver`）

## 残余风险

1. `serde_yaml` 为维护归档状态（a 节已论证，备选方案已列出）。
2. 多实例同时运行 Lure 时 projects.yaml 后写覆盖先写（单实例假设，未加文件锁）。
3. 后台 tab 数量无硬上限，极端情况下内存（每会话一个含 scrollback 的 Term）需实测评估。
4. `$HOME/.pi` 为 Pi 配置目录，Lure 只新增并独占 `projects.yaml` 一个文件；若未来 Pi 官方占用同名文件需迁移（设计已把读写收敛在 `projects.rs` 单点，迁移成本低）。
5. 运行时更新窗口标题依赖 gpui-ce 暴露相应 API（`TitlebarOptions` 仅在 `open_window` 时设置，`crates/lure-desktop/src/main.rs:1120-1125`），未验证；不可行时回退为静态标题「Lure」（d 节）。
6. 切 tab 时平台 IME 组合态的取消依赖焦点切换机制，`unmark_text` 手动调用仅清本地状态（f 节）；阶段 4 须实测验证预编辑不串 tab，不通过时需引入显式的输入上下文注销。

---

## 评审处置

本节逐条记录两份评审（reviewer / oracle）finding 的处置结果。两份评审无直接冲突项；oracle 偏离项 1 与需求原文存在张力，按「不偏离 AGENTS.md 产品边界」的原则采纳其全部缓解措施。产品层已确认固定使用 `$HOME/.pi/projects.yaml`（D1），并确认同一项目不允许重复 tab（D2）。

### reviewer findings

| Finding | 处置 | 说明 |
| --- | --- | --- |
| P1 活动会话输出重绘链路缺失 | **采纳** | b 节新增「活动会话的重绘链路」：壳创建会话时 `cx.observe(&session, \|_shell, _, cx\| cx.notify()).detach()`；e 节切换语义同步引用；k 节阶段 4 验收加入「活动 tab 输出实时刷新」 |
| P2 行号引用偏差 | **采纳** | 已逐项核实并修正：`AppMessage` → `main.rs:36-58`；常量块 → `terminal.rs:12-15`（`TITLEBAR_HEIGHT` :14）；`window.handle_input` → `main.rs:863-865`；标题栏拖动/双击 → `main.rs:708-716`；目录截断 → `main.rs:719-728`；`ui-design.md` 条款行号 → :25/:26/:27/:31/:41/:42/:45；`source_audit.rs` 测试区间 → :16-31、:33-56、:57-69、:4-14、:70-82、:84-90（全文 90 行），并补全文件路径 |
| P2 需求 3 解释偏差（同项目不允许多 tab） | **采纳并确认** | 产品层已确认 D2：同项目按 canonical 路径去重，重复打开时切换到已有 tab，不创建第二个 Pi 进程 |
| P2 首页路由下重启/停止按钮行为未说明 | **采纳** | d 节补充：`active = None`（首页路由）时不渲染这两个按钮 |
| P2 「返回首页」表述与 UI 不一致 | **采纳** | a 节读写时机表加注：tab 存活期间无返回首页入口，关闭最后一个 tab 是唯一回程 |
| P2 窗口标题运行时更新 API 未验证 | **采纳** | d 节加注并记入残余风险 5；不可行时回退静态标题「Lure」 |
| P2 IME `unmark_text` 手动调用不取消平台组合态 | **采纳** | f 节改写：取消依赖焦点切换机制，手动清理仅兑底，阶段 4 实测验证并记入残余风险 6 |
| P2 i 节新断言未按阶段拆分 | **采纳** | i 节「新增测试」按阶段 1/2/3/4 拆分，阶段 2 只断言拆文件与 `session: SessionId` 路由，不断言 tab 条与启动流程 |

### oracle findings

| Finding | 处置 | 说明 |
| --- | --- | --- |
| 偏离项 1（主要）：projects.yaml 落点与产品边界张力 | **部分采纳，落点已确认** | 采纳全部缓解措施：(1) 不由 Lure 代建 `.pi` 目录——目录不存在视为「Pi 未初始化」，读取按空列表 + 提示，创建项目时中止写盘（a/c 节）；(2) 归属契约（Lure 独占该文件、Pi 不感知、不触碰其余内容、不代建目录）写入 j 节条款 1。产品层已确认 D1：固定使用 `$HOME/.pi/projects.yaml`，不迁移到 `$HOME/.lure/` |
| 偏离项 2（次要）：j 节规约修订清单不完备 | **采纳** | j 节补齐 4 处：职责清单补项目列表持久化与归属契约（:5-10）、页面布局 ASCII 图（:15-23）、工作目录截断改写（:27）、焦点条款整句改写含「目录选择结束后可继续输入」后半句（:41）；并将「最小化保留当前 Pi 进程」并入窗口回收语义条款（:26）。j 节由 9 条扩为 12 条 |

### 已确认的产品决策

- **D1**：projects.yaml 固定存放在 `$HOME/.pi/projects.yaml`。Lure 不代建 `.pi` 目录、不读取或修改其中其他内容。
- **D2**：同一项目不允许多个 tab。按 canonical 路径判重，重复打开时切换到已有 tab，不创建新的 Pi 进程。
