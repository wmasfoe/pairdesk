//! 跨进程验证：QUIC 直连承载【PairDesk 帧协议】双向往返。
//!
//! 这是把 QUIC 接进会话层的最后一块基石：
//!  - 用 `QuicFrameStream`（异步流 block_on 桥成同步帧流）在 QUIC 上收发完整帧；
//!  - 帧格式与 TCP 完全一致（8B 头 + payload），证明两端 TCP/QUIC 协议互通。
//!
//! 运行（两个终端）：
//!   A: cargo run -p pairdesk-core --example quic_frame_p2p -- host 127.0.0.1:29601
//!   B: cargo run -p pairdesk-core --example quic_frame_p2p -- viewer 127.0.0.1:29601

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, bail};
use pairdesk_core::certs;
use pairdesk_core::protocol::FrameType;
use pairdesk_core::quic_frame::{FrameStream, QuicFrameStream};

const CERT_DIR: &str = "/tmp/pd-quic";

fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // 与 QUIC endpoint 共享的多线程运行时（QuicFrameStream 用它 block_on 收/发帧）
    let rt = Arc::new(tokio::runtime::Runtime::new()?);

    let args: Vec<String> = std::env::args().collect();
    let role = args.get(1).map(|s| s.as_str()).unwrap_or("host");
    let addr: SocketAddr = args
        .get(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or("127.0.0.1:29601".parse()?);

    match role {
        "host" => run_host(addr, rt),
        "viewer" => run_viewer(addr, rt),
        _ => bail!("用法: quic_frame_p2p [host|viewer] <addr>"),
    }
}

/// 被控端：QUIC server，接受一条双向流 → 包成 QuicFrameStream → 收一帧回一帧。
fn run_host(addr: SocketAddr, rt: Arc<tokio::runtime::Runtime>) -> Result<()> {
    let id = certs::ensure_identity(Path::new(CERT_DIR))?;
    println!("[host] QUIC 帧流服务监听 {addr}");

    // Endpoint 与 accept 都需在 runtime context 内创建/推进
    let (send, recv) = rt.block_on(async {
        let server = quinn::Endpoint::server(certs::server_quic_config(&id)?, addr)?;
        let inc = server
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("server 关闭"))?;
        let conn = inc.await?;
        conn.accept_bi().await.map_err(anyhow::Error::from)
    })?;

    let mut fs = QuicFrameStream::new(rt.clone(), send, recv);
    let f = fs
        .recv_frame()?
        .expect("应收到一帧");
    println!(
        "[host] 收到帧 {:?} = {:?}",
        f.ty,
        String::from_utf8_lossy(&f.payload)
    );
    fs.send_frame(FrameType::Heartbeat, b"pong")?;
    println!("[host] ✅ 已回射 pong（帧流 host 侧往返完成）");
    // 保持连接片刻，让 viewer 读走回帧（真实会话 host 常驻，这里模拟）
    rt.block_on(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    });
    Ok(())
}

/// 控制端：QUIC client → 双向流 → QuicFrameStream → 发一帧收一帧。
fn run_viewer(addr: SocketAddr, rt: Arc<tokio::runtime::Runtime>) -> Result<()> {
    let id = certs::ensure_identity(Path::new(CERT_DIR))?;
    let cfg = certs::client_quic_config(&id)?;

    let (send, recv) = rt.block_on(async {
        let mut ep = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
        ep.set_default_client_config(cfg);
        let conn = ep
            .connect(addr, "pairdesk.local")?
            .await
            .map_err(anyhow::Error::from)?;
        conn.open_bi().await.map_err(anyhow::Error::from)
    })?;

    let mut fs = QuicFrameStream::new(rt.clone(), send, recv);
    fs.send_frame(FrameType::Heartbeat, b"ping")?;
    let f = fs
        .recv_frame()?
        .expect("应收到回帧");
    println!(
        "[viewer] 收到回帧 {:?} = {:?}",
        f.ty,
        String::from_utf8_lossy(&f.payload)
    );
    if &f.payload[..] == b"pong" {
        println!("[viewer] ✅ QUIC 承载帧协议双向往返内容一致");
        Ok(())
    } else {
        bail!("回帧内容不符");
    }
}
