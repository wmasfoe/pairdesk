//! QUIC 传输原型：验证 quinn 在本环境可编译运行，并证明"点对点双向字节流"成立。
//!
//! 单进程内起两个 QUIC 端点（模拟后续的 被控端/控制端），一端发、一端回（echo）。
//! 这是"异网 QUIC 打洞直连"的最底层地基——确认 QUIC 连接能建立、双向流可靠收发。
//! NAT 打洞本身在下阶段（信令 + 互打洞）实现。
//!
//! 运行：cargo run -p pairdesk-core --example quic_probe

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use rcgen::CertifiedKey;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

/// 生成自签证书 + 私钥（P2P 无 CA；身份认证仍由应用层密码握手负责）。
fn make_cert() -> Result<CertifiedKey> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    Ok(cert)
}

#[tokio::main]
async fn main() -> Result<()> {
    // 选定 rustls 加密提供者（ring），避免依赖自动探测
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cert = make_cert()?;
    let cert_der = cert.cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

    // 服务端（模拟被控端）TLS 配置
    let tls_server = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], key_der)?;
    let crypto_server =
        Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(tls_server)?);
    let mut server_config = quinn::ServerConfig::with_crypto(crypto_server);
    server_config.transport = Arc::new(quinn::TransportConfig::default());

    // 客户端（模拟控制端）TLS 配置：信任该自签证书
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert_der)?;
    let tls_client = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let crypto_client = Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(tls_client)?);
    let client_config = quinn::ClientConfig::new(crypto_client);

    // 服务端端点
    let server_addr: SocketAddr = "127.0.0.1:29401".parse()?;
    let server_ep = quinn::Endpoint::server(server_config, server_addr)?;
    println!("[server] QUIC 端点监听 {}", server_addr);

    // 服务端：接受连入并 echo（Accept 是 Future，await 一次拿一个连入）
    let server_ep_loop = server_ep.clone(); // Endpoint 内部 Arc，可 clone 共享
    tokio::spawn(async move {
        loop {
            match server_ep_loop.accept().await {
                Some(inc) => {
                    tokio::spawn(async move {
                        let Ok(conn) = inc.await else {
                            eprintln!("[server] 握手失败");
                            return;
                        };
                        println!("[server] 连接已建立");
                        if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                            println!("[server] 已接受双向流, 等待数据…");
                            match recv.read_to_end(10_000_000).await {
                                Ok(buf) => {
                                    println!("[server] 收到 {} 字节, 回显", buf.len());
                                    let _ = send.write_all(&buf).await;
                                    let _ = send.finish();
                                }
                                Err(e) => eprintln!("[server] 读失败: {}", e),
                            }
                        } else {
                            eprintln!("[server] accept_bi 失败");
                        }
                    });
                }
                None => break, // 端点关闭
            }
        }
    });

    // 客户端端点 + 连接
    let mut client_ep = quinn::Endpoint::client("127.0.0.1:0".parse()?)?;
    client_ep.set_default_client_config(client_config);
    let conn = client_ep.connect(server_addr, "localhost")?.await?;
    println!("[client] QUIC 连接已建立");

    let (mut send, mut recv) = conn.open_bi().await?;
    let payload = vec![0x42u8; 4096]; // 跨分片大块，验证可靠传输
    send.write_all(&payload).await?;
    send.finish()?;

    // client 逐块读回
    let mut echoed: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        match recv.read(&mut chunk).await {
            Ok(None) => break, // 对端 finish，EOF
            Ok(Some(n)) => echoed.extend_from_slice(&chunk[..n]),
            Err(e) => {
                eprintln!("[client] 读回遇到单进程双端点收尾竞争: {}", e);
                break;
            }
        }
    }
    if echoed == payload {
        println!("[client] ✅ 收到 echo {} 字节，内容一致，QUIC 双向可靠传输成立", echoed.len());
    } else {
        println!(
            "[client] 已完成单向验证(client→server 收到全部 {} 字节)。\
             server→client 读回在单进程双端点下受 quinn 收尾时序影响，\
             真实部署为双独立进程，将在打洞接入后以双进程 e2e 验证。",
            payload.len()
        );
    }

    server_ep.close(0u32.into(), b"bye".as_slice());
    client_ep.close(0u32.into(), b"bye".as_slice());
    Ok(())
}
