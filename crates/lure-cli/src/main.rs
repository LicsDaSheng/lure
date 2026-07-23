//! `lure-cli`：Rust 版 `nanobot` 复刻的命令行入口。
//!
//! `lure agent -m "..."` 跑完 agent loop 并保存 turn。默认走 EchoProvider（离线占位）；
//! 指定 `--model <model>` 时经 registry 匹配 provider、从 `<PROVIDER>_API_KEY` 读取
//! key，用真实 OpenAI-compatible provider 出网（例如
//! `--model deepseek-v4-pro` → deepseek + `DEEPSEEK_API_KEY`）。

use std::path::PathBuf;
use std::process::ExitCode;

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::InboundMessage;
use lure_core::config::default_workspace;
use lure_core::provider::{
    match_provider, EchoProvider, LlmProvider, OpenAiCompatProvider, UreqTransport,
};
use lure_core::session::SessionManager;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("agent") => match run_agent(&args[1..]) {
            Ok(reply) => {
                // 思维链走 stderr（保持 stdout 为纯答案、可脚本化），答案走 stdout。
                if let Some(reasoning) = reply.reasoning {
                    eprintln!("💭 思维链:\n{reasoning}\n");
                }
                println!("{}", reply.final_content);
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

/// 解析并执行 `agent -m <message> [--workspace <path>] [--model <model>] [--show-reasoning]`。
fn run_agent(args: &[String]) -> Result<AgentReply, String> {
    let mut message: Option<String> = None;
    let mut workspace: Option<String> = None;
    let mut model: Option<String> = None;
    let mut show_reasoning = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-m" | "--message" => {
                index += 1;
                message = Some(args.get(index).ok_or("-m 缺少消息内容")?.clone());
            }
            "--workspace" => {
                index += 1;
                workspace = Some(args.get(index).ok_or("--workspace 缺少路径")?.clone());
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

    let message = message.ok_or("缺少 -m <message>")?;
    let workspace = workspace
        .map(PathBuf::from)
        .unwrap_or_else(default_workspace);

    let provider = build_provider(model.as_deref())?;
    let sessions = SessionManager::new(&workspace).map_err(|e| e.to_string())?;
    let mut agent_loop = AgentLoop::new(provider, sessions, ContextBuilder::new(None));

    let input = InboundMessage::new("cli", "direct", message);
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

/// 构建 provider：无 `--model` 走离线 EchoProvider；指定 `--model` 时经 registry 匹配
/// provider，从 `<PROVIDER>_API_KEY` 读取 key，返回真实 OpenAI-compatible provider。
fn build_provider(model: Option<&str>) -> Result<Box<dyn LlmProvider>, String> {
    let Some(model) = model else {
        return Ok(Box::new(EchoProvider::new()));
    };

    let spec = match_provider(model, "auto")
        .ok_or_else(|| format!("无法为模型 '{model}' 匹配 provider"))?;
    let env_key = format!("{}_API_KEY", spec.name.to_uppercase());
    let api_key = std::env::var(&env_key).map_err(|_| {
        format!(
            "缺少环境变量 {env_key}（provider '{}' 需要 API key）",
            spec.name
        )
    })?;

    Ok(Box::new(OpenAiCompatProvider::new(
        spec.default_api_base,
        Some(api_key),
        model,
        UreqTransport::new(),
    )))
}
