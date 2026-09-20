#![doc = "Lure 的应用领域模型与用例边界。"]
#![doc = ""]
#![doc = "该 crate 不依赖具体 GUI 或进程实现，用于承载会话生命周期等稳定的业务抽象。"]

mod connection;
mod conversation;
mod error;

pub use connection::{ConnectionPhase, ConnectionSnapshot, ModelSnapshot};
pub use conversation::{EventEnvelope, LureEvent, MessageBlock, MessageBlockKind};
pub use error::{ErrorCode, LureError};
