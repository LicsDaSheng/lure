//! 映射上游 `tests/triggers/test_local_triggers.py` 的 at-least-once 与忙等语义。

use lure_core::trigger::{never_busy, LocalTriggerQueue};

#[tokio::test]
async fn enqueue_claim_complete_happy_path() {
    let mut queue = LocalTriggerQueue::new();
    queue.enqueue("t1", "cli:direct", "run me");
    assert_eq!(queue.pending_len(), 1);

    let claimed = queue.claim(10, never_busy);
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].content, "run me");
    assert_eq!(claimed[0].attempts, 1);
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(queue.processing_len(), 1);

    assert!(queue.complete(claimed[0].id));
    assert_eq!(queue.processing_len(), 0);
}

#[tokio::test]
async fn recover_requeues_uncompleted_for_at_least_once() {
    let mut queue = LocalTriggerQueue::new();
    let id = queue.enqueue("t1", "cli:direct", "run me");

    let first = queue.claim(10, never_busy);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].attempts, 1);

    // 未 complete 就“崩溃”：recover 把 processing 重新入队。
    assert_eq!(queue.recover(), 1);
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(queue.processing_len(), 0);

    // 再次认领 → 重投递（at-least-once），attempts 递增。
    let second = queue.claim(10, never_busy);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, id);
    assert_eq!(second[0].content, "run me");
    assert_eq!(second[0].attempts, 2);

    // 这次 complete 后不再重投。
    assert!(queue.complete(second[0].id));
    assert_eq!(queue.recover(), 0);
    assert_eq!(queue.pending_len(), 0);
}

#[tokio::test]
async fn busy_session_deliveries_wait_in_pending() {
    let mut queue = LocalTriggerQueue::new();
    queue.enqueue("t1", "busy:session", "later");
    queue.enqueue("t2", "free:session", "now");

    // busy:session 忙 → 其投递保留 pending，free:session 被认领。
    let claimed = queue.claim(10, |session| session == "busy:session");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].session_key, "free:session");
    assert_eq!(queue.pending_len(), 1);

    // session 不再忙 → 可认领。
    let later = queue.claim(10, never_busy);
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].session_key, "busy:session");
}

#[tokio::test]
async fn claim_respects_limit() {
    let mut queue = LocalTriggerQueue::new();
    queue.enqueue("t1", "s", "a");
    queue.enqueue("t2", "s", "b");
    queue.enqueue("t3", "s", "c");

    let claimed = queue.claim(2, never_busy);
    assert_eq!(claimed.len(), 2);
    assert_eq!(queue.pending_len(), 1);
    // 保序：第三条留在 pending。
    let rest = queue.claim(10, never_busy);
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].content, "c");
}
