//! Lure 桌面应用的 Tauri 组合根。

mod base64;
mod commands;
mod events;
mod state;

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
            commands::get_default_workspace,
            commands::get_pi_state,
            commands::connect_pi,
            commands::new_pi_session,
            commands::disconnect_pi,
            commands::send_prompt,
            commands::abort_pi,
            commands::get_available_models,
            commands::set_model,
            commands::set_thinking_level,
            commands::get_commands,
            commands::respond_extension_ui,
            commands::get_workspace_context,
            commands::read_image_attachments,
        ])
        .build(tauri::generate_context!())?;

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            let state = app_handle.state::<Arc<AppState>>().inner().clone();
            let _ = tauri::async_runtime::block_on(commands::disconnect_inner(app_handle, state));
        }
    });
    Ok(())
}
