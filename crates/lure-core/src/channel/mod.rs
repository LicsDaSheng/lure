//! Channel 子系统：chat 平台的最小契约。
//!
//! 对齐上游 `nanobot/channels/base.py` 的职责边界，但收敛为同步最小契约：
//! channel 声明名字、校验配置、投递 outbound。上游的 async start/stop 长驻监听、
//! 热加载、delta coalescing、pairing 等留待接入真实平台时补齐。

mod contract;

pub use contract::{Channel, ChannelError, DeliveryLog, RecordingChannel};
