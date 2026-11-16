//! Lure 桌面应用的 Tauri 组合根。

mod state;
mod terminal_commands;

use std::sync::Arc;

use state::AppState;
use tauri::{Manager, RunEvent};

/// 创建并运行 Lure 桌面应用。
///
/// # Errors
///
/// 当 Tauri 上下文初始化失败时返回错误。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            terminal_commands::get_default_workspace,
            terminal_commands::start_terminal,
            terminal_commands::write_terminal,
            terminal_commands::resize_terminal,
            terminal_commands::stop_terminal,
        ])
        .build(tauri::generate_context!())?;

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            let state = app_handle.state::<Arc<AppState>>().inner().clone();
            let _ = tauri::async_runtime::block_on(terminal_commands::stop_all(state));
        }
    });
    Ok(())
}
