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

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use anyhow::Result;

use crate::transport::Connection;

/// Host 通过中继登记（携带打洞 UDP 端口）并等待 viewer。
/// 返回"桥接后"的连接（中继兜底通道）——QuIC 打洞若失败则用它。
pub fn register_host(relay: SocketAddr, sid: &str, hole_port: u16) -> Result<Connection> {
    let stream = std::net::TcpStream::connect(relay)?;
    stream.set_nodelay(true)?;
    send_intro(&stream, b'H', sid, Some(hole_port))?;
    Ok(Connection::new(stream))
}

/// Viewer 通过中继匹配 host。
/// 返回：(中继桥接后的连接, 由信令得到的 host 打洞端点)。
/// 调用方可用打洞端点尝试 QUIC 直连，失败则用返回的中继连接兜底。
pub fn connect_viewer(relay: SocketAddr, sid: &str) -> Result<(Connection, SocketAddr)> {
    let stream = std::net::TcpStream::connect(relay)?;
    stream.set_nodelay(true)?;
    send_intro(&stream, b'V', sid, None)?;

    // 读 relay 信令：S | family(1) | ip_octets(4/16) | hole_port(2)
    let mut stream2 = stream.try_clone()?;
    let mut head = [0u8; 1];
    stream2.read_exact(&mut head)?;
    match head[0] {
        b'E' => {
            anyhow::bail!("中继服务器上不存在会话 {}（对方尚未就绪）", sid);
        }
        b'S' => {
            let mut fam = [0u8; 1];
            stream2.read_exact(&mut fam)?;
            let ip: IpAddr = match fam[0] {
                4 => {
                    let mut o = [0u8; 4];
                    stream2.read_exact(&mut o)?;
                    IpAddr::V4(Ipv4Addr::from(o))
                }
                6 => {
                    let mut o = [0u8; 16];
                    stream2.read_exact(&mut o)?;
                    IpAddr::V6(Ipv6Addr::from(o))
                }
                other => anyhow::bail!("未知 IP 簇: {}", other),
            };
            let mut p = [0u8; 2];
            stream2.read_exact(&mut p)?;
            let hole = SocketAddr::new(ip, u16::from_be_bytes(p));
            // 信令已读完；stream 剩余即桥接数据，交由 Connection
            Ok((Connection::new(stream), hole))
        }
        other => anyhow::bail!("未知中继信令: {}", other),
    }
}

/// 发送角色头 + sid（host 附加打洞端口）。
fn send_intro(
    stream: &std::net::TcpStream,
    role: u8,
    sid: &str,
    hole_port: Option<u16>,
) -> Result<()> {
    if sid.is_empty() || sid.len() > 64 {
        anyhow::bail!("sid 长度非法");
    }
    let mut buf = Vec::with_capacity(2 + sid.len() + 2);
    buf.push(role);
    buf.push(sid.len() as u8);
    buf.extend_from_slice(sid.as_bytes());
    if let Some(p) = hole_port {
        buf.extend_from_slice(&p.to_be_bytes());
    }
    let mut stream = stream.try_clone()?;
    stream.write_all(&buf)?;
    Ok(())
}
