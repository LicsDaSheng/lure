//! 用真机采集生成的 mock 会话驱动 RPC 集成行为。
//!
//! 采集文件与 mock 定义位于 `tests/fixtures/mock-sessions/`；`replay-pi.py`
//! 按录制回放响应与事件流，因此这里可以在没有真实 Pi 的情况下验证完整链路。

use std::path::{Path, PathBuf};
use std::time::Duration;

use lure_core::LureEvent;
use lure_rpc::{PiProcessConfig, PiRpcClient};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

/// 事件间隔默认 1ms，一次运行最多 2279 行，留出充足余量。
const RUN_LIMIT: Duration = Duration::from_secs(60);
/// 连续这么久没有新事件就认为这一段回放结束。
const QUIET: Duration = Duration::from_millis(600);

fn replay_pi() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/replay-pi.py")
}

async fn connect() -> (PiRpcClient, lure_rpc::RpcSessionState) {
    PiRpcClient::connect(PiProcessConfig::new(
        replay_pi(),
        env!("CARGO_MANIFEST_DIR"),
    ))
    .await
    .unwrap()
}

/// 收集事件直到安静一段时间，或超过整体上限。
///
/// 录制里最后一次运行停在等待用户输入，没有 `agent_settled`，因此不能只等 settled。
async fn collect_until_quiet(events: &mut Receiver<LureEvent>) -> Vec<LureEvent> {
    let deadline = tokio::time::Instant::now() + RUN_LIMIT;
    let mut collected = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(QUIET.min(remaining), events.recv()).await {
            Ok(Ok(event)) => collected.push(event),
            Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => break,
        }
    }
    collected
}

fn tool_completions(events: &[LureEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            LureEvent::ToolCompleted { tool_name, .. } => Some(tool_name.as_str()),
            _ => None,
        })
        .collect()
}

async fn replay_run(
    client: &PiRpcClient,
    events: &mut Receiver<LureEvent>,
    index: usize,
) -> Vec<LureEvent> {
    client.prompt(format!("第 {index} 次测试")).await.unwrap();
    collect_until_quiet(events).await
}

#[tokio::test]
async fn replays_the_recorded_handshake() {
    let (client, state) = connect().await;

    let model = state.model.clone().unwrap();
    assert_eq!(model.provider, "openai-codex");
    assert_eq!(model.id, "gpt-5.5");
    assert_eq!(state.thinking_level, "medium");
    assert!(state.session_id.len() > 8, "sessionId 应来自真实会话");
    let session_file = state.session_file.expect("sessionFile 应指向真实会话文件");
    assert!(
        Path::new(&session_file)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl")),
        "sessionFile 应是会话 JSONL：{session_file}"
    );

    client.stop().await.unwrap();
}

#[tokio::test]
async fn replays_a_recorded_run_as_normalized_events() {
    let (client, _) = connect().await;
    let mut events = client.subscribe();

    client
        .prompt("查看有哪些工具，并依次进行工具的测试。")
        .await
        .unwrap();
    let collected = collect_until_quiet(&mut events).await;

    assert!(
        matches!(
            collected.first(),
            Some(LureEvent::UserMessageAccepted { .. })
        ),
        "运行应从用户消息被接受开始，实际：{:?}",
        collected.first()
    );
    assert!(
        matches!(collected.last(), Some(LureEvent::RunSettled)),
        "录制里这次运行以 agent_settled 结束，实际：{:?}",
        collected.last()
    );
    assert!(
        collected
            .iter()
            .any(|event| matches!(event, LureEvent::RunStarted)),
        "运行应包含 run_started"
    );

    let completed_tools = tool_completions(&collected);
    assert!(
        completed_tools.contains(&"mcp"),
        "回放应包含 mcp 工具执行，实际：{completed_tools:?}"
    );
    assert!(
        completed_tools.len() >= 7,
        "录制的这次运行完成了 7 次工具调用，实际：{completed_tools:?}"
    );

    let thinking_deltas = collected
        .iter()
        .filter(|event| matches!(event, LureEvent::AssistantThinkingDelta { .. }))
        .count();
    let text_deltas = collected
        .iter()
        .filter(|event| matches!(event, LureEvent::AssistantTextDelta { .. }))
        .count();
    assert!(thinking_deltas > 0, "回放应包含思考增量");
    assert!(text_deltas > 0, "回放应包含正文增量");

    let completed_text = collected.iter().find_map(|event| match event {
        LureEvent::AssistantMessageCompleted { text, .. } => Some(text.clone()),
        _ => None,
    });
    assert!(
        completed_text.is_some_and(|text| !text.is_empty()),
        "回放应包含完整的助手消息"
    );

    // 真实录制的每次运行都有 turn_start/turn_end：它们是单次 assistant/tool
    // turn 的权威边界，且始终位于 message_end 之后、agent_end 之前。
    let turn_started = collected
        .iter()
        .position(|event| matches!(event, LureEvent::TurnStarted))
        .expect("回放应包含 turn_start");
    let turn_ended = collected
        .iter()
        .position(|event| matches!(event, LureEvent::TurnEnded { .. }))
        .expect("回放应包含 turn_end");
    let first_message_end = collected
        .iter()
        .position(|event| matches!(event, LureEvent::AssistantMessageCompleted { .. }))
        .expect("回放应包含 message_end");
    let run_finished = collected
        .iter()
        .position(|event| matches!(event, LureEvent::RunFinished { .. }))
        .expect("回放应包含 agent_end");
    assert!(turn_started < turn_ended, "turn_start 应在 turn_end 之前");
    assert!(
        first_message_end < turn_ended,
        "turn_end 应在 message_end 之后"
    );
    assert!(turn_ended < run_finished, "turn_end 应在 agent_end 之前");

    let turn_ended_count = collected
        .iter()
        .filter(|event| matches!(event, LureEvent::TurnEnded { .. }))
        .count();
    assert!(
        turn_ended_count >= 6,
        "这次录制完成了 7 次工具调用，应有多个 turn 边界，实际：{turn_ended_count}"
    );
    assert!(
        collected.iter().any(|event| matches!(
            event,
            LureEvent::TurnEnded { stop_reason: Some(reason), .. } if reason == "stop"
        )),
        "录制里最后一次 turn 以 stop 结束"
    );

    client.stop().await.unwrap();
}

#[tokio::test]
async fn replays_every_recorded_run_in_order() {
    let (client, _) = connect().await;
    let mut events = client.subscribe();

    for index in 1..=2 {
        let collected = replay_run(&client, &mut events, index).await;
        assert!(
            matches!(collected.last(), Some(LureEvent::RunSettled)),
            "第 {index} 次运行应以 RunSettled 结束"
        );
    }

    // 采集的第三次运行在结束后又收到 vision 扩展的交互请求。
    let third = replay_run(&client, &mut events, 3).await;
    let settled_at = third
        .iter()
        .position(|event| matches!(event, LureEvent::RunSettled))
        .expect("第三次运行也以 agent_settled 结束");
    assert!(
        third[settled_at + 1..]
            .iter()
            .any(|event| matches!(event, LureEvent::ExtensionUiRequested { .. })),
        "运行结束后仍可能出现扩展 UI 请求"
    );

    client.stop().await.unwrap();
}

#[tokio::test]
async fn the_fourth_prompt_only_answers_and_replays_no_events() {
    let (client, _) = connect().await;
    let mut events = client.subscribe();

    for index in 1..=3 {
        replay_run(&client, &mut events, index).await;
    }

    // 第四次 prompt 在采集里只有响应，没有后续事件。
    client.prompt("第四次没有事件").await.unwrap();
    let extra = collect_until_quiet(&mut events).await;

    // 用户消息被接受是桌面端发出 prompt 时根据请求就地产生的，不是回放的事件。
    assert!(
        extra
            .iter()
            .all(|event| matches!(event, LureEvent::UserMessageAccepted { .. })),
        "第四次运行不应凭空产生事件：{extra:?}"
    );
    client.stop().await.unwrap();
}

#[tokio::test]
async fn synthesizes_only_the_responses_missing_from_the_recording() {
    let (client, state) = connect().await;

    let session = client.new_session().await.unwrap();
    assert_ne!(session.session_id, state.session_id, "新会话应有独立标识");
    assert!(session.session_id.ends_with("-mock-1"));
    assert_eq!(session.message_count, 0);

    assert_eq!(client.set_thinking_level("high").await.unwrap(), "high");
    client.abort().await.unwrap();

    client.stop().await.unwrap();
}
