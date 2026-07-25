//! Agent loop 走 streaming provider 时发出细粒度 `ContentDelta` progress。
//!
//! 用一个覆盖 `complete_streaming` 的 fake provider 逐块回调，验证 loop 把每个内容增量
//! 转成 `ProgressEvent::ContentDelta`，且最终回复与顺序正确。

use lure_core::agent::{AgentLoop, ContextBuilder, ProgressEvent};
use lure_core::bus::InboundMessage;
use lure_core::provider::{
    CompletionRequest, LlmProvider, LlmResponse, ProviderError, StreamChunk,
};
use lure_core::session::SessionManager;

/// 按块回调内容增量的 fake streaming provider。
struct StreamingProvider {
    deltas: Vec<String>,
}

impl LlmProvider for StreamingProvider {
    fn default_model(&self) -> &str {
        "stream-model"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        Ok(LlmResponse::text(self.deltas.concat()))
    }

    fn complete_streaming(
        &self,
        _request: &CompletionRequest,
        on_delta: &mut dyn FnMut(&StreamChunk),
    ) -> Result<LlmResponse, ProviderError> {
        for delta in &self.deltas {
            on_delta(&StreamChunk {
                content_delta: Some(delta.clone()),
                ..StreamChunk::default()
            });
        }
        Ok(LlmResponse::text(self.deltas.concat()))
    }
}

/// 先流式回调若干推理增量、再回调内容增量的 fake provider。
struct ReasoningStreamProvider {
    reasoning: Vec<String>,
    content: Vec<String>,
}

impl LlmProvider for ReasoningStreamProvider {
    fn default_model(&self) -> &str {
        "reason-model"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        Ok(LlmResponse::text(self.content.concat()))
    }

    fn complete_streaming(
        &self,
        _request: &CompletionRequest,
        on_delta: &mut dyn FnMut(&StreamChunk),
    ) -> Result<LlmResponse, ProviderError> {
        for r in &self.reasoning {
            on_delta(&StreamChunk {
                reasoning_delta: Some(r.clone()),
                ..StreamChunk::default()
            });
        }
        for c in &self.content {
            on_delta(&StreamChunk {
                content_delta: Some(c.clone()),
                ..StreamChunk::default()
            });
        }
        let mut resp = LlmResponse::text(self.content.concat());
        resp.reasoning_content = Some(self.reasoning.concat());
        Ok(resp)
    }
}

#[test]
fn streaming_provider_emits_reasoning_delta_progress_before_content() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let provider = ReasoningStreamProvider {
        reasoning: vec!["思考".to_string(), "中".to_string()],
        content: vec!["答案".to_string()],
    };
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    let mut events: Vec<ProgressEvent> = Vec::new();
    let outcome = agent_loop
        .process_streaming(&InboundMessage::new("cli", "direct", "hi"), &mut |e| {
            events.push(e.clone())
        })
        .unwrap();

    assert_eq!(outcome.final_content, "答案");
    // 实时回调含逐段 ReasoningDelta，且在 ContentDelta 之前。
    let reasoning: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ProgressEvent::ReasoningDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(reasoning, vec!["思考", "中"]);

    let first_reasoning = events
        .iter()
        .position(|e| matches!(e, ProgressEvent::ReasoningDelta { .. }));
    let first_content = events
        .iter()
        .position(|e| matches!(e, ProgressEvent::ContentDelta { .. }));
    assert!(first_reasoning < first_content, "推理增量应先于内容增量");
    // 回调序列与最终 progress 一致。
    assert_eq!(events, outcome.progress);
}

#[test]
fn streaming_provider_emits_content_delta_progress_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let provider = StreamingProvider {
        deltas: vec!["你好".to_string(), "，".to_string(), "世界".to_string()],
    };
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .unwrap();

    assert_eq!(outcome.final_content, "你好，世界");

    let deltas: Vec<&str> = outcome
        .progress
        .iter()
        .filter_map(|e| match e {
            ProgressEvent::ContentDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec!["你好", "，", "世界"]);

    // 顺序：TurnStarted 在首、FinalResponse 在尾。
    assert!(matches!(
        outcome.progress.first(),
        Some(ProgressEvent::TurnStarted { .. })
    ));
    assert!(matches!(
        outcome.progress.last(),
        Some(ProgressEvent::FinalResponse { .. })
    ));
}
