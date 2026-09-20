use std::path::PathBuf;
use std::time::Duration;

use lure_core::LureEvent;
use lure_rpc::{PiProcessConfig, PiRpcClient, RpcError, RpcImage};

fn fake_pi() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-pi.py")
}

fn fake_pi_without_handshake() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-pi-no-handshake.py")
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
