//! `lure-desktop`：内嵌 WebUI 的桌面应用。
//!
//! 进程内启动 loopback HTTP server（静态资源 + `/webui/bootstrap` + `/api/*`）与
//! WS server（复用协议），wry webview 窗口加载 `http://127.0.0.1:<port>`——
//! 前端原样复用 nanobot WebUI（浏览器模式路径），不跑独立 web 服务进程。

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;

use lure_cli::build_agent_loop;
use lure_core::session::SessionManager;
use lure_core::webui::http_server::{StaticAssets, WebuiServer, WebuiServerConfig};
use lure_core::webui::tokens::TokenIssuer;
use lure_core::webui::ws_server::{AgentTurnRunner, WsServer};
use rust_embed::RustEmbed;

/// 内嵌的前端构建产物（`frontend/dist`，构建前需先 `bun run build`，见 frontend/README）。
#[derive(RustEmbed)]
#[folder = "../../frontend/dist/"]
struct FrontendAssets;

impl StaticAssets for FrontendAssets {
    fn asset(&self, path: &str) -> Option<(Vec<u8>, &'static str)> {
        let file = FrontendAssets::get(path)?;
        Some((file.data.into_owned(), mime_for(path)))
    }
}

/// 按扩展名映射 MIME（覆盖 Vite 产物常见类型）。
fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 命令行参数（与 CLI 对齐：`--config`/`--preset`/`--model`/`--workspace`）。
struct Args {
    config: Option<String>,
    preset: Option<String>,
    model: Option<String>,
    workspace: Option<String>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        config: None,
        preset: None,
        model: None,
        workspace: None,
    };
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let value = argv.get(i + 1).ok_or_else(|| format!("{flag} 缺参数值"))?;
        match flag {
            "--config" => args.config = Some(value.clone()),
            "--preset" => args.preset = Some(value.clone()),
            "--model" => args.model = Some(value.clone()),
            "--workspace" => args.workspace = Some(value.clone()),
            other => return Err(format!("未知参数: {other}")),
        }
        i += 2;
    }
    Ok(args)
}

/// desktop 默认 workspace：`~/.lure/workspace`（先跑 `lure onboard` 初始化）。
fn default_workspace() -> PathBuf {
    lure_core::config::home_dir()
        .join(".lure")
        .join("workspace")
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("错误: {message}");
            return ExitCode::FAILURE;
        }
    };
    let workspace = args
        .workspace
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(default_workspace);
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!("错误: 创建 workspace 失败: {e}");
        return ExitCode::FAILURE;
    }

    // 共享 token 签发器：bootstrap 签发 → /api/* 与 WS 握手双侧校验。
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(3600, 16)));

    // transcript 存储：turn 结束时写入，webui-thread GET 读取。
    let webui_dir = workspace.join("webui");
    let transcript = lure_core::webui::transcript::TranscripStore::new(&webui_dir)
        .expect("创建 webui/ transcript 目录失败");

    // WS server：每条连接在连接线程内构建独立 AgentLoop（复用 CLI 构建逻辑）。
    let factory = {
        let config = args.config.clone();
        let preset = args.preset.clone();
        let model = args.model.clone();
        let workspace = workspace.clone();
        move || {
            let sessions = SessionManager::new(&workspace).expect("session 存储可用");
            let agent = build_agent_loop(
                config.as_deref(),
                preset.as_deref(),
                model.as_deref(),
                &workspace,
                sessions,
            )
            .expect("agent loop 构建失败（检查 --config/--preset/--model 与 API key）");
            let dream = Box::new(lure_core::memory::ProviderDreamRunner::new(Box::new(
                lure_core::provider::EchoProvider::new(),
            )));
            AgentTurnRunner::new(agent).with_dream(dream, 10)
        }
    };
    let mut ws_server = match WsServer::bind(
        "127.0.0.1:0",
        factory,
        issuer.clone(),
        Some(transcript.clone()),
    ) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("错误: WS server 绑定失败: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ws_addr = match ws_server.local_addr() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("错误: WS server 地址不可用: {e}");
            return ExitCode::FAILURE;
        }
    };
    thread::spawn(move || {
        let _ = ws_server.serve_forever();
    });

    // HTTP server：静态资源 + bootstrap + /api/*。
    let http_config = WebuiServerConfig {
        workspace: workspace.clone(),
        model_name: args.model.clone(),
        ws_path: "/ws".to_string(),
        ws_url: format!("ws://{ws_addr}/ws"),
        token_ttl_secs: 3600,
    };
    // lure config（缺省路径回落默认；解析失败也回落默认）：派生 /api/settings 载荷。
    let lure_config = {
        let path = args
            .config
            .clone()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(lure_core::config::default_config_path);
        lure_core::config::load_config(&path).unwrap_or_default()
    };
    let mut http_server = match WebuiServer::bind(
        "127.0.0.1:0",
        FrontendAssets,
        http_config,
        lure_config,
        issuer,
        transcript,
    ) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("错误: HTTP server 绑定失败: {e}");
            return ExitCode::FAILURE;
        }
    };
    let http_addr = match http_server.local_addr() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("错误: HTTP server 地址不可用: {e}");
            return ExitCode::FAILURE;
        }
    };
    thread::spawn(move || {
        let _ = http_server.serve_forever();
    });

    // 桌面窗口加载内嵌应用。
    let url = format!("http://{http_addr}/");
    eprintln!("Lure desktop 已启动: {url}（WS: ws://{ws_addr}/ws）");
    run_window(&url);
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn run_window(url: &str) {
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Lure")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 860.0))
        .build(&event_loop)
        .expect("创建窗口失败");
    let _webview = WebViewBuilder::new()
        .with_url(url)
        .build(&window)
        .expect("创建 webview 失败");
    event_loop.run(|_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
    });
}

#[cfg(not(target_os = "macos"))]
fn run_window(url: &str) {
    eprintln!("当前平台暂未接入桌面窗口；请在浏览器打开 {url}");
    loop {
        std::thread::park();
    }
}
