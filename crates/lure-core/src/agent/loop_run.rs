//! 最小 `AgentLoop`：CLI one-shot 到 provider 再到 session 保存的纵向闭环。
//!
//! 对齐上游 `nanobot/agent/loop.py` 的职责边界，但只保留最小闭环：
//! 追加 user turn → 构建 context → 调 runner/provider → 追加 assistant turn →
//! 保存 session → 返回最终输出与结构化 progress。
//!
//! Phase 3 不做：async/streaming、tool 执行、goal/subagent、consolidation、
//! channel/gateway 投递。progress 先做结构化枚举，不急于完整事件流。

use std::fmt;

use crate::agent::context::ContextBuilder;
use crate::agent::runner::AgentRunner;
use crate::bus::InboundMessage;
use crate::provider::{GenerationSettings, LlmProvider, ProviderError};
use crate::session::{SessionError, SessionManager};

/// 结构化 progress 事件。
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent {
    /// 本轮开始。
    TurnStarted {
        /// 目标 session key。
        session_key: String,
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
    /// 结构化 progress 序列。
    pub progress: Vec<ProgressEvent>,
}

/// agent loop 结构化错误。
#[derive(Debug)]
pub enum AgentError {
    /// provider 调用失败。
    Provider(ProviderError),
    /// session 读写失败。
    Session(SessionError),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Provider(e) => write!(f, "agent provider 错误: {e}"),
            AgentError::Session(e) => write!(f, "agent session 错误: {e}"),
        }
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AgentError::Provider(e) => Some(e),
            AgentError::Session(e) => Some(e),
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
        }
    }

    /// 只读访问 session 管理器（测试与诊断用）。
    pub fn sessions(&self) -> &SessionManager {
        &self.sessions
    }

    /// 可变访问 session 管理器。
    pub fn sessions_mut(&mut self) -> &mut SessionManager {
        &mut self.sessions
    }

    /// 处理一条 inbound 消息，跑完最小闭环并返回产出。
    pub fn process(&mut self, input: &InboundMessage) -> Result<TurnOutcome, AgentError> {
        let key = input.session_key();
        let mut progress = vec![ProgressEvent::TurnStarted {
            session_key: key.clone(),
        }];

        // 1) 追加 user turn。
        self.sessions
            .get_or_create(&key)?
            .add_message("user", &input.content);

        // 2) 构建 context（历史此时已含 user turn）。
        let history = self.sessions.get_or_create(&key)?.get_history(0);
        let messages = self.context.build(&history);

        // 3) 调 runner/provider（借用在此块内结束）。
        let content = {
            let runner = AgentRunner::new(self.provider.as_ref(), self.settings.clone());
            let response = runner.run(&self.model, messages)?;
            response.content.unwrap_or_default()
        };

        // 4) 追加 assistant turn 并保存。
        self.sessions
            .get_or_create(&key)?
            .add_message("assistant", &content);
        self.sessions.save(&key, false)?;

        progress.push(ProgressEvent::FinalResponse {
            content: content.clone(),
        });
        Ok(TurnOutcome {
            final_content: content,
            progress,
        })
    }
}
