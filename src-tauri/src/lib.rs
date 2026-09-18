//! Lure 桌面应用的 Tauri 组合根。

/// 创建并运行 Lure 桌面应用。
///
/// # Errors
///
/// 当 Tauri 上下文初始化或事件循环启动失败时返回错误。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
