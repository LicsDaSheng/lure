//! Provider usage 归一：把各家 provider 的 cached_tokens 按优先级链归到单一顶层键。
//!
//! 对齐上游 `openai_compat_provider._extract_usage` + `_get_nested_int`，优先级链：
//!
//! 1. `prompt_tokens_details.cached_tokens`（OpenAI/Zhipu/Qwen/Mistral/xAI）
//! 2. `cached_tokens`（StepFun/Moonshot，顶层）
//! 3. `prompt_cache_hit_tokens`（DeepSeek/SiliconFlow）
//!
//! 首个非零命中即写 `cached_tokens`。

use lure_core::provider::normalize_usage;
use serde_json::{json, Map, Value};

fn map(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap()
}

#[tokio::test]
async fn empty_usage_normalizes_to_empty() {
    assert!(normalize_usage(&Map::new()).is_empty());
}

#[tokio::test]
async fn base_fields_are_preserved_with_defaults() {
    let out = normalize_usage(&map(json!({"prompt_tokens": 10, "completion_tokens": 3})));
    assert_eq!(out.get("prompt_tokens").and_then(Value::as_i64), Some(10));
    assert_eq!(
        out.get("completion_tokens").and_then(Value::as_i64),
        Some(3)
    );
    // total 缺失时默认为 0。
    assert_eq!(out.get("total_tokens").and_then(Value::as_i64), Some(0));
    // 无缓存字段时不产出 cached_tokens。
    assert!(!out.contains_key("cached_tokens"));
}

#[tokio::test]
async fn cached_tokens_from_nested_prompt_tokens_details() {
    let out = normalize_usage(&map(json!({
        "prompt_tokens": 100,
        "completion_tokens": 10,
        "total_tokens": 110,
        "prompt_tokens_details": {"cached_tokens": 80}
    })));
    assert_eq!(out.get("cached_tokens").and_then(Value::as_i64), Some(80));
}

#[tokio::test]
async fn cached_tokens_from_top_level_key() {
    let out = normalize_usage(&map(json!({
        "prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110,
        "cached_tokens": 42
    })));
    assert_eq!(out.get("cached_tokens").and_then(Value::as_i64), Some(42));
}

#[tokio::test]
async fn cached_tokens_from_prompt_cache_hit_tokens() {
    // DeepSeek/SiliconFlow 风格。
    let out = normalize_usage(&map(json!({
        "prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110,
        "prompt_cache_hit_tokens": 64
    })));
    assert_eq!(out.get("cached_tokens").and_then(Value::as_i64), Some(64));
}

#[tokio::test]
async fn nested_path_wins_over_alternates_by_priority() {
    // 三路同时存在：嵌套 details 优先。
    let out = normalize_usage(&map(json!({
        "prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110,
        "prompt_tokens_details": {"cached_tokens": 80},
        "cached_tokens": 42,
        "prompt_cache_hit_tokens": 64
    })));
    assert_eq!(out.get("cached_tokens").and_then(Value::as_i64), Some(80));
}

#[tokio::test]
async fn zero_cached_is_skipped_and_falls_through() {
    // 优先路径为 0 时跳过，回退到下一非零路径。
    let out = normalize_usage(&map(json!({
        "prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110,
        "prompt_tokens_details": {"cached_tokens": 0},
        "prompt_cache_hit_tokens": 64
    })));
    assert_eq!(out.get("cached_tokens").and_then(Value::as_i64), Some(64));
}
