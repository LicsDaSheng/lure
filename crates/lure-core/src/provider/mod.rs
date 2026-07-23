//! Provider 子系统：LLM provider 契约与占位实现。
//!
//! Phase 3 只提供同步 `LlmProvider` trait、契约类型与 `EchoProvider` 占位。
//! registry、model runtime resolver、真实 OpenAI-compatible provider 属 Phase 4。

mod echo;
mod types;

pub use echo::EchoProvider;
pub use types::{CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError};
