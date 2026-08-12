//! provider registry 选择顺序（参数化）。
//!
//! 覆盖：按名查找、forced provider、auto 前缀优先、auto 关键字匹配、前缀胜过关键字。

use lure_core::provider::{find_by_name, match_provider};

#[tokio::test]
async fn find_by_name_normalizes_case_and_separators() {
    assert_eq!(find_by_name("openai").unwrap().name, "openai");
    assert_eq!(find_by_name("OpenAI").unwrap().name, "openai");
    assert_eq!(find_by_name("anthropic").unwrap().name, "anthropic");
    assert!(find_by_name("unknown-provider").is_none());
}

#[tokio::test]
async fn forced_provider_resolves_by_name_ignoring_model() {
    let spec = match_provider("some-unmatched-model", "anthropic").unwrap();
    assert_eq!(spec.name, "anthropic");
    assert_eq!(spec.default_api_base, "https://api.anthropic.com/v1");
}

#[tokio::test]
async fn forced_unknown_provider_is_none() {
    assert!(match_provider("gpt-4o", "nonexistent").is_none());
}

#[tokio::test]
async fn auto_matches_explicit_prefix() {
    let cases = [
        ("openai/gpt-4o", "openai"),
        ("anthropic/claude-opus-4-5", "anthropic"),
        ("deepseek/deepseek-chat", "deepseek"),
    ];
    for (model, expected) in cases {
        assert_eq!(
            match_provider(model, "auto").unwrap().name,
            expected,
            "model={model}"
        );
    }
}

#[tokio::test]
async fn auto_matches_keyword_in_registry_order() {
    let cases = [
        ("gpt-4o", "openai"),
        ("claude-opus-4-5", "anthropic"),
        ("deepseek-chat", "deepseek"),
        ("kimi-k2", "moonshot"),
        ("codestral-latest", "mistral"),
    ];
    for (model, expected) in cases {
        assert_eq!(
            match_provider(model, "auto").unwrap().name,
            expected,
            "model={model}"
        );
    }
}

#[tokio::test]
async fn auto_prefix_wins_over_keyword() {
    // 前缀 openrouter 应胜过内部 "claude" 关键字。
    let spec = match_provider("openrouter/anthropic/claude-3", "auto").unwrap();
    assert_eq!(spec.name, "openrouter");
}

#[tokio::test]
async fn auto_unmatched_model_is_none() {
    assert!(match_provider("some-random-model", "auto").is_none());
}
