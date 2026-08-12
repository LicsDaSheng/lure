//! 异步 bus 契约测试（Stage 1）：channel → agent 的 async 投递面。
//!
//! 契约（对齐上游 asyncio.Queue 语义）：
//! 1. FIFO 消费顺序。
//! 2. 多 producer（sender clone）并发发布，总量不丢。
//! 3. 有界容量 + backpressure：满时 send 等待，消费后放行。
//! 4. 所有 sender drop 后 consume 返回 None（通道关闭）。

use std::time::Duration;

use lure_core::bus::{async_bus_channel, InboundMessage};

fn msg(chat_id: &str, content: &str) -> InboundMessage {
    InboundMessage::new("test", chat_id, content)
}

#[tokio::test]
async fn fifo_consumption_order() {
    let (tx, mut rx) = async_bus_channel(8);
    for i in 0..3 {
        tx.publish(msg("c1", &format!("m{i}"))).await.unwrap();
    }
    drop(tx);
    let mut got = Vec::new();
    while let Some(m) = rx.consume().await {
        got.push(m.content);
    }
    assert_eq!(
        got,
        vec!["m0".to_string(), "m1".to_string(), "m2".to_string()]
    );
}

#[tokio::test]
async fn multi_producer_no_loss() {
    let (tx, mut rx) = async_bus_channel(8);
    let tx1 = tx.clone();
    let tx2 = tx.clone();
    let a = tokio::spawn(async move {
        for i in 0..3 {
            tx1.publish(msg("a", &format!("a{i}"))).await.unwrap();
        }
    });
    let b = tokio::spawn(async move {
        for i in 0..3 {
            tx2.publish(msg("b", &format!("b{i}"))).await.unwrap();
        }
    });
    a.await.unwrap();
    b.await.unwrap();
    drop(tx); // 原始 sender drop → 通道关闭，消费循环可退出。
    let mut count = 0;
    while let Some(m) = rx.consume().await {
        assert!(m.content.starts_with('a') || m.content.starts_with('b'));
        count += 1;
    }
    assert_eq!(count, 6);
}

#[tokio::test]
async fn bounded_capacity_backpressure() {
    let (tx, mut rx) = async_bus_channel(1);
    tx.publish(msg("c1", "first")).await.unwrap();
    // 第二条在容量满时应阻塞（receiver 未消费）。
    let mut pending = tokio::spawn(async move {
        tx.publish(msg("c1", "second")).await.unwrap();
    });
    let blocked = tokio::time::timeout(Duration::from_millis(100), &mut pending).await;
    assert!(blocked.is_err(), "满容量时 send 应阻塞");
    // 消费一条后放行。
    let first = rx.consume().await.expect("第一条可消费");
    assert_eq!(first.content, "first");
    pending.await.unwrap();
    let second = rx.consume().await.expect("第二条可消费");
    assert_eq!(second.content, "second");
}

#[tokio::test]
async fn receiver_gets_none_after_all_senders_dropped() {
    let (tx, mut rx) = async_bus_channel(4);
    tx.publish(msg("c1", "only")).await.unwrap();
    drop(tx);
    assert!(rx.consume().await.is_some());
    assert!(
        rx.consume().await.is_none(),
        "全部 sender drop 后应返回 None"
    );
}
