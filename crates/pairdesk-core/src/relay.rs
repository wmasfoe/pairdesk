//! 客户端中继连接：与 `pairdesk-relay` 服务器交互，完成端点交换与透明桥接。
//!
//! 流程：
//!  - Host 连入中继，上报 sid 与自身的候选打洞端点 (Candidates)，并等待 Viewer；
//!  - Viewer 连入中继，上报 sid 与自身的候选打洞端点；
//!  - 中继向双方互送对端的 Candidates 触发双向打洞，并开启 TCP 透明桥接作为兜底连接。

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use anyhow::{bail, Result};

use crate::punch::deserialize_candidates;
use crate::transport::Connection;

/// Host 通过中继登记（携带候选打洞端点字符串）并等待 viewer。
/// 当 Viewer 连入时，中继会发回 Viewer 的候选端点列表，并建立 TCP 桥接兜底连接。
pub fn register_host(
    relay: SocketAddr,
    sid: &str,
    candidates_str: &str,
) -> Result<(Connection, Vec<SocketAddr>)> {
    let stream = std::net::TcpStream::connect(relay)?;
    stream.set_nodelay(true)?;
    send_intro(&stream, b'H', sid, candidates_str)?;

    let mut stream2 = stream.try_clone()?;
    let mut head = [0u8; 1];
    stream2.read_exact(&mut head)?;
    match head[0] {
        b'C' => {
            let mut lenb = [0u8; 2];
            stream2.read_exact(&mut lenb)?;
            let len = u16::from_be_bytes(lenb) as usize;
            let mut buf = vec![0u8; len];
            stream2.read_exact(&mut buf)?;
            let cands_str = String::from_utf8_lossy(&buf);
            let cands = deserialize_candidates(&cands_str);
            Ok((Connection::new(stream), cands))
        }
        b'E' => bail!("中继注册失败"),
        other => bail!("未知中继推送信令: {}", other),
    }
}

/// Viewer 通过中继匹配 host（携带自身的候选打洞端点字符串）。
/// 返回：(中继桥接后的兜底连接, 由信令得到的 host 候选端点列表)。
pub fn connect_viewer(
    relay: SocketAddr,
    sid: &str,
    candidates_str: &str,
) -> Result<(Connection, Vec<SocketAddr>)> {
    let stream = std::net::TcpStream::connect(relay)?;
    stream.set_nodelay(true)?;
    send_intro(&stream, b'V', sid, candidates_str)?;

    let mut stream2 = stream.try_clone()?;
    let mut head = [0u8; 1];
    stream2.read_exact(&mut head)?;
    match head[0] {
        b'E' => {
            bail!("中继服务器上不存在会话 {}（对方尚未就绪）", sid);
        }
        b'C' => {
            let mut lenb = [0u8; 2];
            stream2.read_exact(&mut lenb)?;
            let len = u16::from_be_bytes(lenb) as usize;
            let mut buf = vec![0u8; len];
            stream2.read_exact(&mut buf)?;
            let cands_str = String::from_utf8_lossy(&buf);
            let cands = deserialize_candidates(&cands_str);
            Ok((Connection::new(stream), cands))
        }
        // 兼容旧版信令格式：S | family(1) | ip_octets(4/16) | hole_port(2)
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
                other => bail!("未知 IP 簇: {}", other),
            };
            let mut p = [0u8; 2];
            stream2.read_exact(&mut p)?;
            let hole = SocketAddr::new(ip, u16::from_be_bytes(p));
            Ok((Connection::new(stream), vec![hole]))
        }
        other => bail!("未知中继信令: {}", other),
    }
}

/// 发送角色头 + sid + 候选端点字符串
fn send_intro(
    stream: &std::net::TcpStream,
    role: u8,
    sid: &str,
    candidates_str: &str,
) -> Result<()> {
    if sid.is_empty() || sid.len() > 64 {
        bail!("sid 长度非法");
    }
    let cands_bytes = candidates_str.as_bytes();
    let cands_len = cands_bytes.len() as u16;

    let mut buf = Vec::with_capacity(2 + sid.len() + 2 + cands_bytes.len());
    buf.push(role);
    buf.push(sid.len() as u8);
    buf.extend_from_slice(sid.as_bytes());
    buf.extend_from_slice(&cands_len.to_be_bytes());
    buf.extend_from_slice(cands_bytes);

    let mut stream = stream.try_clone()?;
    stream.write_all(&buf)?;
    stream.flush()?;
    Ok(())
}
