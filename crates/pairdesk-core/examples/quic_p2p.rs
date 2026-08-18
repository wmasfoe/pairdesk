//! 跨进程 QUIC 点对点验证（模拟"打洞成功后的异网直连"）。
//!
//! 两个独立进程、各自 QUIC 端点、用同一份内置证书 TLS 互认：
//!  - host 进程：起 QUIC 服务端，接受双向流并回显
//!  - viewer 进程：起 QUIC 客户端，连 host、发大块数据、读回验证
//!
//! 这验证的是打洞成功后最关键的一环：跨进程(真实双机形态)下 QUIC 双向可靠传输。
//! 运行（两个终端）：
//!   A: cargo run -p pairdesk-core --example quic_p2p -- host 127.0.0.1:29501
//!   B: cargo run -p pairdesk-core --example quic_p2p -- viewer 127.0.0.1:29501
//!
//! 注意：NAT 打洞本身(信令交换公网地址+互撞洞)是更高一层，见打洞模块。

use std::net::SocketAddr;
use std::path::Path;

use anyhow::{Result, bail};
use pairdesk_core::certs;
use tokio::io::AsyncReadExt;

/// 测试用证书目录（双进程共享同一身份，模拟同机两端；真实跨机靠信令交换公钥）
const CERT_DIR: &str = "/tmp/pd-quic";
/// 固定载荷大小（精确读写，避开 finish/EOF 语义干扰）
const PAYLOAD_LEN: usize = 64 * 1024;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = std::env::args().collect();
    let role = args.get(1).map(|s| s.as_str()).unwrap_or("host");
    let addr: SocketAddr = args
        .get(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or("127.0.0.1:29501".parse()?);

    match role {
        "host" => run_host(addr).await,
        "viewer" => run_viewer(addr).await,
        _ => bail!("用法: quic_p2p [host|viewer] <addr>"),
    }
}

/// 被控端：QUIC 服务端，accept 双向流 → 读 → 回显。
async fn run_host(addr: SocketAddr) -> Result<()> {
    let id = certs::ensure_identity(Path::new(CERT_DIR))?;
    let server = quinn::Endpoint::server(certs::server_quic_config(&id)?, addr)?;
    println!("[host] QUIC 服务端监听 {addr}");
    loop {
        match server.accept().await {
            Some(inc) => {
                tokio::spawn(async move {
                    match inc.await {
                        Ok(conn) => {
                            println!("[host] 连接已建立, 等待双向流…");
                            if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                                // 精确读固定字节，验证 A→B 可靠传输
                                let mut buf = vec![0u8; PAYLOAD_LEN];
                                match recv.read_exact(&mut buf).await {
                                    Ok(_) => {
                                        println!("[host] 收到 {} 字节", buf.len());
                                        // 回显（B→A），并保持连接存活片刻（验证是不是 task 结束导致 conn 关闭）
                                        match send.write_all(&buf).await {
                                            Ok(_) => {
                                                println!("[host] 回显 {0} 字节完成, 保持连接 2s…", buf.len());
                                                // 用 async sleep，避免阻塞 tokio worker（会饿死 quinn 驱动）
                                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                            }
                                            Err(e) => eprintln!("[host] 回显写失败: {e:#}"),
                                        }
                                    }
                                    Err(e) => eprintln!("[host] 读失败: {e:#}"),
                                }
                            } else {
                                eprintln!("[host] accept_bi 失败");
                            }
                        }
                        Err(e) => eprintln!("[host] 握手失败: {e}"),
                    }
                });
            }
            None => break,
        }
    }
    Ok(())
}

/// 控制端：QUIC 客户端，连 host → 发大块数据 → 读回验证。
async fn run_viewer(addr: SocketAddr) -> Result<()> {
    let id = certs::ensure_identity(Path::new(CERT_DIR))?;
    let mut ep = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
    ep.set_default_client_config(certs::client_quic_config(&id)?);

    let conn = ep
        .connect(addr, "pairdesk.local")?
        .await
        .map_err(|e| anyhow::anyhow!("QUIC 连接失败: {e}"))?;
    println!("[viewer] QUIC 连接已建立 → {addr}");

    let (mut send, mut recv) = conn.open_bi().await?;
    let payload = vec![0x5Au8; PAYLOAD_LEN];
    send.write_all(&payload).await?;
    // 不 finish：保持流打开，精确读回 host 的 echo
    let mut echoed = vec![0u8; PAYLOAD_LEN];
    match recv.read_exact(&mut echoed).await {
        Ok(_) => {
            if echoed == payload {
                println!(
                    "[viewer] ✅ 跨进程 QUIC 双向可靠传输成立: 收到 echo {} 字节内容一致",
                    echoed.len()
                );
            } else {
                anyhow::bail!("❌ echo 内容不一致");
            }
        }
        Err(e) => {
            // 诊断：连接为何被判定丢失
            let why = tokio::time::timeout(std::time::Duration::from_secs(3), conn.closed()).await;
            eprintln!("[viewer] 连接关闭原因: {:?}", why);
            anyhow::bail!("读 echo 失败: {e:#}")
        }
    }
    Ok(())
}
