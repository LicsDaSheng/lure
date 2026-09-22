use std::path::PathBuf;
use std::time::Duration;

use lure_core::LureEvent;
use lure_rpc::{PiProcessConfig, PiRpcClient};

fn temporary_working_directory(suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("lure-pi-{suffix}-{}", std::process::id()))
}

#[tokio::test]
#[ignore = "需要本机已安装、已认证且可联网的 Pi"]
async fn completes_a_real_streaming_pi_rpc_conversation() {
    let working_directory = temporary_working_directory("smoke");
    std::fs::create_dir_all(&working_directory).unwrap();
    let config = PiProcessConfig::for_working_directory(&working_directory);
    let (client, state) = PiRpcClient::connect(config).await.unwrap();
    assert!(!state.session_id.is_empty());

    let mut events = client.subscribe();
    client
        .prompt("只回复 RPC_OK，不要调用工具。")
        .await
        .unwrap();

    let mut final_text = String::new();
    let mut turn_stops: Vec<Option<String>> = Vec::new();
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            match events.recv().await.unwrap() {
                LureEvent::AssistantMessageCompleted { text, .. } => final_text = text,
                LureEvent::TurnEnded { stop_reason, .. } => turn_stops.push(stop_reason),
                LureEvent::RunSettled => break,
                _ => {}
            }
        }
    })
    .await
    .expect("真实 Pi RPC 对话应在两分钟内结束");

    client.stop().await.unwrap();
    let _ = std::fs::remove_dir_all(&working_directory);
    assert!(final_text.contains("RPC_OK"), "实际回复：{final_text}");
    // 真实 Pi 在每次运行里都会给出 turn 边界，这是最终结果的权威来源。
    assert!(!turn_stops.is_empty(), "真实运行应包含 turn_end");
    assert_eq!(
        turn_stops.last().cloned().flatten().as_deref(),
        Some("stop"),
        "最后一次 turn 应以 stop 结束，实际：{turn_stops:?}"
    );
}

#[tokio::test]
#[ignore = "需要本机已安装、已认证且可联网的 Pi"]
async fn aborts_a_real_pi_run_and_reaches_settled() {
    let working_directory = temporary_working_directory("abort");
    std::fs::create_dir_all(&working_directory).unwrap();
    let config = PiProcessConfig::for_working_directory(&working_directory);
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let mut events = client.subscribe();

    client
        .prompt("请依次输出从 1 到 10000 的所有整数，每行一个。")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(events.recv().await.unwrap(), LureEvent::RunStarted) {
                break;
            }
        }
    })
    .await
    .expect("Pi 应开始运行");

    client.abort().await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(events.recv().await.unwrap(), LureEvent::RunSettled) {
                break;
            }
        }
    })
    .await
    .expect("中止后 Pi 应进入 settled");

    client.stop().await.unwrap();
    let _ = std::fs::remove_dir_all(&working_directory);
}
