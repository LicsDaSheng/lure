#![doc = "跨平台 PTY 子进程边界。"]
#![doc = ""]
#![doc = "该 crate 负责 Pi 可执行文件定位、环境合成与终端字节转发，不解析 ANSI 或 Pi 语义。"]

mod env;

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

pub use env::{EnvVars, SpawnEnvResolver};

/// 定位 Pi 可执行文件；`LURE_PI_PATH` 具有最高优先级。
#[must_use]
pub fn resolve_pi_executable() -> PathBuf {
    std::env::var_os("LURE_PI_PATH").map_or_else(
        || find_pi_executable().unwrap_or_else(|| PathBuf::from("pi")),
        PathBuf::from,
    )
}

fn find_pi_executable() -> Option<PathBuf> {
    let search_path = env::pi_runtime_path(
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("PATH").as_deref(),
    );
    find_executable_in_path("pi", &search_path)
}

fn find_executable_in_path(executable: &str, search_path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(search_path)
        .map(|directory| directory.join(executable))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

impl TerminalSize {
    #[must_use]
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }

    fn to_pty(self) -> PtySize {
        PtySize {
            rows: self.rows,
            cols: self.columns,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PtyCommand {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub working_directory: PathBuf,
    pub environment: Option<Vec<(OsString, OsString)>>,
}

impl PtyCommand {
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>, working_directory: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            working_directory: working_directory.into(),
            environment: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyExitStatus {
    pub exit_code: u32,
    pub success: bool,
    pub signal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyEvent {
    Output(Vec<u8>),
    Exited(PtyExitStatus),
    ReadFailed(String),
}

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("终端列数和行数必须大于零")]
    InvalidSize,
    #[error("工作目录无效：{0}")]
    InvalidWorkingDirectory(String),
    #[error("无法创建 PTY：{0}")]
    Open(String),
    #[error("无法启动终端子进程：{0}")]
    Spawn(String),
    #[error("无法打开 PTY 输入输出：{0}")]
    Stream(String),
    #[error("PTY 输入输出锁已损坏")]
    LockPoisoned,
    #[error("无法写入 PTY：{0}")]
    Write(String),
    #[error("无法调整 PTY 尺寸：{0}")]
    Resize(String),
    #[error("无法停止终端子进程：{0}")]
    Stop(String),
}

pub struct PtyProcess {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
}

impl fmt::Debug for PtyProcess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("PtyProcess").finish_non_exhaustive()
    }
}

impl PtyProcess {
    /// 创建 PTY 并启动子进程。
    ///
    /// # Errors
    ///
    /// 尺寸、工作目录、PTY 创建或子进程启动失败时返回错误。
    pub fn spawn(
        mut command: PtyCommand,
        size: TerminalSize,
    ) -> Result<(Self, mpsc::Receiver<PtyEvent>), PtyError> {
        validate_size(size)?;
        command.working_directory = canonical_directory(&command.working_directory)?;

        let pair = native_pty_system()
            .openpty(size.to_pty())
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyError::Stream(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyError::Stream(error.to_string()))?;

        let mut builder = CommandBuilder::new(&command.executable);
        builder.args(&command.args);
        builder.cwd(&command.working_directory);
        if let Some(environment) = command.environment {
            builder.env_clear();
            for (key, value) in environment {
                builder.env(key, value);
            }
        }

        let mut child = pair
            .slave
            .spawn_command(builder)
            .map_err(|error| PtyError::Spawn(error.to_string()))?;
        drop(pair.slave);

        let killer = child.clone_killer();
        let master = Arc::new(Mutex::new(pair.master));
        let writer = Arc::new(Mutex::new(writer));
        let killer = Arc::new(Mutex::new(killer));
        let (events_tx, events_rx) = mpsc::channel();

        let output_tx = events_tx.clone();
        let reader_thread = thread::Builder::new()
            .name("lure-pty-reader".into())
            .spawn(move || read_output(reader, &output_tx))
            .map_err(|error| PtyError::Stream(error.to_string()))?;

        thread::Builder::new()
            .name("lure-pty-waiter".into())
            .spawn(move || match child.wait() {
                Ok(status) => {
                    // 等待读取线程排空 PTY，确保最终输出先于退出事件抵达前端。
                    let _ = reader_thread.join();
                    let _ = events_tx.send(PtyEvent::Exited(PtyExitStatus {
                        exit_code: status.exit_code(),
                        success: status.success(),
                        signal: status.signal().map(ToOwned::to_owned),
                    }));
                }
                Err(error) => {
                    let _ = events_tx.send(PtyEvent::ReadFailed(format!(
                        "等待终端子进程退出失败：{error}"
                    )));
                }
            })
            .map_err(|error| PtyError::Stream(error.to_string()))?;

        Ok((
            Self {
                master,
                writer,
                killer,
            },
            events_rx,
        ))
    }

    /// 将原始字节写入 PTY。
    ///
    /// # Errors
    ///
    /// 写入或刷新失败时返回错误。
    pub fn write_all(&mut self, data: &[u8]) -> Result<(), PtyError> {
        let mut writer = self.writer.lock().map_err(|_| PtyError::LockPoisoned)?;
        writer
            .write_all(data)
            .and_then(|()| writer.flush())
            .map_err(|error| PtyError::Write(error.to_string()))
    }

    /// 调整 PTY 的字符网格尺寸。
    ///
    /// # Errors
    ///
    /// 尺寸非法或系统调用失败时返回错误。
    pub fn resize(&mut self, size: TerminalSize) -> Result<(), PtyError> {
        validate_size(size)?;
        self.master
            .lock()
            .map_err(|_| PtyError::LockPoisoned)?
            .resize(size.to_pty())
            .map_err(|error| PtyError::Resize(error.to_string()))
    }

    /// 读取系统记录的当前 PTY 尺寸。
    ///
    /// # Errors
    ///
    /// PTY 查询失败时返回错误。
    pub fn size(&self) -> Result<TerminalSize, PtyError> {
        let size = self
            .master
            .lock()
            .map_err(|_| PtyError::LockPoisoned)?
            .get_size()
            .map_err(|error| PtyError::Resize(error.to_string()))?;
        Ok(TerminalSize::new(size.cols, size.rows))
    }

    /// 终止 PTY 子进程。
    ///
    /// # Errors
    ///
    /// 系统无法终止子进程时返回错误。
    pub fn stop(&mut self) -> Result<(), PtyError> {
        self.killer
            .lock()
            .map_err(|_| PtyError::LockPoisoned)?
            .kill()
            .map_err(|error| PtyError::Stop(error.to_string()))
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }
}

fn validate_size(size: TerminalSize) -> Result<(), PtyError> {
    if size.columns == 0 || size.rows == 0 {
        return Err(PtyError::InvalidSize);
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, PtyError> {
    let canonical = path
        .canonicalize()
        .map_err(|error| PtyError::InvalidWorkingDirectory(error.to_string()))?;
    if !canonical.is_dir() {
        return Err(PtyError::InvalidWorkingDirectory(format!(
            "{} 不是目录",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn read_output(mut reader: Box<dyn Read + Send>, events: &mpsc::Sender<PtyEvent>) {
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => {
                if events
                    .send(PtyEvent::Output(buffer[..count].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) if expected_pty_close(&error) => return,
            Err(error) => {
                let _ = events.send(PtyEvent::ReadFailed(error.to_string()));
                return;
            }
        }
    }
}

fn expected_pty_close(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::UnexpectedEof || (cfg!(unix) && error.raw_os_error() == Some(5))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::find_executable_in_path;

    #[cfg(unix)]
    #[test]
    fn finds_an_executable_in_the_supplied_path() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            std::env::temp_dir().join(format!("lure-pty-path-test-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("创建测试目录");
        let executable = directory.join("pi");
        fs::write(&executable, "#!/bin/sh\n").expect("创建测试可执行文件");
        let mut permissions = fs::metadata(&executable)
            .expect("读取测试文件权限")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("设置测试文件权限");

        let found = find_executable_in_path("pi", directory.as_os_str());
        fs::remove_dir_all(&directory).expect("删除测试目录");

        assert_eq!(found, Some(executable));
    }
}
