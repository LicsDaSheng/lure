//! `lure-core`：Rust 版 `nanobot` 复刻的核心领域库。
//!
//! Phase 0 仅建立 crate 边界与测试入口，暂不实现任何 provider / channel /
//! gateway 行为。公开 API 保持最小，等待上游契约确认后再逐步收敛扩展。

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
