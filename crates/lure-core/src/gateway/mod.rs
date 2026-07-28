//! Gateway 子系统：最小消息编排。
//!
//! 对齐上游 `nanobot/gateway` 的职责边界，但收敛为同步最小编排：注册 channel、
//! 生命周期启停、把 inbound 经 AgentLoop 处理为 outbound 并路由到目标 channel、
//! 把 agent 的 progress 事件流（Started/ToolInvoked/Final）转发给 channel、暴露健康状态。
//!
//! Phase 7 不做：真实 HTTP health endpoint、进程管理 runtime、async 调度、
//! channel 热加载（见 upstream-test-ledger）。

use std::collections::HashMap;
use std::fmt;

use crate::agent::{AgentError, AgentLoop, ProgressEvent};
use crate::bus::{InboundMessage, MessageBus, OutboundMessage, ProgressKind, ProgressUpdate};
use crate::channel::{Channel, ChannelError};

/// gateway 编排错误。
#[derive(Debug)]
pub enum GatewayError {
    /// agent 处理失败。
    Agent(AgentError),
    /// channel 投递失败。
    Channel(ChannelError),
    /// outbound 指向未注册的 channel。
    UnknownChannel(String),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GatewayError::Agent(e) => write!(f, "gateway agent 错误: {e}"),
            GatewayError::Channel(e) => write!(f, "gateway channel 错误: {e}"),
            GatewayError::UnknownChannel(name) => write!(f, "gateway 未注册 channel: {name}"),
        }
    }
}

impl std::error::Error for GatewayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GatewayError::Agent(e) => Some(e),
            GatewayError::Channel(e) => Some(e),
            GatewayError::UnknownChannel(_) => None,
        }
    }
}

/// gateway 健康状态快照。
#[derive(Debug, Clone, PartialEq)]
pub struct GatewayHealth {
    /// 是否处于运行态。
    pub running: bool,
    /// 已注册 channel 数。
    pub channels: usize,
    /// 待处理 inbound 数。
    pub pending_inbound: usize,
}

/// 最小 gateway：编排 bus、agent 与 channel。
pub struct Gateway {
    bus: MessageBus,
    agent: AgentLoop,
    channels: HashMap<String, Box<dyn Channel>>,
    running: bool,
}

impl Gateway {
    /// 绑定 AgentLoop，初始为停止态。
    pub fn new(agent: AgentLoop) -> Self {
        Self {
            bus: MessageBus::new(),
            agent,
            channels: HashMap::new(),
            running: false,
        }
    }

    /// 注册 channel（先校验配置）。
    pub fn register_channel(&mut self, channel: Box<dyn Channel>) -> Result<(), ChannelError> {
        channel.validate()?;
        self.channels.insert(channel.name().to_string(), channel);
        Ok(())
    }

    /// 进入运行态。
    pub fn start(&mut self) {
        self.running = true;
    }

    /// 退出运行态（不丢弃待处理任务）。
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// 提交一条 inbound 消息到总线。
    pub fn submit(&mut self, message: InboundMessage) {
        self.bus.publish_inbound(message);
    }

    /// 处理所有待处理 inbound：agent → outbound → 路由到目标 channel。
    ///
    /// 非运行态时不处理，待处理任务保留在总线中。返回本次处理条数。
    pub fn dispatch_pending(&mut self) -> Result<usize, GatewayError> {
        if !self.running {
            return Ok(0);
        }
        let mut processed = 0;
        while let Some(inbound) = self.bus.consume_inbound() {
            let outcome = self.agent.process(&inbound).map_err(GatewayError::Agent)?;
            // 先把 progress 事件流转发给目标 channel（outbound 运行时事件）。
            for event in &outcome.progress {
                let update = progress_update(&inbound, event);
                self.route_progress(&inbound.channel, &update)?;
            }
            let outbound = OutboundMessage::reply(&inbound, outcome.final_content);
            self.route(&outbound)?;
            processed += 1;
        }
        Ok(processed)
    }

    /// 健康状态快照。
    pub fn health(&self) -> GatewayHealth {
        GatewayHealth {
            running: self.running,
            channels: self.channels.len(),
            pending_inbound: self.bus.inbound_size(),
        }
    }

    /// 待处理 inbound 数。
    pub fn pending_inbound(&self) -> usize {
        self.bus.inbound_size()
    }

    fn route(&self, outbound: &OutboundMessage) -> Result<(), GatewayError> {
        match self.channels.get(&outbound.channel) {
            Some(channel) => channel.deliver(outbound).map_err(GatewayError::Channel),
            None => Err(GatewayError::UnknownChannel(outbound.channel.clone())),
        }
    }

    fn route_progress(
        &self,
        channel_name: &str,
        update: &ProgressUpdate,
    ) -> Result<(), GatewayError> {
        match self.channels.get(channel_name) {
            Some(channel) => channel
                .deliver_progress(update)
                .map_err(GatewayError::Channel),
            None => Err(GatewayError::UnknownChannel(channel_name.to_string())),
        }
    }
}

/// 把 agent 的 [`ProgressEvent`] 加上路由信息映射为传输无关的 [`ProgressUpdate`]。
fn progress_update(inbound: &InboundMessage, event: &ProgressEvent) -> ProgressUpdate {
    let (kind, content) = match event {
        ProgressEvent::TurnStarted { session_key } => (ProgressKind::Started, session_key.clone()),
        ProgressEvent::ContentDelta { text } => (ProgressKind::ContentDelta, text.clone()),
        ProgressEvent::ReasoningDelta { text } => (ProgressKind::ReasoningDelta, text.clone()),
        ProgressEvent::ToolInvoked { name } => (ProgressKind::ToolInvoked, name.clone()),
        ProgressEvent::Finalizing => (ProgressKind::Finalizing, String::new()),
        ProgressEvent::FinalResponse { content } => (ProgressKind::Final, content.clone()),
    };
    ProgressUpdate::new(
        inbound.channel.clone(),
        inbound.chat_id.clone(),
        kind,
        content,
    )
}

/// 用 [`Gateway`] 执行 cron job 的 [`CronJobRunner`] 适配器。
///
/// 到期 job → 按其 origin 构造 [`InboundMessage`] 提交给 gateway → dispatch（agent 处理
/// → outbound 路由回 origin channel）。缺 origin 上下文的 job 记为 `Skipped`，dispatch
/// 失败记为 `Error`。持有 gateway 所有权，供专用 cron 宿主线程独占驱动。
pub struct GatewayCronRunner {
    gateway: Gateway,
}

impl GatewayCronRunner {
    /// 绑定 gateway（自动置为运行态，确保 dispatch 生效）。
    pub fn new(mut gateway: Gateway) -> Self {
        gateway.start();
        Self { gateway }
    }

    /// 只读访问内部 gateway（诊断/健康）。
    pub fn gateway(&self) -> &Gateway {
        &self.gateway
    }
}

impl crate::cron::CronJobRunner for GatewayCronRunner {
    fn run(&mut self, job: &crate::cron::CronJob) -> crate::cron::RunStatus {
        let (channel, chat_id, _meta) = match crate::cron::origin_delivery_context(job) {
            Ok(ctx) => ctx,
            Err(_) => return crate::cron::RunStatus::Skipped,
        };
        self.gateway.submit(InboundMessage::new(
            channel,
            chat_id,
            job.payload.message.clone(),
        ));
        match self.gateway.dispatch_pending() {
            Ok(_) => crate::cron::RunStatus::Ok,
            Err(_) => crate::cron::RunStatus::Error,
        }
    }
}
