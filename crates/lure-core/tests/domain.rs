use lure_core::{
    ConnectionPhase, ConnectionSnapshot, ErrorCode, LureError, LureEvent, ModelSnapshot,
};

#[test]
fn connection_snapshot_starts_disconnected() {
    let snapshot = ConnectionSnapshot::default();

    assert_eq!(snapshot.phase, ConnectionPhase::Disconnected);
    assert!(snapshot.working_directory.is_none());
    assert!(snapshot.model.is_none());
    assert!(snapshot.error.is_none());
}

#[test]
fn domain_types_serialize_with_frontend_friendly_tags() {
    let event = LureEvent::AssistantTextDelta {
        content_index: 2,
        delta: "你好".into(),
    };
    let json = serde_json::to_value(event).unwrap();

    assert_eq!(json["type"], "assistant_text_delta");
    assert_eq!(json["contentIndex"], 2);
    assert_eq!(json["delta"], "你好");
}

#[test]
fn snapshot_contains_session_and_model_details() {
    let snapshot = ConnectionSnapshot {
        phase: ConnectionPhase::Ready,
        working_directory: Some("/tmp/project".into()),
        session_id: Some("session-1".into()),
        session_file: Some("/tmp/session.jsonl".into()),
        model: Some(ModelSnapshot {
            provider: "openai".into(),
            id: "gpt-test".into(),
        }),
        thinking_level: Some("medium".into()),
        error: None,
    };

    assert_eq!(snapshot.model.unwrap().id, "gpt-test");
}

#[test]
fn errors_expose_stable_codes() {
    let error = LureError::new(ErrorCode::PiNotFound, "未找到 Pi");
    let json = serde_json::to_value(error).unwrap();

    assert_eq!(json["code"], "PI_NOT_FOUND");
    assert_eq!(json["message"], "未找到 Pi");
}
