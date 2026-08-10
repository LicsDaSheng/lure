//! 编译期准备：
//! 1) 确保 rust-embed 内嵌目录 `frontend/dist/` 存在（gitignore 产物，干净检出时缺失，
//!    rust-embed 派生宏扫不到目录会编译失败；按需建空目录让构建通过，需 `bun run build`
//!    后才有真实页面）。
//! 2) 运行 `tauri_build::build()` 生成 Tauri context/capabilities schema。

use std::path::Path;

fn main() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let dist = Path::new(manifest).join("../../frontend/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
    println!("cargo:rerun-if-changed=build.rs");

    tauri_build::build();
}
