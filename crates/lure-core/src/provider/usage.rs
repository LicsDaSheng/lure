//! Usage 归一：把各家 provider 的 cached_tokens 折算到单一顶层 `cached_tokens` 键。
//!
//! 对齐上游 `openai_compat_provider._extract_usage` + `_get_nested_int`：不同 provider
//! 把「缓存命中 token」放在不同位置，归一后 loop 侧 `accumulate_usage` 才能按统一键累加。

use serde_json::{Map, Value};

/// cached_tokens 的优先级路径链：首个非零命中即采用（对齐上游 `_extract_usage`）。
const CACHED_TOKEN_PATHS: &[&[&str]] = &[
    &["prompt_tokens_details", "cached_tokens"], // OpenAI/Zhipu/MiniMax/Qwen/Mistral/xAI
    &["cached_tokens"],                          // StepFun/Moonshot（顶层）
    &["prompt_cache_hit_tokens"],                // DeepSeek/SiliconFlow
];

/// 把原始 usage 归一为 `{prompt_tokens, completion_tokens, total_tokens[, cached_tokens]}`。
///
/// 空输入返回空 map（对齐上游「无 usage 对象返回 `{}`」）。基础三字段缺失时默认 0；
/// cached_tokens 按 [`CACHED_TOKEN_PATHS`] 优先级取首个非零值，全为零/缺失则不产出该键。
pub fn normalize_usage(raw: &Map<String, Value>) -> Map<String, Value> {
    if raw.is_empty() {
        return Map::new();
    }

    let mut result = Map::new();
    for key in ["prompt_tokens", "completion_tokens", "total_tokens"] {
        let n = raw.get(key).and_then(Value::as_i64).unwrap_or(0);
        result.insert(key.to_string(), Value::from(n));
    }

    for path in CACHED_TOKEN_PATHS {
        let cached = get_nested_int(raw, path);
        if cached != 0 {
            result.insert("cached_tokens".to_string(), Value::from(cached));
            break;
        }
    }

    result
}

/// 沿 `path` 逐段下钻取整数；任一段缺失或非对象返回 0。对齐上游 `_get_nested_int`。
fn get_nested_int(root: &Map<String, Value>, path: &[&str]) -> i64 {
    let Some((last, parents)) = path.split_last() else {
        return 0;
    };
    let mut current = root;
    for segment in parents {
        match current.get(*segment).and_then(Value::as_object) {
            Some(next) => current = next,
            None => return 0,
        }
    }
    current.get(*last).and_then(Value::as_i64).unwrap_or(0)
}
