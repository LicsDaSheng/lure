//! 映射上游 `tests/agent/test_context_builder.py` 与
//! `tests/agent/test_context_prompt_cache.py` 的系统提示词装配主路径。

use std::fs;

use lure_core::agent::{
    ContextBuilder, PromptBuildOptions, DEFAULT_AGENTS, DEFAULT_SOUL, DEFAULT_USER,
};
use lure_core::memory::MemoryStore;
use serde_json::json;

fn write_skill(root: &std::path::Path, name: &str, frontmatter: &str, body: &str) {
    let dir = root.join("skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {name} description\n{frontmatter}---\n\n{body}"),
    )
    .unwrap();
}

#[test]
fn workspace_prompt_assembles_all_sections_in_nanobot_order() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    fs::write(workspace.join("AGENTS.md"), "project rules").unwrap();
    fs::write(workspace.join("SOUL.md"), "agent soul").unwrap();
    fs::write(workspace.join("USER.md"), "user profile").unwrap();

    let memory = MemoryStore::new(workspace).unwrap();
    memory.write_memory("用户喜欢简洁回答");
    memory
        .append_history("previous useful fact", Some("cli:direct"))
        .unwrap();
    memory
        .append_history("other session fact", Some("cli:other"))
        .unwrap();

    write_skill(
        workspace,
        "always-on",
        "always: true\n",
        "always instructions",
    );
    write_skill(workspace, "review", "", "review instructions");
    write_skill(workspace, "library-only", "", "library instructions");

    let builder = ContextBuilder::for_workspace(workspace);
    let prepared = builder.prepare_turn(PromptBuildOptions {
        current_message: "please $review this",
        channel: Some("cli"),
        session_key: Some("cli:direct"),
        session_summary: Some("archived discussion"),
    });
    let messages = prepared.build(&[json!({"role": "user", "content": "hello"})]);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    let prompt = messages[0]["content"].as_str().unwrap();

    let expected_in_order = [
        "## Runtime",
        "## Workspace",
        "## AGENTS.md",
        "project rules",
        "## SOUL.md",
        "agent soul",
        "## USER.md",
        "user profile",
        "# Tool Usage Notes",
        "# Memory",
        "用户喜欢简洁回答",
        "# Active Skills",
        "always instructions",
        "review instructions",
        "# Skills",
        "library-only",
        "# Recent History",
        "previous useful fact",
        "[Archived Context Summary]",
        "archived discussion",
    ];
    let mut cursor = 0;
    for needle in expected_in_order {
        let offset = prompt[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("提示词缺少或顺序错误: {needle}\n{prompt}"));
        cursor += offset + needle.len();
    }

    assert!(prompt.contains(&workspace.display().to_string()));
    assert!(prompt.contains("Output is rendered in a terminal"));
    assert!(!prompt.contains("other session fact"));
    assert!(!prompt.contains("library instructions"));
    assert_eq!(prompt.matches("### Skill: review").count(), 1);
    assert!(prompt.matches("\n\n---\n\n").count() >= 6);
}

#[test]
fn empty_optional_content_keeps_identity_and_tool_contract_only() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("memory")).unwrap();
    fs::write(dir.path().join("memory/MEMORY.md"), "# Memory\n\n").unwrap();

    let messages = ContextBuilder::for_workspace(dir.path())
        .prepare_turn(PromptBuildOptions {
            current_message: "$unknown is a shell literal",
            channel: None,
            session_key: Some("cli:direct"),
            session_summary: None,
        })
        .build(&[]);
    let prompt = messages[0]["content"].as_str().unwrap();

    assert!(prompt.contains("## Runtime"));
    assert!(prompt.contains("# Tool Usage Notes"));
    assert!(!prompt.contains("# Active Skills"));
    assert!(!prompt.contains("# Skills\n"));
    assert!(!prompt.contains("# Recent History"));
    assert!(!prompt.contains("# Memory\n\n## Long-term Memory"));
}

#[test]
fn unmodified_agents_and_user_templates_are_skipped_but_soul_is_loaded() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("AGENTS.md"), DEFAULT_AGENTS).unwrap();
    fs::write(dir.path().join("SOUL.md"), DEFAULT_SOUL).unwrap();
    fs::write(dir.path().join("USER.md"), DEFAULT_USER).unwrap();

    let prompt = ContextBuilder::for_workspace(dir.path())
        .prepare_turn(PromptBuildOptions {
            current_message: "hello",
            channel: None,
            session_key: None,
            session_summary: None,
        })
        .build(&[])[0]["content"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(!prompt.contains("## AGENTS.md"));
    assert!(prompt.contains("## SOUL.md"));
    assert!(!prompt.contains("## USER.md"));
}

#[test]
fn recent_history_uses_last_fifty_entries_after_dream_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let memory = MemoryStore::new(dir.path()).unwrap();
    for index in 0..55 {
        memory
            .append_history(&format!("entry-{index}"), Some("cli:direct"))
            .unwrap();
    }
    memory.set_last_dream_cursor(2);

    let prompt = ContextBuilder::for_workspace(dir.path())
        .prepare_turn(PromptBuildOptions {
            current_message: "hello",
            channel: Some("cli"),
            session_key: Some("cli:direct"),
            session_summary: None,
        })
        .build(&[])[0]["content"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(!prompt.contains("entry-4\n"));
    assert!(prompt.contains("entry-5"));
    assert!(prompt.contains("entry-54"));
}
