//! NAT 打洞与 STUN 探测核心模块（参考 Tailscale DERP/Disco 机制）。
//!
//! 核心设计：
//! 1. STUN 探测：客户端向 Relay UDP 端口发送轻量 STUN 请求，获取自身的公网反射地址 (Reflexive IP:Port)；
//! 2. 候选端点收集：汇总公网反射地址与本地内网网卡地址；
//! 3. 双向对称打洞 (Disco Punching)：两端使用同一个 UDP Socket，同时向对方的所有候选端点密集发送 Ping；
//!    任何一端收到 Ping 立即回复 Pong，收到 Pong 即确认双向 NAT 通道穿透成功；
//! 4. 穿透成功后，将打洞完成的同一只 `std::net::UdpSocket` 移交给 Quinn 建立 QUIC 加密数据传输。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};

pub const MAGIC: [u8; 4] = *b"PDSK";

pub const MSG_STUN_REQ: u8 = 0x01;
pub const MSG_STUN_RESP: u8 = 0x02;
pub const MSG_DISCO_PING: u8 = 0x03;
pub const MSG_DISCO_PONG: u8 = 0x04;

const HEADER_LEN: usize = 4 + 1 + 16; // magic(4) + type(1) + token(16)

/// 根据 sid 和 password 派生打洞阶段的防串扰 Token（16 字节）
pub fn derive_punch_token(sid: &str, password: &str) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(b"pairdesk-punch-v1:");
    hasher.update(sid.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    let hash = hasher.finalize();
    let mut token = [0u8; 16];
    token.copy_from_slice(&hash[..16]);
    token
}

/// 编码打洞 / STUN 报文
pub fn encode_packet(msg_type: u8, token: &[u8; 16], payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_LEN + payload.len());
    buf.extend_from_slice(&MAGIC);
    buf.push(msg_type);
    buf.extend_from_slice(token);
    buf.extend_from_slice(payload);
    buf
}

/// 解析打洞 / STUN 报文，返回 (msg_type, token, payload)
pub fn parse_packet(buf: &[u8]) -> Option<(u8, [u8; 16], &[u8])> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    if &buf[0..4] != &MAGIC {
        return None;
    }
    let msg_type = buf[4];
    let mut token = [0u8; 16];
    token.copy_from_slice(&buf[5..21]);
    let payload = &buf[21..];
    Some((msg_type, token, payload))
}

/// 编码 STUN 响应中包含的映射地址
pub fn encode_addr_payload(addr: SocketAddr) -> Vec<u8> {
    let mut out = Vec::new();
    match addr {
        SocketAddr::V4(v4) => {
            out.push(4);
            out.extend_from_slice(&v4.ip().octets());
            out.extend_from_slice(&v4.port().to_be_bytes());
        }
        SocketAddr::V6(v6) => {
            out.push(6);
            out.extend_from_slice(&v6.ip().octets());
            out.extend_from_slice(&v6.port().to_be_bytes());
        }
    }
    out
}

/// 解析 STUN 响应中的映射地址
pub fn parse_addr_payload(payload: &[u8]) -> Option<SocketAddr> {
    if payload.is_empty() {
        return None;
    }
    match payload[0] {
        4 => {
            if payload.len() < 1 + 4 + 2 {
                return None;
            }
            let mut octets = [0u8; 4];
            octets.copy_from_slice(&payload[1..5]);
            let port = u16::from_be_bytes([payload[5], payload[6]]);
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(octets)), port))
        }
        6 => {
            if payload.len() < 1 + 16 + 2 {
                return None;
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&payload[1..17]);
            let port = u16::from_be_bytes([payload[17], payload[18]]);
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(octets)), port))
        }
        _ => None,
    }
}

/// 向 Relay UDP 端口发送 STUN 探测，获取本机在 NAT 映射后的公网反射端点
pub fn stun_probe(
    sock: &UdpSocket,
    relay_addr: SocketAddr,
    timeout: Duration,
) -> Result<SocketAddr> {
    let mut token = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut token);
    let req = encode_packet(MSG_STUN_REQ, &token, &[]);

    sock.set_read_timeout(Some(Duration::from_millis(150)))?;

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 128];

    while Instant::now() < deadline {
        let _ = sock.send_to(&req, relay_addr);
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                if from.ip() == relay_addr.ip() {
                    if let Some((ty, resp_token, payload)) = parse_packet(&buf[..n]) {
                        if ty == MSG_STUN_RESP && resp_token == token {
                            if let Some(mapped) = parse_addr_payload(payload) {
                                return Ok(mapped);
                            }
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    bail!("STUN 探测超时 (relay={})", relay_addr);
}

/// 收集本地 Candidates（包括通过 connect 探测得出的本机网卡内网 IP）
pub fn collect_local_candidates(bound_port: u16) -> Vec<SocketAddr> {
    let mut addrs = Vec::new();
    // 探测通往公网的局域网出口 IP
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = s.local_addr() {
                if !local.ip().is_loopback() && !local.ip().is_unspecified() {
                    addrs.push(SocketAddr::new(local.ip(), bound_port));
                }
            }
        }
    }
    // 补齐 127.0.0.1 供单机 e2e 测试
    addrs.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bound_port));
    addrs.dedup();
    addrs
}

/// 将候选端点列表格式化为逗号分隔的字符串供信令交换
pub fn serialize_candidates(candidates: &[SocketAddr]) -> String {
    candidates
        .iter()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// 解析候选端点列表字符串
pub fn deserialize_candidates(s: &str) -> Vec<SocketAddr> {
    s.split(',')
        .filter_map(|part| part.trim().parse::<SocketAddr>().ok())
        .collect()
}

/// 双向对称打洞（Disco Punching）：
/// 在同一个 UDP Socket 上，向对端的所有候选端点密集发送 Ping；
/// 收到对端的 Ping 立即回复 Pong，收到对端的 Pong 则确认双向穿透成功并返回打通的对端地址。
pub fn disco_punch(
    sock: &UdpSocket,
    candidates: &[SocketAddr],
    token: &[u8; 16],
    timeout: Duration,
) -> Result<SocketAddr> {
    if candidates.is_empty() {
        bail!("没有可用的对端候选打洞端点");
    }

    sock.set_read_timeout(Some(Duration::from_millis(25)))?;

    let ping_pkt = encode_packet(MSG_DISCO_PING, token, &[]);
    let pong_pkt = encode_packet(MSG_DISCO_PONG, token, &[]);

    let start = Instant::now();
    let mut last_send = Instant::now() - Duration::from_secs(1);
    let mut buf = [0u8; 128];
    let mut verified_peer: Option<SocketAddr> = None;

    while Instant::now().duration_since(start) < timeout {
        // 每 35ms 遍历发送一次 Ping 给对端的所有候选端点
        if last_send.elapsed() >= Duration::from_millis(35) {
            for &cand in candidates {
                let _ = sock.send_to(&ping_pkt, cand);
            }
            last_send = Instant::now();
        }

        // 接收对端发来的打洞包
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                if let Some((ty, pkt_token, _)) = parse_packet(&buf[..n]) {
                    if &pkt_token == token {
                        match ty {
                            MSG_DISCO_PING => {
                                // 对方的探测包打进来了！立即回 Pong 给对方开门
                                let _ = sock.send_to(&pong_pkt, from);
                            }
                            MSG_DISCO_PONG => {
                                // 收到对方的 Pong，说明双向开门完全成功！
                                verified_peer = Some(from);
                                // 连发 3 次 Pong 确保对端也收敛
                                for _ in 0..3 {
                                    let _ = sock.send_to(&pong_pkt, from);
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                continue;
            }
            Err(_e) => {
                // 忽略非致命网络抖动
            }
        }
    }

    // 清理 socket 读超时，恢复默认阻塞
    let _ = sock.set_read_timeout(None);

    if let Some(peer) = verified_peer {
        Ok(peer)
    } else {
        bail!("打洞超时，对端未能及时响应双向探测");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_token_derivation_deterministic() {
        let t1 = derive_punch_token("mysid", "p@ss123");
        let t2 = derive_punch_token("mysid", "p@ss123");
        let t3 = derive_punch_token("othersid", "p@ss123");
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
    }

    #[test]
    fn test_packet_encode_decode() {
        let token = [7u8; 16];
        let payload = b"hello_nat";
        let pkt = encode_packet(MSG_DISCO_PING, &token, payload);
        let parsed = parse_packet(&pkt).expect("parse packet failed");
        assert_eq!(parsed.0, MSG_DISCO_PING);
        assert_eq!(parsed.1, token);
        assert_eq!(parsed.2, payload);
    }

    #[test]
    fn test_addr_encode_decode() {
        let addr: SocketAddr = "114.114.114.114:8989".parse().unwrap();
        let payload = encode_addr_payload(addr);
        let decoded = parse_addr_payload(&payload).expect("decode failed");
        assert_eq!(addr, decoded);
    }

    #[test]
    fn test_candidates_serialization() {
        let list = vec![
            "1.2.3.4:8888".parse().unwrap(),
            "192.168.1.1:23517".parse().unwrap(),
        ];
        let s = serialize_candidates(&list);
        let restored = deserialize_candidates(&s);
        assert_eq!(list, restored);
    }

    #[test]
    fn test_disco_punch_loopback() {
        let a_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let a_addr = a_sock.local_addr().unwrap();
        let b_addr = b_sock.local_addr().unwrap();

        let token = derive_punch_token("sess-test", "secret-test");

        let token_b = token;
        let b_handle = thread::spawn(move || {
            disco_punch(&b_sock, &[a_addr], &token_b, Duration::from_secs(2))
        });

        let a_res = disco_punch(&a_sock, &[b_addr], &token, Duration::from_secs(2)).expect("A 打洞成功");
        let b_res = b_handle.join().unwrap().expect("B 打洞成功");

        assert_eq!(a_res, b_addr);
        assert_eq!(b_res, a_addr);
    }
}
