//! PairDesk 核心引擎 —— 与 UI 完全无关的远程协助逻辑层。
//!
//! 分层铁律：本 crate 不引用任何 UI/系统窗口代码，只通过
//! [`CoreHandle`]（命令入口）与事件流（[`CoreEvent`]）与上层通信。

pub mod capture;
pub mod certs;
pub mod encode;
pub mod input;
pub mod protocol;
pub mod relay;
pub mod session;
pub mod transport;

use std::net::SocketAddr;
use std::sync::mpsc::{Receiver, Sender};

/// 核心层向上层输出的事件（上层订阅）。
#[derive(Debug, Clone)]
pub enum CoreEvent {
    /// 一帧远程画面（JPEG 编码后的字节），控制端渲染用
    ScreenFrame(Vec<u8>),
    /// 首帧前告知远端屏幕分辨率
    Size { w: u32, h: u32 },
    /// 对端已建立连接（被控端视角：有人连进来了）
    PeerConnected,
    /// 对端断开
    PeerDisconnected,
    /// 认证结果
    AuthResult { ok: bool, reason: Option<String> },
    /// 运行统计（帧率/码率/延迟）
    Stats { fps: u32, kbps: u32, ping_ms: u32 },
    /// 错误（网络/采集等）
    Error(String),
}

/// 上层对核心层的控制命令。
#[derive(Debug, Clone)]
pub enum ControlCommand {
    /// 调节画质与帧率
    SetQuality(Quality),
    /// 发送输入事件（控制端 → 被控端）
    SendInput(crate::protocol::InputMsg),
    /// 主动断开/停止会话
    Stop,
}

/// 会话质量档位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quality {
    /// JPEG 质量 0-100
    pub jpeg: u8,
    /// 目标帧率（实际帧率受采集速度约束）
    pub fps: u32,
}

impl Default for Quality {
    fn default() -> Self {
        Quality { jpeg: 75, fps: 20 }
    }
}

/// 对上层暴露的核心引擎句柄。
///
/// 内部持有命令通道；事件流由启动函数（[`start_host`]/[`connect`]）随句柄一并返回，
/// 上层通过返回的 [`Receiver<CoreEvent>`] 订阅事件。
#[derive(Clone)]
pub struct CoreHandle {
    tx: Sender<ControlCommand>,
}

impl CoreHandle {
    /// 由 session 内部构造（命令通道的所有者）。
    pub(crate) fn from_tx(tx: Sender<ControlCommand>) -> CoreHandle {
        CoreHandle { tx }
    }

    /// 内部命令通道访问（session 用）。
    pub(crate) fn tx(&self) -> &Sender<ControlCommand> {
        &self.tx
    }

    /// 启动"被控端"：监听端口等待连接。
    /// 返回 (句柄, 事件接收器)。
    pub fn start_host(port: u16, password: String) -> anyhow::Result<(CoreHandle, Receiver<CoreEvent>)> {
        session::spawn_host(port, password)
    }

    /// 启动"控制端"：连接指定地址。
    /// 返回 (句柄, 事件接收器)。
    pub fn connect(addr: SocketAddr, password: String) -> anyhow::Result<(CoreHandle, Receiver<CoreEvent>)> {
        session::spawn_viewer(addr, password)
    }

    /// 启动"被控端"（经中继）：向 relay 登记 sid，等待 viewer 经同一中继加入。
    pub fn start_host_via_relay(
        relay: SocketAddr,
        sid: String,
        password: String,
    ) -> anyhow::Result<(CoreHandle, Receiver<CoreEvent>)> {
        session::spawn_host_via_relay(relay, sid, password)
    }

    /// 启动"控制端"（经中继）：经 relay 匹配同 sid 的 host。
    pub fn connect_via_relay(
        relay: SocketAddr,
        sid: String,
        password: String,
    ) -> anyhow::Result<(CoreHandle, Receiver<CoreEvent>)> {
        session::spawn_viewer_via_relay(relay, sid, password)
    }

    /// 调节画质/帧率。
    pub fn set_quality(&self, q: Quality) {
        let _ = self.tx.send(ControlCommand::SetQuality(q));
    }

    /// 发送输入事件（仅控制端有意义：转发到被控端注入）。
    pub fn send_input(&self, msg: crate::protocol::InputMsg) {
        let _ = self.tx.send(ControlCommand::SendInput(msg));
    }

    /// 主动停止/断开。
    pub fn stop(&self) {
        let _ = self.tx.send(ControlCommand::Stop);
    }
}