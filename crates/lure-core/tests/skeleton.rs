//! Phase 0 集成测试入口：确认 crate 的公开 API 与集成测试组织方式可用。
//!
//! 后续 phase 的集成测试按上游领域拆分文件，例如
//! `session_goal_state.rs`、`loop_runner_integration.rs` 等。

#[test]
fn public_version_api_is_reachable() {
    assert!(!lure_core::version().is_empty());
}
