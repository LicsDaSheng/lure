//! Provider 子系统：LLM provider 契约、占位、OpenAI-compatible 实现与 registry。
//!
//! Phase 3 提供同步 `LlmProvider` trait、契约类型与 `EchoProvider` 占位；
//! Phase 4 增加 HTTP 传输抽象、`OpenAiCompatProvider`（请求/响应/错误分类）与最小
//! provider registry（选择顺序）；`UreqTransport` 提供真实同步网络出口；
//! `ModelRuntimeResolver` 把 config preset 解析成不可变 `LlmRuntime`/`ProviderSnapshot`
//! 并提供 admit/refresh/invalidate 生命周期。config 驱动的完整 provider 自动匹配
//! （api_key/OAuth/local fallback）留待后续。

mod echo;
mod http;
mod openai;
pub mod registry;
mod runtime;
mod transport;
mod types;

pub use echo::EchoProvider;
pub use http::{HttpRequest, HttpResponse, HttpTransport};
pub use openai::{build_chat_request, parse_chat_response, OpenAiCompatProvider};
pub use registry::{find_by_name, match_provider, ProviderSpec, PROVIDERS};
pub use runtime::{LlmRuntime, ModelRuntimeResolver, ProviderSnapshot, RuntimeError};
pub use transport::UreqTransport;
pub use types::{CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError};
