//! `lure-desktop`：内嵌 WebUI 的桌面应用。
//!
//! 进程内启动 loopback HTTP server（静态资源 + `/webui/bootstrap` + `/api/*`）与
//! WS server（复用协议），Tauri V2 窗口加载 `http://127.0.0.1:<port>`——
//! 前端为自有 shadcn/Tailwind WebUI（`frontend/app` 构建到 `frontend/dist`），
//! Tauri 仅作外壳（不迁移到 IPC），能力仍由进程内 HTTP/WS 提供。

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use lure_cli::{build_agent_loop, build_provider};
use lure_core::agent::AgentLoop;
use lure_core::bus::InboundMessage;
use lure_core::cron::{
    origin_delivery_context, CronJob, CronJobRunner, CronScheduler, CronService, RunStatus,
};
use lure_core::session::SessionManager;
use lure_core::webui::hub::WsHub;
use lure_core::webui::transcript::TranscripStore;

/// cron 调度默认轮询间隔（ms）：cron 最小粒度为分钟，30s 轮询确保到期后半分钟内触发。
const CRON_POLL_DEFAULT_MS: u64 = 30_000;

/// cron 轮询间隔：`LURE_CRON_POLL_MS` 环境变量覆盖（供测试用短间隔），否则默认 30s。
fn cron_poll_interval() -> Duration {
    let ms = std::env::var("LURE_CRON_POLL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CRON_POLL_DEFAULT_MS);
    Duration::from_millis(ms)
}

/// 桌面 cron 执行：跑 job 的 agent turn，把 (message, reply) 写入 origin 会话 transcript，
/// 并向**在线**查看该会话的 WS 连接实时推送产出（无需刷新即见）。
/// 缺 origin 记 `Skipped`，agent 出错记 `Error`。
struct CronTurnRunner {
    agent: AgentLoop,
    transcript: TranscripStore,
    hub: WsHub,
}

impl CronJobRunner for CronTurnRunner {
    fn run(&mut self, job: &CronJob) -> RunStatus {
        let (channel, chat_id, _meta) = match origin_delivery_context(job) {
            Ok(ctx) => ctx,
            Err(_) => return RunStatus::Skipped,
        };
        let inbound = InboundMessage::new(&channel, &chat_id, &job.payload.message);
        let session_key = job
            .payload
            .session_key
            .clone()
            .unwrap_or_else(|| inbound.session_key());
        match self.agent.process(&inbound) {
            Ok(outcome) => {
                let _ = self.transcript.append_turn(
                    &session_key,
                    &job.payload.message,
                    &outcome.final_content,
                );
                // 向在线连接实时推送：assistant 回复 + session_updated 刷新侧栏。
                // 无在线连接（push 返回 0）时静默——transcript 已落，下次打开可见。
                let reply = serde_json::json!({
                    "event": "message",
                    "chat_id": chat_id,
                    "text": outcome.final_content,
                });
                self.hub.push(&chat_id, &reply);
                self.hub.push(
                    &chat_id,
                    &serde_json::json!({"event": "session_updated", "chat_id": chat_id}),
                );
                RunStatus::Ok
            }
            Err(_) => RunStatus::Error,
        }
    }
}
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
///
/// `--headless`：不开窗口，仅起进程内 HTTP/WS server 并 park，stdout 打印
/// `LURE_HTTP_URL=...` 供 E2E（Playwright）用真实浏览器驱动后端做契约测试。
struct Args {
    config: Option<String>,
    preset: Option<String>,
    model: Option<String>,
    workspace: Option<String>,
    headless: bool,
    /// HTTP server 绑定端口（默认 `0` = 随机）；E2E 固定端口便于 webServer 轮询。
    http_port: Option<u16>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        config: None,
        preset: None,
        model: None,
        workspace: None,
        headless: false,
        http_port: None,
    };
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        // 无值 flag 先处理，避免误吞下一个参数。
        if flag == "--headless" {
            args.headless = true;
            i += 1;
            continue;
        }
        let value = argv.get(i + 1).ok_or_else(|| format!("{flag} 缺参数值"))?;
        match flag {
            "--config" => args.config = Some(value.clone()),
            "--preset" => args.preset = Some(value.clone()),
            "--model" => args.model = Some(value.clone()),
            "--workspace" => args.workspace = Some(value.clone()),
            "--http-port" => {
                args.http_port = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--http-port 非法: {value}"))?,
                );
            }
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
    // cron 调度器用的 transcript 副本（transcript 稍后被 http server 移入）。
    let cron_transcript = transcript.clone();

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
            // dream consolidation 复用与 chat 同源的真实 provider（config 驱动）；
            // 解析失败（缺 key 等）时优雅回落离线 Echo，避免拖垮整条连接。
            let dream_provider =
                build_provider(config.as_deref(), preset.as_deref(), model.as_deref())
                    .unwrap_or_else(|_| Box::new(lure_core::provider::EchoProvider::new()));
            let dream = Box::new(lure_core::memory::ProviderDreamRunner::new(dream_provider));
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
    // 在移入 serve 线程前取 hub 句柄：交给 cron runner 做服务端实时推送。
    let cron_hub = ws_server.hub();
    thread::spawn(move || {
        let _ = ws_server.serve_forever();
    });

    // lure config 路径（缺省回落 default_config_path）：既用于加载 /api/settings 载荷，
    // 也作为 /api/settings/*/update 写入落盘的目标。
    let config_path = args
        .config
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(lure_core::config::default_config_path);
    // HTTP server：静态资源 + bootstrap + /api/*。
    let http_config = WebuiServerConfig {
        workspace: workspace.clone(),
        config_path: config_path.clone(),
        model_name: args.model.clone(),
        ws_path: "/ws".to_string(),
        ws_url: format!("ws://{ws_addr}/ws"),
        token_ttl_secs: 3600,
    };
    // lure config（解析失败回落默认）：派生 /api/settings 载荷。
    let lure_config = lure_core::config::load_config(&config_path).unwrap_or_default();
    let http_bind = format!("127.0.0.1:{}", args.http_port.unwrap_or(0));
    let mut http_server = match WebuiServer::bind(
        &http_bind,
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

    // cron 后台调度：常驻线程每 CRON_POLL 轮询到期 job，跑 agent turn，把结果写入
    // transcript 并向在线连接实时推送——在线时即刻可见，离线时下次打开会话可见。
    // runner 在线程内构建（AgentLoop/TranscripStore 非 Send，经工厂闭包避免跨线程移动）。
    let _cron_scheduler = {
        let config = args.config.clone();
        let preset = args.preset.clone();
        let model = args.model.clone();
        let ws = workspace.clone();
        let transcript = cron_transcript;
        let hub = cron_hub;
        CronScheduler::spawn(
            CronService::new(&workspace),
            move || {
                let sessions = SessionManager::new(&ws).expect("session 存储可用");
                let agent = build_agent_loop(
                    config.as_deref(),
                    preset.as_deref(),
                    model.as_deref(),
                    &ws,
                    sessions,
                )
                .expect("cron agent loop 构建失败（检查 --config/--preset/--model 与 API key）");
                CronTurnRunner {
                    agent,
                    transcript,
                    hub: hub.clone(),
                }
            },
            cron_poll_interval(),
        )
    };

    // 桌面窗口加载内嵌应用。
    let url = format!("http://{http_addr}/");
    eprintln!("Lure desktop 已启动: {url}（WS: ws://{ws_addr}/ws）");

    // headless：不开窗口，打印机器可读 URL 后 park，供 E2E 真实浏览器驱动后端。
    if args.headless {
        println!("LURE_HTTP_URL={url}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        loop {
            thread::park();
        }
    }

    run_window(&url);
    ExitCode::SUCCESS
}

/// Tauri V2 外壳：在主线程启动应用，运行期创建 `main` 窗口加载 loopback URL。
///
/// 窗口以 `WebviewUrl::External` 指向进程内 HTTP server（能力仍走 HTTP/WS，不用 IPC）。
/// macOS 下 Tauri 自带标准菜单（含 Edit 的 Cut/Copy/Paste/Select All），故无需手挂菜单，
/// 输入框粘贴等标准编辑快捷键开箱可用。`run` 接管主线程事件循环，发散不返回。
fn run_window(url: &str) {
    let external: tauri::Url = url.parse().expect("非法 loopback URL");
    tauri::Builder::default()
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External(external))
                .title("Lure")
                .inner_size(1280.0, 860.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
