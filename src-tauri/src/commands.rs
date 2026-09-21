use std::path::{Path, PathBuf};
use std::time::Duration;

use lure_core::{
    ConnectionPhase, ConnectionSnapshot, ErrorCode, LureError, LureEvent, ModelSnapshot,
};
use lure_rpc::{
    PiProcessConfig, PiRpcClient, RpcCommand, RpcError, RpcImage, RpcModel, RpcSessionState,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tokio::process::Command;

use crate::events::{emit_lure_event, forward_pi_events};
use crate::state::{DesktopSession, Operation, SharedAppState, ensure_operation_allowed};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestAccepted {
    accepted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceContext {
    working_directory: String,
    branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageAttachment {
    name: String,
    data: String,
    mime_type: String,
}

const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_WORKSPACE_NAME: &str = "lure";

#[tauri::command]
pub(crate) async fn get_default_workspace(app: AppHandle) -> Result<String, LureError> {
    let home = app.path().home_dir().map_err(|error| {
        LureError::new(
            ErrorCode::InvalidWorkingDirectory,
            format!("无法确定用户家目录：{error}"),
        )
    })?;
    Ok(ensure_default_workspace(&home).await?.display().to_string())
}

async fn ensure_default_workspace(home: &Path) -> Result<PathBuf, LureError> {
    let workspace = home.join(DEFAULT_WORKSPACE_NAME);
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| {
            LureError::new(
                ErrorCode::InvalidWorkingDirectory,
                format!("无法创建默认工作目录 {}：{error}", workspace.display()),
            )
        })?;
    canonical_directory(&workspace.display().to_string())
}

/// 返回桌面进程当前持有的 Pi 会话快照，供 `WebView` 重载后恢复 UI 状态。
#[tauri::command]
pub(crate) async fn get_pi_state(
    state: State<'_, SharedAppState>,
) -> Result<ConnectionSnapshot, LureError> {
    Ok(state.snapshot.read().await.clone())
}

#[tauri::command]
pub(crate) async fn connect_pi(
    app: AppHandle,
    state: State<'_, SharedAppState>,
    working_directory: String,
) -> Result<ConnectionSnapshot, LureError> {
    let state = state.inner().clone();
    let _command_guard = state.command_lock.lock().await;
    let phase = state.snapshot.read().await.phase;
    ensure_operation_allowed(phase, Operation::Connect)?;

    let mut session_guard = state.session.lock().await;
    if session_guard.is_some() {
        return Err(LureError::new(
            ErrorCode::AlreadyConnected,
            "请先断开现有 Pi 会话",
        ));
    }

    let canonical = canonical_directory(&working_directory)?;
    let connecting = ConnectionSnapshot {
        phase: ConnectionPhase::Connecting,
        working_directory: Some(canonical.display().to_string()),
        ..ConnectionSnapshot::default()
    };
    *state.snapshot.write().await = connecting.clone();
    emit_lure_event(
        &app,
        &state,
        LureEvent::ConnectionChanged {
            snapshot: connecting,
        },
    );

    let config = PiProcessConfig::for_working_directory(&canonical);
    let (client, rpc_state) = match PiRpcClient::connect(config).await {
        Ok(value) => value,
        Err(error) => {
            let error = map_connect_error(&error);
            let failed = ConnectionSnapshot {
                phase: ConnectionPhase::Failed,
                working_directory: Some(canonical.display().to_string()),
                error: Some(error.clone()),
                ..ConnectionSnapshot::default()
            };
            *state.snapshot.write().await = failed.clone();
            emit_lure_event(
                &app,
                &state,
                LureEvent::ConnectionChanged { snapshot: failed },
            );
            return Err(error);
        }
    };

    let ready = ready_snapshot(rpc_state, canonical.display().to_string());

    let receiver = client.subscribe();
    let event_task = tokio::spawn(forward_pi_events(app.clone(), state.clone(), receiver));
    *session_guard = Some(DesktopSession { client, event_task });
    *state.snapshot.write().await = ready.clone();
    emit_lure_event(
        &app,
        &state,
        LureEvent::SessionReady {
            snapshot: ready.clone(),
        },
    );
    emit_lure_event(
        &app,
        &state,
        LureEvent::ConnectionChanged {
            snapshot: ready.clone(),
        },
    );
    Ok(ready)
}

#[tauri::command]
pub(crate) async fn new_pi_session(
    app: AppHandle,
    state: State<'_, SharedAppState>,
) -> Result<ConnectionSnapshot, LureError> {
    let state = state.inner().clone();
    let _command_guard = state.command_lock.lock().await;
    ensure_operation_allowed(state.snapshot.read().await.phase, Operation::NewSession)?;

    let client = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.client.clone())
        .ok_or_else(|| LureError::new(ErrorCode::NotConnected, "Pi 尚未连接"))?;
    let working_directory = state
        .snapshot
        .read()
        .await
        .working_directory
        .clone()
        .ok_or_else(|| LureError::new(ErrorCode::NotConnected, "Pi 工作目录不可用"))?;
    let rpc_state = client
        .new_session()
        .await
        .map_err(|error| map_rpc_error(&error))?;
    let ready = ready_snapshot(rpc_state, working_directory);
    *state.snapshot.write().await = ready.clone();
    emit_lure_event(
        &app,
        &state,
        LureEvent::SessionReady {
            snapshot: ready.clone(),
        },
    );
    emit_lure_event(
        &app,
        &state,
        LureEvent::ConnectionChanged {
            snapshot: ready.clone(),
        },
    );
    Ok(ready)
}

fn ready_snapshot(rpc_state: RpcSessionState, working_directory: String) -> ConnectionSnapshot {
    ConnectionSnapshot {
        phase: ConnectionPhase::Ready,
        working_directory: Some(working_directory),
        session_id: Some(rpc_state.session_id),
        session_file: rpc_state.session_file,
        model: rpc_state.model.map(|model| ModelSnapshot {
            provider: model.provider,
            id: model.id,
        }),
        thinking_level: Some(rpc_state.thinking_level),
        error: None,
    }
}

#[tauri::command]
pub(crate) async fn disconnect_pi(
    app: AppHandle,
    state: State<'_, SharedAppState>,
) -> Result<(), LureError> {
    disconnect_inner(&app, state.inner().clone()).await
}

pub(crate) async fn disconnect_inner(
    app: &AppHandle,
    state: SharedAppState,
) -> Result<(), LureError> {
    let _command_guard = state.command_lock.lock().await;
    let session = state.session.lock().await.take();
    if let Some(session) = session {
        session.event_task.abort();
        if state.snapshot.read().await.phase == ConnectionPhase::Running {
            let _ = tokio::time::timeout(Duration::from_secs(2), session.client.abort()).await;
        }
        session
            .client
            .stop()
            .await
            .map_err(|error| map_rpc_error(&error))?;
    }

    let disconnected = ConnectionSnapshot::default();
    *state.snapshot.write().await = disconnected.clone();
    emit_lure_event(
        app,
        &state,
        LureEvent::ConnectionChanged {
            snapshot: disconnected,
        },
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn send_prompt(
    app: AppHandle,
    state: State<'_, SharedAppState>,
    message: String,
    images: Vec<RpcImage>,
) -> Result<RequestAccepted, LureError> {
    let state = state.inner().clone();
    let _command_guard = state.command_lock.lock().await;
    let phase = state.snapshot.read().await.phase;
    ensure_operation_allowed(phase, Operation::Prompt)?;
    if message.trim().is_empty() {
        return Err(LureError::new(
            ErrorCode::RpcCommandRejected,
            "消息不能为空",
        ));
    }

    let client = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.client.clone())
        .ok_or_else(|| LureError::new(ErrorCode::NotConnected, "Pi 尚未连接"))?;

    let running = {
        let mut current = state.snapshot.write().await;
        current.phase = ConnectionPhase::Running;
        current.clone()
    };
    emit_lure_event(
        &app,
        &state,
        LureEvent::ConnectionChanged { snapshot: running },
    );

    let result = if images.is_empty() {
        client.prompt(message).await
    } else {
        client.prompt_with_images(message, images).await
    };
    if let Err(error) = result {
        let ready = {
            let mut current = state.snapshot.write().await;
            current.phase = ConnectionPhase::Ready;
            current.clone()
        };
        emit_lure_event(
            &app,
            &state,
            LureEvent::ConnectionChanged { snapshot: ready },
        );
        return Err(map_rpc_error(&error));
    }
    Ok(RequestAccepted { accepted: true })
}

#[tauri::command]
pub(crate) async fn get_available_models(
    state: State<'_, SharedAppState>,
) -> Result<Vec<RpcModel>, LureError> {
    session_client(&state)
        .await?
        .get_available_models()
        .await
        .map_err(|error| map_rpc_error(&error))
}

#[tauri::command]
pub(crate) async fn set_model(
    app: AppHandle,
    state: State<'_, SharedAppState>,
    provider: String,
    model_id: String,
) -> Result<RpcModel, LureError> {
    let model = session_client(&state)
        .await?
        .set_model(&provider, &model_id)
        .await
        .map_err(|error| map_rpc_error(&error))?;
    let snapshot = {
        let mut snapshot = state.snapshot.write().await;
        snapshot.model = Some(ModelSnapshot {
            provider: model.provider.clone(),
            id: model.id.clone(),
        });
        snapshot.clone()
    };
    emit_lure_event(
        &app,
        state.inner(),
        LureEvent::ConnectionChanged { snapshot },
    );
    Ok(model)
}

#[tauri::command]
pub(crate) async fn set_thinking_level(
    app: AppHandle,
    state: State<'_, SharedAppState>,
    level: String,
) -> Result<String, LureError> {
    let level = session_client(&state)
        .await?
        .set_thinking_level(&level)
        .await
        .map_err(|error| map_rpc_error(&error))?;
    let snapshot = {
        let mut snapshot = state.snapshot.write().await;
        snapshot.thinking_level = Some(level.clone());
        snapshot.clone()
    };
    emit_lure_event(
        &app,
        state.inner(),
        LureEvent::ConnectionChanged { snapshot },
    );
    Ok(level)
}

#[tauri::command]
pub(crate) async fn get_commands(
    state: State<'_, SharedAppState>,
) -> Result<Vec<RpcCommand>, LureError> {
    session_client(&state)
        .await?
        .get_commands()
        .await
        .map_err(|error| map_rpc_error(&error))
}

#[tauri::command]
pub(crate) async fn respond_extension_ui(
    app: AppHandle,
    state: State<'_, SharedAppState>,
    request_id: String,
    value: Option<Value>,
    cancelled: bool,
) -> Result<(), LureError> {
    session_client(&state)
        .await?
        .respond_to_extension(&request_id, value, cancelled)
        .await
        .map_err(|error| map_rpc_error(&error))?;
    emit_lure_event(
        &app,
        state.inner(),
        LureEvent::ExtensionUiResolved {
            request_id,
            cancelled,
        },
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_workspace_context(
    working_directory: String,
) -> Result<WorkspaceContext, LureError> {
    let directory = canonical_directory(&working_directory)?;
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&directory)
        .output()
        .await;
    let branch = output.ok().and_then(|output| {
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!value.is_empty()).then_some(value)
    });
    Ok(WorkspaceContext {
        working_directory: directory.display().to_string(),
        branch,
    })
}

#[tauri::command]
pub(crate) async fn abort_pi(state: State<'_, SharedAppState>) -> Result<(), LureError> {
    let state = state.inner().clone();
    let _command_guard = state.command_lock.lock().await;
    ensure_operation_allowed(state.snapshot.read().await.phase, Operation::Abort)?;
    let client = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.client.clone())
        .ok_or_else(|| LureError::new(ErrorCode::NotConnected, "Pi 尚未连接"))?;
    client.abort().await.map_err(|error| map_rpc_error(&error))
}

async fn session_client(state: &State<'_, SharedAppState>) -> Result<PiRpcClient, LureError> {
    state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.client.clone())
        .ok_or_else(|| LureError::new(ErrorCode::NotConnected, "Pi 尚未连接"))
}

#[tauri::command]
pub(crate) async fn read_image_attachments(
    paths: Vec<String>,
) -> Result<Vec<ImageAttachment>, LureError> {
    let mut attachments = Vec::with_capacity(paths.len());
    for path in paths {
        let path = PathBuf::from(path);
        let metadata = tokio::fs::metadata(&path).await.map_err(|error| {
            LureError::new(
                ErrorCode::AttachmentUnreadable,
                format!("无法读取 {}：{error}", path.display()),
            )
        })?;
        if metadata.len() > MAX_IMAGE_BYTES {
            return Err(LureError::new(
                ErrorCode::AttachmentUnreadable,
                format!("{} 超过 8 MB 上限", path.display()),
            ));
        }
        let bytes = tokio::fs::read(&path).await.map_err(|error| {
            LureError::new(
                ErrorCode::AttachmentUnreadable,
                format!("无法读取 {}：{error}", path.display()),
            )
        })?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("image")
            .to_owned();
        attachments.push(ImageAttachment {
            data: crate::base64::encode(&bytes),
            mime_type: image_mime_type(&path).to_owned(),
            name,
        });
    }
    Ok(attachments)
}

fn image_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn canonical_directory(path: &str) -> Result<PathBuf, LureError> {
    let original = PathBuf::from(path);
    let canonical = original.canonicalize().map_err(|_| {
        LureError::new(
            ErrorCode::InvalidWorkingDirectory,
            format!("工作目录不存在：{}", original.display()),
        )
    })?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(LureError::new(
            ErrorCode::InvalidWorkingDirectory,
            format!("路径不是目录：{}", original.display()),
        ))
    }
}

fn map_connect_error(error: &RpcError) -> LureError {
    match error {
        RpcError::Timeout(_) => LureError::new(ErrorCode::HandshakeTimeout, error.to_string()),
        other => map_rpc_error(other),
    }
}

fn map_rpc_error(error: &RpcError) -> LureError {
    let code = match error {
        RpcError::InvalidWorkingDirectory(_) => ErrorCode::InvalidWorkingDirectory,
        RpcError::PiNotFound(_) => ErrorCode::PiNotFound,
        RpcError::SpawnFailed(_) => ErrorCode::SpawnFailed,
        RpcError::FrameTooLarge { .. } => ErrorCode::RpcFrameTooLarge,
        RpcError::CommandRejected { .. } => ErrorCode::RpcCommandRejected,
        RpcError::ProcessExited | RpcError::ActorStopped => ErrorCode::ProcessExited,
        RpcError::Io(_)
        | RpcError::InvalidUtf8(_)
        | RpcError::InvalidJson(_)
        | RpcError::Timeout(_)
        | RpcError::Protocol(_) => ErrorCode::RpcProtocolError,
    };
    LureError::new(code, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::ensure_default_workspace;

    #[tokio::test]
    async fn creates_the_lure_directory_under_the_home_directory() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("lure-home-{}-{suffix}", std::process::id()));
        tokio::fs::create_dir_all(&home).await.unwrap();

        let workspace = ensure_default_workspace(&home).await.unwrap();

        assert_eq!(workspace, home.join("lure").canonicalize().unwrap());
        assert!(workspace.is_dir());
        tokio::fs::remove_dir_all(home).await.unwrap();
    }
}
