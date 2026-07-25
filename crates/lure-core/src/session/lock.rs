//! Per-session 并发锁：同一 session key 互斥、不同 key 独立。
//!
//! 忠实移植上游 `nanobot/api/server.py` 的 per-session `asyncio.Lock`：并发处理同一
//! session 的请求会串行化，避免交错读写 session 状态；不同 session 并行不受影响。
//!
//! 实现用 `Mutex<HashSet<String>> + Condvar`：持有中的 key 记入集合，`lock` 阻塞等到
//! key 空闲，[`SessionGuard`] 释放时移除 key 并唤醒等待者——集合随释放自清理，无泄漏。
//!
//! 当前同步单请求 HTTP server 的请求天然串行，本原语面向未来多线程 runner；同时以真
//! 多线程测试锁定契约（见 `tests/session_lock.rs`）。

use std::collections::HashSet;
use std::sync::{Condvar, Mutex};

/// 一组按 session key 互斥的锁。跨线程共享时包在 `Arc` 中。
#[derive(Debug, Default)]
pub struct SessionLocks {
    /// 当前被持有的 session key 集合。
    held: Mutex<HashSet<String>>,
    /// key 释放时唤醒等待线程。
    released: Condvar,
}

impl SessionLocks {
    /// 新建一个空锁表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 获取 `key` 的独占锁，**阻塞**直到该 key 空闲；返回 RAII 守卫，drop 时释放。
    ///
    /// 同一 key 的多个调用相互串行；不同 key 互不阻塞。
    pub fn lock(&self, key: &str) -> SessionGuard<'_> {
        let mut held = self.held.lock().expect("session 锁未中毒");
        while held.contains(key) {
            held = self.released.wait(held).expect("session 锁未中毒");
        }
        held.insert(key.to_string());
        SessionGuard {
            locks: self,
            key: key.to_string(),
        }
    }

    /// 尝试获取 `key` 的独占锁：空闲返回 `Some(guard)`，已被持有返回 `None`（不阻塞）。
    pub fn try_lock(&self, key: &str) -> Option<SessionGuard<'_>> {
        let mut held = self.held.lock().expect("session 锁未中毒");
        if held.contains(key) {
            return None;
        }
        held.insert(key.to_string());
        Some(SessionGuard {
            locks: self,
            key: key.to_string(),
        })
    }
}

/// [`SessionLocks::lock`] / [`SessionLocks::try_lock`] 返回的持有守卫。
///
/// 存活期间独占对应 session key；drop 时自动释放并唤醒等待该 key 的线程。
#[derive(Debug)]
pub struct SessionGuard<'a> {
    locks: &'a SessionLocks,
    key: String,
}

impl SessionGuard<'_> {
    /// 本守卫持有的 session key。
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let mut held = self.locks.held.lock().expect("session 锁未中毒");
        held.remove(&self.key);
        // 释放一个 key 后唤醒所有等待者；各线程醒来后重查自身 key 是否空闲。
        self.locks.released.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_lock_blocks_second_same_key_until_release() {
        let locks = SessionLocks::new();
        let g = locks.try_lock("k").expect("首次可获取");
        assert_eq!(g.key(), "k");
        assert!(locks.try_lock("k").is_none(), "同 key 互斥");
        drop(g);
        assert!(locks.try_lock("k").is_some(), "释放后可重获");
    }

    #[test]
    fn distinct_keys_are_independent() {
        let locks = SessionLocks::new();
        let _a = locks.try_lock("a").expect("a 可获取");
        assert!(locks.try_lock("b").is_some(), "不同 key 独立");
    }
}
