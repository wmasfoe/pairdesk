//! PairDesk 中继服务器（独立部署于 VPS，不进客户端安装包）。
//!
//! 职责（方案 B / Tailscale 模式升级）：
//!  1. STUN 服务：在同一端口监听 UDP，响应客户端的 STUN 探测，返回客户端真实映射的公网端点 (IP:Port)。
//!  2. 信令牵线：Host 与 Viewer 分别上报自身的打洞候选端点列表 (Candidates)，
//!     双方匹配时双向推送信令，协调双方启动双向对称打洞。
//!  3. 透明桥接：同时在 TCP 链路上启动字节级双向转发，作为打洞未成时的无感中继兜底。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{bail, Context, Result};

const DEFAULT_PORT: u16 = 8989;
const MAX_SID: usize = 64;
const MAGIC: [u8; 4] = *b"PDSK";
const MSG_STUN_REQ: u8 = 0x01;
const MSG_STUN_RESP: u8 = 0x02;

/// 等待中的 host 连接池：sid → (TcpStream, host_candidates_string)。
type HostPool = Arc<Mutex<HashMap<String, (TcpStream, String)>>>;

fn main() -> Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    // 启动 UDP STUN 探测服务（轻量、无状态）
    let udp_sock = UdpSocket::bind(("0.0.0.0", port)).context("绑定 UDP 端口失败")?;
    println!("[relay] UDP STUN 服务就绪 0.0.0.0:{}", port);
    thread::Builder::new()
        .name("relay-stun".into())
        .spawn(move || run_stun_service(udp_sock))?;

    // 启动 TCP 信令与中继桥接服务
    let listener = TcpListener::bind(("0.0.0.0", port)).context("绑定 TCP 监听端口失败")?;
    println!("[relay] TCP 信令与中继服务监听 0.0.0.0:{}", port);

    let pool: HostPool = Arc::new(Mutex::new(HashMap::new()));
    for conn in listener.incoming() {
        let stream = match conn {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[relay] 接受连接失败: {}", e);
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let pool = pool.clone();
        thread::Builder::new()
            .name("relay-conn".into())
            .spawn(move || {
                if let Err(e) = handle_client(stream, &pool) {
                    eprintln!("[relay] 连接处理: {:#}", e);
                }
            })?;
    }
    Ok(())
}

/// UDP STUN 探测响应服务：
/// 客户端发送: MAGIC(4) | MSG_STUN_REQ(1) | token(16)
/// 中继回送:   MAGIC(4) | MSG_STUN_RESP(1) | token(16) | family(1) | ip_bytes | port(2, be)
fn run_stun_service(sock: UdpSocket) {
    let mut buf = [0u8; 256];
    loop {
        let (n, src) = match sock.recv_from(&mut buf) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("[relay-stun] 接收出错: {}", e);
                continue;
            }
        };
        if n >= 21 && &buf[0..4] == &MAGIC && buf[4] == MSG_STUN_REQ {
            let mut resp = Vec::with_capacity(32);
            resp.extend_from_slice(&MAGIC);
            resp.push(MSG_STUN_RESP);
            resp.extend_from_slice(&buf[5..21]); // 回填相同 token
            match src {
                SocketAddr::V4(v4) => {
                    resp.push(4);
                    resp.extend_from_slice(&v4.ip().octets());
                    resp.extend_from_slice(&v4.port().to_be_bytes());
                }
                SocketAddr::V6(v6) => {
                    resp.push(6);
                    resp.extend_from_slice(&v6.ip().octets());
                    resp.extend_from_slice(&v6.port().to_be_bytes());
                }
            }
            let _ = sock.send_to(&resp, src);
        }
    }
}

/// 处理客户端 TCP 连接（信令交换 + 中继转发）
fn handle_client(mut stream: TcpStream, pool: &HostPool) -> Result<()> {
    let mut head = [0u8; 1];
    stream.read_exact(&mut head)?;
    let role = head[0];

    // 读 sid
    let mut lenb = [0u8; 1];
    stream.read_exact(&mut lenb)?;
    let sid_len = lenb[0] as usize;
    if sid_len == 0 || sid_len > MAX_SID {
        bail!("sid 长度非法: {}", sid_len);
    }
    let mut sid = vec![0u8; sid_len];
    stream.read_exact(&mut sid)?;
    let sid = String::from_utf8_lossy(&sid).to_string();

    match role {
        // Host 注册：携带自身候选端点字符串
        b'H' => {
            let cands = read_string_payload(&mut stream)?;
            let pool_copy = stream.try_clone().context("克隆 host 连接失败")?;
            pool.lock().unwrap().insert(sid.clone(), (pool_copy, cands.clone()));
            println!("[relay] host 注册 会话={} 候选端点=[{}] (等待 viewer)", sid, cands);

            // 监听连接状态：一旦 host 异常断开，立刻从 pool 中清理，避免死连接堆积与线程泄露
            let mut dummy = [0u8; 1];
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
            loop {
                // 如果会话已经被 viewer 匹配拿走，则该注册监听线程任务完成，正常退出
                if !pool.lock().unwrap().contains_key(&sid) {
                    break;
                }
                match stream.peek(&mut dummy) {
                    Ok(0) => {
                        // EOF：Host 端主动断开连接
                        println!("[relay] host 主动断开 会话={}", sid);
                        pool.lock().unwrap().remove(&sid);
                        break;
                    }
                    Ok(_) => {
                        // 有数据进入（可能是匹配后 host 发送的数据），检查是否仍在 pool 中
                        if !pool.lock().unwrap().contains_key(&sid) {
                            break;
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                        continue;
                    }
                    Err(e) => {
                        println!("[relay] host 连接异常断开 会话={}: {}", sid, e);
                        pool.lock().unwrap().remove(&sid);
                        break;
                    }
                }
            }
            Ok(())
        }
        // Viewer 请求：发送自身候选端点，换取 Host 候选端点，并触发双向打洞与透明桥接
        b'V' => {
            let viewer_cands = read_string_payload(&mut stream)?;
            let (mut host, host_cands) = match pool.lock().unwrap().remove(&sid) {
                Some(h) => h,
                None => {
                    let _ = stream.write_all(&[b'E']);
                    bail!("会话 {} 无等待中的 host", sid);
                }
            };

            // 确保桥接双向读写前清理读超时设置
            let _ = host.set_read_timeout(None);
            let _ = stream.set_read_timeout(None);

            // 1. 给 Viewer 推送 Host 的 Candidates (信令格式: b'C' | len(2, be) | str)
            send_string_payload(&mut stream, b'C', &host_cands)?;

            // 2. 同时给 Host 推送 Viewer 的 Candidates，触发双向打洞
            send_string_payload(&mut host, b'C', &viewer_cands)?;

            println!(
                "[relay] viewer 匹配 会话={} → 双向推送信令 (Host cands=[{}], Viewer cands=[{}]) → 桥接",
                sid, host_cands, viewer_cands
            );

            // 3. 启动双向透明中继桥接（兜底通道）
            bridge(host, stream);
            Ok(())
        }
        other => bail!("未知角色: {}", other as u8),
    }
}

/// 辅助读取以 2 字节大端长度前缀开头的字符串
fn read_string_payload(stream: &mut TcpStream) -> Result<String> {
    let mut lenb = [0u8; 2];
    stream.read_exact(&mut lenb)?;
    let len = u16::from_be_bytes(lenb) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

/// 辅助发送带标志字节与 2 字节大端长度前缀的字符串
fn send_string_payload(stream: &mut TcpStream, tag: u8, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    let len = bytes.len() as u16;
    let mut out = Vec::with_capacity(1 + 2 + bytes.len());
    out.push(tag);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    stream.write_all(&out)?;
    stream.flush()?;
    Ok(())
}

/// 双向透明桥接（TCP 中继兜底）
fn bridge(a: TcpStream, b: TcpStream) {
    let (Ok(mut a2), Ok(mut b2)) = (a.try_clone(), b.try_clone()) else {
        return;
    };
    let mut a = a;
    let mut b = b;
    // a → b
    let t1 = thread::spawn(move || {
        let _ = std::io::copy(&mut a, &mut b);
    });
    // b → a
    let t2 = thread::spawn(move || {
        let _ = std::io::copy(&mut b2, &mut a2);
    });
    let _ = t1.join();
    let _ = t2.join();
}
