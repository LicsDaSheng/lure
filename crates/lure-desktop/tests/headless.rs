//! `--headless`：不开窗口，起进程内 HTTP/WS server 并 park，stdout 打印机器可读
//! `LURE_HTTP_URL=...` 供 E2E（Playwright）解析。为真实浏览器契约测试提供后端。

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

/// 启动 headless 二进制，读出 stdout 的 `LURE_HTTP_URL=...`，返回 (子进程, url, workspace)。
/// workspace TempDir 随返回值存活，调用方持有以控制生命周期。
fn spawn_headless(extra: &[&str]) -> (Child, String, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut args = vec!["--headless", "--model", "echo", "--workspace"];
    args.push(dir.path().to_str().unwrap());
    args.extend_from_slice(extra);
    let mut child = Command::new(env!("CARGO_BIN_EXE_lure-desktop"))
        .args(&args)
        .stdout(Stdio::piped())
        .spawn()
        .expect("启动 lure-desktop --headless 失败");

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap_or_default();
            if let Some(url) = line.strip_prefix("LURE_HTTP_URL=") {
                let _ = tx.send(url.trim().to_string());
                break;
            }
        }
    });
    let url = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("headless 应打印 LURE_HTTP_URL");
    (child, url, dir)
}

/// 取一个当前空闲的本地端口（bind :0 后立即释放）。
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// 收尾子进程，避免测试残留。
fn kill(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn headless_prints_http_url_and_serves_bootstrap() {
    let (child, url, _dir) = spawn_headless(&[]);

    // 真实 HTTP 打 bootstrap：无需 window，验证进程内 server 已服务。
    let resp = ureq::get(&format!("{url}webui/bootstrap")).call();
    let result = (|| -> Result<(u16, String), String> {
        match resp {
            Ok(r) => Ok((r.status(), r.into_string().map_err(|e| e.to_string())?)),
            Err(ureq::Error::Status(code, r)) => Ok((code, r.into_string().unwrap_or_default())),
            Err(e) => Err(format!("传输错误: {e}")),
        }
    })();

    let assert = || {
        let (status, body) = result.expect("bootstrap 请求应完成");
        assert_eq!(status, 200, "bootstrap 应 200，实际 {status}: {body}");
        let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(
            payload["api_token"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "bootstrap 应含非空 api_token: {body}"
        );
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(assert));
    kill(child);
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn headless_binds_requested_http_port() {
    // Playwright 用固定端口做 webServer 轮询，故 --http-port 须反映在 URL 上。
    let port = free_port();
    let port_str = port.to_string();
    let (child, url, _dir) = spawn_headless(&["--http-port", &port_str]);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            url.contains(&format!(":{port}/")),
            "URL 应含指定端口 {port}，实际 {url}"
        );
    }));
    kill(child);
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn headless_cron_scheduler_runs_due_job_into_transcript() {
    use lure_core::cron::{CronJob, CronJobState, CronPayload, CronSchedule, CronStore};
    use lure_core::webui::transcript::TranscripStore;

    let dir = tempfile::tempdir().unwrap();

    // 预置一个立即到期的循环 cron job：origin websocket:t1，消息 "ping"。
    let mut store = CronStore::load(dir.path()).unwrap();
    let job = CronJob {
        id: "j1".to_string(),
        name: "ping-job".to_string(),
        enabled: true,
        schedule: CronSchedule::every(1),
        payload: CronPayload {
            message: "ping".to_string(),
            session_key: Some("websocket:t1".to_string()),
            origin_channel: Some("websocket".to_string()),
            origin_chat_id: Some("t1".to_string()),
            ..CronPayload::default()
        },
        state: CronJobState::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
        delete_after_run: false,
    };
    store.add(job, 0).unwrap();

    // 短轮询启动 headless（--model echo 离线确定性）。
    let mut child = Command::new(env!("CARGO_BIN_EXE_lure-desktop"))
        .args([
            "--headless",
            "--model",
            "echo",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .env("LURE_CRON_POLL_MS", "200")
        .stdout(Stdio::piped())
        .spawn()
        .expect("启动 headless 失败");

    // 轮询 transcript：cron 跑通后应写入 (ping, echo: ping)。
    let webui_dir = dir.path().join("webui");
    let start = std::time::Instant::now();
    let mut hit = false;
    while start.elapsed() < Duration::from_secs(10) {
        if let Ok(store) = TranscripStore::new(&webui_dir) {
            if let Ok(Some(thread)) = store.read_thread("websocket:t1") {
                if thread.to_string().contains("echo: ping") {
                    hit = true;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(150));
    }

    let _ = child.kill();
    let _ = child.wait();
    assert!(hit, "cron 调度应把到期 job 的产出写入 transcript");
}
