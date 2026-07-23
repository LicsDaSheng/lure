//! Agent 子系统：最小 loop / runner / context 纵向闭环。
//!
//! Phase 3 覆盖 CLI one-shot 到 provider 再到 session 保存的最小闭环。tool 执行、
//! streaming、goal/subagent、consolidation、channel/gateway 投递留待后续 phase。

mod context;
mod loop_run;
mod runner;

pub use context::ContextBuilder;
pub use loop_run::{AgentError, AgentInput, AgentLoop, ProgressEvent, TurnOutcome};
pub use runner::AgentRunner;
