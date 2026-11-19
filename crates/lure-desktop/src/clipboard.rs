//! 将系统剪贴板转换为终端粘贴文本，不解释 Pi 输入语义。
//!
//! 图片保留原始编码，保存到独立临时文件；成功后保留文件供子进程延迟读取，
//! 生命周期由系统临时目录策略管理，不随会话关闭而删除。

use std::io::{self, Write};
use std::path::Path;

use gpui::{ClipboardEntry, ClipboardItem};

/// 图片优先落盘并返回绝对路径，否则沿用 GPUI 的文本/文件路径表示。
///
/// `directory` 必须已存在。文件使用随机名称与独占创建，Unix 权限为 0600。
///
/// # Errors
/// 目录解析、文件创建、写入或保留失败时返回 I/O 错误，不回退到描述文本。
pub fn paste_text(item: &ClipboardItem, directory: &Path) -> io::Result<Option<String>> {
    let Some(image) = item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image),
        _ => None,
    }) else {
        return Ok(item.text());
    };
    let directory = directory.canonicalize()?;
    let mut file = tempfile::Builder::new()
        .prefix("lure-clipboard-")
        .suffix(&format!(".{}", image.format().extension()))
        .tempfile_in(directory)?;
    let path = file
        .path()
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "图片路径不是有效的 UTF-8"))?
        .to_owned();
    file.write_all(image.bytes())?;
    file.flush()?;
    file.keep().map_err(|error| error.error)?;
    Ok(Some(path))
}
