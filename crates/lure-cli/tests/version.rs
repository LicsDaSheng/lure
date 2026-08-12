//! 映射上游 `tests/test_package_version.py`：发布产物的版本核对。
//!
//! CLI `--version`（含无子命令）输出的版本必须与包版本一致。

use std::process::Command;

#[tokio::test]
async fn version_flag_matches_package_version() {
    for args in [vec!["--version"], vec!["-V"], vec![]] {
        let output = Command::new(env!("CARGO_BIN_EXE_lure"))
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.trim(), format!("lure {}", env!("CARGO_PKG_VERSION")));
    }
}
