use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("无法读取会话目录 {path}：{message}")]
    DirectoryUnreadable { path: PathBuf, message: String },
    #[error("无法读取会话文件 {path}：{message}")]
    FileUnreadable { path: PathBuf, message: String },
}
