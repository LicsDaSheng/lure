//! 映射上游 `tests/cron/test_session_delivery.py`。

use lure_core::cron::{origin_delivery_context, CronJob, CronPayload, CronSchedule};
use serde_json::json;

fn bound_job(payload: CronPayload) -> CronJob {
    CronJob {
        id: "job".to_string(),
        name: "bound".to_string(),
        enabled: true,
        schedule: CronSchedule::every(1000),
        payload,
        state: Default::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
        delete_after_run: false,
    }
}

#[test]
fn origin_context_uses_explicit_origin_fields() {
    let metadata = json!({"thread_id": "777", "parent_channel_id": "456"});
    let payload = CronPayload {
        message: "check".to_string(),
        session_key: Some("discord:456:thread:777".to_string()),
        origin_channel: Some("discord".to_string()),
        origin_chat_id: Some("777".to_string()),
        origin_metadata: metadata.as_object().unwrap().clone(),
    };

    let (channel, chat_id, returned) = origin_delivery_context(&bound_job(payload)).unwrap();
    assert_eq!(channel, "discord");
    assert_eq!(chat_id, "777");
    assert_eq!(returned.get("thread_id").unwrap(), "777");
}

#[test]
fn origin_context_rejects_missing_origin_fields() {
    let payload = CronPayload {
        message: "check".to_string(),
        session_key: Some("websocket:chat-1".to_string()),
        ..CronPayload::default()
    };
    let err = origin_delivery_context(&bound_job(payload)).unwrap_err();
    assert!(err.to_string().contains("missing origin delivery context"));
}
