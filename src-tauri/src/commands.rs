use std::path::PathBuf;
use std::time::Duration;

use lure_core::{
    ConnectionPhase, ConnectionSnapshot, ErrorCode, LureError, LureEvent, ModelSnapshot,
};
use lure_rpc::{PiProcessConfig, PiRpcClient, RpcError};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::events::{emit_lure_event, forward_pi_events};
use crate::state::{DesktopSession, Operation, SharedAppState, ensure_operation_allowed};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestAccepted {
    accepted: bool,
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

    let ready = ConnectionSnapshot {
        phase: ConnectionPhase::Ready,
        working_directory: Some(canonical.display().to_string()),
        session_id: Some(rpc_state.session_id),
        session_file: rpc_state.session_file,
        model: rpc_state.model.map(|model| ModelSnapshot {
            provider: model.provider,
            id: model.id,
        }),
        thinking_level: Some(rpc_state.thinking_level),
        error: None,
    };

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

    if let Err(error) = client.prompt(message).await {
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
