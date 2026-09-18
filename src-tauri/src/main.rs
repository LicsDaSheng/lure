// 发布构建在 Windows 上运行时不额外显示控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> tauri::Result<()> {
    lure_desktop_lib::run()
}
