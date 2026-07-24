//! `lure-cli`：Rust 版 `nanobot` 复刻的命令行入口。
//!
//! `lure agent -m "..."` 跑完 agent loop 并保存 turn；`lure agent` 进入交互模式。
//! 默认走 EchoProvider（离线占位）；provider 选择统一经 `ModelRuntimeResolver`。
//! 指定 `--preset <name>`（从 `--config` 加载的 config 选中命名 preset）或
//! `--model <model>`（覆盖默认 preset 的 model，二者互斥）时，由 resolver 解析出不可变
//! runtime（provider 身份 + 生成参数），据此从 `<PROVIDER>_API_KEY` 读取 key 构造真实
//! OpenAI-compatible provider，并把 runtime 的 model/settings 注入 loop（例如
//! `--model deepseek-v4-pro` → deepseek + `DEEPSEEK_API_KEY`）。

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::InboundMessage;
use lure_core::config::{default_config_path, default_workspace, load_config, Config};
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

        let reply = process_cli_turn(&mut agent_loop, channel, chat_id, line, show_reasoning)?;
        if let Some(reasoning) = reply.reasoning {
            eprintln!("💭 思维链:\n{reasoning}\n");
        }
        println!("Assistant: {}", reply.final_content);
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

    if preset.is_none() && model.is_none() {
        return Ok(AgentLoop::new(
            Box::new(EchoProvider::new()),
            sessions,
            context,
        ));
    }

    let config = load_cli_config(config_path)?;
    let runtime = resolve_runtime(config.clone(), preset, model)?;
    // 工具运行时先于 provider 构建：config 里的 exec 策略正则等错误 fail-fast，
    // 不必等到读取 API key / 出网。
    let tools = registry_from_config(&config, workspace).map_err(|e| e.to_string())?;
    let provider = build_provider_from_runtime(&config, &runtime)?;
    Ok(AgentLoop::new(provider, sessions, context)
        .with_runtime(&runtime)
        .with_tools(tools))
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
