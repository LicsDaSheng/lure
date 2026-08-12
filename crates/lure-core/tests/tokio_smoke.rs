//! tokio 运行时 smoke（Stage 0）：验证异步基础设施可用。
//!
//! 契约：
//! 1. `#[tokio::test]` 下 spawn + mpsc 通道往返（消息传递、发送端 drop 后接收端收尾）。
//! 2. `tokio::time::timeout` 超时语义（慢 future 被超时取消）。
//! 3. 多任务并发（`tokio::join!` 汇合）。

use std::time::Duration;

#[tokio::test]
async fn mpsc_channel_roundtrip() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    tokio::spawn(async move {
        for i in 0..4 {
            tx.send(i).await.unwrap();
        }
    });
    let mut got = Vec::new();
    while let Some(v) = rx.recv().await {
        got.push(v);
    }
    assert_eq!(got, vec![0, 1, 2, 3]);
}

#[tokio::test]
async fn timeout_cancels_slow_future() {
    let slow = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        42
    };
    let result = tokio::time::timeout(Duration::from_millis(50), slow).await;
    assert!(result.is_err(), "慢 future 应被 timeout 取消");
}

#[tokio::test]
async fn concurrent_tasks_join() {
    let t1 = tokio::spawn(async { 1 + 1 });
    let t2 = tokio::spawn(async { 2 + 2 });
    let (a, b) = tokio::join!(t1, t2);
    assert_eq!(a.unwrap(), 2);
    assert_eq!(b.unwrap(), 4);
}
