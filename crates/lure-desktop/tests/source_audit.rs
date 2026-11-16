use std::fs;
use std::path::Path;

#[test]
fn workspace_is_pure_rust_and_contains_no_tauri_or_web_frontend_entrypoints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!manifest.to_ascii_lowercase().contains("tauri"));
    assert!(!root.join("src-tauri").exists());
    assert!(!root.join("package.json").exists());
    assert!(!root.join("src/main.tsx").exists());
}

#[test]
fn gpui_ce_is_pinned_to_the_requested_upstream_repository() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("https://github.com/gpui-ce/gpui-ce"));
    assert!(manifest.contains("rev ="));
}
