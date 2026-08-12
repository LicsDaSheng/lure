//! 运行时桥接（Stage 2 过渡）：在同步线程内驱动 async 任务。
//!
//! 生产侧（tiny_http/WS 连接线程）与测试侧（`#[tokio::test]` runtime 线程）都通过
//! [`block_on`] 执行：**无条件另起 scoped 线程** `Handle::block_on`，避免 tokio 禁止
//! 「runtime 线程内 block_on」的 panic。scoped 线程允许 future 借用非 `'static` 状态
//! （如 `&mut runner`），仅要求 future `Send`。Stage 3 全量 async（axum）后本模块退役。

/// 在给定 runtime 句柄上阻塞执行一个 `Send` future，返回其输出。
pub fn block_on<F>(handle: &tokio::runtime::Handle, future: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || handle.block_on(future))
            .join()
            .expect("block_on 辅助线程不应 panic")
    })
}
