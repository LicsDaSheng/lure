//! `lure-core`：Rust 版 `nanobot` 复刻的核心领域库。
//!
//! 公开 API 保持最小，等待上游契约确认后再逐步收敛扩展。
//!
//! 当前已落地：
//! - [`config`]：Phase 1 的配置 schema、路径解析与读写。
//! - [`session`]：Phase 2 的 session key、存储、goal 派生视图。
//! - [`provider`]：Phase 3 的 LLM provider 契约与占位实现。
//! - [`agent`]：Phase 3 的最小 loop / runner / context 闭环。
//! - [`security`]：Phase 5 的 workspace 路径边界。
//! - [`tool`]：Phase 5 的 tool trait / registry / 文件与 shell 工具。

pub mod agent;
pub mod config;
pub mod provider;
pub mod security;
pub mod session;
pub mod tool;

/// 返回 `lure-core` crate 的版本号。
///
/// 该函数用于 CLI 与诊断输出，属于构建元数据，不代表任何产品行为契约。
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_package_metadata() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
