//! `lure-cli`：Rust 版 `nanobot` 复刻的命令行入口。
//!
//! `lure agent -m "..."` 跑完 agent loop 并保存 turn；`lure agent` 进入交互模式。
//! provider 选择统一经 `ModelRuntimeResolver`：无 `--preset`/`--model` 时使用 config
//! 默认 provider；`--model echo` 显式启用离线 EchoProvider 测试脚手架。
//! 指定 `--preset <name>`（从 `--config` 加载的 config 选中命名 preset）或
//! `--model <model>`（覆盖默认 preset 的 model，二者互斥）时，由 resolver 解析出不可变
//! runtime（provider 身份 + 生成参数），据此从 `<PROVIDER>_API_KEY` 读取 key 构造真实
//! OpenAI-compatible provider，并把 runtime 的 model/settings 注入 loop（例如
//! `--model deepseek-v4-pro` → deepseek + `DEEPSEEK_API_KEY`）。所有分支都挂载
//! workspace 绑定的长期记忆（注入记忆块 + 记录 `history.jsonl`）。

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lure_cli::build_agent_loop;
use lure_core::agent::{AgentLoop, ProgressEvent};
use lure_core::bus::InboundMessage;
use lure_core::config::{default_workspace, home_dir, save_config, Config};
use lure_core::session::SessionManager;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("onboard") => match run_onboard(&args[1..]) {
            Ok(summary) => {
                println!("{summary}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("错误: {message}");
                ExitCode::FAILURE
            }
        },
        Some("agent") => match run_agent(&args[1..]) {
            Ok(run) => {
                if let AgentRun::Single(reply) = run {
                    // 思维链走 stderr（保持 stdout 为纯答案、可脚本化），答案走 stdout。
                    if let Some(reasoning) = reply.reasoning {
                        eprintln!("💭 思维链:\n{reasoning}\n");
                    }
                    println!("{}", reply.final_content);
                }
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("错误: {message}");
                ExitCode::FAILURE
            }
        },
        // `--version` / `-V` 与无子命令都输出版本，供发布产物版本核对。
        _ => {
            println!("lure {}", lure_core::version());
            ExitCode::SUCCESS
        }
    }
}

struct OnboardSummary {
    root: PathBuf,
    config_path: PathBuf,
    workspace: PathBuf,
}

impl std::fmt::Display for OnboardSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "已初始化 Lure 目录: {}\nconfig: {}\nworkspace: {}",
            self.root.display(),
            self.config_path.display(),
            self.workspace.display()
        )
    }
}

/// 初始化 Lure 专用数据目录。
///
/// 默认创建 `~/.lure`，目录结构参考上游 `~/.nanobot`：顶层保留 channel/app/runtime
/// 数据目录，`workspace/` 下创建 session、memory、cron、trigger 与基础提示文件。
fn run_onboard(args: &[String]) -> Result<OnboardSummary, String> {
    let mut root: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-r" | "--root" => {
                index += 1;
                root = Some(PathBuf::from(args.get(index).ok_or("--root 缺少路径")?));
            }
            other => return Err(format!("未知参数: {other}")),
        }
        index += 1;
    }

    let root = root.unwrap_or_else(default_lure_root);
    let workspace = root.join("workspace");

    for dir in [
        root.join("cli-apps"),
        root.join("cron"),
        root.join("history"),
        root.join("webui"),
        workspace.clone(),
        workspace.join("cron"),
        workspace.join("memory"),
        workspace.join("prompts"),
        workspace.join("sessions"),
        workspace.join("skills"),
        workspace.join("triggers"),
    ] {
        fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败 {}: {e}", dir.display()))?;
    }

    let config_path = root.join("config.json");
    if !config_path.exists() {
        let mut config = Config::default();
        config.agents.defaults.workspace = if root == default_lure_root() {
            "~/.lure/workspace".to_string()
        } else {
            workspace.to_string_lossy().into_owned()
        };
        save_config(&config, &config_path).map_err(|e| e.to_string())?;
    }

    write_if_missing(&workspace.join(".gitignore"), DEFAULT_GITIGNORE)?;
    write_if_missing(&workspace.join("SOUL.md"), DEFAULT_SOUL)?;
    write_if_missing(&workspace.join("USER.md"), DEFAULT_USER)?;
    write_if_missing(&workspace.join("AGENTS.md"), DEFAULT_AGENTS)?;
    write_if_missing(&workspace.join("HEARTBEAT.md"), DEFAULT_HEARTBEAT)?;
    write_if_missing(&workspace.join("memory").join("MEMORY.md"), "# Memory\n\n")?;

    Ok(OnboardSummary {
        root,
        config_path,
        workspace,
    })
}

fn default_lure_root() -> PathBuf {
    home_dir().join(".lure")
}

fn write_if_missing(path: &Path, contents: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("写入文件失败 {}: {e}", path.display()))
}

const DEFAULT_GITIGNORE: &str =
    "/*\n!memory/\n!SOUL.md\n!USER.md\n!memory/MEMORY.md\n!.gitignore\n";

const DEFAULT_SOUL: &str = r#"# Soul

I am Lure, a personal AI assistant.

## Core Principles

- Solve by doing, not by describing what I would do.
- Keep responses short unless depth is asked for.
- Say what I know, flag what I don't, and never fake confidence.
- Stay friendly and curious; ask a good question when guessing would be risky.
- Treat the user's time as scarce and their trust as valuable.
"#;

const DEFAULT_USER: &str = r#"# User Profile

Information about the user to help personalize interactions.

## Basic Information

- **Name**: (your name)
- **Timezone**: (your timezone, e.g., UTC+8)
- **Language**: (preferred language)

## Preferences

### Communication Style

- [ ] Casual
- [ ] Professional
- [ ] Technical

### Response Length

- [ ] Brief and concise
- [ ] Detailed explanations
- [ ] Adaptive based on question

### Technical Level

- [ ] Beginner
- [ ] Intermediate
- [ ] Expert

## Work Context

- **Primary Role**: (your role, e.g., developer, researcher)
- **Main Projects**: (what you're working on)
- **Tools You Use**: (IDEs, languages, frameworks)

## Topics of Interest

-
-
-

## Special Instructions

(Any specific instructions for how the assistant should behave)
"#;

const DEFAULT_AGENTS: &str = r#"# Agent Instructions

## Workspace Guidance

Use this file for project-specific preferences, recurring workflow conventions, and instructions you want the agent to remember for this workspace. Keep durable facts about the user in `USER.md`, personality/style guidance in `SOUL.md`, and long-term memory in `memory/MEMORY.md`.

## Scheduled Reminders

- Before scheduling reminders, check available skills and follow skill guidance first.
- Use the built-in `cron` tool to create/list/remove jobs.
- Cron jobs run as scheduled turns in the origin chat/session and normally deliver the result back to that channel. Do not use cron for background checks that should stay silent when there is nothing useful to report; use `HEARTBEAT.md` instead.

## Heartbeat Tasks

`HEARTBEAT.md` is checked periodically by the protected heartbeat job. Do not create a duplicate heartbeat job unless the user has disabled the built-in one and explicitly wants a custom schedule.
"#;

const DEFAULT_HEARTBEAT: &str = r#"# Heartbeat Tasks

<!--
This file is checked periodically by your Lure agent.

Use this file for recurring background checks that should stay quiet unless there is something useful to report. Regular cron jobs are different: they normally deliver each run's result back to the chat/session where they were created.

If this file has no tasks (only headers and comments), the agent will skip it. Completed tasks should be deleted, not kept - heartbeat only reads "Active Tasks".
-->

## Active Tasks

<!-- Add your periodic tasks below this line -->
"#;

/// CLI 的一次回复：最终答案 + 可选思维链（仅 `--show-reasoning` 时携带）。
struct AgentReply {
    final_content: String,
    reasoning: Option<String>,
}

enum AgentRun {
    Single(AgentReply),
    Interactive,
}

/// 解析并执行 `agent [-m <message>] [--session <id>] [--workspace <path>]
/// [--config <path>] [--preset <name>] [--model <model>] [--show-reasoning]`。
///
/// `--preset` 与 `--model` 互斥：前者从加载的 config 选中命名 preset，后者覆盖默认
/// preset 的 model。
fn run_agent(args: &[String]) -> Result<AgentRun, String> {
    let mut message: Option<String> = None;
    let mut session_id = String::from("cli:direct");
    let mut workspace: Option<String> = None;
    let mut config_path: Option<String> = None;
    let mut preset: Option<String> = None;
    let mut model: Option<String> = None;
    let mut show_reasoning = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-m" | "--message" => {
                index += 1;
                message = Some(args.get(index).ok_or("-m 缺少消息内容")?.clone());
            }
            "-s" | "--session" => {
                index += 1;
                session_id = args.get(index).ok_or("--session 缺少会话 ID")?.clone();
            }
            "-w" | "--workspace" => {
                index += 1;
                workspace = Some(args.get(index).ok_or("--workspace 缺少路径")?.clone());
            }
            "-c" | "--config" => {
                index += 1;
                config_path = Some(args.get(index).ok_or("--config 缺少路径")?.clone());
            }
            "-p" | "--preset" => {
                index += 1;
                preset = Some(args.get(index).ok_or("--preset 缺少 preset 名")?.clone());
            }
            "--model" => {
                index += 1;
                model = Some(args.get(index).ok_or("--model 缺少模型名")?.clone());
            }
            "--show-reasoning" => show_reasoning = true,
            other => return Err(format!("未知参数: {other}")),
        }
        index += 1;
    }

    let workspace = workspace
        .map(PathBuf::from)
        .unwrap_or_else(default_workspace);

    let sessions = SessionManager::new(&workspace).map_err(|e| e.to_string())?;
    let mut agent_loop = build_agent_loop(
        config_path.as_deref(),
        preset.as_deref(),
        model.as_deref(),
        &workspace,
        sessions,
    )?;
    let (channel, chat_id) = split_session_id(&session_id);

    match message {
        Some(message) => {
            let reply =
                process_cli_turn(&mut agent_loop, &channel, &chat_id, message, show_reasoning)?;
            Ok(AgentRun::Single(reply))
        }
        None => {
            run_interactive(agent_loop, &channel, &chat_id, show_reasoning)?;
            Ok(AgentRun::Interactive)
        }
    }
}

fn process_cli_turn(
    agent_loop: &mut AgentLoop,
    channel: &str,
    chat_id: &str,
    message: String,
    show_reasoning: bool,
) -> Result<AgentReply, String> {
    let input = InboundMessage::new(channel, chat_id, message);
    let outcome = agent_loop.process(&input).map_err(|e| e.to_string())?;
    Ok(AgentReply {
        final_content: outcome.final_content,
        reasoning: if show_reasoning {
            outcome.reasoning
        } else {
            None
        },
    })
}

/// 交互模式的流式渲染器：把内容增量实时写入调用方给的 sink，首个增量前写一次 `prefix`，
/// 结束时补一个换行。无增量时不输出任何内容（由调用方回退打印最终内容）。
///
/// 只持有渲染状态（前缀 + 是否已开始），sink 每次方法传入——这样它能与 [`Spinner`]
/// 共享同一个 `stdout`（二者交替写同一行，不能各自长期独占 `&mut`）。
struct StreamRenderer {
    prefix: String,
    started: bool,
}

impl StreamRenderer {
    fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            started: false,
        }
    }

    /// 是否已写出任何内容（即是否收到过增量）。
    fn started(&self) -> bool {
        self.started
    }

    /// 写入一个内容增量：首次调用前先写 `prefix`，并逐增量 flush 以实时可见。
    fn push<W: Write>(&mut self, out: &mut W, delta: &str) -> io::Result<()> {
        if !self.started {
            write!(out, "{}", self.prefix)?;
            self.started = true;
        }
        write!(out, "{delta}")?;
        out.flush()
    }

    /// 收尾：已输出内容时补一个换行。
    fn finish<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if self.started {
            writeln!(out)?;
        }
        Ok(())
    }

    /// 另起一行输出一条独立信息（如工具活动）：先断开进行中的内容行并重置前缀状态，
    /// 使随后的内容增量重新带前缀。
    fn line<W: Write>(&mut self, out: &mut W, text: &str) -> io::Result<()> {
        if self.started {
            writeln!(out)?;
            self.started = false;
        }
        writeln!(out, "{text}")?;
        out.flush()
    }
}

/// 交互模式的等待指示器：在 `TurnStarted` 到首个内容增量之间（以及每次工具执行后
/// 等待模型下一段输出时）用回车覆写当前行，滚动 braille 帧，提示“正在处理”。
///
/// 事件驱动而非定时器驱动：每次 [`tick`](Self::tick) 前进一帧并重画，
/// [`clear`](Self::clear) 在写出真实内容前用等宽空格擦除自身。与 [`StreamRenderer`]
/// 共享 sink，故方法都不长期持有 `&mut W`。
struct Spinner {
    label: String,
    index: usize,
    active: bool,
    /// 上次渲染 payload 的显示宽度（字符数），供 `clear` 生成等宽擦除空格。
    last_width: usize,
}

/// braille 旋转帧。
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl Spinner {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            index: 0,
            active: false,
            last_width: 0,
        }
    }

    /// 当前是否正在显示（决定 `clear` 是否需要擦除）。
    fn active(&self) -> bool {
        self.active
    }

    /// 前进一帧并回车重画：`\r{frame} {label}`；标记 active。
    fn tick<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        let frame = SPINNER_FRAMES[self.index % SPINNER_FRAMES.len()];
        let payload = format!("{frame} {}", self.label);
        write!(out, "\r{payload}")?;
        self.last_width = payload.chars().count();
        self.index = (self.index + 1) % SPINNER_FRAMES.len();
        self.active = true;
        out.flush()
    }

    /// 擦除自身：仅在 active 时以等宽空格覆盖并回到行首；随后标记 inactive。非 active 为空操作。
    fn clear<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        write!(out, "\r{}\r", " ".repeat(self.last_width))?;
        self.active = false;
        out.flush()
    }
}

/// spinner 定时动画的默认帧间隔。
const SPINNER_INTERVAL: Duration = Duration::from_millis(120);

/// [`Spinner`] 的定时驱动器：后台线程每隔 `interval` 取锁并前进一帧，使**等待期**
/// （无 progress 事件的空档，如首个 token 到达前）也能持续滚动，而非停在某一帧。
///
/// 协调保证：动画线程与前台真实输出**共享同一把锁包裹的 sink**。前台写正文/工具行前
/// 调用 [`suspend`](Self::suspend)（锁内擦除 spinner + 暂停 + 执行写入），故帧与正文
/// 永不交错；[`resume`](Self::resume) 在重新进入等待（如工具执行完）时恢复滚动；
/// [`stop`](Self::stop) 结束线程并擦除残帧。
struct SpinnerAnimator<W: Write + Send + 'static> {
    shared: Arc<Mutex<AnimatorState<W>>>,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

/// 动画线程与前台共享的可变状态（sink + spinner + 是否暂停），统一由一把锁保护。
struct AnimatorState<W> {
    out: W,
    spinner: Spinner,
    paused: bool,
}

impl<W: Write + Send + 'static> SpinnerAnimator<W> {
    /// 启动动画：接管 `out`，后台线程每 `interval` 前进一帧（暂停时跳过）。
    fn new(out: W, spinner: Spinner, interval: Duration) -> Self {
        let shared = Arc::new(Mutex::new(AnimatorState {
            out,
            spinner,
            paused: false,
        }));
        let running = Arc::new(AtomicBool::new(true));
        let handle = {
            let shared = Arc::clone(&shared);
            let running = Arc::clone(&running);
            thread::spawn(move || {
                while running.load(Ordering::Relaxed) {
                    thread::sleep(interval);
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut state = shared.lock().unwrap();
                    if !state.paused {
                        let AnimatorState { out, spinner, .. } = &mut *state;
                        let _ = spinner.tick(out);
                    }
                }
            })
        };
        Self {
            shared,
            running,
            handle: Some(handle),
        }
    }

    /// 前台写真实输出：锁内擦除 spinner → 暂停动画 → 执行 `f`（写同一 sink），
    /// 全程持锁，确保动画线程不会与真实输出交错。
    fn suspend<F: FnOnce(&mut W)>(&self, f: F) {
        let mut state = self.shared.lock().unwrap();
        {
            let AnimatorState { out, spinner, .. } = &mut *state;
            let _ = spinner.clear(out);
        }
        state.paused = true;
        f(&mut state.out);
    }

    /// 恢复动画（重新进入等待，如工具执行完毕）。下一个 `interval` 起继续滚动。
    fn resume(&self) {
        self.shared.lock().unwrap().paused = false;
    }

    /// 结束动画线程并擦除残留 spinner；幂等（可与 `Drop` 重复调用）。
    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let mut state = self.shared.lock().unwrap();
        if state.spinner.active() {
            let AnimatorState { out, spinner, .. } = &mut *state;
            let _ = spinner.clear(out);
        }
    }
}

impl<W: Write + Send + 'static> Drop for SpinnerAnimator<W> {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 把一个非内容的 progress 事件渲染为交互显示行；无需显示的事件返回 `None`。
fn format_progress_line(event: &ProgressEvent) -> Option<String> {
    match event {
        ProgressEvent::ToolInvoked { name } => Some(format!("🔧 {name}")),
        _ => None,
    }
}

/// 推理增量的句子级缓冲：累积推理增量，遇换行/句末标点/超长即吐出一段完整文本，
/// 避免逐字碎片刷屏。对齐上游 `_ReasoningBuffer`。
struct ReasoningBuffer {
    text: String,
}

/// 句末标点（中英）。
const REASONING_SENTENCE_ENDINGS: [char; 6] = ['.', '!', '?', '。', '！', '？'];
/// 未遇边界也强制 flush 的字符上限。
const REASONING_FLUSH_CHARS: usize = 60;

impl ReasoningBuffer {
    fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    /// 追加一段推理增量：达 flush 条件时返回已累积的整段（trim 后），否则 `None`。
    fn add(&mut self, delta: &str) -> Option<String> {
        if delta.is_empty() {
            return None;
        }
        self.text.push_str(delta);
        if self.should_flush(delta) {
            self.flush()
        } else {
            None
        }
    }

    /// 吐出剩余缓冲（trim 后）；空则 `None`。
    fn flush(&mut self) -> Option<String> {
        let out = self.text.trim().to_string();
        self.text.clear();
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    fn should_flush(&self, delta: &str) -> bool {
        delta.contains('\n')
            || delta.trim_end().ends_with(REASONING_SENTENCE_ENDINGS)
            || self.text.chars().count() >= REASONING_FLUSH_CHARS
    }
}

/// 指向当前交互会话取消令牌的裸指针，供 SIGINT handler 直接置位。
/// 仅在 `run_interactive` 安装 handler 后、清空前有效（令牌 Arc 存活于该函数栈）。
static CANCEL_PTR: AtomicPtr<AtomicBool> = AtomicPtr::new(std::ptr::null_mut());

/// SIGINT（Ctrl-C）处理器：置位当前 turn 的取消令牌，使交互模式**中断本轮**而非退出进程。
///
/// 异步信号安全：仅做原子 load + 原子 store，无分配、无锁、无重入不安全调用。
extern "C" fn on_sigint(_sig: libc::c_int) {
    let ptr = CANCEL_PTR.load(Ordering::SeqCst);
    if !ptr.is_null() {
        // SAFETY: ptr 指向 run_interactive 栈上存活的 AtomicBool——其 Arc 在
        // CANCEL_PTR 置入后、清空为 null 前始终存活；此处仅原子写。
        unsafe { (*ptr).store(true, Ordering::SeqCst) };
    }
}

fn run_interactive(
    agent_loop: AgentLoop,
    channel: &str,
    chat_id: &str,
    show_reasoning: bool,
) -> Result<(), String> {
    println!(
        "Lure interactive mode ({channel}:{chat_id}) — type exit, quit, /exit, /quit, or :q to quit；Ctrl-C 中断当前回合"
    );

    // 取消令牌：SIGINT handler 经 CANCEL_PTR 置位，process_streaming 于检查点中止本轮。
    let cancel = Arc::new(AtomicBool::new(false));
    CANCEL_PTR.store(Arc::as_ptr(&cancel) as *mut AtomicBool, Ordering::SeqCst);
    // 安装 SIGINT handler（Ctrl-C 中断当前 turn，而非默认终止进程）。
    // SAFETY: on_sigint 异步信号安全（仅原子操作）。
    unsafe { libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t) };
    let mut agent_loop = agent_loop.with_cancel(Arc::clone(&cancel));

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let result = run_interactive_loop(
        &mut agent_loop,
        &mut stdin,
        channel,
        chat_id,
        show_reasoning,
        &cancel,
    );

    // 复位信号处理与指针，避免退出后悬垂：先摘 handler 再清空指针。
    unsafe { libc::signal(libc::SIGINT, libc::SIG_DFL) };
    CANCEL_PTR.store(std::ptr::null_mut(), Ordering::SeqCst);
    result
}

/// 交互主循环：读一行 → 驱动一轮流式 → 渲染/中断处理。抽出为独立函数，使
/// [`run_interactive`] 能在其返回后统一复位信号处理与 [`CANCEL_PTR`]。
fn run_interactive_loop(
    agent_loop: &mut AgentLoop,
    stdin: &mut impl BufRead,
    channel: &str,
    chat_id: &str,
    show_reasoning: bool,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    loop {
        print!("You: ");
        io::stdout()
            .flush()
            .map_err(|e| format!("刷新输出失败: {e}"))?;

        let mut raw = Vec::new();
        let bytes_read = stdin
            .read_until(b'\n', &mut raw)
            .map_err(|e| format!("读取输入失败: {e}"))?;
        if bytes_read == 0 {
            println!();
            println!("Goodbye!");
            break;
        }
        let line = decode_interactive_line(&raw);
        let command = line.trim();
        if command.is_empty() {
            continue;
        }
        if is_exit_command(command) {
            println!("Goodbye!");
            break;
        }

        // 流式驱动：内容增量实时写 stdout，工具调用等 progress 事件另起行显示，
        // 推理增量（show_reasoning 时）按句缓冲后写 stderr。
        let input = InboundMessage::new(channel, chat_id, line);
        // 本轮开始前复位取消令牌（上一轮的 Ctrl-C 不应影响本轮）。
        cancel.store(false, Ordering::SeqCst);
        let mut renderer = StreamRenderer::new("Assistant: ");
        let mut reasoning_buffer = ReasoningBuffer::new();
        let mut streamed_reasoning = false;
        // 定时动画：等待期（TurnStarted→首个 token）后台线程持续滚动帧；写真实输出
        // 前经 suspend 擦除+暂停，避免帧与正文交错。
        let mut animator =
            SpinnerAnimator::new(io::stdout(), Spinner::new("Working"), SPINNER_INTERVAL);
        let result = agent_loop.process_streaming(&input, &mut |event| match event {
            // 动画已在跑，TurnStarted 无需额外处理。
            ProgressEvent::TurnStarted { .. } => {}
            ProgressEvent::ContentDelta { text } => {
                // 已按 Ctrl-C：停止渲染后续增量（core 会在流后作废本轮）。
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                animator.suspend(|out| {
                    let _ = renderer.push(out, text);
                });
            }
            ProgressEvent::ReasoningDelta { text } => {
                if show_reasoning {
                    streamed_reasoning = true;
                    if let Some(sentence) = reasoning_buffer.add(text) {
                        // 推理写 stderr，仍经 suspend 擦除 spinner（stdout）并暂停动画。
                        animator.suspend(|_out| {
                            eprintln!("✻ {sentence}");
                        });
                    }
                }
            }
            other => {
                if let Some(line) = format_progress_line(other) {
                    animator.suspend(|out| {
                        let _ = renderer.line(out, &line);
                    });
                    // 工具执行完毕、重新等待模型下一段输出：恢复滚动。
                    animator.resume();
                }
            }
        });
        // 停止动画线程并擦除残帧，然后再处理结果/收尾输出。
        animator.stop();
        // 单轮 provider/turn 错误不终止会话：打印到 stderr（保持 stdout 干净）后回到提示符。
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(err) => {
                eprintln!("⚠ 本轮出错: {err}");
                continue;
            }
        };

        // Ctrl-C 中断本轮：core 返回 interrupted。断行后提示并回到提示符（不退出、不落库）。
        if outcome.stop_reason == "interrupted" {
            println!("\n⚠ 已中断当前回合");
            continue;
        }

        let mut stdout = io::stdout();
        // 无增量（空内容/兜底文案）时回退打印最终内容。
        if !renderer.started() {
            renderer
                .push(&mut stdout, &outcome.final_content)
                .map_err(|e| e.to_string())?;
        }
        renderer.finish(&mut stdout).map_err(|e| e.to_string())?;

        if show_reasoning {
            if streamed_reasoning {
                // 流式路径：吐出剩余未成句的推理缓冲。
                if let Some(rest) = reasoning_buffer.flush() {
                    eprintln!("✻ {rest}");
                }
            } else if let Some(reasoning) = outcome.reasoning {
                // 非流式推理：一次性打印最终思维链。
                eprintln!("💭 思维链:\n{reasoning}\n");
            }
        }
    }
    Ok(())
}

fn decode_interactive_line(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

fn split_session_id(session_id: &str) -> (String, String) {
    match session_id.split_once(':') {
        Some((channel, chat_id)) => (channel.to_string(), chat_id.to_string()),
        None => ("cli".to_string(), session_id.to_string()),
    }
}

fn is_exit_command(command: &str) -> bool {
    matches!(
        command.to_ascii_lowercase().as_str(),
        "exit" | "quit" | "/exit" | "/quit" | ":q"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(prefix: &str, deltas: &[&str]) -> String {
        let mut buf: Vec<u8> = Vec::new();
        let mut renderer = StreamRenderer::new(prefix);
        for d in deltas {
            renderer.push(&mut buf, d).unwrap();
        }
        renderer.finish(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn stream_renderer_writes_prefix_once_and_trailing_newline() {
        assert_eq!(
            rendered("Assistant: ", &["echo: ", "hi"]),
            "Assistant: echo: hi\n"
        );
    }

    #[test]
    fn stream_renderer_no_deltas_writes_nothing() {
        // 无增量（如空内容）：不输出前缀，交由回退路径打印最终内容。
        assert_eq!(rendered("Assistant: ", &[]), "");
    }

    #[test]
    fn stream_renderer_reports_whether_started() {
        let mut buf: Vec<u8> = Vec::new();
        let mut renderer = StreamRenderer::new("P:");
        assert!(!renderer.started());
        renderer.push(&mut buf, "x").unwrap();
        assert!(renderer.started());
    }

    #[test]
    fn stream_renderer_line_breaks_content_and_resets_prefix() {
        let mut buf: Vec<u8> = Vec::new();
        let mut renderer = StreamRenderer::new("A: ");
        renderer.push(&mut buf, "part-a").unwrap();
        renderer.line(&mut buf, "🔧 echo").unwrap();
        renderer.push(&mut buf, "part-b").unwrap();
        renderer.finish(&mut buf).unwrap();
        // 内容行被断开、工具行独立、随后内容重新带前缀。
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "A: part-a\n🔧 echo\nA: part-b\n"
        );
    }

    #[test]
    fn spinner_tick_advances_frames_with_carriage_return() {
        let mut buf: Vec<u8> = Vec::new();
        let mut spinner = Spinner::new("Working");
        spinner.tick(&mut buf).unwrap();
        spinner.tick(&mut buf).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            format!(
                "\r{} Working\r{} Working",
                SPINNER_FRAMES[0], SPINNER_FRAMES[1]
            )
        );
    }

    #[test]
    fn spinner_reports_active_after_tick() {
        let mut buf: Vec<u8> = Vec::new();
        let mut spinner = Spinner::new("Working");
        assert!(!spinner.active());
        spinner.tick(&mut buf).unwrap();
        assert!(spinner.active());
    }

    #[test]
    fn spinner_clear_erases_line_with_equal_width_spaces() {
        let mut buf: Vec<u8> = Vec::new();
        let mut spinner = Spinner::new("Working");
        spinner.tick(&mut buf).unwrap();
        buf.clear();
        spinner.clear(&mut buf).unwrap();
        let width = format!("{} Working", SPINNER_FRAMES[0]).chars().count();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            format!("\r{}\r", " ".repeat(width))
        );
        assert!(!spinner.active());
    }

    #[test]
    fn spinner_clear_is_noop_when_inactive() {
        let mut buf: Vec<u8> = Vec::new();
        let mut spinner = Spinner::new("Working");
        spinner.clear(&mut buf).unwrap();
        assert!(buf.is_empty());
        assert!(!spinner.active());
    }

    #[test]
    fn spinner_frame_wraps_after_full_cycle() {
        let mut buf: Vec<u8> = Vec::new();
        let mut spinner = Spinner::new("W");
        // 走满一整圈后应回到首帧。
        for _ in 0..SPINNER_FRAMES.len() {
            spinner.tick(&mut buf).unwrap();
        }
        buf.clear();
        spinner.tick(&mut buf).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            format!("\r{} W", SPINNER_FRAMES[0])
        );
    }

    /// 线程安全的可克隆 sink：动画线程持一份写入，测试持一份读取。
    #[derive(Clone)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl SharedBuf {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedBuf {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn animator_ticks_repeatedly_over_time_without_events() {
        let sink = SharedBuf::new();
        let mut animator =
            SpinnerAnimator::new(sink.clone(), Spinner::new("W"), Duration::from_millis(5));
        thread::sleep(Duration::from_millis(60));
        animator.stop();
        // 无事件的等待期里，后台线程应已滚动多帧（每次 tick/clear 以 '\r' 起头）。
        let ticks = sink.contents().matches('\r').count();
        assert!(ticks >= 2, "expected repeated timer ticks, got {ticks}");
    }

    #[test]
    fn animator_suspend_clears_then_writes_real_output_without_trailing_frame() {
        let sink = SharedBuf::new();
        let mut animator =
            SpinnerAnimator::new(sink.clone(), Spinner::new("W"), Duration::from_millis(5));
        thread::sleep(Duration::from_millis(30));
        animator.suspend(|out| {
            let _ = write!(out, "REAL");
        });
        animator.stop();
        let out = sink.contents();
        // suspend 后动画暂停：真实输出之后不再追加任何帧字符。
        let tail = &out[out.rfind("REAL").expect("real output present")..];
        assert_eq!(tail, "REAL");
    }

    #[test]
    fn animator_stop_erases_trailing_spinner() {
        let sink = SharedBuf::new();
        let mut animator =
            SpinnerAnimator::new(sink.clone(), Spinner::new("W"), Duration::from_millis(5));
        thread::sleep(Duration::from_millis(40));
        animator.stop();
        // 停止时应擦除残帧（以回车收尾回到行首），不把 spinner 留在屏幕上。
        assert!(
            sink.contents().ends_with('\r'),
            "spinner should be erased on stop"
        );
    }

    #[test]
    fn animator_resume_restarts_ticking_after_suspend() {
        let sink = SharedBuf::new();
        let mut animator =
            SpinnerAnimator::new(sink.clone(), Spinner::new("W"), Duration::from_millis(5));
        animator.suspend(|out| {
            let _ = writeln!(out, "LINE");
        });
        animator.resume();
        thread::sleep(Duration::from_millis(40));
        animator.stop();
        // resume 后应在 LINE 之后重新出现滚动帧。
        let out = sink.contents();
        let tail = &out[out.find("LINE\n").unwrap() + "LINE\n".len()..];
        assert!(
            tail.contains('\r'),
            "resume should restart ticking, tail={tail:?}"
        );
    }

    #[test]
    fn reasoning_buffer_holds_until_sentence_boundary() {
        let mut buf = ReasoningBuffer::new();
        assert_eq!(buf.add("The"), None);
        assert_eq!(buf.add(" user asked."), Some("The user asked.".to_string()));
    }

    #[test]
    fn reasoning_buffer_flushes_on_newline() {
        let mut buf = ReasoningBuffer::new();
        assert_eq!(buf.add("partial\n"), Some("partial".to_string()));
    }

    #[test]
    fn reasoning_buffer_flush_returns_remainder_then_none() {
        let mut buf = ReasoningBuffer::new();
        assert_eq!(buf.add("no boundary yet"), None);
        assert_eq!(buf.flush(), Some("no boundary yet".to_string()));
        assert_eq!(buf.flush(), None);
    }

    #[test]
    fn reasoning_buffer_empty_add_is_none() {
        let mut buf = ReasoningBuffer::new();
        assert_eq!(buf.add(""), None);
    }

    #[test]
    fn format_progress_line_renders_tool_invoked_only() {
        assert_eq!(
            format_progress_line(&ProgressEvent::ToolInvoked {
                name: "echo".into()
            }),
            Some("🔧 echo".to_string())
        );
        assert_eq!(
            format_progress_line(&ProgressEvent::ContentDelta { text: "x".into() }),
            None
        );
        assert_eq!(
            format_progress_line(&ProgressEvent::FinalResponse {
                content: "y".into()
            }),
            None
        );
    }
}
