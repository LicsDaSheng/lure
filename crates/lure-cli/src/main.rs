//! `lure-cli`：Rust 版 `nanobot` 复刻的命令行入口。
//!
//! Phase 0 仅提供最小骨架，尚未接入 agent loop / provider。第一条纵向闭环
//! （CLI one-shot + fake provider）在 Phase 3 落地，此处保持最小可运行入口。

fn main() {
    println!("lure {}", lure_core::version());
}
