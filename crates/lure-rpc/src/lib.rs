#![doc = "Pi RPC 子进程与 JSONL 协议适配器。"]
#![doc = ""]
#![doc = "该 crate 只通过 `pi --mode rpc` 的 stdin/stdout 与 Pi 通信，不解析 TUI 输出。"]

pub mod capture;
mod client;
mod error;
pub mod jsonl;
mod normalize;
mod protocol;

pub use capture::StdoutCapture;
pub use client::{PiProcessConfig, PiRpcClient};
pub use error::RpcError;
pub use protocol::{RpcCommand, RpcImage, RpcModel, RpcSessionState};
