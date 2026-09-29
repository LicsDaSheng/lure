use std::path::PathBuf;
use std::time::Duration;

use lure_core::LureEvent;
use lure_rpc::{PiProcessConfig, PiRpcClient, RpcError, RpcImage, SessionSwitch, SpawnEnv};

fn fake_pi() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-pi.py")
}

fn fake_pi_without_handshake() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-pi-no-handshake.py")
}

fn fake_pi_rpc_only() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-pi-rpc-only.py")
}

#[tokio::test]
async fn connects_with_handshake_in_the_selected_working_directory() {
    let working_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config = PiProcessConfig::new(fake_pi(), &working_directory);

    let (client, state) = PiRpcClient::connect(config).await.unwrap();

    assert_eq!(state.session_id, "fake-session");
    assert_eq!(state.model.unwrap().id, "fake-model");
    assert_eq!(state.thinking_level, "medium");
    assert_eq!(
        state.session_file.unwrap(),
        working_directory
            .join("session.jsonl")
            .display()
            .to_string()
    );

    client.stop().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn provided_spawn_env_replaces_child_environment() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!("lure-spawn-env-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let dump_path = temp.join("child.env");
    let wrapper = temp.join("fake-pi-wrapper.sh");
    std::fs::write(
        &wrapper,
        "#!/bin/sh\nenv > \"$LURE_ENV_DUMP\"\nexec \"$LURE_FAKE_PI\" \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();

    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let mut config = PiProcessConfig::new(&wrapper, env!("CARGO_MANIFEST_DIR"));
    config.spawn_env = SpawnEnv::Provided(vec![
        ("PATH".into(), inherited_path),
        ("LURE_SPAWN_MARKER".into(), "synthesized".into()),
        ("LURE_ENV_DUMP".into(), dump_path.as_os_str().to_os_string()),
        ("LURE_FAKE_PI".into(), fake_pi().as_os_str().to_os_string()),
    ]);

    let (client, state) = PiRpcClient::connect(config).await.unwrap();
    assert_eq!(state.session_id, "fake-session");
    client.stop().await.unwrap();

    let dump = std::fs::read_to_string(&dump_path).unwrap();
    let _ = std::fs::remove_dir_all(&temp);
    // 合成变量到达子进程。
    assert!(dump.contains("LURE_SPAWN_MARKER=synthesized"));
    // env_clear 生效：cargo 测试进程特有的变量不泄入子进程。
    assert!(!dump.contains("CARGO_MANIFEST_DIR="));
}

#[tokio::test]
async fn connects_without_running_a_separate_version_process() {
    let config = PiProcessConfig::new(fake_pi_rpc_only(), env!("CARGO_MANIFEST_DIR"));

    let (client, state) = PiRpcClient::connect(config).await.unwrap();

    assert_eq!(state.session_id, "rpc-only-session");
    assert_eq!(state.model.unwrap().id, "rpc-only-model");
    client.stop().await.unwrap();
}

#[tokio::test]
async fn prompt_emits_normalized_events_and_only_settles_on_agent_settled() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let mut events = client.subscribe();

    client.prompt("测试消息").await.unwrap();

    let mut received = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        let settled = matches!(event, LureEvent::RunSettled);
        received.push(event);
        if settled {
            break;
        }
    }

    assert!(matches!(
        received.first(),
        Some(LureEvent::UserMessageAccepted { message, .. }) if message == "测试消息"
    ));
    assert!(received.iter().any(|event| matches!(
        event,
        LureEvent::UserMessageObserved { message } if message == "测试消息"
    )));
    assert!(received.iter().any(|event| matches!(
        event,
        LureEvent::AssistantTextDelta { delta, .. } if delta == "RPC_OK"
    )));
    assert!(received.iter().any(|event| matches!(
        event,
        LureEvent::AssistantThinkingDelta { delta, .. } if delta == "思考"
    )));
    assert!(received.iter().any(|event| matches!(
        event,
        LureEvent::ToolCompleted { tool_call_id, is_error, .. }
            if tool_call_id == "tool-1" && !is_error
    )));

    let run_finished = received
        .iter()
        .position(|event| matches!(event, LureEvent::RunFinished { .. }))
        .unwrap();
    let run_settled = received
        .iter()
        .position(|event| matches!(event, LureEvent::RunSettled))
        .unwrap();
    assert!(run_finished < run_settled);

    // turn 边界跟随 message_end，并提供该轮次的权威终止原因。
    let message_end = received
        .iter()
        .position(|event| matches!(event, LureEvent::AssistantMessageCompleted { .. }))
        .unwrap();
    let turn_ended = received
        .iter()
        .position(|event| matches!(event, LureEvent::TurnEnded { .. }))
        .unwrap();
    assert!(message_end < turn_ended && turn_ended < run_finished);
    assert!(received.iter().any(|event| matches!(
        event,
        LureEvent::TurnEnded { stop_reason: Some(reason), .. } if reason == "stop"
    )));

    client.stop().await.unwrap();
}

#[tokio::test]
async fn keeps_stderr_out_of_the_json_protocol() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let mut events = client.subscribe();

    client.prompt("stderr").await.unwrap();
    let mut saw_user = false;
    let mut saw_stderr = false;
    tokio::time::timeout(Duration::from_secs(2), async {
        while !(saw_user && saw_stderr) {
            match events.recv().await.unwrap() {
                LureEvent::UserMessageAccepted { .. } => saw_user = true,
                LureEvent::ProcessStderr { message } => {
                    saw_stderr = message == "not-json-from-stderr";
                }
                LureEvent::ProtocolError { message } => panic!("stderr 污染了协议：{message}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    client.stop().await.unwrap();
}

#[tokio::test]
async fn exposes_extension_dialogs_and_forwards_the_user_response() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let mut events = client.subscribe();

    client.prompt("extension").await.unwrap();

    let request_id = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let LureEvent::ExtensionUiRequested {
                request_id, method, ..
            } = events.recv().await.unwrap()
            {
                assert_eq!(method, "confirm");
                break request_id;
            }
        }
    })
    .await
    .unwrap();

    client
        .respond_to_extension(&request_id, Some(serde_json::json!(true)), false)
        .await
        .unwrap();

    let acknowledgement = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let LureEvent::Notification { message, .. } = events.recv().await.unwrap() {
                break message;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(acknowledgement, "response-received");

    client.stop().await.unwrap();
}

#[tokio::test]
async fn manages_models_thinking_commands_and_image_prompts() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();

    let models = client.get_available_models().await.unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[1].id, "other-model");

    let selected = client.set_model("test", "other-model").await.unwrap();
    assert_eq!(selected.id, "other-model");
    assert_eq!(client.set_thinking_level("high").await.unwrap(), "high");

    let commands = client.get_commands().await.unwrap();
    assert_eq!(commands[0].name, "review");

    client
        .prompt_with_images(
            "images",
            vec![RpcImage {
                data: "aGVsbG8=".into(),
                mime_type: "image/png".into(),
            }],
        )
        .await
        .unwrap();

    client.stop().await.unwrap();
}

#[tokio::test]
async fn creates_a_new_session_on_the_existing_rpc_client() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, state) = PiRpcClient::connect(config).await.unwrap();

    let next_state = client.new_session().await.unwrap();

    assert_ne!(next_state.session_id, state.session_id);
    assert_eq!(next_state.session_id, "fake-session-2");
    client.stop().await.unwrap();
}

#[tokio::test]
async fn switches_to_a_recorded_session_and_reports_the_new_state() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, state) = PiRpcClient::connect(config).await.unwrap();

    let switched = client
        .switch_session("/tmp/recorded-session.jsonl")
        .await
        .unwrap();

    let SessionSwitch::Switched(next) = switched else {
        panic!("应当完成切换");
    };
    assert_eq!(
        next.session_file.as_deref(),
        Some("/tmp/recorded-session.jsonl")
    );
    assert_eq!(next.session_id, "switched-session");
    assert_ne!(next.session_id, state.session_id);
    client.stop().await.unwrap();
}

#[tokio::test]
async fn reports_a_session_switch_cancelled_by_an_extension() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();

    let switched = client.switch_session("/tmp/cancelled.jsonl").await.unwrap();

    assert_eq!(switched, SessionSwitch::Cancelled);
    client.stop().await.unwrap();
}

#[tokio::test]
async fn reads_session_entries_with_an_optional_cursor() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();

    let all = client.get_entries(None).await.unwrap();
    assert_eq!(all.leaf_id.as_deref(), Some("entry-2"));
    assert_eq!(all.entries.len(), 2);
    assert_eq!(all.entries[0]["id"], "entry-1");

    let after_cursor = client.get_entries(Some("entry-1")).await.unwrap();
    assert_eq!(after_cursor.entries.len(), 1);
    assert_eq!(after_cursor.entries[0]["id"], "entry-2");

    assert!(matches!(
        client.get_entries(Some("missing")).await,
        Err(RpcError::CommandRejected { command, .. }) if command == "get_entries"
    ));
    client.stop().await.unwrap();
}

#[tokio::test]
async fn correlates_concurrent_responses_by_request_id() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let prompt_client = client.clone();
    let prompt = tokio::spawn(async move { prompt_client.prompt("out-of-order").await });
    tokio::time::sleep(Duration::from_millis(10)).await;

    client.abort().await.unwrap();
    prompt.await.unwrap().unwrap();

    client.stop().await.unwrap();
}

#[tokio::test]
async fn times_out_commands_that_receive_no_response() {
    let mut config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    config.command_timeout = Duration::from_millis(50);
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();

    assert!(matches!(
        client.prompt("never-respond").await,
        Err(RpcError::Timeout(command)) if command == "prompt"
    ));

    client.stop().await.unwrap();
}

#[tokio::test]
async fn stop_reclaims_the_process_and_rejects_future_commands() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();

    client.stop().await.unwrap();

    assert!(matches!(
        client.prompt("不应发送").await,
        Err(RpcError::ActorStopped)
    ));
    client.stop().await.unwrap();
}

#[tokio::test]
async fn times_out_an_unresponsive_handshake() {
    let mut config = PiProcessConfig::new(fake_pi_without_handshake(), env!("CARGO_MANIFEST_DIR"));
    config.handshake_timeout = Duration::from_millis(50);

    let error = PiRpcClient::connect(config).await.unwrap_err();

    assert!(matches!(error, RpcError::Timeout(command) if command == "get_state"));
}

#[tokio::test]
async fn reports_a_missing_pi_executable() {
    let config = PiProcessConfig::new(
        "/definitely/missing/lure-pi-executable",
        env!("CARGO_MANIFEST_DIR"),
    );

    let error = PiRpcClient::connect(config).await.unwrap_err();

    assert!(matches!(error, RpcError::PiNotFound(_)));
}

#[tokio::test]
async fn rejects_a_missing_working_directory_before_spawning() {
    let config = PiProcessConfig::new(fake_pi(), "/definitely/missing/lure-directory");

    let error = PiRpcClient::connect(config).await.unwrap_err();

    assert!(matches!(error, RpcError::InvalidWorkingDirectory(_)));
}

#[tokio::test]
async fn steer_and_follow_up_queue_messages_and_report_queue_changes() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let mut events = client.subscribe();

    client.steer("先停下重构", vec![]).await.unwrap();
    client.follow_up("接着补测试", vec![]).await.unwrap();

    let mut queue_updates = Vec::new();
    while queue_updates.len() < 2 {
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        if let LureEvent::QueueChanged {
            steering,
            follow_up,
        } = event
        {
            queue_updates.push((steering, follow_up));
        }
    }

    assert_eq!(queue_updates[0], (vec!["先停下重构".to_owned()], vec![]));
    assert_eq!(
        queue_updates[1],
        (vec!["先停下重构".to_owned()], vec!["接着补测试".to_owned()])
    );

    client.stop().await.unwrap();
}

#[tokio::test]
async fn clear_queue_returns_cleared_messages() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();

    client.steer("插队", vec![]).await.unwrap();
    client.follow_up("排队", vec![]).await.unwrap();

    let cleared = client.clear_queue().await.unwrap();
    assert_eq!(cleared.steering, vec!["插队".to_owned()]);
    assert_eq!(cleared.follow_up, vec!["排队".to_owned()]);

    client.stop().await.unwrap();
}

#[tokio::test]
async fn queue_commands_do_not_emit_user_message_accepted() {
    let config = PiProcessConfig::new(fake_pi(), env!("CARGO_MANIFEST_DIR"));
    let (client, _state) = PiRpcClient::connect(config).await.unwrap();
    let mut events = client.subscribe();

    client.steer("插队", vec![]).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, LureEvent::QueueChanged { .. }));

    client.stop().await.unwrap();
}
