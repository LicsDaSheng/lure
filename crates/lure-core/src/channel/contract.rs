//! Channel 契约与测试用记录 channel。

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::bus::{OutboundMessage, ProgressUpdate};

/// 共享的投递记录日志。
pub type DeliveryLog = Rc<RefCell<Vec<OutboundMessage>>>;

/// 共享的 progress 记录日志。
pub type ProgressLog = Rc<RefCell<Vec<ProgressUpdate>>>;

/// channel 相关错误。
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelError {
    /// 配置缺少必填字段。
    MissingConfig {
        /// channel 名。
        channel: String,
        /// 缺失字段列表。
        fields: Vec<String>,
    },
    /// 投递失败。
    Delivery {
        /// channel 名。
        channel: String,
        /// 失败原因。
        reason: String,
    },
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelError::MissingConfig { channel, fields } => {
                write!(f, "channel '{channel}' 缺少必填配置: {}", fields.join(", "))
            }
            ChannelError::Delivery { channel, reason } => {
                write!(f, "channel '{channel}' 投递失败: {reason}")
            }
        }
    }
}

impl std::error::Error for ChannelError {}

/// chat channel 契约。
pub trait Channel {
    /// channel 名（用于 outbound 路由）。
    fn name(&self) -> &str;

    /// 校验配置；缺字段等返回结构化错误。
    fn validate(&self) -> Result<(), ChannelError>;

    /// 投递一条 outbound 消息到平台。
    fn deliver(&self, message: &OutboundMessage) -> Result<(), ChannelError>;

    /// 转发一条运行时 progress 更新（默认 no-op：不支持进度的 channel 可忽略）。
    fn deliver_progress(&self, _update: &ProgressUpdate) -> Result<(), ChannelError> {
        Ok(())
    }
}

/// 记录投递内容的测试用 channel。
pub struct RecordingChannel {
    name: String,
    missing_fields: Vec<String>,
    delivered: DeliveryLog,
    progress: ProgressLog,
}

impl RecordingChannel {
    /// 新建一个配置完整的记录 channel。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            missing_fields: Vec::new(),
            delivered: Rc::new(RefCell::new(Vec::new())),
            progress: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// 声明缺失的必填字段（用于校验测试）。
    pub fn with_missing_config(mut self, missing: &[&str]) -> Self {
        self.missing_fields = missing.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 返回共享的投递日志（移入 gateway 后仍可从测试侧查询）。
    pub fn delivery_log(&self) -> DeliveryLog {
        Rc::clone(&self.delivered)
    }

    /// 返回共享的 progress 日志（移入 gateway 后仍可从测试侧查询）。
    pub fn progress_log(&self) -> ProgressLog {
        Rc::clone(&self.progress)
    }

    /// 已投递的消息快照。
    pub fn delivered(&self) -> Vec<OutboundMessage> {
        self.delivered.borrow().clone()
    }
}

impl Channel for RecordingChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn validate(&self) -> Result<(), ChannelError> {
        if self.missing_fields.is_empty() {
            Ok(())
        } else {
            Err(ChannelError::MissingConfig {
                channel: self.name.clone(),
                fields: self.missing_fields.clone(),
            })
        }
    }

    fn deliver(&self, message: &OutboundMessage) -> Result<(), ChannelError> {
        self.delivered.borrow_mut().push(message.clone());
        Ok(())
    }

    fn deliver_progress(&self, update: &ProgressUpdate) -> Result<(), ChannelError> {
        self.progress.borrow_mut().push(update.clone());
        Ok(())
    }
}
