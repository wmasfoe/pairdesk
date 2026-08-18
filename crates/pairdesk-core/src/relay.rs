//! 客户端中继连接：与 `pairdesk-relay` 服务器交互，完成注册/匹配。
//!
//! 概念：
//!  - 被控端(Host)与中继服务器建立 TCP 连接并登记"会话码 sid"，然后等待；
//!  - 控制端(Viewer)同样连向中继，用同一 sid 请求连接；
//!  - 中继把两条连接桥接(透明字节流转发)，此后两端把它当作一条直连 TCP 使用，
//!    握手/加密/数据流照常（中继不解析 PairDesk 协议）。
//!
//! 协议(与 relay 对齐)：
//! ```text
//! 发: role(1) | sid_len(1) | sid
//! Host 注册后不发确认；Viewer 请求时 relay 可能回 1 字节 b'E' 表示"无对端"。
//! ```

use std::io::Write;
use std::net::SocketAddr;

use anyhow::Result;

use crate::transport::Connection;

/// Host 通过中继登记并等待 viewer，返回"桥接后"的连接。
/// 该连接用于之后正常收发（等 viewer 的 HELLO）。
pub fn register_host(relay: SocketAddr, sid: &str) -> Result<Connection> {
    let stream = std::net::TcpStream::connect(relay)?;
    stream.set_nodelay(true)?;
    send_intro(&stream, b'H', sid)?;
    Ok(Connection::new(stream))
}

/// Viewer 通过中继匹配 host，返回"桥接后"的连接。
///
/// 说明：匹配成功后中继即透明桥接，viewer 应立即开始握手。
/// 若 relay 上无等待中的 host，relay 会回 1 字节 b'E' 并关闭连接——
/// 此刻打开连接后第一个 recv_frame 就会因收到非法字节报错，无需在此预读。
pub fn connect_viewer(relay: SocketAddr, sid: &str) -> Result<Connection> {
    let stream = std::net::TcpStream::connect(relay)?;
    stream.set_nodelay(true)?;
    send_intro(&stream, b'V', sid)?;
    Ok(Connection::new(stream))
}

/// 发送角色头 + sid。
fn send_intro(stream: &std::net::TcpStream, role: u8, sid: &str) -> Result<()> {
    if sid.is_empty() || sid.len() > 64 {
        anyhow::bail!("sid 长度非法");
    }
    let mut buf = Vec::with_capacity(2 + sid.len());
    buf.push(role);
    buf.push(sid.len() as u8);
    buf.extend_from_slice(sid.as_bytes());
    let mut stream = stream.try_clone()?;
    stream.write_all(&buf)?;
    Ok(())
}
