use std::sync::atomic::Ordering;

use lure_core::{ConnectionPhase, EventEnvelope, LureEvent};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

use crate::state::SharedAppState;

pub(crate) const PI_EVENT_NAME: &str = "lure://pi-event";

pub(crate) fn emit_lure_event(app: &AppHandle, state: &SharedAppState, event: LureEvent) {
    let sequence = state.sequence.fetch_add(1, Ordering::Relaxed);
    let _ = app.emit(PI_EVENT_NAME, EventEnvelope { sequence, event });
}

pub(crate) async fn forward_pi_events(
    app: AppHandle,
    state: SharedAppState,
    mut events: broadcast::Receiver<LureEvent>,
) {
    loop {
        match events.recv().await {
            Ok(event) => {
                update_snapshot(&state, &event).await;
                emit_lure_event(&app, &state, event);
            }
            Err(broadcast::error::RecvError::Lagged(count)) => {
                let event = LureEvent::ProtocolError {
                    message: format!("桌面事件处理落后，丢失 {count} 条 Pi 事件"),
                };
                update_snapshot(&state, &event).await;
                emit_lure_event(&app, &state, event);
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn update_snapshot(state: &SharedAppState, event: &LureEvent) {
    let mut snapshot = state.snapshot.write().await;
    match event {
        LureEvent::RunStarted => snapshot.phase = ConnectionPhase::Running,
        LureEvent::RunSettled => snapshot.phase = ConnectionPhase::Ready,
        LureEvent::ProcessExited { .. } | LureEvent::ProtocolError { .. } => {
            snapshot.phase = ConnectionPhase::Failed;
        }
        _ => {}
    }
}
