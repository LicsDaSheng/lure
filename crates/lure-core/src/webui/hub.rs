//! 在线 WS 连接注册表：服务端主动推送的路由表。
//!
//! WS 连接是「客户端发起、服务端应答」的请求/响应模型；但 cron 定时产出需要**服务端
//! 主动**把结果推给正在查看该会话的在线连接（否则只落 transcript，用户需刷新才见）。
//!
//! [`WsHub`] 按 `chat_id` 索引在线连接的出站 [`Sender`]：连接 attach 到某会话时订阅，
//! 断开时注销；cron runner 产出后 [`WsHub::push`] 向该会话的所有在线连接投递事件帧。
//! 连接侧在其读循环里 drain 收到的帧并写回 socket（单线程持有 socket，无并发写竞争）。

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// 单个订阅：连接的唯一 id + 其出站帧 sender。
struct Subscriber {
    conn_id: u64,
    sender: Sender<Value>,
}

/// 在线 WS 连接注册表（可克隆句柄，内部共享）。
#[derive(Clone, Default)]
pub struct WsHub {
    // chat_id -> 订阅该会话的在线连接。
    inner: Arc<Mutex<HashMap<String, Vec<Subscriber>>>>,
}

impl WsHub {
    /// 构造空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册连接 `conn_id` 对 `chat_id` 的订阅。
    ///
    /// 幂等：同一 `(chat_id, conn_id)` 重复订阅只保留最新 sender（连接换绑会话时的
    /// 重复 attach 不会堆积僵尸订阅）。
    pub fn subscribe(&self, chat_id: &str, conn_id: u64, sender: Sender<Value>) {
        let mut map = self.inner.lock().expect("hub 锁中毒");
        let subs = map.entry(chat_id.to_string()).or_default();
        if let Some(existing) = subs.iter_mut().find(|s| s.conn_id == conn_id) {
            existing.sender = sender;
        } else {
            subs.push(Subscriber { conn_id, sender });
        }
    }

    /// 注销某连接的**全部**订阅（连接断开时调用）。
    pub fn remove_conn(&self, conn_id: u64) {
        let mut map = self.inner.lock().expect("hub 锁中毒");
        for subs in map.values_mut() {
            subs.retain(|s| s.conn_id != conn_id);
        }
        map.retain(|_, subs| !subs.is_empty());
    }

    /// 向订阅 `chat_id` 的所有在线连接推送一帧，返回成功投递的连接数。
    ///
    /// 顺带清理已死连接：`send` 失败（接收端已 drop）的订阅被移除。
    pub fn push(&self, chat_id: &str, frame: &Value) -> usize {
        let mut map = self.inner.lock().expect("hub 锁中毒");
        let Some(subs) = map.get_mut(chat_id) else {
            return 0;
        };
        let mut delivered = 0;
        subs.retain(|s| match s.sender.send(frame.clone()) {
            Ok(()) => {
                delivered += 1;
                true
            }
            Err(_) => false,
        });
        if subs.is_empty() {
            map.remove(chat_id);
        }
        delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::channel;

    #[test]
    fn push_delivers_to_all_subscribers_of_chat() {
        let hub = WsHub::new();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        hub.subscribe("c1", 1, tx1);
        hub.subscribe("c1", 2, tx2);

        let n = hub.push("c1", &json!({"event": "message", "text": "hi"}));

        assert_eq!(n, 2, "两个订阅都应收到");
        assert_eq!(rx1.recv().unwrap()["text"], "hi");
        assert_eq!(rx2.recv().unwrap()["text"], "hi");
    }

    #[test]
    fn push_to_unknown_chat_delivers_to_none() {
        let hub = WsHub::new();
        let (tx, _rx) = channel();
        hub.subscribe("c1", 1, tx);
        assert_eq!(hub.push("other", &json!({"event": "x"})), 0);
    }

    #[test]
    fn push_isolates_by_chat_id() {
        let hub = WsHub::new();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        hub.subscribe("c1", 1, tx1);
        hub.subscribe("c2", 2, tx2);

        hub.push("c1", &json!({"text": "for-c1"}));

        assert_eq!(rx1.recv().unwrap()["text"], "for-c1");
        assert!(rx2.try_recv().is_err(), "c2 不应收到 c1 的推送");
    }

    #[test]
    fn subscribe_is_idempotent_per_conn_keeping_latest_sender() {
        let hub = WsHub::new();
        let (tx_old, rx_old) = channel();
        let (tx_new, rx_new) = channel();
        hub.subscribe("c1", 1, tx_old);
        hub.subscribe("c1", 1, tx_new); // 同 conn 重订阅 → 覆盖，不堆积。

        let n = hub.push("c1", &json!({"text": "hi"}));

        assert_eq!(n, 1, "同一连接只投递一次");
        assert!(rx_old.try_recv().is_err(), "旧 sender 应被替换");
        assert_eq!(rx_new.recv().unwrap()["text"], "hi");
    }

    #[test]
    fn remove_conn_unsubscribes_all_its_chats() {
        let hub = WsHub::new();
        let (tx_a, rx_a) = channel();
        let (tx_b, rx_b) = channel();
        hub.subscribe("c1", 1, tx_a);
        hub.subscribe("c2", 1, tx_b);

        hub.remove_conn(1);

        assert_eq!(hub.push("c1", &json!({"x": 1})), 0);
        assert_eq!(hub.push("c2", &json!({"x": 1})), 0);
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_err());
    }

    #[test]
    fn push_prunes_dead_subscribers() {
        let hub = WsHub::new();
        let (tx_live, rx_live) = channel();
        let (tx_dead, rx_dead) = channel();
        hub.subscribe("c1", 1, tx_live);
        hub.subscribe("c1", 2, tx_dead);
        drop(rx_dead); // 连接 2 已断。

        let n = hub.push("c1", &json!({"text": "hi"}));

        assert_eq!(n, 1, "只投递给存活连接");
        assert_eq!(rx_live.recv().unwrap()["text"], "hi");
        // 死订阅已被清理：下次推送不再尝试它。
        assert_eq!(hub.push("c1", &json!({"text": "again"})), 1);
    }
}
