//! Agent 子系统：loop / runner / workspace context 纵向闭环。
//!
//! Phase 3 覆盖 CLI one-shot 到 provider 再到 session 保存的最小闭环；Phase 5 增加
//! tool-call 循环（provider tool_calls → registry 执行 → tool turn 回灌，至多
//! [`MAX_TOOL_ITERATIONS`] 轮）；Phase 6 增加完整 workspace system prompt、长期记忆与 skills
//! 接入（每轮冻结 prompt、user/assistant turn 追加 `history.jsonl`、可用 `DreamRunner` 触发整合）。
//! media/runtime-context、并行 tool 与 context governance 留待后续 phase。

mod context;
mod loop_run;
mod runner;
pub mod scheduler;
pub mod skills;
pub mod subagent;
pub mod subagent_run;
mod workspace_templates;

pub use context::{ContextBuilder, PromptBuildOptions};
pub use loop_run::{
    AgentError, AgentLoop, ProgressEvent, TurnOutcome, EMPTY_FINAL_RESPONSE_MESSAGE,
    FINALIZATION_RETRY_PROMPT, MAX_EMPTY_RETRIES, MAX_TOOL_ITERATIONS,
};
pub use runner::AgentRunner;
pub use scheduler::{AgentLoopScheduler, SchedulerBuilder, SchedulerConfig};
pub use workspace_templates::{DEFAULT_AGENTS, DEFAULT_HEARTBEAT, DEFAULT_SOUL, DEFAULT_USER};
