//! `strip_think` 行为与 context builder 的 memory 注入顺序。

use lure_core::agent::ContextBuilder;
use lure_core::memory::strip_think;
use serde_json::json;

#[test]
fn strip_think_removes_well_formed_block() {
    assert_eq!(
        strip_think("<think>reasoning</think>final answer"),
        "final answer"
    );
}

#[test]
fn strip_think_drops_pure_leak_to_empty() {
    assert_eq!(strip_think("<think>nothing user-facing</think>"), "");
    assert_eq!(strip_think("<channel|>"), "");
    assert_eq!(strip_think("<|channel|>"), "");
}

#[test]
fn strip_think_keeps_ordinary_text() {
    assert_eq!(
        strip_think("just a normal message"),
        "just a normal message"
    );
}

#[test]
fn context_injects_memory_between_system_and_history() {
    let builder = ContextBuilder::new(Some("system prompt".to_string()))
        .with_memory(Some("## Long-term Memory\nremember X".to_string()));
    let history = vec![json!({"role": "user", "content": "hi"})];

    let built = builder.build(&history);
    assert_eq!(built.len(), 3);
    assert_eq!(built[0]["role"], "system");
    assert_eq!(built[0]["content"], "system prompt");
    assert_eq!(built[1]["role"], "system");
    assert!(built[1]["content"]
        .as_str()
        .unwrap()
        .contains("Long-term Memory"));
    assert_eq!(built[2], json!({"role": "user", "content": "hi"}));
}

#[test]
fn context_skips_empty_memory() {
    let builder = ContextBuilder::new(None).with_memory(Some(String::new()));
    let history = vec![json!({"role": "user", "content": "hi"})];

    let built = builder.build(&history);
    assert_eq!(built.len(), 1);
    assert_eq!(built[0], json!({"role": "user", "content": "hi"}));
}
