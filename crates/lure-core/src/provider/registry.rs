//! 最小 provider registry 与选择顺序。
//!
//! 对齐上游 `nanobot/providers/registry.py` 与 `Config._match_provider` 的**选择
//! 顺序本质**：强制 provider 按名查找；`auto` 时先看 `provider/model` 显式前缀，
//! 再按 registry 顺序做关键字匹配。
//!
//! Phase 4 只收录代表性子集，且不含 config 驱动的 api_key/OAuth/local fallback
//! 与 transcription-only 过滤——这些依赖尚未建模的 `ProvidersConfig`，留待后续
//! （见 upstream-test-ledger）。

/// 一个 provider 规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSpec {
    /// registry 名称。
    pub name: &'static str,
    /// 关键字（用于 auto 模型匹配）。
    pub keywords: &'static [&'static str],
    /// 默认 API base URL。
    pub default_api_base: &'static str,
}

/// 代表性 provider 子集，顺序对齐上游 registry 的相对次序。
pub static PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        name: "openrouter",
        keywords: &["openrouter"],
        default_api_base: "https://openrouter.ai/api/v1",
    },
    ProviderSpec {
        name: "anthropic",
        keywords: &["anthropic", "claude"],
        default_api_base: "https://api.anthropic.com/v1",
    },
    ProviderSpec {
        name: "openai",
        keywords: &["openai", "gpt"],
        default_api_base: "https://api.openai.com/v1",
    },
    ProviderSpec {
        name: "deepseek",
        keywords: &["deepseek"],
        default_api_base: "https://api.deepseek.com",
    },
    ProviderSpec {
        name: "gemini",
        keywords: &["gemini", "gemma"],
        default_api_base: "https://generativelanguage.googleapis.com/v1beta/openai/",
    },
    ProviderSpec {
        name: "moonshot",
        keywords: &["moonshot", "kimi"],
        default_api_base: "https://api.moonshot.ai/v1",
    },
    ProviderSpec {
        name: "mistral",
        keywords: &["mistral", "magistral", "ministral", "codestral", "devstral"],
        default_api_base: "https://api.mistral.ai/v1",
    },
    ProviderSpec {
        name: "groq",
        keywords: &["groq"],
        default_api_base: "https://api.groq.com/openai/v1",
    },
    ProviderSpec {
        name: "novita",
        keywords: &["novita"],
        default_api_base: "https://api.novita.ai/openai",
    },
];

/// 按名精确查找 provider（大小写、连字符/下划线归一）。
pub fn find_by_name(name: &str) -> Option<&'static ProviderSpec> {
    let normalized = normalize(name);
    PROVIDERS
        .iter()
        .find(|spec| normalize(spec.name) == normalized)
}

/// 解析模型应使用的 provider。
///
/// - `forced != "auto"`：按名查找。
/// - `auto`：先看 `provider/model` 显式前缀，再按 registry 顺序关键字匹配。
pub fn match_provider(model: &str, forced: &str) -> Option<&'static ProviderSpec> {
    if forced != "auto" {
        return find_by_name(forced);
    }

    if let Some((prefix, _)) = model.split_once('/') {
        if let Some(spec) = find_by_name(prefix) {
            return Some(spec);
        }
    }

    let lower = model.to_lowercase();
    PROVIDERS
        .iter()
        .find(|spec| spec.keywords.iter().any(|kw| lower.contains(kw)))
}

fn normalize(value: &str) -> String {
    value.to_lowercase().replace('-', "_")
}
