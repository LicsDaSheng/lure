//! `lure-cli`：Rust 版 `nanobot` 复刻的命令行入口。
//!
//! Phase 3 打通第一条纵向闭环：`lure agent -m "..."` 走 EchoProvider（占位）跑完
//! agent loop，保存 user/assistant turn 并输出最终回复。真实 provider 属 Phase 4。

use std::path::PathBuf;
use std::process::ExitCode;

use lure_core::agent::{AgentInput, AgentLoop, ContextBuilder};
use lure_core::config::default_workspace;
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("agent") => match run_agent(&args[1..]) {
            Ok(reply) => {
                println!("{reply}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("错误: {message}");
                ExitCode::FAILURE
            }
        },
        _ => {
            println!("lure {}", lure_core::version());
            ExitCode::SUCCESS
        }
    }
}

/// 解析并执行 `agent -m <message> [--workspace <path>]`。
fn run_agent(args: &[String]) -> Result<String, String> {
    let mut message: Option<String> = None;
    let mut workspace: Option<String> = None;

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
            other => return Err(format!("未知参数: {other}")),
        }
        index += 1;
    }

    let message = message.ok_or("缺少 -m <message>")?;
    let workspace = workspace
        .map(PathBuf::from)
        .unwrap_or_else(default_workspace);

    let sessions = SessionManager::new(&workspace).map_err(|e| e.to_string())?;
    let mut agent_loop = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );

    let input = AgentInput::new("cli", "direct", message);
    let outcome = agent_loop.process(&input).map_err(|e| e.to_string())?;
    Ok(outcome.final_content)
}
