//! Agent 子系统：最小 loop / runner / context 纵向闭环。
//!
//! Phase 3 覆盖 CLI one-shot 到 provider 再到 session 保存的最小闭环；Phase 5 增加
//! tool-call 循环（provider tool_calls → registry 执行 → tool turn 回灌，至多
//! [`MAX_TOOL_ITERATIONS`] 轮）。streaming、并行/goal/subagent、consolidation、
//! channel/gateway 投递留待后续 phase。

mod context;
mod loop_run;
mod runner;

pub use context::ContextBuilder;
pub use loop_run::{AgentError, AgentLoop, ProgressEvent, TurnOutcome, MAX_TOOL_ITERATIONS};
pub use runner::AgentRunner;
