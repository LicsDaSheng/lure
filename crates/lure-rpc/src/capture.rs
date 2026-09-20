//! 临时采集：把 Pi RPC stdout 的原始行写入当前工作目录，供真机测试录制 mock 数据。
//!
//! 该模块与 `src-tauri/src/capture.rs` 都是一次性采集工具：真机数据录完、
//! mock 数据固定下来后，应连同 `PiProcessConfig::stdout_capture` 一起删除。

use std::path::{Path, PathBuf};

use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::RpcError;

/// 按写入顺序原样追加 Pi RPC stdout 的每一行。
#[derive(Debug)]
pub struct StdoutCapture {
    file: File,
    path: PathBuf,
    failure: Option<String>,
}

impl StdoutCapture {
    /// 创建采集文件，必要时创建其父目录。
    ///
    /// # Errors
    ///
    /// 父目录无法创建或文件无法创建时返回错误。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, RpcError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        Ok(Self {
            file,
            path,
            failure: None,
        })
    }

    /// 采集文件的路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 原样记录一行 Pi RPC 输出。
    ///
    /// 记录的是读取到的原始字节；只有末行缺少换行符时补一个 `\n`，
    /// 使采集文件本身始终是合法的 JSONL。写入失败不会中断 RPC 读取，
    /// 而是记录首个失败原因并通过 [`Self::take_failure`] 暴露。
    pub async fn record(&mut self, raw: &[u8]) {
        if self.failure.is_some() || raw.is_empty() {
            return;
        }
        let mut line = raw.to_vec();
        if line.last() != Some(&b'\n') {
            line.push(b'\n');
        }
        if let Err(error) = self.file.write_all(&line).await {
            self.failure = Some(error.to_string());
        }
    }

    /// 取出首个采集失败原因。
    pub fn take_failure(&mut self) -> Option<String> {
        self.failure.take()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::StdoutCapture;

    fn directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("lure-rpc-capture-{}-{name}", std::process::id()))
    }

    #[tokio::test]
    async fn creates_parent_directories_and_appends_original_bytes() {
        let directory = directory("append");
        let _ = tokio::fs::remove_dir_all(&directory).await;
        let path = directory.join("nested").join("stdout.jsonl");

        let mut capture = StdoutCapture::open(&path).await.unwrap();
        capture.record(b"{\"a\":1}\r\n").await;
        capture.record(b"{\"b\":2}\n").await;

        assert_eq!(capture.path(), path);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"{\"a\":1}\r\n{\"b\":2}\n".to_vec()
        );
        assert!(capture.take_failure().is_none());
        tokio::fs::remove_dir_all(&directory).await.unwrap();
    }

    #[tokio::test]
    async fn keeps_existing_content_and_ignores_empty_reads() {
        let directory = directory("existing");
        let _ = tokio::fs::remove_dir_all(&directory).await;
        let path = directory.join("stdout.jsonl");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&path, b"{\"existing\":true}\n").unwrap();

        let mut capture = StdoutCapture::open(&path).await.unwrap();
        capture.record(b"").await;
        capture.record(b"{\"next\":true}").await;

        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"{\"existing\":true}\n{\"next\":true}\n".to_vec()
        );
        tokio::fs::remove_dir_all(&directory).await.unwrap();
    }

    #[tokio::test]
    async fn reports_the_first_write_failure_without_failing_the_read_loop() {
        let directory = directory("failure");
        let _ = tokio::fs::remove_dir_all(&directory).await;
        let path = directory.join("stdout.jsonl");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&path, b"seed\n").unwrap();

        // 只读句柄：写入必定失败，用于验证采集失败被记录而不是向外冒泡。
        let readonly = tokio::fs::File::from_std(std::fs::File::open(&path).unwrap());
        let mut capture = StdoutCapture {
            file: readonly,
            path: path.clone(),
            failure: None,
        };

        capture.record(b"{\"a\":1}\n").await;
        capture.record(b"{\"b\":2}\n").await;

        assert!(capture.take_failure().is_some());
        assert!(capture.take_failure().is_none());
        assert_eq!(std::fs::read(&path).unwrap(), b"seed\n".to_vec());
        tokio::fs::remove_dir_all(&directory).await.unwrap();
    }

    #[tokio::test]
    async fn reports_unwritable_parent_directories() {
        let directory = directory("unwritable");
        let _ = tokio::fs::remove_dir_all(&directory).await;
        // 父路径是一个已有文件，无法作为目录创建。
        let blocker = directory.join("blocker");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&blocker, b"not a directory").unwrap();

        let error = StdoutCapture::open(blocker.join("stdout.jsonl"))
            .await
            .unwrap_err();

        assert!(matches!(error, crate::RpcError::Io(_)));
        tokio::fs::remove_dir_all(&directory).await.unwrap();
    }
}
