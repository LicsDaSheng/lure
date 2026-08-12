//! Phase 9 外围：per-session 并发锁原语。
//!
//! 忠实移植上游 `nanobot/api/server.py` 的 per-session `asyncio.Lock` 语义——
//! **同一 session key 互斥、不同 key 独立**。当前同步 HTTP server 请求天然串行，
//! 本原语为将来多线程 runner 就绪，并以真多线程测试锁定契约。

use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use lure_core::session::SessionLocks;

#[tokio::test]
async fn try_lock_is_exclusive_per_key_and_releases_on_drop() {
    let locks = SessionLocks::new();

    // 空闲 key：try_lock 成功。
    let g = locks.try_lock("api:default");
    assert!(g.is_some(), "空闲 key 应可获取");

    // 同 key 已持有：第二次 try_lock 失败。
    assert!(
        locks.try_lock("api:default").is_none(),
        "已持有的 key 不应再次获取"
    );

    // 释放后再次可获取。
    drop(g);
    assert!(
        locks.try_lock("api:default").is_some(),
        "释放后应可重新获取"
    );
}

#[tokio::test]
async fn distinct_keys_do_not_contend() {
    let locks = SessionLocks::new();

    let _a = locks.try_lock("api:alice").expect("alice 可获取");
    // 持有 alice 时，另一 key 不受影响。
    let b = locks.try_lock("api:bob");
    assert!(b.is_some(), "不同 key 应相互独立");
    // 同 key 仍互斥。
    assert!(locks.try_lock("api:alice").is_none(), "同 key 仍互斥");
}

#[tokio::test]
async fn blocking_lock_serializes_same_key_across_threads() {
    let locks = Arc::new(SessionLocks::new());
    let inside = Arc::new(AtomicUsize::new(0));
    let max_overlap = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let locks = Arc::clone(&locks);
            let inside = Arc::clone(&inside);
            let max_overlap = Arc::clone(&max_overlap);
            thread::spawn(move || {
                for _ in 0..40 {
                    let _guard = locks.lock("api:default");
                    // 临界区：记录同时在内的线程数，断言永不 >1。
                    let now = inside.fetch_add(1, SeqCst) + 1;
                    max_overlap.fetch_max(now, SeqCst);
                    thread::sleep(Duration::from_micros(50));
                    inside.fetch_sub(1, SeqCst);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        max_overlap.load(SeqCst),
        1,
        "同一 key 的临界区任一时刻至多一个线程"
    );
}

#[tokio::test]
async fn blocking_lock_on_distinct_keys_runs_concurrently() {
    let locks = Arc::new(SessionLocks::new());
    // 两线程各锁不同 key，用 Barrier(2) 要求二者同时进入临界区；
    // 若不同 key 被错误串行化，barrier 将永远等不齐（超时/挂起）。
    let barrier = Arc::new(Barrier::new(2));

    let handles: Vec<_> = ["api:a", "api:b"]
        .into_iter()
        .map(|key| {
            let locks = Arc::clone(&locks);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let _guard = locks.lock(key);
                // 二者必须同时在各自临界区内才能越过 barrier。
                barrier.wait();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
