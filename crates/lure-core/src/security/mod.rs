//! 安全子系统：workspace 路径边界等应用层守卫。
//!
//! Phase 5 覆盖 workspace 边界；network SSRF、启动安全等留待对应 phase。

mod workspace;

pub use workspace::{
    is_path_allowed, is_path_within, resolve_allowed_path, resolve_path, WorkspaceBoundaryError,
    WORKSPACE_BOUNDARY_NOTE,
};
