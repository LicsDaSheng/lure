use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use lure_pty::{PtyProcess, SpawnEnvResolver};
use tokio::sync::Mutex;

pub(crate) struct TerminalSession {
    pub id: u64,
    pub process: PtyProcess,
}

pub(crate) struct AppState {
    pub command_lock: Mutex<()>,
    pub terminal: Mutex<Option<TerminalSession>>,
    pub next_session_id: AtomicU64,
    pub spawn_env_resolver: SpawnEnvResolver,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            command_lock: Mutex::new(()),
            terminal: Mutex::new(None),
            next_session_id: AtomicU64::new(1),
            spawn_env_resolver: SpawnEnvResolver::new(),
        }
    }
}

pub(crate) type SharedAppState = Arc<AppState>;
