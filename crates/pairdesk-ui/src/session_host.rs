//! 内核会话的持有者：把 `pairdesk-shell` 的意图翻译成 `pairdesk-core` 调用。
//!
//! 这是适配器层唯一的"脏活"所在——起线程、持句柄、投递事件。所有业务判断
//! 都在 `pairdesk_shell::session` 里，这里只做翻译。

use std::net::SocketAddr;
use std::sync::mpsc::Receiver;

use pairdesk_core::{CoreEvent, CoreHandle, Quality};
use pairdesk_shell::input::InputMsg;
use pairdesk_shell::session::{ConnectParams, ConnectionMode, HostParams, Intent};

use crate::frame::split_events;

/// 活动会话。
pub struct ActiveSession {
    handle: CoreHandle,
    /// 画面帧通道（交给 `FramePump`）。`Option` 是为了能被取出 move。
    pub frame_rx: Option<Receiver<Vec<u8>>>,
    /// 状态事件通道（交给会话状态机）
    pub event_rx: Receiver<CoreEvent>,
}

impl ActiveSession {
    /// 取出画面帧接收器（只能取一次）。
    pub fn take_frame_rx(&mut self) -> Option<Receiver<Vec<u8>>> {
        self.frame_rx.take()
    }

    /// 停止会话。幂等。
    pub fn stop(self) {
        self.handle.stop();
    }
}

/// 解析地址。支持 `host:port` 与 `ip:port`。
fn parse_addr(s: &str) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    s.to_socket_addrs()
        .map_err(|e| format!("地址解析失败 {s}: {e}"))?
        .next()
        .ok_or_else(|| format!("未解析出有效网络地址: {s}"))
}

/// 按意图启动会话。
///
/// 逐条对应旧 `bridge.rs::pd_start_host` / `pd_connect` 的 mode 分支。
pub fn start(intent: Intent) -> Result<ActiveSession, String> {
    match intent {
        Intent::StartHost(p) => start_host(p),
        Intent::Connect(p) => connect(p),
        Intent::Stop => Err("Stop 意图不应传给 start".into()),
        Intent::SendInput(_) => Err("SendInput 意图不应传给 start".into()),
        Intent::None => Err("空意图".into()),
    }
}

fn start_host(p: HostParams) -> Result<ActiveSession, String> {
    let (handle, rx) = match p.mode {
        ConnectionMode::Quic => CoreHandle::start_host_via_quic(p.hole_port, p.password)
            .map_err(|e| e.to_string())?,
        ConnectionMode::Direct => {
            CoreHandle::start_host(p.hole_port, p.password).map_err(|e| e.to_string())?
        }
        ConnectionMode::Auto => {
            let addr = parse_addr(&p.relay)?;
            CoreHandle::start_host_auto(addr, p.sid, p.hole_port, p.password)
                .map_err(|e| e.to_string())?
        }
        ConnectionMode::Relay => {
            let addr = parse_addr(&p.relay)?;
            CoreHandle::start_host_via_relay(addr, p.sid, p.hole_port, p.password)
                .map_err(|e| e.to_string())?
        }
    };

    // 画质与旧实现一致：bridge.rs 硬编码 jpeg:80 fps:20
    handle.set_quality(Quality {
        jpeg: pairdesk_shell::session::DEFAULT_QUALITY.jpeg,
        fps: pairdesk_shell::session::DEFAULT_QUALITY.fps,
    });

    let (frame_rx, event_rx) = split_events(rx);
    Ok(ActiveSession {
        handle,
        frame_rx: Some(frame_rx),
        event_rx,
    })
}

fn connect(p: ConnectParams) -> Result<ActiveSession, String> {
    let (handle, rx) = match p.mode {
        ConnectionMode::Quic => {
            let addr = parse_addr(&p.target)?;
            CoreHandle::connect_via_quic(addr, p.password).map_err(|e| e.to_string())?
        }
        ConnectionMode::Direct => {
            let addr = parse_addr(&p.target)?;
            CoreHandle::connect(addr, p.password).map_err(|e| e.to_string())?
        }
        ConnectionMode::Auto => {
            let addr = parse_addr(&p.target)?;
            CoreHandle::connect_auto(addr, p.sid, p.password).map_err(|e| e.to_string())?
        }
        ConnectionMode::Relay => {
            let addr = parse_addr(&p.target)?;
            CoreHandle::connect_via_relay(addr, p.sid, p.password).map_err(|e| e.to_string())?
        }
    };

    let (frame_rx, event_rx) = split_events(rx);
    Ok(ActiveSession {
        handle,
        frame_rx: Some(frame_rx),
        event_rx,
    })
}

/// 把输入消息投递给活动会话。
pub fn send_input(session: &ActiveSession, msg: InputMsg) {
    session.handle.send_input(msg.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 地址解析支持_ip_与主机名() {
        assert!(parse_addr("127.0.0.1:8888").is_ok());
        assert!(parse_addr("1.2.3.4:8977").is_ok());
        assert!(parse_addr("localhost:8888").is_ok());
    }

    #[test]
    fn 地址解析拒绝非法输入() {
        // 缺端口、空串、乱码都必须报错而不是 panic
        assert!(parse_addr("").is_err());
        assert!(parse_addr("127.0.0.1").is_err());
        assert!(parse_addr("not an address").is_err());
        assert!(parse_addr(":::::").is_err());
    }

    #[test]
    fn 错误意图被拒绝() {
        assert!(start(Intent::None).is_err());
        assert!(start(Intent::Stop).is_err());
        assert!(start(Intent::SendInput(InputMsg::Button {
            btn: 1,
            down: true
        }))
        .is_err());
    }
}
