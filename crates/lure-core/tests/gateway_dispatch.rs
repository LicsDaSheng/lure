//! Gateway 编排闭环：InboundMessage → AgentLoop → OutboundMessage → channel。
//!
//! 覆盖：闭环投递、channel 配置校验、启停不丢任务、未知 channel 路由错误、健康状态。

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::InboundMessage;
use lure_core::channel::{ChannelError, RecordingChannel};
use lure_core::gateway::{Gateway, GatewayError};
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;
use tempfile::tempdir;

fn gateway() -> (tempfile::TempDir, Gateway) {
    let dir = tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    (dir, Gateway::new(agent))
}

#[tokio::test]
async fn inbound_flows_through_agent_to_channel_as_echo_reply() {
    let (_dir, mut gateway) = gateway();
    // 保留共享投递日志，以便 channel 移入 gateway 后仍可断言投递内容。
    let channel = RecordingChannel::new("cli");
    let log = channel.delivery_log();
    gateway.register_channel(Box::new(channel)).unwrap();
    gateway.start();

    gateway.submit(InboundMessage::new("cli", "direct", "hello"));
    assert_eq!(gateway.dispatch_pending().await.unwrap(), 1);
    assert_eq!(gateway.pending_inbound(), 0);

    let delivered = log.lock().unwrap();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].channel, "cli");
    assert_eq!(delivered[0].chat_id, "direct");
    assert_eq!(delivered[0].content, "echo: hello");
}

#[tokio::test]
async fn register_channel_rejects_invalid_config() {
    let (_dir, mut gateway) = gateway();
    let channel = RecordingChannel::new("telegram").with_missing_config(&["token"]);

    let err = gateway.register_channel(Box::new(channel)).unwrap_err();
    assert!(matches!(err, ChannelError::MissingConfig { .. }));
    assert!(err.to_string().contains("token"));
}

#[tokio::test]
async fn stopped_gateway_preserves_pending_tasks() {
    let (_dir, mut gateway) = gateway();
    gateway
        .register_channel(Box::new(RecordingChannel::new("cli")))
        .unwrap();

    // 未启动：提交的任务不被处理也不丢弃。
    gateway.submit(InboundMessage::new("cli", "direct", "one"));
    gateway.submit(InboundMessage::new("cli", "direct", "two"));
    assert_eq!(gateway.dispatch_pending().await.unwrap(), 0);
    assert_eq!(gateway.pending_inbound(), 2);

    // 启动后全部处理。
    gateway.start();
    assert_eq!(gateway.dispatch_pending().await.unwrap(), 2);
    assert_eq!(gateway.pending_inbound(), 0);
}

#[tokio::test]
async fn unknown_target_channel_is_reported() {
    let (_dir, mut gateway) = gateway();
    // 注册一个 channel，但 inbound 来自未注册的 channel 名。
    gateway
        .register_channel(Box::new(RecordingChannel::new("cli")))
        .unwrap();
    gateway.start();
    gateway.submit(InboundMessage::new("telegram", "direct", "hi"));

    let err = gateway.dispatch_pending().await.unwrap_err();
    assert!(matches!(err, GatewayError::UnknownChannel(name) if name == "telegram"));
}

#[tokio::test]
async fn health_reflects_state() {
    let (_dir, mut gateway) = gateway();
    gateway
        .register_channel(Box::new(RecordingChannel::new("cli")))
        .unwrap();
    gateway.submit(InboundMessage::new("cli", "direct", "hi"));

    let health = gateway.health();
    assert!(!health.running);
    assert_eq!(health.channels, 1);
    assert_eq!(health.pending_inbound, 1);

    gateway.start();
    assert!(gateway.health().running);
}
