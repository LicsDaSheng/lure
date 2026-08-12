//! Stage 4 cron/trigger 异步化契约测试。
//!
//! 对齐 handbook Stage 4 + 上游 `submit_cron_turn` / `defer_if_active`：
//! 1. `AsyncCronScheduler` 为 tokio interval task（非后台线程），按 poll 周期触发 submit。
//! 2. cron 到期 → `cron_submit_message` 构建 `InboundMessage` → 进 `AsyncBus` → 共享
//!    `AgentLoopScheduler` 消费执行（turn 完成经 `on_completed` 拿到回复文本）。
//! 3. `SessionBusy` 跨实例 defer：目标 session 有活跃 turn 时 cron 让位（不提交，job 留待下个 tick）。
//! 4. trigger 队列 at-least-once 语义在异步投递循环下保持（claim → deliver → complete；interrupt recover）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;

use lure_core::agent::scheduler::AgentLoopScheduler;
use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::{async_bus_channel, InboundMessage};
use lure_core::cron::{
    AsyncCronScheduler, CronJob, CronPayload, CronSchedule, CronService, CronStore,
};
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;
use lure_core::trigger::{run_delivery_once, LocalTriggerQueue};
use tempfile::TempDir;

// ---- 测试辅助 -----------------------------------------------------------

/// 构造一个立即到期、origin websocket:t1、消息为 `message` 的循环 cron job。
fn due_job(id: &str, message: &str) -> CronJob {
    CronJob {
        id: id.to_string(),
        name: format!("job-{id}"),
        enabled: true,
        schedule: CronSchedule::every(1),
        payload: CronPayload {
            message: message.to_string(),
            session_key: Some("websocket:t1".to_string()),
            origin_channel: Some("websocket".to_string()),
            origin_chat_id: Some("t1".to_string()),
            ..CronPayload::default()
        },
        state: Default::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
        delete_after_run: false,
    }
}

// ---- SessionBusy：跨实例 defer 注册表 ------------------------------------

#[test]
fn session_busy_mark_unmark_and_isolated() {
    use lure_core::agent::SessionBusy;

    let busy = SessionBusy::default();
    assert!(!busy.is_busy("websocket:t1"));
    busy.mark("websocket:t1");
    assert!(busy.is_busy("websocket:t1"), "mark 后应 busy");
    assert!(!busy.is_busy("websocket:t2"), "不同 session 互不影响");
    busy.unmark("websocket:t1");
    assert!(!busy.is_busy("websocket:t1"), "unmark 后应空闲");
}

// ---- AsyncCronScheduler：tokio interval task -----------------------------

/// interval task 按 poll 周期对到期 job 触发 submit；stop 后终止。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_cron_scheduler_fires_submit_on_interval_and_stops() {
    let dir = TempDir::new().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store.add(due_job("j1", "ping"), 0).unwrap();
    let service = CronService::new(dir.path());

    let submitted = Arc::new(AtomicUsize::new(0));
    let count = submitted.clone();
    let seen = Arc::new(AtomicUsize::new(0));
    let seen2 = seen.clone();
    let scheduler = AsyncCronScheduler::spawn(
        &tokio::runtime::Handle::current(),
        service,
        move |job: &CronJob| {
            assert_eq!(job.id, "j1", "submit 应携带到期 job");
            count.fetch_add(1, Ordering::Relaxed);
            seen2.fetch_add(1, Ordering::Relaxed);
        },
        Duration::from_millis(30),
    );

    tokio::time::sleep(Duration::from_millis(150)).await;
    scheduler.stop().await;

    assert!(
        submitted.load(Ordering::Relaxed) >= 1,
        "150ms/30ms interval 应至少 submit 一次"
    );
    assert_eq!(
        seen.load(Ordering::Relaxed),
        submitted.load(Ordering::Relaxed)
    );
}

/// 无到期 job 时 submit 不被调用。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_cron_scheduler_no_due_job_no_submit() {
    let dir = TempDir::new().unwrap();
    let service = CronService::new(dir.path());
    let submitted = Arc::new(AtomicUsize::new(0));
    let count = submitted.clone();
    let scheduler = AsyncCronScheduler::spawn(
        &tokio::runtime::Handle::current(),
        service,
        move |_job: &CronJob| {
            count.fetch_add(1, Ordering::Relaxed);
        },
        Duration::from_millis(20),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    scheduler.stop().await;
    assert_eq!(submitted.load(Ordering::Relaxed), 0, "空 store 不应 submit");
}

// ---- cron submit 消息构建 -------------------------------------------------

#[test]
fn cron_submit_message_builds_inbound_with_origin() {
    use lure_core::cron::cron_submit_message;

    let job = due_job("j1", "ping");
    let msg = cron_submit_message(&job).unwrap();
    assert_eq!(msg.channel, "websocket");
    assert_eq!(msg.chat_id, "t1");
    assert_eq!(msg.content, "ping");
    assert_eq!(
        msg.session_key(),
        "websocket:t1",
        "session_key 用 origin 派生"
    );
}

#[test]
fn cron_submit_message_missing_origin_errors() {
    use lure_core::cron::cron_submit_message;

    let mut job = due_job("j1", "ping");
    job.payload.origin_channel = None;
    job.payload.origin_chat_id = None;
    assert!(cron_submit_message(&job).is_err(), "缺 origin 应报错");
}

// ---- 调度器 completion 钩子：turn 完成拿到回复文本 -------------------------

/// 经 bus + 共享调度器跑一条消息，turn 完成时 `on_completed` 收到 (msg, reply)。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_on_completed_receives_reply_text() {
    let tmp = TempDir::new().unwrap();
    let sessions = SessionManager::new(tmp.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    let completed: Arc<AsyncMutex<Vec<(String, String, String)>>> =
        Arc::new(AsyncMutex::new(Vec::new()));
    let rec = completed.clone();
    let scheduler = AgentLoopScheduler::builder(agent)
        .on_completed(move |msg: &InboundMessage, reply: &str| {
            if let Ok(mut guard) = rec.try_lock() {
                guard.push((msg.session_key(), msg.content.clone(), reply.to_string()));
            }
        })
        .build();
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tx.publish(InboundMessage::new("websocket", "t1", "ping"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(tx);
    run.await.unwrap();

    let got = completed.lock().await.clone();
    assert_eq!(got.len(), 1, "turn 完成应触发 on_completed");
    assert_eq!(got[0].0, "websocket:t1");
    assert_eq!(got[0].1, "ping");
    assert!(
        got[0].2.contains("echo: ping"),
        "回复文本应可用: {}",
        got[0].2
    );
}

// ---- cron submit → bus → 共享调度器 → completion（整条 submit 链）-----------

/// 到期 job 构建的 cron 消息经 bus 投递，由共享调度器执行并把回复文本交回。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cron_submit_through_shared_scheduler_runs_turn() {
    use lure_core::cron::cron_submit_message;

    let job = due_job("j1", "ping");
    let msg = cron_submit_message(&job).unwrap();

    let tmp = TempDir::new().unwrap();
    let sessions = SessionManager::new(tmp.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    let replies: Arc<AsyncMutex<Vec<String>>> = Arc::new(AsyncMutex::new(Vec::new()));
    let rec = replies.clone();
    let scheduler = AgentLoopScheduler::builder(agent)
        .on_completed(move |_msg: &InboundMessage, reply: &str| {
            if let Ok(mut guard) = rec.try_lock() {
                guard.push(reply.to_string());
            }
        })
        .build();
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tx.publish(msg).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(tx);
    run.await.unwrap();

    let got = replies.lock().await.clone();
    assert_eq!(
        got,
        vec!["echo: ping".to_string()],
        "cron turn 应经调度器执行"
    );
}

// ---- trigger：异步投递循环下 at-least-once 保持 ----------------------------

/// run_delivery_once：claim（跳过忙 session）→ 逐条 async deliver → complete。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trigger_async_delivery_claims_completes_and_skips_busy() {
    let queue = Arc::new(AsyncMutex::new(LocalTriggerQueue::new()));
    {
        let mut q = queue.lock().await;
        q.enqueue("trig-1", "websocket:free", "hello free");
        q.enqueue("trig-2", "websocket:busy", "hello busy");
    }

    let delivered: Arc<AsyncMutex<Vec<String>>> = Arc::new(AsyncMutex::new(Vec::new()));
    let rec = delivered.clone();
    // 忙 session 投递被跳过（等待语义）；非忙投递执行并 complete。
    let n = run_delivery_once(
        &queue,
        8,
        |session_key: &str| session_key == "websocket:busy",
        |delivery| {
            let rec = rec.clone();
            async move {
                rec.lock().await.push(delivery.content);
                true
            }
        },
    )
    .await;

    assert_eq!(n, 1, "仅非忙投递被执行");
    let q = queue.lock().await;
    assert_eq!(q.processing_len(), 0, "completed 后 processing 应为空");
    assert_eq!(q.pending_len(), 1, "忙 session 投递留在 pending");
    assert_eq!(
        delivered.lock().await.clone(),
        vec!["hello free".to_string()]
    );
}

/// interrupt（deliver 失败）→ 未完成投递 recover 重新入队（at-least-once）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trigger_async_delivery_recover_on_interrupt() {
    let queue = Arc::new(AsyncMutex::new(LocalTriggerQueue::new()));
    {
        let mut q = queue.lock().await;
        q.enqueue("trig-1", "websocket:s1", "doomed");
    }

    // deliver 返回 false 模拟执行中断：已 claim 的投递不 complete → recover。
    let n = run_delivery_once(&queue, 8, |_| false, |_delivery| async { false }).await;
    assert_eq!(n, 0, "deliver 失败不计入完成");

    let q = queue.lock().await;
    assert_eq!(q.processing_len(), 0, "失败投递不应滞留 processing");
    assert_eq!(
        q.pending_len(),
        1,
        "失败投递应 recover 回 pending（at-least-once）"
    );
}
