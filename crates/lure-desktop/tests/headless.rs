//! `--headless`：不开窗口，起进程内 HTTP/WS server 并 park，stdout 打印机器可读
//! `LURE_HTTP_URL=...` 供 E2E（Playwright）解析。为真实浏览器契约测试提供后端。

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;

fn desktop() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lure-desktop"))
}

#[test]
fn headless_prints_http_url_and_serves_bootstrap() {
    let dir = tempdir().unwrap();
    let mut child = desktop()
        .args([
            "--headless",
            "--model",
            "echo",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("启动 lure-desktop --headless 失败");

    // 子线程读 stdout，直到捕获 LURE_HTTP_URL=... 行。
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

    // 真实 HTTP 打 bootstrap：无需 window，验证进程内 server 已服务。
    let resp = ureq::get(&format!("{url}webui/bootstrap")).call();
    let result = (|| -> Result<(u16, String), String> {
        match resp {
            Ok(r) => Ok((r.status(), r.into_string().map_err(|e| e.to_string())?)),
            Err(ureq::Error::Status(code, r)) => Ok((code, r.into_string().unwrap_or_default())),
            Err(e) => Err(format!("传输错误: {e}")),
        }
    })();

    // 无论断言成败都要收掉子进程，避免测试残留。
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
    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
}
