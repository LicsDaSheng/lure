//! `lure-desktop`：内嵌 WebUI 的桌面应用。
//!
//! 进程内启动 loopback HTTP server（静态资源 + `/webui/bootstrap` + `/api/*`）与
//! WS server（复用协议），Tauri V2 窗口加载 `http://127.0.0.1:<port>`——
//! 前端为自有 shadcn/Tailwind WebUI（`frontend/app` 构建到 `frontend/dist`），
//! Tauri 仅作外壳（不迁移到 IPC），能力仍由进程内 HTTP/WS 提供。

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lure_cli::{build_agent_loop, build_provider};
use lure_core::agent::scheduler::AgentLoopScheduler;
use lure_core::bus::{async_bus_channel, InboundMessage};
use lure_core::channel::{BusTurnRunner, TurnEventRegistry};
use lure_core::cron::{
    cron_submit_message, AsyncCronScheduler, CronJob, CronService, CronStore, RunStatus,
};
use lure_core::session::SessionManager;

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

/// 按 job id 推进 cron 运行状态（record_run：推进 next_run / 删一次性）。
///
/// 共享调度核心的 completion 钩子在 cron turn 完成后调用（异步执行，tick 内不再同步记录）。
fn record_cron_run(workspace: &std::path::Path, job_id: &str) {
    let Ok(mut store) = CronStore::load(workspace) else {
        return;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let _ = store.record_run(job_id, RunStatus::Ok, now_ms);
}
use lure_core::webui::axum_server::{StaticAssets, WebuiServer, WebuiServerConfig};
use lure_core::webui::tokens::TokenIssuer;
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

    // tokio 异步运行时（Stage 0）：WS/HTTP/cron 仍由独立线程承载（行为不变），
    // runtime 供异步基础设施使用（Stage 1 起 AgentLoop.run 常驻调度接入）。
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("错误: 创建 tokio runtime 失败: {e}");
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

    // Stage 5：单实例共享调度核心的 bus 与 turn 事件路由（chat/cron 共用）。
    let (bus_tx, bus_rx) = async_bus_channel(64);
    let turn_events = TurnEventRegistry::new();

    // WS + HTTP 统一 server（axum）。每连接不再建独立 AgentLoop：连接侧 runner 为
    // `BusTurnRunner`（发布到共享 bus + 等待 turn 事件流），协议层（mux）不变。
    let factory = {
        let bus = bus_tx.clone();
        let events = turn_events.clone();
        move || BusTurnRunner::new(bus.clone(), events.clone())
    };
    // lure config 路径（缺省回落 default_config_path）：既用于加载 /api/settings 载荷，
    // 也作为 /api/settings/*/update 写入落盘的目标。
    let config_path = args
        .config
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(lure_core::config::default_config_path);
    let http_bind = format!("127.0.0.1:{}", args.http_port.unwrap_or(0));
    // 统一 server：静态资源 + bootstrap + /api/* + /ws（同一端口，ws_url bind 内回填）。
    let lure_config = lure_core::config::load_config(&config_path).unwrap_or_default();
    let webui_server = match rt.block_on(WebuiServer::bind(
        &http_bind,
        FrontendAssets,
        WebuiServerConfig {
            workspace: workspace.clone(),
            config_path: config_path.clone(),
            model_name: args.model.clone(),
            ws_path: "/ws".to_string(),
            ws_url: String::new(),
            token_ttl_secs: 3600,
        },
        lure_config,
        issuer,
        transcript,
        factory,
    )) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("错误: WebUI server 绑定失败: {e}");
            return ExitCode::FAILURE;
        }
    };
    let http_addr = match webui_server.local_addr() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("错误: WebUI server 地址不可用: {e}");
            return ExitCode::FAILURE;
        }
    };
    // 在移入 runtime 前取 hub 句柄：交给 cron runner 做服务端实时推送。
    let cron_hub = webui_server.hub();
    rt.spawn(async move {
        let _ = webui_server.serve_forever().await;
    });

    // Stage 5：chat 与 cron 共用单实例 AgentLoop 调度核心——浏览器 WS 消息经
    // BusTurnRunner 进共享 bus，与 cron 同一调度器按 session 串行/让位；turn 事件
    // 经 registry 流式回发起连接。投递（transcript + hub + record_run）仅对 cron 在此
    // 统一处理（chat 投递由连接侧 mux + BusTurnRunner 完成）。
    let cron_sessions = SessionManager::new(&workspace).expect("session 存储可用");
    let cron_agent = build_agent_loop(
        args.config.as_deref(),
        args.preset.as_deref(),
        args.model.as_deref(),
        &workspace,
        cron_sessions,
    )
    .expect("agent loop 构建失败（检查 --config/--preset/--model 与 API key）");
    // dream consolidation 复用与 chat 同源的真实 provider（config 驱动）；
    // 解析失败（缺 key 等）时优雅回落离线 Echo。
    let dream_provider = build_provider(
        args.config.as_deref(),
        args.preset.as_deref(),
        args.model.as_deref(),
    )
    .unwrap_or_else(|_| Box::new(lure_core::provider::EchoProvider::new()));
    let dream = Box::new(lure_core::memory::ProviderDreamRunner::new(dream_provider));
    let cron_transcript = cron_transcript.clone();
    let cron_hub = cron_hub.clone();
    let cron_workspace = workspace.clone();
    let cron_scheduler = AgentLoopScheduler::builder(cron_agent)
        .turn_events(turn_events.clone())
        .with_dream(dream, 10)
        .on_completed(move |msg: &InboundMessage, reply: &str| {
            // 仅 cron 在此统一投递（chat 投递由连接侧 mux + BusTurnRunner 完成）。
            if msg.metadata.get("cron_job_id").is_none() {
                return;
            }
            let session_key = msg.session_key();
            let chat_id = msg.chat_id.clone();
            // transcript 落库（origin 会话历史可回看，webui-thread GET 读取）。
            let _ = cron_transcript.append_turn(&session_key, &msg.content, reply);
            // 向在线连接实时推送：assistant 回复 + session_updated 刷新侧栏。
            // 无在线连接（push 返回 0）时静默——transcript 已落，下次打开可见。
            cron_hub.push(
                &chat_id,
                &serde_json::json!({"event": "message", "chat_id": chat_id, "text": reply}),
            );
            cron_hub.push(
                &chat_id,
                &serde_json::json!({"event": "session_updated", "chat_id": chat_id}),
            );
            // 推进 cron 状态（submit 时经 metadata 携带 job id）。
            if let Some(job_id) = msg
                .metadata
                .get("cron_job_id")
                .and_then(serde_json::Value::as_str)
            {
                record_cron_run(&cron_workspace, job_id);
            }
        })
        .build();
    rt.spawn(async move {
        cron_scheduler.run(bus_rx).await;
    });

    // cron 调度：tokio interval task，submit 语义投递进共享 bus。chat 已并入同一 bus，
    // 同 session 活跃时的让位由调度器 pending 队列处理（不再需要跨实例 busy 注册表）。
    let _cron_async = {
        let tx = bus_tx;
        AsyncCronScheduler::spawn(
            rt.handle(),
            CronService::new(&workspace),
            move |job: &CronJob| {
                if let Ok(mut msg) = cron_submit_message(job) {
                    msg.metadata
                        .insert("cron_job_id".into(), serde_json::json!(job.id));
                    let _ = tx.try_publish(msg);
                }
            },
            cron_poll_interval(),
        )
    };

    // 桌面窗口加载内嵌应用。
    let url = format!("http://{http_addr}/");
    eprintln!("Lure desktop 已启动: {url}（WS: {url}ws）");

    // headless：不开窗口，打印机器可读 URL 后 park，供 E2E 真实浏览器驱动后端。
    if args.headless {
        println!("LURE_HTTP_URL={url}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        // 在 tokio runtime 内永久挂起主 future：与 thread::park 等价（进程保活），
        // 顺带验证 runtime block_on 语义；WS/HTTP/cron 线程不受影响。
        rt.block_on(std::future::pending::<()>());
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
