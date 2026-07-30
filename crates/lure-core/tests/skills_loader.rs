//! Skills loader 集成测试。
//!
//! 对照上游 `tests/agent/test_skills_loader.py`：workspace/builtin 两源枚举、同名 workspace
//! 遮蔽 builtin、跳过非目录/缺 SKILL.md、disabled 过滤、bins/env 需求可用性过滤、openclaw
//! 别名元数据、YAML frontmatter（flow mapping / 折叠 `>` / 字面 `|` / 原生 bool）、
//! `build_skills_summary` 按根分组相对路径。需求探针（which/env）以注入方式替代上游 monkeypatch。
//!
//! 暂缓：`test_bundled_*`（依赖 nanobot 内置 skills 目录，属外部 vendored 资产，lure 未随包携带）。

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use lure_core::agent::skills::{SkillEntry, SkillsLoader};
use tempfile::TempDir;

/// 写入 `base/name/SKILL.md`，可选 nanobot 元数据 JSON（内联为 YAML flow mapping）。
fn write_skill(base: &Path, name: &str, metadata_json: Option<&str>, body: &str) -> String {
    let dir = base.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let mut lines = vec!["---".to_string()];
    if let Some(json) = metadata_json {
        // 对齐上游 _write_skill：把元数据包在 nanobot key 下（内联 YAML flow mapping）。
        lines.push(format!("metadata: {{\"nanobot\":{json}}}"));
    }
    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    let path = dir.join("SKILL.md");
    std::fs::write(&path, lines.join("\n")).unwrap();
    path.to_string_lossy().into_owned()
}

fn entry(name: &str, path: &str, source: &str) -> SkillEntry {
    SkillEntry {
        name: name.to_string(),
        path: path.to_string(),
        source: source.to_string(),
    }
}

fn always_true(_: &str) -> bool {
    true
}
fn always_false(_: &str) -> bool {
    false
}

fn loader(workspace: &Path, builtin: &Path) -> SkillsLoader {
    SkillsLoader::new(workspace, Some(builtin.to_path_buf()), BTreeSet::new())
}

// —— list_skills 基础 —— //

#[test]
fn empty_when_skills_dir_missing() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();
    assert!(loader(&ws, &builtin).list_skills(false).is_empty());
}

#[test]
fn empty_when_skills_dir_exists_but_empty() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    std::fs::create_dir_all(ws.join("skills")).unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();
    assert!(loader(&ws, &builtin).list_skills(false).is_empty());
}

#[test]
fn workspace_entry_shape_and_source() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    let path = write_skill(&skills_root, "alpha", None, "# Alpha");
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let entries = loader(&ws, &builtin).list_skills(false);
    assert_eq!(entries, vec![entry("alpha", &path, "workspace")]);
}

#[test]
fn skips_non_directories_and_missing_skill_md() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    std::fs::write(skills_root.join("not_a_dir.txt"), "x").unwrap();
    std::fs::create_dir_all(skills_root.join("no_skill_md")).unwrap();
    let ok_path = write_skill(&skills_root, "ok", None, "# Ok");
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let entries = loader(&ws, &builtin).list_skills(false);
    assert_eq!(entries, vec![entry("ok", &ok_path, "workspace")]);
}

#[test]
fn workspace_shadows_builtin_same_name() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    let ws_path = write_skill(&ws_skills, "dup", None, "# Workspace wins");
    let builtin = d.path().join("builtin");
    write_skill(&builtin, "dup", None, "# Builtin");

    let entries = loader(&ws, &builtin).list_skills(false);
    assert_eq!(entries, vec![entry("dup", &ws_path, "workspace")]);
}

#[test]
fn merges_workspace_and_builtin() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    let ws_path = write_skill(&ws_skills, "ws_only", None, "# W");
    let builtin = d.path().join("builtin");
    let bi_path = write_skill(&builtin, "bi_only", None, "# B");

    let mut entries = loader(&ws, &builtin).list_skills(false);
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(
        entries,
        vec![
            entry("bi_only", &bi_path, "builtin"),
            entry("ws_only", &ws_path, "workspace"),
        ]
    );
}

#[test]
fn builtin_omitted_when_dir_missing() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    let ws_path = write_skill(&ws_skills, "solo", None, "# S");
    let missing = d.path().join("no_such_builtin");

    let entries = loader(&ws, &missing).list_skills(false);
    assert_eq!(entries, vec![entry("solo", &ws_path, "workspace")]);
}

// —— 需求过滤（which/env 注入）—— //

#[test]
fn filter_unavailable_excludes_unmet_bin() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    write_skill(
        &skills_root,
        "needs_bin",
        Some(r#"{"requires":{"bins":["fake_binary"]}}"#),
        "# X",
    );
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let which = Arc::new(|cmd: &str| cmd != "fake_binary");
    let l = loader(&ws, &builtin).with_probes(which, Arc::new(always_true));
    assert!(l.list_skills(true).is_empty());
}

#[test]
fn filter_unavailable_includes_when_bin_met() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    let path = write_skill(
        &skills_root,
        "has_bin",
        Some(r#"{"requires":{"bins":["fake_binary"]}}"#),
        "# X",
    );
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let which = Arc::new(|cmd: &str| cmd == "fake_binary");
    let l = loader(&ws, &builtin).with_probes(which, Arc::new(always_false));
    assert_eq!(
        l.list_skills(true),
        vec![entry("has_bin", &path, "workspace")]
    );
}

#[test]
fn filter_unavailable_false_keeps_unmet() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    let path = write_skill(
        &skills_root,
        "blocked",
        Some(r#"{"requires":{"bins":["fake_binary"]}}"#),
        "# X",
    );
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let l = loader(&ws, &builtin).with_probes(Arc::new(always_false), Arc::new(always_false));
    assert_eq!(
        l.list_skills(false),
        vec![entry("blocked", &path, "workspace")]
    );
}

#[test]
fn filter_unavailable_excludes_unmet_env() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    write_skill(
        &skills_root,
        "needs_env",
        Some(r#"{"requires":{"env":["FAKE_ENV"]}}"#),
        "# X",
    );
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let l = loader(&ws, &builtin).with_probes(Arc::new(always_true), Arc::new(always_false));
    assert!(l.list_skills(true).is_empty());
}

#[test]
fn openclaw_metadata_parsed_for_requirements() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let skills_root = ws.join("skills");
    std::fs::create_dir_all(&skills_root).unwrap();
    // openclaw 别名（非 nanobot key）。
    let path = write_skill(
        &skills_root,
        "openclaw_skill",
        Some(r#"{"openclaw":{"requires":{"bins":["oc_bin"]}}}"#),
        "# OC",
    );
    // 注意 write_skill 把 metadata_json 包在 nanobot 下——此处直接写原始 openclaw。
    std::fs::write(
        skills_root.join("openclaw_skill").join("SKILL.md"),
        "---\nmetadata: {\"openclaw\":{\"requires\":{\"bins\":[\"oc_bin\"]}}}\n---\n\n# OC",
    )
    .unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let unmet = loader(&ws, &builtin).with_probes(Arc::new(always_false), Arc::new(always_true));
    assert!(unmet.list_skills(true).is_empty());

    let which = Arc::new(|cmd: &str| cmd == "oc_bin");
    let met = loader(&ws, &builtin).with_probes(which, Arc::new(always_true));
    assert_eq!(
        met.list_skills(true),
        vec![entry("openclaw_skill", &path, "workspace")]
    );
}

// —— disabled —— //

#[test]
fn disabled_excluded_from_list() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    write_skill(&ws_skills, "alpha", None, "# Alpha");
    let beta = write_skill(&ws_skills, "beta", None, "# Beta");
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let disabled: BTreeSet<String> = ["alpha".to_string()].into_iter().collect();
    let l = SkillsLoader::new(&ws, Some(builtin), disabled);
    let entries = l.list_skills(false);
    assert_eq!(entries, vec![entry("beta", &beta, "workspace")]);
}

#[test]
fn disabled_empty_set_no_effect() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    write_skill(&ws_skills, "alpha", None, "# Alpha");
    write_skill(&ws_skills, "beta", None, "# Beta");
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    assert_eq!(loader(&ws, &builtin).list_skills(false).len(), 2);
}

#[test]
fn disabled_excluded_from_summary() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    write_skill(&ws_skills, "alpha", None, "# Alpha");
    write_skill(&ws_skills, "beta", None, "# Beta");
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let disabled: BTreeSet<String> = ["alpha".to_string()].into_iter().collect();
    let summary = SkillsLoader::new(&ws, Some(builtin), disabled).build_skills_summary(None);
    assert!(!summary.contains("alpha"));
    assert!(summary.contains("beta"));
}

#[test]
fn disabled_excluded_from_get_always_skills() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    write_skill(&ws_skills, "alpha", Some(r#"{"always":true}"#), "# Alpha");
    write_skill(&ws_skills, "beta", Some(r#"{"always":true}"#), "# Beta");
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let disabled: BTreeSet<String> = ["alpha".to_string()].into_iter().collect();
    let always = SkillsLoader::new(&ws, Some(builtin), disabled)
        .with_probes(Arc::new(always_true), Arc::new(always_true))
        .get_always_skills();
    assert!(!always.contains(&"alpha".to_string()));
    assert!(always.contains(&"beta".to_string()));
}

// —— build_skills_summary 分组 —— //

#[test]
fn summary_groups_paths_by_root() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(&ws_skills).unwrap();
    let ws_path = write_skill(&ws_skills, "alpha", None, "# Alpha");
    let builtin = d.path().join("builtin");
    let bi_path = write_skill(&builtin, "beta", None, "# Beta");

    let summary = loader(&ws, &builtin).build_skills_summary(None);
    assert_eq!(
        summary
            .matches(&ws_skills.to_string_lossy().as_ref())
            .count(),
        1
    );
    assert_eq!(
        summary.matches(&builtin.to_string_lossy().as_ref()).count(),
        1
    );
    assert!(!summary.contains(&ws_path));
    assert!(!summary.contains(&bi_path));
    assert!(summary.contains("`alpha/SKILL.md`"));
    assert!(summary.contains("`beta/SKILL.md`"));
}

// —— YAML 类型 —— //

#[test]
fn get_skill_metadata_handles_yaml_types() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(ws_skills.join("typed")).unwrap();
    std::fs::write(
        ws_skills.join("typed").join("SKILL.md"),
        "---\nname: typed\nmetadata: {\"nanobot\":{\"requires\":{\"bins\":[\"gh\"]},\"always\":true}}\nalways: true\n---\n\n# Typed\n",
    )
    .unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let meta = loader(&ws, &builtin).get_skill_metadata("typed").unwrap();
    // YAML 原生 bool（非字符串 "true"）。
    assert_eq!(meta.get("always").and_then(|v| v.as_bool()), Some(true));
    // metadata 为解析后的对象，非 JSON 字符串。
    assert!(meta.get("metadata").is_some_and(|v| v.is_object()));
}

#[test]
fn summary_folded_description() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(ws_skills.join("pdf")).unwrap();
    std::fs::write(
        ws_skills.join("pdf").join("SKILL.md"),
        "---\nname: pdf\ndescription: >\n  Use this skill when visual quality and design identity matter for a PDF.\n  CREATE (generate from scratch).\n---\n\n# PDF Skill\n",
    )
    .unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let summary = loader(&ws, &builtin).build_skills_summary(None);
    assert!(summary.contains("pdf"));
    assert!(summary.contains("visual quality"));
}

#[test]
fn literal_description_parsed() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(ws_skills.join("multi")).unwrap();
    std::fs::write(
        ws_skills.join("multi").join("SKILL.md"),
        "---\nname: multi\ndescription: |\n  Line one of description.\n  Line two of description.\n---\n\n# Multi\n",
    )
    .unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let meta = loader(&ws, &builtin).get_skill_metadata("multi").unwrap();
    let desc = meta.get("description").and_then(|v| v.as_str()).unwrap();
    assert!(desc.contains("Line one"));
    assert!(desc.contains("Line two"));
}

// —— load_skill / context —— //

#[test]
fn load_skills_for_context_strips_frontmatter() {
    let d = TempDir::new().unwrap();
    let ws = d.path().join("ws");
    let ws_skills = ws.join("skills");
    std::fs::create_dir_all(ws_skills.join("alpha")).unwrap();
    // 真实 frontmatter（非空 YAML 体），strip 后仅留正文。
    std::fs::write(
        ws_skills.join("alpha").join("SKILL.md"),
        "---\nname: alpha\n---\n\n# Alpha body",
    )
    .unwrap();
    let builtin = d.path().join("builtin");
    std::fs::create_dir_all(&builtin).unwrap();

    let out = loader(&ws, &builtin).load_skills_for_context(&["alpha".to_string()]);
    assert!(out.contains("### Skill: alpha"));
    assert!(out.contains("# Alpha body"));
    assert!(!out.contains("name: alpha"), "frontmatter 应被剥离");
}
