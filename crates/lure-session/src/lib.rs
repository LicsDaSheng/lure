#![doc = "Pi 本地会话存储的只读适配层。"]
#![doc = ""]
#![doc = "该 crate 只读取 Pi 写入的会话目录与 JSONL 文件，用于给桌面端提供历史会话列表；"]
#![doc = "它不写入会话文件，也不复制 Pi 的会话续写、压缩或分支规则。"]

mod error;
mod layout;
mod scan;
mod summary;
mod time;

pub use error::SessionError;
pub use layout::{agent_directory, session_directory};
pub use scan::{list_sessions, list_sessions_page, read_session_summary};
pub use summary::{SessionPage, SessionSummary};
