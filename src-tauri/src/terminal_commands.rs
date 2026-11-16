use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;

use lure_pty::{PtyCommand, PtyEvent, PtyProcess, TerminalSize};
use lure_rpc::PiProcessConfig;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{SharedAppState, TerminalSession};

pub(crate) const TERMINAL_OUTPUT_EVENT: &str = "lure://terminal-output";
pub(crate) const TERMINAL_EXIT_EVENT: &str = "lure://terminal-exit";
const DEFAULT_WORKSPACE_NAME: &str = "lure";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TerminalStarted {
    session_id: u64,
    working_directory: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalOutput {
    session_id: u64,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalExit {
    session_id: u64,
    exit_code: Option<u32>,
    success: bool,
}

#[tauri::command]
pub(crate) async fn get_default_workspace(app: AppHandle) -> Result<String, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| format!("无法确定用户家目录：{error}"))?;
    let workspace = home.join(DEFAULT_WORKSPACE_NAME);
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| format!("无法创建默认工作目录 {}：{error}", workspace.display()))?;
    canonical_directory(&workspace).map(|path| path.display().to_string())
}

#[tauri::command]
pub(crate) async fn start_terminal(
    app: AppHandle,
    state: State<'_, SharedAppState>,
    working_directory: String,
    columns: u16,
    rows: u16,
) -> Result<TerminalStarted, String> {
    let state = state.inner().clone();
    let _command_guard = state.command_lock.lock().await;
    stop_current(&state).await?;

    let directory = canonical_directory(Path::new(&working_directory))?;
    let mut environment = state.spawn_env_resolver.resolve().await;
    set_environment(&mut environment, "TERM", "xterm-256color");
    set_environment(&mut environment, "COLORTERM", "truecolor");
    set_environment(&mut environment, "TERM_PROGRAM", "Lure");
    let process_config = PiProcessConfig::for_working_directory(&directory);
    let mut command = PtyCommand::new(process_config.executable, &directory);
    command.environment = Some(environment);
    let size = TerminalSize::new(columns, rows);

    let (process, events) = tokio::task::spawn_blocking(move || PtyProcess::spawn(command, size))
        .await
        .map_err(|error| format!("创建 PTY 任务失败：{error}"))?
        .map_err(|error| error.to_string())?;

    let session_id = state.next_session_id.fetch_add(1, Ordering::Relaxed);
    let working_directory = directory.display().to_string();
    *state.terminal.lock().await = Some(TerminalSession {
        id: session_id,
        process,
    });
    if let Err(error) = forward_events(app, state.clone(), session_id, events) {
        let _ = stop_current(&state).await;
        return Err(error);
    }

    Ok(TerminalStarted {
        session_id,
        working_directory,
    })
}

#[tauri::command]
pub(crate) async fn write_terminal(
    state: State<'_, SharedAppState>,
    session_id: u64,
    data: String,
) -> Result<(), String> {
    let mut terminal = state.terminal.lock().await;
    let session = matching_session(&mut terminal, session_id)?;
    session
        .process
        .write_all(data.as_bytes())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn resize_terminal(
    state: State<'_, SharedAppState>,
    session_id: u64,
    columns: u16,
    rows: u16,
) -> Result<(), String> {
    let mut terminal = state.terminal.lock().await;
    let session = matching_session(&mut terminal, session_id)?;
    session
        .process
        .resize(TerminalSize::new(columns, rows))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn stop_terminal(
    state: State<'_, SharedAppState>,
    session_id: u64,
) -> Result<(), String> {
    let state = state.inner().clone();
    let _command_guard = state.command_lock.lock().await;
    let mut terminal = state.terminal.lock().await;
    let Some(session) = terminal.as_ref() else {
        return Ok(());
    };
    if session.id != session_id {
        return Err("终端会话已被替换".into());
    }
    let Some(mut session) = terminal.take() else {
        return Ok(());
    };
    session.process.stop().map_err(|error| error.to_string())
}

pub(crate) async fn stop_all(state: SharedAppState) -> Result<(), String> {
    let _command_guard = state.command_lock.lock().await;
    stop_current(&state).await
}

async fn stop_current(state: &SharedAppState) -> Result<(), String> {
    let mut terminal = state.terminal.lock().await;
    let Some(mut session) = terminal.take() else {
        return Ok(());
    };
    session.process.stop().map_err(|error| error.to_string())
}

fn matching_session(
    terminal: &mut Option<TerminalSession>,
    session_id: u64,
) -> Result<&mut TerminalSession, String> {
    match terminal.as_mut() {
        Some(session) if session.id == session_id => Ok(session),
        Some(_) => Err("终端会话已被替换".into()),
        None => Err("当前没有运行中的 Pi 终端".into()),
    }
}

fn forward_events(
    app: AppHandle,
    state: SharedAppState,
    session_id: u64,
    events: std::sync::mpsc::Receiver<PtyEvent>,
) -> Result<(), String> {
    thread::Builder::new()
        .name(format!("lure-terminal-events-{session_id}"))
        .spawn(move || {
            while let Ok(event) = events.recv() {
                match event {
                    PtyEvent::Output(data) => {
                        let _ =
                            app.emit(TERMINAL_OUTPUT_EVENT, TerminalOutput { session_id, data });
                    }
                    PtyEvent::Exited(status) => {
                        let _ = app.emit(
                            TERMINAL_EXIT_EVENT,
                            TerminalExit {
                                session_id,
                                exit_code: Some(status.exit_code),
                                success: status.success,
                            },
                        );
                        let mut terminal = state.terminal.blocking_lock();
                        if terminal
                            .as_ref()
                            .is_some_and(|session| session.id == session_id)
                        {
                            terminal.take();
                        }
                        return;
                    }
                    PtyEvent::ReadFailed(message) => {
                        let _ = app.emit(
                            TERMINAL_OUTPUT_EVENT,
                            TerminalOutput {
                                session_id,
                                data: format!("\r\n[Lure PTY] {message}\r\n").into_bytes(),
                            },
                        );
                    }
                }
            }
        })
        .map(|_| ())
        .map_err(|error| format!("无法启动终端事件转发线程：{error}"))
}

fn set_environment(environment: &mut Vec<(OsString, OsString)>, key: &str, value: &str) {
    if let Some((_, current)) = environment
        .iter_mut()
        .find(|(name, _)| name == OsStr::new(key))
    {
        *current = OsString::from(value);
    } else {
        environment.push((OsString::from(key), OsString::from(value)));
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("工作目录无效 {}：{error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("工作目录不是文件夹：{}", canonical.display()));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{canonical_directory, set_environment};

    #[test]
    fn terminal_environment_overrides_existing_values() {
        let mut environment = vec![("TERM".into(), "dumb".into())];
        set_environment(&mut environment, "TERM", "xterm-256color");
        set_environment(&mut environment, "COLORTERM", "truecolor");
        assert_eq!(
            environment,
            vec![
                ("TERM".into(), "xterm-256color".into()),
                ("COLORTERM".into(), "truecolor".into())
            ]
        );
    }

    #[test]
    fn canonical_directory_rejects_files() {
        let path = std::env::temp_dir().join(format!(
            "lure-terminal-file-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("系统时间有效")
                .as_nanos()
        ));
        fs::write(&path, "not a directory").expect("创建临时文件");
        let result = canonical_directory(&path);
        fs::remove_file(path).expect("删除临时文件");
        assert!(result.is_err());
    }
}
