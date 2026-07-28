//! 确保 rust-embed 的内嵌目录 `frontend/dist/` 在编译期存在。
//!
//! `frontend/dist` 是 gitignore 的前端构建产物；干净检出时不存在，rust-embed 的
//! 派生宏扫不到目录会编译失败。此处按需创建空目录，让 `cargo build`/`cargo test`
//! 在未先 `bun run build` 时也能通过（此时无内嵌资源，需构建前端后才有页面）。

use std::path::Path;

fn main() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let dist = Path::new(manifest).join("../../frontend/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
    // dist 内容变化时无需重跑 build.rs（rust-embed 自行处理资源变更）。
    println!("cargo:rerun-if-changed=build.rs");
}
