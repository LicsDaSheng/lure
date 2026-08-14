//! Stage 5 WebSocket channel 化契约测试。
//!
//! 对齐 handbook Stage 5 + 上游 `channels/websocket/runtime.py`：
//! 1. `TurnEventRegistry`：按 turn_id 路由 turn 事件（delta/final/error/done）到发起连接。
//! 2. 调度核心流式：入站消息带 turn_id 时，`process_streaming` 的进度经 registry 转发。
//! 3. `BusTurnRunner`（WebSocketChannel 的 turn 侧）：发布到共享 bus + 等待事件流，
//!    替代每连接独立 AgentLoop——chat 与 cron 共用单实例调度核心。
//! 4. mux 全事件序列（goal_status → delta* → message → turn_end → session_updated）不变。

use std::time::Duration;

use tokio::sync::mpsc;

use lure_core::agent::scheduler::AgentLoopScheduler;
use lure_core::agent::{AgentLoop, ContextBuilder, ProgressEvent};
use lure_core::bus::{async_bus_channel, InboundMessage};
use lure_core::channel::turn_events::{TurnEvent, TurnEventRegistry};
use lure_core::channel::BusTurnRunner;
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;
use lure_core::webui::mux::{self, MuxSession, TurnRunner};
use serde_json::json;
use tempfile::TempDir;

/// 独立 echo 调度器（tempdir 随返回值存活到测试结束，session 目录不提前消失）。
fn fresh_scheduler(registry: TurnEventRegistry) -> (TempDir, AgentLoopScheduler) {
    let tmp = TempDir::new().unwrap();
    let sessions = SessionManager::new(tmp.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    let scheduler = AgentLoopScheduler::builder(agent)
        .turn_events(registry)
        .build();
    (tmp, scheduler)
}

// ---- TurnEventRegistry：按 turn_id 路由 -----------------------------------

#[tokio::test]
async fn turn_event_registry_routes_by_turn_id() {
    let registry = TurnEventRegistry::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<TurnEvent>();
    registry.register("turn-1", tx);

    assert!(registry.route("turn-1", TurnEvent::Delta("你".into())));
    assert_eq!(
        rx.recv().await,
        Some(TurnEvent::Delta("你".into())),
        "route 应投递到对应 receiver"
    );
    assert!(registry.route("turn-1", TurnEvent::Tool("read_file".into())));
    assert_eq!(
        rx.recv().await,
        Some(TurnEvent::Tool("read_file".into())),
        "tool 事件也应投递到对应 receiver"
    );

    registry.unregister("turn-1");
    assert!(
        !registry.route("turn-1", TurnEvent::Done),
        "unregister 后 route 应失败"
    );
}

// ---- 调度核心流式 turn 事件 ----------------------------------------------

/// 入站消息带 turn_id → dispatch 用 process_streaming，进度经 registry 转发。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_streams_turn_events_for_turn_id() {
    let registry = TurnEventRegistry::new();
    let (_tmp, scheduler) = fresh_scheduler(registry.clone());
    let (bus_tx, bus_rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(bus_rx));

    let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<TurnEvent>();
    registry.register("turn-1", evt_tx);

    let mut msg = InboundMessage::new("websocket", "t1", "ping");
    msg.metadata.insert("turn_id".into(), json!("turn-1"));
    bus_tx.publish(msg).await.unwrap();

    let mut got = Vec::new();
    while let Some(evt) = evt_rx.recv().await {
        let is_done = matches!(evt, TurnEvent::Done);
        got.push(evt);
        if is_done {
            break;
        }
    }
    drop(bus_tx);
    run.await.unwrap();

    assert!(
        got.contains(&TurnEvent::Delta("echo: ping".into())),
        "应流式 delta: {got:?}"
    );
    assert!(
        got.contains(&TurnEvent::Final("echo: ping".into())),
        "应投递最终文本: {got:?}"
    );
    assert!(
        got.iter().any(|e| matches!(e, TurnEvent::Done)),
        "应以 Done 收尾: {got:?}"
    );
    // 顺序：delta 在 final 之前。
    let d = got.iter().position(|e| matches!(e, TurnEvent::Delta(_)));
    let f = got.iter().position(|e| matches!(e, TurnEvent::Final(_)));
    assert!(
        d.is_some() && f.is_some() && d.unwrap() < f.unwrap(),
        "delta 先于 final"
    );
}

// ---- BusTurnRunner：发布 + 等待事件流（替代每连接 AgentLoop）---------------

/// BusTurnRunner 经共享 bus + 调度核心跑一轮 turn，把 delta 转发给 on_progress。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bus_turn_runner_runs_turn_through_shared_core() {
    let registry = TurnEventRegistry::new();
    let (_tmp, scheduler) = fresh_scheduler(registry.clone());
    let (bus_tx, bus_rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(bus_rx));

    let mut runner = BusTurnRunner::new(bus_tx.clone(), registry.clone());
    let mut deltas = Vec::new();
    let final_text = runner
        .run_turn("t1", "ping", &mut |event| {
            if let ProgressEvent::ContentDelta { text } = event {
                deltas.push(text);
            }
        })
        .await
        .unwrap();
    assert_eq!(final_text, "echo: ping");
    assert_eq!(deltas, vec!["echo: ping".to_string()]);

    drop(runner); // 释放 runner 持有的 bus sender，令调度器 run 退出
    drop(bus_tx);
    run.await.unwrap();
}

// ---- mux 全事件序列经 BusTurnRunner 保持不变 ------------------------------

/// mux（connection 侧协议层）挂 BusTurnRunner：message 帧 → 完整事件序列。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mux_full_sequence_unchanged_with_bus_runner() {
    let registry = TurnEventRegistry::new();
    let (_tmp, scheduler) = fresh_scheduler(registry.clone());
    let (bus_tx, bus_rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(bus_rx));

    let mut mux = MuxSession::new(
        BusTurnRunner::new(bus_tx.clone(), registry.clone()),
        tokio::runtime::Handle::current(),
    );
    let out = mux::collect_frames(
        &mut mux,
        &json!({"type": "message", "chat_id": "c1", "content": "ping"}),
    );
    let names: Vec<&str> = out
        .iter()
        .filter_map(|f| f.get("event").and_then(serde_json::Value::as_str))
        .collect();
    assert_eq!(
        names,
        vec![
            "goal_status",
            "delta",
            "message",
            "turn_end",
            "session_updated"
        ],
        "事件序列应保持不变: {names:?}"
    );
    assert_eq!(out[2]["text"], "echo: ping");

    drop(mux); // 释放 mux 内 runner 持有的 bus sender，令调度器 run 退出
    drop(bus_tx);
    run.await.unwrap();
}

// ---- 共享实例：chat 与 cron 同一调度核心 ----------------------------------

/// chat（turn_id）与 cron（cron_job_id）的 turn 都经同一调度核心，事件各自路由。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_and_cron_share_single_dispatch_core() {
    let registry = TurnEventRegistry::new();
    let (_tmp, scheduler) = fresh_scheduler(registry.clone());
    let (bus_tx, bus_rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(bus_rx));

    let (chat_tx, mut chat_rx) = mpsc::unbounded_channel::<TurnEvent>();
    registry.register("chat-1", chat_tx);
    let mut chat_msg = InboundMessage::new("websocket", "s1", "chat");
    chat_msg.metadata.insert("turn_id".into(), json!("chat-1"));

    let (cron_tx, mut cron_rx) = mpsc::unbounded_channel::<TurnEvent>();
    registry.register("cron-1", cron_tx);
    let mut cron_msg = InboundMessage::new("cron", "s1", "cron-ping");
    cron_msg.metadata.insert("turn_id".into(), json!("cron-1"));
    cron_msg
        .metadata
        .insert("cron_job_id".into(), json!("job-1"));

    bus_tx.publish(chat_msg).await.unwrap();
    bus_tx.publish(cron_msg).await.unwrap();

    // 同 session 串行（FIFO）：chat 先、cron 随后；事件各自路由到对应 receiver。
    let chat_done = drain_until_done(&mut chat_rx).await;
    let cron_done = drain_until_done(&mut cron_rx).await;
    drop(bus_tx);
    run.await.unwrap();

    assert!(chat_done.contains(&TurnEvent::Final("echo: chat".into())));
    assert!(cron_done.contains(&TurnEvent::Final("echo: cron-ping".into())));
}

async fn drain_until_done(rx: &mut mpsc::UnboundedReceiver<TurnEvent>) -> Vec<TurnEvent> {
    let mut out = Vec::new();
    while let Some(evt) = rx.recv().await {
        let is_done = matches!(evt, TurnEvent::Done);
        out.push(evt);
        if is_done {
            break;
        }
    }
    out
}

// 让 `Duration` 导入不至于未使用（调度器心跳超时测试预留）。
#[allow(dead_code)]
fn _poll_interval() -> Duration {
    Duration::from_millis(250)
}
