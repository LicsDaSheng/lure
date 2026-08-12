//! 映射上游 `nanobot/session/keys.py` 的 session key 行为。

use lure_core::session::{session_key_for_channel, UNIFIED_SESSION_KEY};

#[tokio::test]
async fn channel_key_joins_channel_and_chat() {
    assert_eq!(
        session_key_for_channel("telegram", "12345", false),
        "telegram:12345"
    );
}

#[tokio::test]
async fn unified_session_overrides_channel_key() {
    assert_eq!(UNIFIED_SESSION_KEY, "unified:default");
    assert_eq!(
        session_key_for_channel("telegram", "12345", true),
        UNIFIED_SESSION_KEY
    );
}
