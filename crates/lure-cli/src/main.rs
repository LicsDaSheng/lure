//! `lure-cli`：Rust 版 `nanobot` 复刻的命令行入口。
//!
//! `lure agent -m "..."` 跑完 agent loop 并保存 turn；`lure agent` 进入交互模式。
//! 默认走 EchoProvider（离线占位）；provider 选择统一经 `ModelRuntimeResolver`。
//! 指定 `--preset <name>`（从 `--config` 加载的 config 选中命名 preset）或
//! `--model <model>`（覆盖默认 preset 的 model，二者互斥）时，由 resolver 解析出不可变
//! runtime（provider 身份 + 生成参数），据此从 `<PROVIDER>_API_KEY` 读取 key 构造真实
//! OpenAI-compatible provider，并把 runtime 的 model/settings 注入 loop（例如
//! `--model deepseek-v4-pro` → deepseek + `DEEPSEEK_API_KEY`）。两个分支都挂载
//! workspace 绑定的长期记忆（注入记忆块 + 记录 `history.jsonl`）。

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lure_core::agent::{AgentLoop, ContextBuilder, ProgressEvent};
use lure_core::bus::InboundMessage;
use lure_core::config::{default_config_path, default_workspace, load_config, Config};
use lure_core::memory::MemoryStore;
use lure_core::provider::{
    EchoProvider, LlmProvider, LlmRuntime, ModelRuntimeResolver, OpenAiCompatProvider,
    UreqTransport,
};
use lure_core::session::SessionManager;
use lure_core::tool::registry_from_config;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
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

fn run_interactive(
    mut agent_loop: AgentLoop,
    channel: &str,
    chat_id: &str,
    show_reasoning: bool,
) -> Result<(), String> {
    println!(
        "Lure interactive mode ({channel}:{chat_id}) — type exit, quit, /exit, /quit, or :q to quit"
    );

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("You: ");
        io::stdout()
            .flush()
            .map_err(|e| format!("刷新输出失败: {e}"))?;

        let Some(line) = lines.next() else {
            println!();
            println!("Goodbye!");
            break;
        };
        let line = line.map_err(|e| format!("读取输入失败: {e}"))?;
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
        let mut stdout = io::stdout();
        let mut renderer = StreamRenderer::new("Assistant: ");
        let mut spinner = Spinner::new("Working");
        let mut reasoning_buffer = ReasoningBuffer::new();
        let mut streamed_reasoning = false;
        let outcome = agent_loop
            .process_streaming(&input, &mut |event| match event {
                // 本轮开始、尚无输出：显示等待指示器。
                ProgressEvent::TurnStarted { .. } => {
                    let _ = spinner.tick(&mut stdout);
                }
                ProgressEvent::ContentDelta { text } => {
                    // 首个内容到达前擦除 spinner，避免与正文同行。
                    let _ = spinner.clear(&mut stdout);
                    let _ = renderer.push(&mut stdout, text);
                }
                ProgressEvent::ReasoningDelta { text } => {
                    if show_reasoning {
                        streamed_reasoning = true;
                        if let Some(sentence) = reasoning_buffer.add(text) {
                            let _ = spinner.clear(&mut stdout);
                            eprintln!("✻ {sentence}");
                        }
                    }
                }
                other => {
                    if let Some(line) = format_progress_line(other) {
                        let _ = spinner.clear(&mut stdout);
                        let _ = renderer.line(&mut stdout, &line);
                        // 工具执行完毕、等待模型下一段输出：恢复等待指示器。
                        let _ = spinner.tick(&mut stdout);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        // 收尾：若 spinner 仍在显示（如空回复、无工具轮），先擦除以免残留在最终输出行。
        if spinner.active() {
            spinner.clear(&mut stdout).map_err(|e| e.to_string())?;
        }
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

/// 构建 agent loop：provider 选择路径统一经 `ModelRuntimeResolver`。
///
/// 未指定 `--preset`/`--model` 时走离线 EchoProvider（占位）；否则加载 config 文件，
/// 由 resolver 解析出不可变 runtime（provider 身份/api_base/model + 生成参数），据此
/// 构造真实 OpenAI-compatible provider 并把 runtime 的 model/settings 注入 loop。
fn build_agent_loop(
    config_path: Option<&str>,
    preset: Option<&str>,
    model: Option<&str>,
    workspace: &Path,
    sessions: SessionManager,
) -> Result<AgentLoop, String> {
    let context = ContextBuilder::new(None);
    // 长期记忆是核心能力，两个分支都挂载：注入记忆块 + 记录 history.jsonl。
    let memory = MemoryStore::new(workspace).map_err(|e| format!("初始化 memory 失败: {e}"))?;

    if preset.is_none() && model.is_none() {
        return Ok(
            AgentLoop::new(Box::new(EchoProvider::new()), sessions, context).with_memory(memory),
        );
    }

    let config = load_cli_config(config_path)?;
    let runtime = resolve_runtime(config.clone(), preset, model)?;
    // 工具运行时先于 provider 构建：config 里的 exec 策略正则等错误 fail-fast，
    // 不必等到读取 API key / 出网。
    let tools = registry_from_config(&config, workspace).map_err(|e| e.to_string())?;
    let provider = build_provider_from_runtime(&config, &runtime)?;
    Ok(AgentLoop::new(provider, sessions, context)
        .with_runtime(&runtime)
        .with_tools(tools)
        .with_memory(memory))
}

/// 加载 config 文件（缺省用 `default_config_path`）；文件不存在时回落到默认配置。
fn load_cli_config(config_path: Option<&str>) -> Result<Config, String> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    load_config(&path).map_err(|e| e.to_string())
}

/// 从 config + flag 解析出不可变 runtime。
///
/// `--preset` 与 `--model` 互斥：`--preset` 选中命名 preset；`--model` 覆盖默认 preset
/// 的 model（并强制 `provider=auto` 走 registry 匹配）。
fn resolve_runtime(
    mut config: Config,
    preset: Option<&str>,
    model: Option<&str>,
) -> Result<LlmRuntime, String> {
    let selected = match (preset, model) {
        (Some(_), Some(_)) => return Err("--preset 与 --model 互斥，只能二选一".to_string()),
        (Some(name), None) => Some(name.to_string()),
        (None, Some(model)) => {
            config.agents.defaults.model = model.to_string();
            config.agents.defaults.provider = "auto".to_string();
            None
        }
        // build_agent_loop 已拦截二者皆无的情况。
        (None, None) => None,
    };

    ModelRuntimeResolver::new(config)
        .admit(selected.as_deref())
        .map_err(|e| e.to_string())
}

/// 由 runtime 的 provider 快照构造真实 provider。
///
/// api_key 解析：优先 `config.providers.<name>.apiKey`，否则回落环境变量
/// `<PROVIDER>_API_KEY`；api_base/model 取 runtime 快照（已含 config 覆盖）。
fn build_provider_from_runtime(
    config: &Config,
    runtime: &LlmRuntime,
) -> Result<Box<dyn LlmProvider>, String> {
    let provider_name = &runtime.provider.provider_name;
    let env_key = format!("{}_API_KEY", provider_name.to_uppercase());
    let api_key = config
        .provider_api_key(provider_name)
        .or_else(|| std::env::var(&env_key).ok())
        .ok_or_else(|| {
            format!("缺少 API key：config.providers.{provider_name}.apiKey 或环境变量 {env_key}")
        })?;

    Ok(Box::new(OpenAiCompatProvider::new(
        &runtime.provider.api_base,
        Some(api_key),
        &runtime.provider.model,
        UreqTransport::new(),
    )))
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
