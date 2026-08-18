//! PairDesk 中继服务器（独立部署于 VPS，不进客户端安装包）。
//!
//! 职责（V1 精简版）：
//!  1. 信令牵线：客户端按"会话码(sid)"注册/匹配，让同会话双方相认。
//!  2. 透明桥接：匹配成功后把两条 TCP 连接做【字节级双向转发】，
//!     完全不解析 PairDesk 帧协议——客户端现有握手/加密/数据流照常运行，
//!     中继只是"看不见的快递管道"。
//!
//! 角色协议（首字节 + 变长 sid）：
//! ```text
//! Host   连入后发  b'H' | sid_len(1) | sid    → 登记，等待 viewer
//! Viewer 连入后发  b'V' | sid_len(1) | sid    → 匹配 host，触发桥接
//! 匹配成功后：所有后续字节双向透传，relay 不再解析。
//! 任一连接断开 → 关闭另一端，会话结束。
//! ```
//!
//! 说明：V1 假设"同一 sid 同时只有一对 host/viewer"。
//! 真正的"打洞优先 + 告知对方公网地址"信令在客户端 Stage2 落地，届时扩展此协议。
//!
//! 用法：`pairdesk-relay [端口]`（默认 8989）。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result, bail};

const DEFAULT_PORT: u16 = 8989;
const MAX_SID: usize = 64;

/// 等待中的 host 连接池：sid → (TcpStream, host 公网IP, 打洞UDP端口)。
type HostPool = Arc<Mutex<HashMap<String, (TcpStream, std::net::IpAddr, u16)>>>;

fn main() -> Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = TcpListener::bind(("0.0.0.0", port)).context("绑定监听端口失败")?;
    println!("[relay] 中继服务器监听 0.0.0.0:{}", port);

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

/// 处理单个客户端连接：读角色头 → 注册或匹配 → 必要时启动桥接。
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
        // Host 注册（携带打洞 UDP 端口）：克隆一份进池，原 stream 留在本线程阻塞等待
        b'H' => {
            // 注册 v2：角色(1) | sid_len(1) | sid | hole_port(2, 大端)
            let mut portb = [0u8; 2];
            stream.read_exact(&mut portb)?;
            let hole_port = u16::from_be_bytes(portb);
            let host_ip = stream.peer_addr()?.ip();
            let pool_copy = stream.try_clone().context("克隆 host 连接失败")?;
            pool.lock().unwrap().insert(sid.clone(), (pool_copy, host_ip, hole_port));
            println!("[relay] host 注册 会话={} 打洞端点={}:{} (等待 viewer)", sid, host_ip, hole_port);
            // 保持原连接不 drop（drop 会关闭 socket），线程常驻等待
            loop {
                thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        // Viewer 请求：把 host 打洞端点作为【信令】回给 viewer，然后触发桥接兜底
        b'V' => {
            let (host, host_ip, hole_port) = match pool.lock().unwrap().remove(&sid) {
                Some(h) => h,
                None => {
                    let _ = stream.write_all(&[b'E']);
                    bail!("会话 {} 无等待中的 host", sid);
                }
            };
            // 信令 v2：S | family(1: 4/6) | ip_octets | hole_port(2, 大端)
            let ip_octets = match host_ip {
                std::net::IpAddr::V4(v4) => {
                    let mut sig = vec![b'S', 4u8];
                    sig.extend_from_slice(&v4.octets());
                    sig.extend_from_slice(&hole_port.to_be_bytes());
                    sig
                }
                std::net::IpAddr::V6(v6) => {
                    let mut sig = vec![b'S', 6u8];
                    sig.extend_from_slice(&v6.octets());
                    sig.extend_from_slice(&hole_port.to_be_bytes());
                    sig
                }
            };
            let mut s = stream.try_clone().context("克隆 viewer 连接失败")?;
            s.write_all(&ip_octets)?;
            println!("[relay] viewer 匹配 会话={} → 信令打洞端点 {}/{} → 桥接", sid, host_ip, hole_port);
            bridge(host, stream); // 所有权移入 bridge，常驻转发（中继兜底）
            Ok(())
        }
        other => bail!("未知角色: {}", other as u8),
    }
}

/// 双向透明桥接。
/// a、b 各自需要两个句柄（一读一写方向分离），故 try_clone 各一份。
/// 任一端读到 EOF/错误 → 该线程结束；两个线程都结束时，底层 socket 一并关闭。
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
