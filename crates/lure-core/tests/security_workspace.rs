//! 映射上游 `tests/security/test_workspace_policy.py`。
//!
//! 覆盖：workspace 相对路径接受、父目录穿越拒绝、前缀兄弟目录拒绝、符号链接逃逸拒绝、
//! 额外 root 接受、精确文件允许。

use std::path::{Path, PathBuf};

use lure_core::security::{is_path_within, resolve_allowed_path, WorkspaceBoundaryError};
use tempfile::tempdir;

fn ws(dir: &Path) -> PathBuf {
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    workspace
}

#[tokio::test]
async fn accepts_workspace_relative_path() {
    let dir = tempdir().unwrap();
    let workspace = ws(dir.path());
    let target = workspace.join("src").join("main.rs");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "fn main() {}").unwrap();

    let resolved = resolve_allowed_path(
        Path::new("src/main.rs"),
        Some(&workspace),
        Some(&workspace),
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(resolved, std::fs::canonicalize(&target).unwrap());
}

#[tokio::test]
async fn blocks_parent_traversal_shapes() {
    let dir = tempdir().unwrap();
    let workspace = ws(dir.path());
    std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();

    let shapes = [
        PathBuf::from("../secret.txt"),
        PathBuf::from("src/../../secret.txt"),
        workspace
            .join("src")
            .join("..")
            .join("..")
            .join("secret.txt"),
    ];
    for shape in shapes {
        let err = resolve_allowed_path(&shape, Some(&workspace), Some(&workspace), &[], &[]);
        assert!(
            matches!(err, Err(WorkspaceBoundaryError { .. })),
            "shape={shape:?} 应被拒绝"
        );
        if let Err(e) = err {
            assert!(e.to_string().contains("outside allowed directory"));
        }
    }
}

#[tokio::test]
async fn blocks_prefix_sibling_directory() {
    let dir = tempdir().unwrap();
    let workspace = ws(dir.path());
    let sibling = dir.path().join("workspace-other");
    std::fs::create_dir_all(&sibling).unwrap();
    let secret = sibling.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();

    let err = resolve_allowed_path(&secret, Some(&workspace), Some(&workspace), &[], &[]);
    assert!(matches!(err, Err(WorkspaceBoundaryError { .. })));
}

#[cfg(unix)]
#[tokio::test]
async fn blocks_symlink_escape() {
    let dir = tempdir().unwrap();
    let workspace = ws(dir.path());
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    let link = workspace.join("linked-secret.txt");
    std::os::unix::fs::symlink(&secret, &link).unwrap();

    assert!(!is_path_within(&link, &workspace));
    let err = resolve_allowed_path(
        Path::new("linked-secret.txt"),
        Some(&workspace),
        Some(&workspace),
        &[],
        &[],
    );
    assert!(matches!(err, Err(WorkspaceBoundaryError { .. })));
}

#[tokio::test]
async fn allows_extra_root() {
    let dir = tempdir().unwrap();
    let workspace = ws(dir.path());
    let media = dir.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    let image = media.join("image.png");
    std::fs::write(&image, b"PNG").unwrap();

    let resolved = resolve_allowed_path(
        &image,
        Some(&workspace),
        Some(&workspace),
        std::slice::from_ref(&media),
        &[],
    )
    .unwrap();
    assert_eq!(resolved, std::fs::canonicalize(&image).unwrap());
}

#[tokio::test]
async fn allows_extra_file_only_exactly() {
    let dir = tempdir().unwrap();
    let workspace = ws(dir.path());
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let allowed = outside.join("allowed.txt");

    let resolved = resolve_allowed_path(
        &allowed,
        Some(&workspace),
        Some(&workspace),
        &[],
        std::slice::from_ref(&allowed),
    )
    .unwrap();
    assert_eq!(resolved.file_name().unwrap(), "allowed.txt");

    // 允许文件的子路径不被放行。
    let child = allowed.join("child.txt");
    let err = resolve_allowed_path(
        &child,
        Some(&workspace),
        Some(&workspace),
        &[],
        std::slice::from_ref(&allowed),
    );
    assert!(matches!(err, Err(WorkspaceBoundaryError { .. })));
}
