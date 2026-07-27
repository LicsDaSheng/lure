//! 最小 `AgentLoop`：CLI one-shot 到 provider 再到 session 保存的纵向闭环。
//!
//! 对齐上游 `nanobot/agent/loop.py` 的职责边界，但只保留最小闭环：
//! 追加 user turn → 构建 context → 调 runner/provider → 追加 assistant turn →
//! 保存 session → 返回最终输出与结构化 progress。
//!
//! Phase 3 不做：async/streaming、tool 执行、goal/subagent、consolidation、
//! channel/gateway 投递。progress 先做结构化枚举，不急于完整事件流。

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::agent::context::ContextBuilder;
use crate::agent::runner::AgentRunner;
use crate::bus::InboundMessage;
use crate::memory::{ConsolidationOutcome, DreamRunner, MemoryStore};
use crate::provider::{GenerationSettings, LlmProvider, LlmRuntime, ProviderError, ToolCall};
use crate::session::{SessionError, SessionManager};
use crate::tool::ToolRegistry;

/// tool-call 循环的最大迭代数：每次迭代一次 provider 调用；达到上限即停止，
/// 避免模型反复请求工具导致死循环。
pub const MAX_TOOL_ITERATIONS: usize = 8;

/// 空终响应的最大静默重试次数：达到后转 finalization。对齐上游 `_MAX_EMPTY_RETRIES`。
pub const MAX_EMPTY_RETRIES: usize = 2;

/// 静默重试 + finalization 仍为空时的固定兜底文案。对齐上游 `EMPTY_FINAL_RESPONSE_MESSAGE`。
pub const EMPTY_FINAL_RESPONSE_MESSAGE: &str =
    "I completed the tool steps but couldn't produce a final answer. \
Please try again or narrow the task.";

/// finalization 请求追加的用户提示（引导模型基于已有对话给出最终答复）。
/// 对齐上游 `FINALIZATION_RETRY_PROMPT`。
pub const FINALIZATION_RETRY_PROMPT: &str =
    "Please provide your response to the user based on the conversation above.";

/// 结构化 progress 事件。
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent {
    /// 本轮开始。
    TurnStarted {
        /// 目标 session key。
        session_key: String,
    },
    /// 流式内容增量（streaming provider 每产生一段文本时发出）。
    ContentDelta {
        /// 本段增量文本。
        text: String,
    },
    /// 流式推理增量（推理模型的思维链逐段发出）。
    ReasoningDelta {
        /// 本段推理增量文本。
        text: String,
    },
    /// 调用了某个工具（tool-call 循环中每次执行工具时发出）。
    ToolInvoked {
        /// 工具名。
        name: String,
    },
    /// 产生最终回复。
    FinalResponse {
        /// 最终回复文本。
        content: String,
    },
}

/// 一次 turn 的产出。
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOutcome {
    /// 最终回复文本。
    pub final_content: String,
    /// 推理内容（思维链）；推理模型提供时非空。
    pub reasoning: Option<String>,
    /// 结构化 progress 序列。
    pub progress: Vec<ProgressEvent>,
    /// 终止原因：`completed`（正常产出）/ `empty_final_response`（空终响应兜底）/
    /// `max_iterations`（达到 tool-call 迭代上限）。对齐上游 `stop_reason`。
    pub stop_reason: String,
    /// 本轮跨所有 provider 调用（含 tool 轮、静默重试、finalization）累加的 usage，
    /// 按整数字段逐一求和（`prompt_tokens`/`completion_tokens`/`cached_tokens` 等）。
    /// 对齐上游 `_accumulate_usage`。
    pub usage: Map<String, Value>,
}

/// agent loop 结构化错误。
#[derive(Debug)]
pub enum AgentError {
    /// provider 调用失败。
    Provider(ProviderError),
    /// session 读写失败。
    Session(SessionError),
    /// memory（history.jsonl 等）读写失败。
    Memory(std::io::Error),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Provider(e) => write!(f, "agent provider 错误: {e}"),
            AgentError::Session(e) => write!(f, "agent session 错误: {e}"),
            AgentError::Memory(e) => write!(f, "agent memory 错误: {e}"),
        }
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AgentError::Provider(e) => Some(e),
            AgentError::Session(e) => Some(e),
            AgentError::Memory(e) => Some(e),
        }
    }
}

impl From<ProviderError> for AgentError {
    fn from(value: ProviderError) -> Self {
        AgentError::Provider(value)
    }
}

impl From<SessionError> for AgentError {
    fn from(value: SessionError) -> Self {
        AgentError::Session(value)
    }
}

/// 最小 agent loop。
pub struct AgentLoop {
    provider: Box<dyn LlmProvider>,
    sessions: SessionManager,
    context: ContextBuilder,
    settings: GenerationSettings,
    model: String,
    tools: Option<ToolRegistry>,
    memory: Option<MemoryStore>,
    /// 取消令牌：置位后 loop 在下个检查点中止本轮（见 [`with_cancel`](Self::with_cancel)）。
    cancel: Option<Arc<AtomicBool>>,
}

impl AgentLoop {
    /// 绑定 provider、session 管理器与 context builder。
    pub fn new(
        provider: Box<dyn LlmProvider>,
        sessions: SessionManager,
        context: ContextBuilder,
    ) -> Self {
        let model = provider.default_model().to_string();
        Self {
            provider,
            sessions,
            context,
            settings: GenerationSettings::default(),
            model,
            tools: None,
            memory: None,
            cancel: None,
        }
    }

    /// 挂载工具注册表，启用 tool-call 循环。
    ///
    /// 未挂载时即便 provider 返回 `tool_calls` 也直接作为终态（向后兼容 Phase 3）。
    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    /// 挂载长期记忆存储：每轮注入记忆块到 context，并把 user/assistant turn 追加到
    /// `history.jsonl`（供 dream consolidation）。
    pub fn with_memory(mut self, memory: MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// 挂载取消令牌：置位（`true`）后，`process_streaming` 在下个检查点——每轮迭代开始前、
    /// 每次流式响应结束后——中止本轮，返回 `stop_reason="interrupted"` 的空产出，且**不**
    /// 持久化 assistant turn、**不**保存 session（本轮作废）。供 CLI 的 Ctrl-C 中断使用。
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// 取消令牌是否已置位。
    fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// 用可替换 [`DreamRunner`] 整合未处理历史；未挂 memory 或无未处理历史返回 `None`。
    pub fn consolidate<R: DreamRunner + ?Sized>(&self, runner: &R) -> Option<ConsolidationOutcome> {
        self.memory.as_ref()?.consolidate(runner)
    }

    /// 阈值触发：未整合 history >= `min_entries` 时执行 `consolidate`。
    pub fn maybe_consolidate<R: DreamRunner + ?Sized>(
        &self,
        runner: &R,
        min_entries: usize,
    ) -> Option<ConsolidationOutcome> {
        let memory = self.memory.as_ref()?;
        if !memory.should_consolidate(min_entries) {
            return None;
        }
        self.consolidate(runner)
    }

    /// 用 `ModelRuntimeResolver` 产出的 [`LlmRuntime`] 覆盖 model 与生成参数。
    ///
    /// 让 provider 选择路径由 resolver 驱动：model 取快照中的 `provider.model`，
    /// 生成参数取 runtime 捕获的 `settings`（provider 实例仍由调用方按快照构造）。
    pub fn with_runtime(mut self, runtime: &LlmRuntime) -> Self {
        self.model = runtime.provider.model.clone();
        self.settings = runtime.settings.clone();
        self
    }

    /// 只读访问 session 管理器（测试与诊断用）。
    pub fn sessions(&self) -> &SessionManager {
        &self.sessions
    }

    /// 可变访问 session 管理器。
    pub fn sessions_mut(&mut self) -> &mut SessionManager {
        &mut self.sessions
    }

    /// 处理一条 inbound 消息，跑完（可能多轮 tool-call 的）闭环并返回产出。
    ///
    /// 每轮：构建 context → 调 provider。无 `tool_calls`（或未挂载 registry）即为终态：
    /// 追加最终 assistant turn 返回。否则追加带 `tool_calls` 的 assistant turn，执行每个
    /// 工具并把结果作为 `tool` turn 回灌历史，进入下一轮；至多 [`MAX_TOOL_ITERATIONS`] 轮。
    pub fn process(&mut self, input: &InboundMessage) -> Result<TurnOutcome, AgentError> {
        self.process_streaming(input, &mut |_: &ProgressEvent| {})
    }

    /// [`process`](Self::process) 的流式变体：每产生一个 [`ProgressEvent`]（TurnStarted /
    /// ContentDelta / ToolInvoked / FinalResponse）即**实时**回调 `on_progress`，让调用方
    /// （SSE HTTP 层、CLI 交互渲染）逐事件推送。返回的 [`TurnOutcome`] 与 `process` 一致，
    /// 其 `progress` 收录同一事件序列（回调是并行的实时通道，二者顺序完全一致）。
    pub fn process_streaming(
        &mut self,
        input: &InboundMessage,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<TurnOutcome, AgentError> {
        let key = input.session_key();
        let mut progress = Vec::new();
        emit(
            &mut progress,
            on_progress,
            ProgressEvent::TurnStarted {
                session_key: key.clone(),
            },
        );

        // 追加 user turn，并把 user 内容记入 history.jsonl（供 dream consolidation）。
        self.sessions
            .get_or_create(&key)?
            .add_message("user", &input.content);
        append_memory_history(self.memory.as_ref(), &key, &input.content)?;

        // 注入长期记忆块（本轮内稳定）：system → memory → 历史。
        let context = match self.memory.as_ref().map(MemoryStore::get_memory_context) {
            Some(memory_context) if !memory_context.is_empty() => {
                self.context.clone().with_memory(Some(memory_context))
            }
            _ => self.context.clone(),
        };

        let mut final_content = String::new();
        let mut final_reasoning = None;
        let mut empty_retries = 0usize;
        let mut usage = Map::new();

        for _ in 0..MAX_TOOL_ITERATIONS {
            // 取消检查点（调 provider 前）：置位则作废本轮，不持久化、不保存 session。
            if self.is_cancelled() {
                return Ok(interrupted_outcome(progress, usage));
            }
            // 构建 context（历史含此前所有 turn），调 provider。
            let history = self.sessions.get_or_create(&key)?.get_history(0);
            let messages = context.build(&history);
            let response = {
                let runner = AgentRunner::new(self.provider.as_ref(), self.settings.clone());
                // 流式驱动：每个内容增量转成细粒度 ContentDelta progress。
                runner.run_streaming(&self.model, messages, &mut |chunk| {
                    if let Some(text) = chunk.reasoning_delta.as_ref().filter(|t| !t.is_empty()) {
                        emit(
                            &mut progress,
                            on_progress,
                            ProgressEvent::ReasoningDelta { text: text.clone() },
                        );
                    }
                    if let Some(text) = chunk.content_delta.as_ref().filter(|t| !t.is_empty()) {
                        emit(
                            &mut progress,
                            on_progress,
                            ProgressEvent::ContentDelta { text: text.clone() },
                        );
                    }
                })?
            };
            accumulate_usage(&mut usage, &response.usage);

            // 取消检查点（流式响应结束后）：Ctrl-C 于本轮流式期间置位时，丢弃已收内容、
            // 不持久化，返回 interrupted。
            if self.is_cancelled() {
                return Ok(interrupted_outcome(progress, usage));
            }

            let content = response.content.clone().unwrap_or_default();
            let reasoning = response.reasoning_content.clone().filter(|r| !r.is_empty());
            let run_tools = self.tools.is_some() && !response.tool_calls.is_empty();

            if !run_tools {
                // 空终响应：先静默重试（不持久化、不改历史，下一轮重新请求），
                // 达到上限后转 finalization（追加瞬态提示后请求一次）。
                if content.trim().is_empty() {
                    empty_retries += 1;
                    if empty_retries < MAX_EMPTY_RETRIES {
                        continue;
                    }
                    let (fin_content, fin_reasoning) = self.finalize_empty_response(
                        &context,
                        &key,
                        &mut progress,
                        &mut usage,
                        on_progress,
                    )?;
                    let (final_text, stop_reason) = if fin_content.trim().is_empty() {
                        (
                            EMPTY_FINAL_RESPONSE_MESSAGE.to_string(),
                            "empty_final_response",
                        )
                    } else {
                        (fin_content, "completed")
                    };
                    persist_assistant(&mut self.sessions, &key, &final_text, &fin_reasoning, &[])?;
                    self.sessions.save(&key, false)?;
                    append_memory_history(self.memory.as_ref(), &key, &final_text)?;
                    emit(
                        &mut progress,
                        on_progress,
                        ProgressEvent::FinalResponse {
                            content: final_text.clone(),
                        },
                    );
                    return Ok(TurnOutcome {
                        final_content: final_text,
                        reasoning: fin_reasoning,
                        progress,
                        stop_reason: stop_reason.to_string(),
                        usage,
                    });
                }

                // 终态：追加最终 assistant turn（reasoning 一并持久化，仅供展示）。
                persist_assistant(&mut self.sessions, &key, &content, &reasoning, &[])?;
                self.sessions.save(&key, false)?;
                append_memory_history(self.memory.as_ref(), &key, &content)?;
                emit(
                    &mut progress,
                    on_progress,
                    ProgressEvent::FinalResponse {
                        content: content.clone(),
                    },
                );
                return Ok(TurnOutcome {
                    final_content: content,
                    reasoning,
                    progress,
                    stop_reason: "completed".to_string(),
                    usage,
                });
            }

            // tool round：先持久化带 tool_calls 的 assistant turn。
            let tool_calls = response.tool_calls.clone();
            persist_assistant(&mut self.sessions, &key, &content, &reasoning, &tool_calls)?;

            // 执行所有工具（借用 registry；此段不改动 session）。
            for call in &tool_calls {
                emit(
                    &mut progress,
                    on_progress,
                    ProgressEvent::ToolInvoked {
                        name: call.name.clone(),
                    },
                );
            }
            let results: Vec<(String, String)> = tool_calls
                .iter()
                .map(|call| {
                    let registry = self.tools.as_ref().expect("run_tools 已确保存在");
                    (call.id.clone(), execute_tool(registry, call))
                })
                .collect();

            // 把工具结果作为 tool turn 回灌历史。
            for (tool_call_id, result) in results {
                persist_tool_result(&mut self.sessions, &key, &tool_call_id, &result)?;
            }

            final_content = content;
            final_reasoning = reasoning;
        }

        // 达到迭代上限：保存并返回最后一轮的 assistant 内容。
        self.sessions.save(&key, false)?;
        append_memory_history(self.memory.as_ref(), &key, &final_content)?;
        emit(
            &mut progress,
            on_progress,
            ProgressEvent::FinalResponse {
                content: final_content.clone(),
            },
        );
        Ok(TurnOutcome {
            final_content,
            reasoning: final_reasoning,
            progress,
            stop_reason: "max_iterations".to_string(),
            usage,
        })
    }

    /// 空终响应达到静默重试上限后的 finalization 请求：在当前 context 之上追加**瞬态**
    /// finalization 提示（不写入 session 历史，对齐上游 `messages_for_model` 副本语义），
    /// 请求一次并返回 `(内容, reasoning)`；本次 usage 累加进 `usage`。内容增量仍以
    /// `ProgressEvent::ContentDelta` 经 `on_progress` 实时推送。
    fn finalize_empty_response(
        &mut self,
        context: &ContextBuilder,
        key: &str,
        progress: &mut Vec<ProgressEvent>,
        usage: &mut Map<String, Value>,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<(String, Option<String>), AgentError> {
        let history = self.sessions.get_or_create(key)?.get_history(0);
        let mut messages = context.build(&history);
        messages.push(json!({"role": "user", "content": FINALIZATION_RETRY_PROMPT}));

        let runner = AgentRunner::new(self.provider.as_ref(), self.settings.clone());
        let response = runner.run_streaming(&self.model, messages, &mut |chunk| {
            if let Some(text) = chunk.reasoning_delta.as_ref().filter(|t| !t.is_empty()) {
                emit(
                    progress,
                    on_progress,
                    ProgressEvent::ReasoningDelta { text: text.clone() },
                );
            }
            if let Some(text) = chunk.content_delta.as_ref().filter(|t| !t.is_empty()) {
                emit(
                    progress,
                    on_progress,
                    ProgressEvent::ContentDelta { text: text.clone() },
                );
            }
        })?;
        accumulate_usage(usage, &response.usage);
        let content = response.content.clone().unwrap_or_default();
        let reasoning = response.reasoning_content.clone().filter(|r| !r.is_empty());
        Ok((content, reasoning))
    }
}

/// 把 `addition` 的整数字段逐一累加进 `target`（缺失键起始为 0）；非整数字段忽略。
/// 对齐上游 `AgentRunner._accumulate_usage`。
fn accumulate_usage(target: &mut Map<String, Value>, addition: &Map<String, Value>) {
    for (k, v) in addition {
        if let Some(n) = v.as_i64() {
            let sum = target.get(k).and_then(Value::as_i64).unwrap_or(0) + n;
            target.insert(k.clone(), Value::from(sum));
        }
    }
}

/// 若挂载了 memory，把一条内容追加到 `history.jsonl`（按 session 归属）。
fn append_memory_history(
    memory: Option<&MemoryStore>,
    key: &str,
    content: &str,
) -> Result<(), AgentError> {
    if let Some(memory) = memory {
        memory
            .append_history(content, Some(key))
            .map_err(AgentError::Memory)?;
    }
    Ok(())
}

/// 持久化 assistant turn；`reasoning_content` 与 `tool_calls` 作为附加字段一并保存。
fn persist_assistant(
    sessions: &mut SessionManager,
    key: &str,
    content: &str,
    reasoning: &Option<String>,
    tool_calls: &[ToolCall],
) -> Result<(), SessionError> {
    let mut extra = Map::new();
    if let Some(reasoning) = reasoning {
        extra.insert(
            "reasoning_content".to_string(),
            Value::String(reasoning.clone()),
        );
    }
    if !tool_calls.is_empty() {
        extra.insert("tool_calls".to_string(), tool_calls_to_json(tool_calls));
    }
    sessions
        .get_or_create(key)?
        .add_message_with("assistant", content, extra);
    Ok(())
}

/// 持久化一条 tool 结果 turn（role `tool` + `tool_call_id`）。
fn persist_tool_result(
    sessions: &mut SessionManager,
    key: &str,
    tool_call_id: &str,
    content: &str,
) -> Result<(), SessionError> {
    let mut extra = Map::new();
    extra.insert(
        "tool_call_id".to_string(),
        Value::String(tool_call_id.to_string()),
    );
    sessions
        .get_or_create(key)?
        .add_message_with("tool", content, extra);
    Ok(())
}

/// 解析参数并派发工具，返回结果文本；参数非法或工具错误均以文本回灌（结构化 is_error
/// 由工具层负责，这里统一转成可回灌的文本供模型自我纠正）。
fn execute_tool(registry: &ToolRegistry, call: &ToolCall) -> String {
    let args = if call.arguments.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str::<Value>(&call.arguments) {
            Ok(value) => value,
            Err(e) => return format!("工具 '{}' 参数不是合法 JSON: {e}", call.name),
        }
    };
    let content = match registry.execute(&call.name, &args) {
        Ok(result) => result.content,
        Err(e) => e.to_string(),
    };
    ensure_nonempty_tool_result(&call.name, content)
}

/// 记录一个 progress 事件：先实时回调 `on_progress`，再收入 `progress` 序列。
/// 保证回调顺序与最终 [`TurnOutcome::progress`] 完全一致。
fn emit(
    progress: &mut Vec<ProgressEvent>,
    on_progress: &mut dyn FnMut(&ProgressEvent),
    event: ProgressEvent,
) {
    on_progress(&event);
    progress.push(event);
}

/// 构造中断产出：空内容、无 reasoning、`stop_reason="interrupted"`，保留已累积的
/// progress 与 usage。不发 `FinalResponse`（本轮作废）。
fn interrupted_outcome(progress: Vec<ProgressEvent>, usage: Map<String, Value>) -> TurnOutcome {
    TurnOutcome {
        final_content: String::new(),
        reasoning: None,
        progress,
        stop_reason: "interrupted".to_string(),
        usage,
    }
}

/// 把语义为空（空串或纯空白）的工具结果替换为短标记，避免回灌历史时出现空白 tool turn
/// 令模型困惑。对齐上游 `nanobot/utils/runtime.py::ensure_nonempty_tool_result`。
fn ensure_nonempty_tool_result(tool_name: &str, content: String) -> String {
    if content.trim().is_empty() {
        format!("({tool_name} completed with no output)")
    } else {
        content
    }
}

/// 把 [`ToolCall`] 还原成 OpenAI function-calling 的 `tool_calls` 数组，供回灌 provider。
fn tool_calls_to_json(tool_calls: &[ToolCall]) -> Value {
    Value::Array(
        tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.name, "arguments": call.arguments},
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace_results_become_marker() {
        assert_eq!(
            ensure_nonempty_tool_result("noop", String::new()),
            "(noop completed with no output)"
        );
        assert_eq!(
            ensure_nonempty_tool_result("noop", "  \n\t ".to_string()),
            "(noop completed with no output)"
        );
    }

    #[test]
    fn nonempty_result_is_unchanged() {
        assert_eq!(
            ensure_nonempty_tool_result("echo", "hello".to_string()),
            "hello"
        );
    }
}
