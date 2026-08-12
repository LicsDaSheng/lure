//! 映射上游 `nanobot/bus` 的消息契约与队列语义。

use lure_core::bus::{InboundMessage, MessageBus, OutboundMessage};

#[tokio::test]
async fn session_key_defaults_to_channel_chat() {
    let msg = InboundMessage::new("telegram", "chat-1", "hi");
    assert_eq!(msg.session_key(), "telegram:chat-1");
}

#[tokio::test]
async fn session_key_override_takes_precedence() {
    let mut msg = InboundMessage::new("telegram", "chat-1", "hi");
    msg.session_key_override = Some("thread:xyz".to_string());
    assert_eq!(msg.session_key(), "thread:xyz");
}

#[tokio::test]
async fn outbound_reply_targets_source_channel_and_chat() {
    let inbound = InboundMessage::new("slack", "chat-9", "ping");
    let outbound = OutboundMessage::reply(&inbound, "pong");
    assert_eq!(outbound.channel, "slack");
    assert_eq!(outbound.chat_id, "chat-9");
    assert_eq!(outbound.content, "pong");
}

#[tokio::test]
async fn bus_is_fifo_and_tracks_sizes() {
    let mut bus = MessageBus::new();
    assert_eq!(bus.inbound_size(), 0);

    bus.publish_inbound(InboundMessage::new("cli", "a", "first"));
    bus.publish_inbound(InboundMessage::new("cli", "a", "second"));
    assert_eq!(bus.inbound_size(), 2);

    assert_eq!(bus.consume_inbound().unwrap().content, "first");
    assert_eq!(bus.consume_inbound().unwrap().content, "second");
    assert!(bus.consume_inbound().is_none());

    bus.publish_outbound(OutboundMessage::new("cli", "a", "reply"));
    assert_eq!(bus.outbound_size(), 1);
    assert_eq!(bus.consume_outbound().unwrap().content, "reply");
}
