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
//! - [`memory`]：Phase 6 的长期记忆存储、history 与 dream consolidation。
//! - [`command`]：slash 命令归一、三层路由与内置命令注册。
//! - [`pairing`]：Phase 7 的 DM 发送者配对码存储与 `/pairing` 派发。
//! - [`bus`]：Phase 7 的 InboundMessage/OutboundMessage 与消息总线。
//! - [`channel`]：Phase 7 的 channel 契约。
//! - [`gateway`]：Phase 7 的最小 gateway 编排。
//! - [`cron`]：Phase 8 的 cron 调度类型、持久化 store 与 session-bound 投递。
//! - [`trigger`]：Phase 8 的本地 trigger at-least-once 投递队列。
//! - [`api`]：Phase 9 的 OpenAI-compatible API 表面（传输无关）。
//! - [`webui`]：Phase 10 的 WebUI 后端服务协议（传输无关）。

pub mod agent;
pub mod api;
pub mod bus;
pub mod channel;
pub mod command;
pub mod config;
pub mod cron;
pub mod gateway;
pub mod memory;
pub mod pairing;
pub mod provider;
pub mod security;
pub mod session;
pub mod tool;
pub mod trigger;
pub mod webui;

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
