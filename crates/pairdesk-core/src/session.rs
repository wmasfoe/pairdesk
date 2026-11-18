//! 会话层：握手 + 双端运行逻辑（被控端/控制端）。
//!
//! 线程模型：
//! ```text
//! 被控端: [接收线程]收帧→解密→输入注入   [采集线程]采屏→编码→加密→发送   [控制线程]消费命令
//! 控制端: [接收线程]收帧→解密→UI事件     [控制线程]消费命令(发输入/停止) + 心跳
//! ```

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::capture::{PlatformCapturer, ScreenCapturer};
use crate::encode::encode_jpeg;
use crate::input::{InputInjector, PlatformInjector};
use crate::protocol::*;
use crate::quic_frame::FrameStream;
use crate::relay;
use crate::transport::{accept_once, connect};
use crate::{ControlCommand, CoreEvent, Quality};

/// 读超时：超过心跳间隔即可（越大越省电，越小断线越快）
const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// 失联判死阈值：读超时 N 次仍无任何帧
const READ_TIMEOUT_TO_DEAD: u32 = 3;

// ---------- 启动入口 ----------

pub fn spawn_host(
    port: u16,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    // 双通道：命令(UI→core) 与 事件(core→UI) 分离
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-host".into())
        .spawn(move || {
            if let Err(e) = host_session(port, &password, &event_tx, &cmd_rx) {
                let _ = event_tx.send(CoreEvent::Error(format!("被控端错误: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

pub fn spawn_viewer(
    addr: SocketAddr,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-viewer".into())
        .spawn(move || {
            if let Err(e) = viewer_session(addr, &password, &event_tx, &cmd_rx) {
                let _ = event_tx.send(CoreEvent::Error(format!("控制端错误: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

// ---------- 中继模式（经 pairdesk-relay 建立连接） ----------

/// 被控端经中继登记：连 relay → 注册 sid → 等 viewer，随后走单会话。
/// 会话结束后自动重新注册继续等待下一个 viewer（被控端常驻），收到 Stop 才退出。
pub fn spawn_host_via_relay(
    relay: SocketAddr,
    sid: String,
    hole_port: u16,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-host-relay".into())
        .spawn(move || {
            loop {
                // 检查是否收到停止命令（会话间隙顺手消费）
                if let Ok(ControlCommand::Stop) = cmd_rx.lock().unwrap().try_recv() {
                    eprintln!("[host-relay] 收到停止命令，退出常驻循环");
                    break;
                }
                let result = (|| -> Result<()> {
                    eprintln!("[host-relay] 正在向中继注册: relay={}, sid={}, hole_port={}", relay, sid, hole_port);
                    let (conn, _) = relay::register_host(relay, &sid, "")?;
                    eprintln!("[host-relay] 中继注册成功，等待控制端连入…");
                    let _ = host_session_once(conn, &password, &event_tx, &cmd_rx);
                    // 会话结束：正常返回继续等待下一个 viewer
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        eprintln!("[host-relay] 会话结束，重新注册等待下一个 viewer…");
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[host-relay] 出现错误: {}", e);
                        let _ = event_tx.send(CoreEvent::Error(format!("中继被控端: {}", e)));
                        // 网络错误时稍作退避再重试注册（避免死循环打爆 relay）
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                }
            }
        })?;
    Ok((h, event_rx))
}

/// 控制端经中继匹配：连 relay → 按 sid 匹配 host → 走握手。
pub fn spawn_viewer_via_relay(
    relay: SocketAddr,
    sid: String,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-viewer-relay".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                eprintln!("[viewer-relay] 正在通过中继匹配 host: relay={}, sid={}", relay, sid);
                let (conn, holes) = relay::connect_viewer(relay, &sid, "")?;
                eprintln!("[viewer-relay] 中继匹配成功，拿到打洞候选端点: {:?}", holes);
                if let Some(first) = holes.first() {
                    let _ = event_tx.send(CoreEvent::SignalHole(*first));
                }
                viewer_session_with_conn(conn, &password, &event_tx, &cmd_rx)
            })();
            if let Err(e) = result {
                eprintln!("[viewer-relay] 出现错误: {}", e);
                let _ = event_tx.send(CoreEvent::Error(format!("中继控制端: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

// ---------- QUIC 直连（异网打洞后的 P2P 传输） ----------

/// 被控端 QUIC 直连：在 hole_port 起 QUIC server，等 viewer 连接后跑会话。
/// 会话结束后自动重新进入 accept 状态等待下一个连接（常驻保活）。
pub fn spawn_host_via_quic(
    hole_port: u16,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-host-quic".into())
        .spawn(move || {
            loop {
                if let Ok(ControlCommand::Stop) = cmd_rx.lock().unwrap().try_recv() {
                    eprintln!("[host-quic] 收到停止命令，退出 QUIC 常驻循环");
                    break;
                }
                let result = (|| -> Result<()> {
                    eprintln!("[host-quic] 正在启动 QUIC Server, 监听端口: {}", hole_port);
                    let id = crate::certs::ensure_identity(&identity_dir())?;
                    let rt = Arc::new(tokio::runtime::Runtime::new()?);
                    let (send, recv) = rt.block_on(async move {
                        let server = quinn::Endpoint::server(
                            crate::certs::server_quic_config(&id)?,
                            (std::net::Ipv4Addr::UNSPECIFIED, hole_port).into(),
                        )?;
                        eprintln!("[host-quic] QUIC Server 监听就绪, 等待连接…");
                        let inc = server
                            .accept()
                            .await
                            .ok_or_else(|| anyhow::anyhow!("QUIC server 关闭"))?;
                        let conn = inc.await?;
                        eprintln!("[host-quic] 收到对端 QUIC 连接, 打开双向流…");
                        conn.accept_bi().await.map_err(anyhow::Error::from)
                    })?;
                    eprintln!("[host-quic] QUIC 帧流已建立, 开始会话握手…");
                    let fs = crate::quic_frame::QuicFrameStream::new(rt, send, recv);
                    let _ = host_session_once(fs, &password, &event_tx, &cmd_rx);
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        eprintln!("[host-quic] 会话结束，重新等待下一个 QUIC 连接…");
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[host-quic] 出现错误: {}", e);
                        let _ = event_tx.send(CoreEvent::Error(format!("QUIC 被控端: {}", e)));
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                }
            }
        })?;
    Ok((h, event_rx))
}

/// 控制端 QUIC 直连：连到打洞端点，跑会话。
pub fn spawn_viewer_via_quic(
    hole: SocketAddr,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-viewer-quic".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let id = crate::certs::ensure_identity(&identity_dir())?;
                let cfg = crate::certs::client_quic_config(&id)?;
                let rt = Arc::new(tokio::runtime::Runtime::new()?);
                let (send, recv) = rt.block_on(async move {
                    let mut ep = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
                    ep.set_default_client_config(cfg);
                    let conn = ep
                        .connect(hole, "pairdesk.local")?
                        .await
                        .map_err(anyhow::Error::from)?;
                    conn.open_bi().await.map_err(anyhow::Error::from)
                })?;
                let fs = crate::quic_frame::QuicFrameStream::new(rt, send, recv);
                viewer_session_with_conn(fs, &password, &event_tx, &cmd_rx)
            })();
            if let Err(e) = result {
                let _ = event_tx.send(CoreEvent::Error(format!("QUIC 控制端: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

/// 身份证书目录（~/.pairdesk；测试环境可经 HOME 调整）。
fn identity_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".pairdesk"))
        .unwrap_or_else(|_| "/tmp/pd-quic".into())
}

// ---------- 自动择一（Tailscale 模式：STUN + 双向对称打洞直连，超时平滑走中继兜底） ----------

/// 从 start 起找空闲 UDP 端口（探测后立即释放，最多顺延 20 个）。
/// 全部被占则返回 start（后续绑定会如实报错，由调用方兜底提示）。
pub fn find_free_udp_port(start: u16) -> u16 {
    for p in start..start.saturating_add(20) {
        if std::net::UdpSocket::bind(("0.0.0.0", p)).is_ok() {
            return p; // 探测 socket 随作用域释放，极短竞态窗口可接受
        }
    }
    start
}

/// 被控端自动就绪（方案 B）：
/// 1. 绑定打洞端口并向 Relay 发起 STUN 探测；
/// 2. 收集公网反射端点与内网网卡候选端点；
/// 3. 向 Relay 登记并等待 Viewer；
/// 4. Viewer 连入后双方同时启动双向对称打洞 (1.5s)；
/// 5. 打洞成功走 QUIC 直连；打洞未穿透无缝降级走中继兜底。
pub fn spawn_host_auto(
    relay: SocketAddr,
    sid: String,
    hole_port: u16,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let port = find_free_udp_port(hole_port);
    if port != hole_port {
        let _ = event_tx.send(CoreEvent::Notice(format!(
            "打洞端口 {} 被占用，已自动改用 {}（中继兜底不受影响）",
            hole_port, port
        )));
    }
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);

    let event_tx_clone = event_tx.clone();
    let cmd_rx_clone = Arc::clone(&cmd_rx);
    thread::Builder::new()
        .name("pairdesk-host-auto".into())
        .spawn(move || {
            loop {
                if let Ok(ControlCommand::Stop) = cmd_rx_clone.lock().unwrap().try_recv() {
                    eprintln!("[host-auto] 收到停止命令，退出常驻循环");
                    break;
                }
                let result = (|| -> Result<()> {
                    // 1. 绑定 UDP 打洞 Socket
                    let sock = std::net::UdpSocket::bind(("0.0.0.0", port))?;
                    let mut candidates = Vec::new();

                    // 2. STUN 探测公网端点
                    if let Ok(mapped) = crate::punch::stun_probe(&sock, relay, Duration::from_millis(800)) {
                        eprintln!("[host-auto] STUN 探测成功，获取公网反射端点: {}", mapped);
                        candidates.push(mapped);
                    }
                    for local in crate::punch::collect_local_candidates(port, Some(relay)) {
                        candidates.push(local);
                    }
                    candidates.dedup();
                    let cands_str = crate::punch::serialize_candidates(&candidates);

                    eprintln!("[host-auto] 正在向中继注册: relay={}, sid={}, cands=[{}]", relay, sid, cands_str);

                    // 启动后台 STUN 心跳保活，防止在等待 Viewer 期间 NAT 映射表项老化失效（通常 20-30s 超时）
                    let keepalive_stop = Arc::new(AtomicBool::new(false));
                    let keepalive_sock = sock.try_clone().ok();
                    let stop_flag = Arc::clone(&keepalive_stop);
                    let keepalive_handle = keepalive_sock.map(|ksock| {
                        thread::Builder::new()
                            .name("host-stun-keepalive".into())
                            .spawn(move || {
                                let mut token = [0u8; 16];
                                while !stop_flag.load(Ordering::Relaxed) {
                                    thread::sleep(Duration::from_secs(5));
                                    if stop_flag.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut token);
                                    let req = crate::punch::encode_packet(crate::punch::MSG_STUN_REQ, &token, &[]);
                                    let _ = ksock.send_to(&req, relay);
                                }
                            })
                    });

                    let register_res = relay::register_host(relay, &sid, &cands_str);
                    keepalive_stop.store(true, Ordering::Relaxed);
                    if let Some(Ok(h)) = keepalive_handle {
                        let _ = h.join();
                    }
                    let (relay_conn, viewer_cands) = register_res?;
                    eprintln!("[host-auto] 中继匹配成功，收到控制端打洞端点: {:?}", viewer_cands);

                    // 3. 执行双向对称打洞 (1.5s)
                    let token = crate::punch::derive_punch_token(&sid, &password);
                    eprintln!("[host-auto] 启动双向对称打洞 (1.5s 超时)…");
                    match crate::punch::disco_punch(&sock, &viewer_cands, &token, Duration::from_millis(1500)) {
                        Ok(verified_peer) => {
                            eprintln!("[host-auto] ✅ 双向打洞穿透成功 (对端={})，切换为 QUIC 直连链路！", verified_peer);
                            let _ = event_tx_clone.send(CoreEvent::Transport("QUIC 打洞直连".into()));
                            match wrap_host_quic_stream(sock) {
                                Ok(fs) => host_session_once(fs, &password, &event_tx_clone, &cmd_rx_clone),
                                Err(e) => {
                                    eprintln!("[host-auto] ⚠️ QUIC 握手异常 ({})，自动降级走中继兜底！", e);
                                    let _ = event_tx_clone.send(CoreEvent::Transport(format!("中继兜底 ({})", e)));
                                    host_session_once(relay_conn, &password, &event_tx_clone, &cmd_rx_clone)
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[host-auto] ⚠️ 打洞未穿透 ({})，平滑降级走中继兜底！", e);
                            let _ = event_tx_clone.send(CoreEvent::Transport("中继兜底".into()));
                            host_session_once(relay_conn, &password, &event_tx_clone, &cmd_rx_clone)
                        }
                    }
                })();
                match result {
                    Ok(()) => {
                        eprintln!("[host-auto] 会话结束，重新就绪等待下一个 viewer…");
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[host-auto] 出现错误: {}", e);
                        let _ = event_tx_clone.send(CoreEvent::Error(format!("自动被控端: {}", e)));
                        thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                }
            }
        })?;

    Ok((h, event_rx))
}

/// 控制端自动择一（方案 B）：
/// 1. 绑定打洞端口并向 Relay 发起 STUN 探测；
/// 2. 收集公网反射端点与内网网卡候选端点；
/// 3. 向 Relay 换取 Host 候选端点并双向推送信令；
/// 4. 双方同时启动双向对称打洞 (1.5s)；
/// 5. 打洞成功走 QUIC 直连；打洞未穿透无缝降级走中继兜底。
pub fn spawn_viewer_auto(
    relay: SocketAddr,
    sid: String,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);
    thread::Builder::new()
        .name("pairdesk-viewer-auto".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                // 1. 绑定打洞 UDP Socket
                let sock = std::net::UdpSocket::bind("0.0.0.0:0")?;
                let bound_port = sock.local_addr()?.port();
                let mut candidates = Vec::new();

                // 2. STUN 探测公网端点
                if let Ok(mapped) = crate::punch::stun_probe(&sock, relay, Duration::from_millis(800)) {
                    eprintln!("[viewer-auto] STUN 探测成功，获取公网反射端点: {}", mapped);
                    candidates.push(mapped);
                }
                for local in crate::punch::collect_local_candidates(bound_port, Some(relay)) {
                    candidates.push(local);
                }
                candidates.dedup();
                let cands_str = crate::punch::serialize_candidates(&candidates);

                eprintln!("[viewer-auto] 正在通过中继匹配 host: relay={}, sid={}, cands=[{}]", relay, sid, cands_str);
                let (relay_conn, host_cands) = relay::connect_viewer(relay, &sid, &cands_str)?;
                eprintln!("[viewer-auto] 中继匹配成功，拿到被控端候选端点: {:?}", host_cands);
                if let Some(first) = host_cands.first() {
                    let _ = event_tx.send(CoreEvent::SignalHole(*first));
                }

                // 3. 执行双向对称打洞 (1.5s)
                let token = crate::punch::derive_punch_token(&sid, &password);
                eprintln!("[viewer-auto] 尝试双向对称打洞 (1.5s 超时)…");
                match crate::punch::disco_punch(&sock, &host_cands, &token, Duration::from_millis(1500)) {
                    Ok(verified_peer) => {
                        eprintln!("[viewer-auto] ✅ 双向打洞穿透成功 (对端={})，切换为 QUIC 直连链路！", verified_peer);
                        let _ = event_tx.send(CoreEvent::Transport("QUIC 打洞直连".into()));
                        match wrap_viewer_quic_stream(sock, verified_peer) {
                            Ok(fs) => viewer_session_with_conn(fs, &password, &event_tx, &cmd_rx),
                            Err(e) => {
                                eprintln!("[viewer-auto] ⚠️ QUIC 握手异常 ({})，自动降级走中继兜底！", e);
                                let _ = event_tx.send(CoreEvent::Transport(format!("中继兜底 ({})", e)));
                                viewer_session_with_conn(relay_conn, &password, &event_tx, &cmd_rx)
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[viewer-auto] ⚠️ 打洞未穿透 ({})，平滑降级走中继兜底！", e);
                        let _ = event_tx.send(CoreEvent::Transport("中继兜底".into()));
                        viewer_session_with_conn(relay_conn, &password, &event_tx, &cmd_rx)
                    }
                }
            })();
            if let Err(e) = result {
                eprintln!("[viewer-auto] 出现错误: {}", e);
                let _ = event_tx.send(CoreEvent::Error(format!("自动择一控制端: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

/// 包装已打洞完成的 UDP Socket 为 QUIC 服务端帧流
fn wrap_host_quic_stream(sock: std::net::UdpSocket) -> Result<crate::quic_frame::QuicFrameStream> {
    let id = crate::certs::ensure_identity(&identity_dir())?;
    let rt = Arc::new(tokio::runtime::Runtime::new()?);
    let (send, recv) = rt.block_on(async move {
        let runtime = quinn::default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
        let server = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(crate::certs::server_quic_config(&id)?),
            sock,
            runtime,
        )?;
        let inc = tokio::time::timeout(Duration::from_secs(3), server.accept())
            .await
            .map_err(|_| anyhow::anyhow!("等待对端 QUIC 连接超时"))?
            .ok_or_else(|| anyhow::anyhow!("QUIC server 关闭"))?;
        let conn = inc.await?;
        conn.accept_bi().await.map_err(anyhow::Error::from)
    })?;
    Ok(crate::quic_frame::QuicFrameStream::new(rt, send, recv))
}

/// 包装已打洞完成的 UDP Socket 为 QUIC 客户端帧流并连向对端
fn wrap_viewer_quic_stream(
    sock: std::net::UdpSocket,
    peer_addr: SocketAddr,
) -> Result<crate::quic_frame::QuicFrameStream> {
    let id = crate::certs::ensure_identity(&identity_dir())?;
    let cfg = crate::certs::client_quic_config(&id)?;
    let rt = Arc::new(tokio::runtime::Runtime::new()?);
    let (send, recv) = rt.block_on(async move {
        let runtime = quinn::default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
        let mut ep = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            sock,
            runtime,
        )?;
        ep.set_default_client_config(cfg);
        let conn = tokio::time::timeout(Duration::from_secs(3), async {
            let connecting = ep.connect(peer_addr, "pairdesk.local")?;
            connecting.await.map_err(anyhow::Error::from)
        })
        .await
        .map_err(|_| anyhow::anyhow!("QUIC 直连握手超时"))??;
        conn.open_bi().await.map_err(anyhow::Error::from)
    })?;
    Ok(crate::quic_frame::QuicFrameStream::new(rt, send, recv))
}

// ---------- 被控端 ----------

fn host_session(
    port: u16,
    password: &str,
    tx: &Sender<CoreEvent>,
    rx: &Arc<Mutex<Receiver<ControlCommand>>>,
) -> Result<()> {
    // 循环接受连接：一次会话结束（正常断开/密码错误）后继续等待下一个。
    // 被控端常驻，不会被"连错一次"拖垮。
    loop {
        let conn = match accept_once(port) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(CoreEvent::Error(format!("接受连接失败: {}", e)));
                return Ok(());
            }
        };
        {
            let _ = tx.send(CoreEvent::PeerConnected);
            let _ = host_session_once(conn, password, tx, rx);
        }
        let _ = tx.send(CoreEvent::PeerDisconnected);
    }
}

/// 处理单个会话（一个连接从握手到断开）。
fn host_session_once<C: FrameStream + Send + 'static>(
    mut conn: C,
    password: &str,
    tx: &Sender<CoreEvent>,
    rx: &Arc<Mutex<Receiver<ControlCommand>>>,
) -> Result<()> {
    conn.set_read_timeout(READ_TIMEOUT)?;
    let _peer = conn.peer_addr()?;

    // ---- 握手 ----
    eprintln!("[host-session] 收到连接，等待 Hello 帧…");
    let hello = recv_typed::<_, HelloMsg>(&mut conn, FrameType::Hello)?;
    eprintln!("[host-session] 收到 Hello, 发送 HelloAck(含随机数与 salt)…");
    let host_random: [u8; 16] = rand::random();
    let salt: [u8; 32] = rand::random();
    conn.send_frame(FrameType::HelloAck, &HelloAckMsg { host_random, salt }.encode())?;

    eprintln!("[host-session] 等待客户端 Auth 认证信息…");
    let auth = recv_typed::<_, AuthMsg>(&mut conn, FrameType::Auth)?;
    let expect = password_hash(&salt, password);
    if auth.hash != expect {
        eprintln!("[host-session] ❌ 密码认证失败！");
        conn.send_frame(FrameType::AuthDenied, &AuthDeniedMsg { reason: "密码错误".into() }.encode())?;
        let _ = tx.send(CoreEvent::AuthResult { ok: false, reason: Some("密码错误".into()) });
        return Ok(());
    }
    eprintln!("[host-session] ✅ 密码认证成功，发送 AuthOk，建立加密通道…");
    conn.send_frame(FrameType::AuthOk, &[])?;
    let _ = tx.send(CoreEvent::AuthResult { ok: true, reason: None });
    let _ = tx.send(CoreEvent::PeerConnected);

    let key = session_key(&hello.viewer_random, &host_random, password);
    let cipher = Arc::new(Mutex::new(Cipher::new(key)));

    // ---- 分三线程运行 ----
    let running = Arc::new(AtomicBool::new(true));
    let quality = Arc::new(Mutex::new(Quality::default()));

    // ① 接收线程：输入注入 + 心跳活跃
    let mut recv_conn = conn.try_clone()?;
    let r_cipher = cipher.clone();
    let r_running = running.clone();
    let r_tx = tx.clone();
    let last_active = Arc::new(Mutex::new(Instant::now()));
    let l_active = last_active.clone();
    let r_password = password.to_string();
    thread::Builder::new()
        .name("host-recv".into())
        .spawn(move || -> Result<()> {
            let mut injector = PlatformInjector::new()?;
            let mut timeouts = 0u32;
            loop {
                if !r_running.load(Ordering::SeqCst) {
                    break;
                }
                match recv_conn.recv_frame()? {
                    Some(frame) => {
                        timeouts = 0;
                        *l_active.lock().unwrap() = Instant::now();
                        match frame.ty {
                            FrameType::Input => {
                                let payload = match r_cipher.lock().unwrap().open(&frame.payload) {
                                    Ok(p) => p,
                                    Err(e) => {
                                        eprintln!("[host-recv] 输入帧解密失败(跳过): {}", e);
                                        continue;
                                    }
                                };
                                let msg = match InputMsg::decode(&payload) {
                                    Ok(m) => m,
                                    Err(e) => {
                                        eprintln!("[host-recv] 输入帧解码失败(跳过): {}", e);
                                        continue;
                                    }
                                };
                                // 单个输入失败不致命：记录后继续，避免整个接收线程退出导致失控
                                match apply_input(&mut injector, msg) {
                                    Ok(()) => {
                                        // 诊断日志：确认输入链路打通（PD_LOG_INPUT=1 才输出，避免刷屏）
                                        if std::env::var("PD_LOG_INPUT").is_ok() {
                                            eprintln!("[host-recv] 输入已注入");
                                        }
                                    }
                                    Err(e) => eprintln!("[host-recv] 输入注入失败(跳过): {}", e),
                                }
                            }
                            FrameType::Heartbeat => {}
                            FrameType::Goodbye => {
                                let _ = r_tx.send(CoreEvent::PeerDisconnected);
                                r_running.store(false, Ordering::SeqCst);
                                break;
                            }
                            _ => {}
                        }
                    }
                    None => {
                        timeouts += 1;
                        if timeouts >= READ_TIMEOUT_TO_DEAD {
                            let _ = r_tx.send(CoreEvent::PeerDisconnected);
                            r_running.store(false, Ordering::SeqCst);
                            break;
                        }
                    }
                }
            }
            let _ = r_password;
            Ok(())
        })?;

    // ② 控制线程：消费上层命令
    let c_running = running.clone();
    let c_quality = quality.clone();
    let mut c_conn = conn.try_clone()?;
    let c_tx = tx.clone();
    let c_rx = rx.clone();
    thread::Builder::new()
        .name("host-ctrl".into())
        .spawn(move || -> Result<()> {
            let mut last_hb = Instant::now();
            loop {
                match c_rx.lock().unwrap().recv_timeout(Duration::from_secs(1)) {
                    Ok(ControlCommand::Stop) => {
                        let _ = c_conn.send_frame(FrameType::Goodbye, &[]);
                        let _ = c_conn.shutdown();
                        c_running.store(false, Ordering::SeqCst);
                        let _ = c_tx.send(CoreEvent::PeerDisconnected);
                        break;
                    }
                    Ok(ControlCommand::SetQuality(q)) => {
                        *c_quality.lock().unwrap() = q;
                    }
                    Ok(ControlCommand::SendInput(_)) => {} // 被控端不对外发输入
                    Err(_) => {
                        // 超时：周期心跳
                        if last_hb.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
                            let _ = c_conn.send_frame(FrameType::Heartbeat, &[]);
                            last_hb = Instant::now();
                        }
                        if !c_running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
            Ok(())
        })?;

    // ③ 主线程：采集 + 编码 + 加密 + 发送
    let mut send_conn = conn;
    let mut capturer = PlatformCapturer::new()?;
    let (w, h) = capturer.display_size();
    let _ = tx.send(CoreEvent::Size { w, h });
    let size_payload = SizeMsg { w, h }.encode();
    let sealed_size = cipher.lock().unwrap().seal(&size_payload)?;
    let _ = send_conn.send_frame(FrameType::Size, &sealed_size);
    let mut seq: u32 = 0;
    while running.load(Ordering::SeqCst) {
        let q = *quality.lock().unwrap();
        let frame = match capturer.capture() {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.send(CoreEvent::Error(format!("采屏失败: {}", e)));
                thread::sleep(Duration::from_millis(200));
                continue;
            }
        };
        let jpeg = match encode_jpeg(&frame.rgb, frame.w, frame.h, q.jpeg) {
            Ok(j) => j,
            Err(e) => {
                let _ = tx.send(CoreEvent::Error(format!("编码失败: {}", e)));
                continue;
            }
        };
        seq = seq.wrapping_add(1);
        let payload = FrameMsg { seq, jpeg }.encode();
        let sealed = cipher.lock().unwrap().seal(&payload)?;
        if let Err(_e) = send_conn.send_frame(FrameType::Frame, &sealed) {
            if running.load(Ordering::SeqCst) {
                let _ = tx.send(CoreEvent::PeerDisconnected);
            }
            break;
        }
        // 简单睡眠控帧率
        thread::sleep(Duration::from_millis(1000 / q.fps.max(1) as u64));
    }
    Ok(())
}

/// 把已解码的输入消息应用到注入器。
fn apply_input(inj: &mut PlatformInjector, msg: InputMsg) -> Result<()> {
    match msg {
        InputMsg::MouseMove { x, y } => inj.move_mouse(x, y)?,
        InputMsg::Button { btn, down } => inj.button(btn, down)?,
        InputMsg::Scroll { dx, dy } => inj.scroll(dx, dy)?,
        InputMsg::Key { keycode, down, mods } => inj.key(keycode, down, mods)?,
    }
    Ok(())
}

// ---------- 控制端 ----------

fn viewer_session(
    addr: SocketAddr,
    password: &str,
    tx: &Sender<CoreEvent>,
    rx: &Arc<Mutex<Receiver<ControlCommand>>>,
) -> Result<()> {
    let conn = connect(addr)?;
    viewer_session_with_conn(conn, password, tx, rx)
}

/// 在已建立的连接上执行控制端握手 + 事件循环（直连与中继复用同一套逻辑）。
fn viewer_session_with_conn<C: FrameStream + Send + 'static>(
    mut conn: C,
    password: &str,
    tx: &Sender<CoreEvent>,
    rx: &Arc<Mutex<Receiver<ControlCommand>>>,
) -> Result<()> {
    conn.set_read_timeout(READ_TIMEOUT)?;

    // ---- 握手 ----
    eprintln!("[viewer-session] 开始握手，发送 Hello 帧…");
    let viewer_random: [u8; 16] = rand::random();
    conn.send_frame(FrameType::Hello, &HelloMsg { viewer_random }.encode())?;
    eprintln!("[viewer-session] 等待 HelloAck…");
    let ack = recv_typed::<_, HelloAckMsg>(&mut conn, FrameType::HelloAck)?;
    eprintln!("[viewer-session] 收到 HelloAck, 发送密码哈希认证…");
    let auth = AuthMsg { hash: password_hash(&ack.salt, password) };
    conn.send_frame(FrameType::Auth, &auth.encode())?;

    // 等待 AUTH_OK 或 AUTH_DENIED（最多 20 秒）
    eprintln!("[viewer-session] 等待 AuthOk 确认…");
    let mut authed = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match conn.recv_frame()? {
            Some(frame) => {
                match frame.ty {
                    FrameType::AuthOk => {
                        authed = true;
                        break;
                    }
                    FrameType::AuthDenied => {
                        let m = AuthDeniedMsg::decode(&frame.payload)?;
                        eprintln!("[viewer-session] ❌ 被控端拒绝认证: {}", m.reason);
                        let _ = tx.send(CoreEvent::AuthResult { ok: false, reason: Some(m.reason) });
                        return Ok(());
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }
    if !authed {
        eprintln!("[viewer-session] ❌ 认证超时(20s 未收到 AuthOk)");
        let _ = tx.send(CoreEvent::AuthResult { ok: false, reason: Some("认证超时".into()) });
        return Ok(());
    }
    eprintln!("[viewer-session] ✅ 认证通过，会话加密建立成功！");
    let _ = tx.send(CoreEvent::AuthResult { ok: true, reason: None });
    let _ = tx.send(CoreEvent::PeerConnected);

    let key = session_key(&viewer_random, &ack.host_random, password);
    let cipher = Arc::new(Mutex::new(Cipher::new(key)));

    let running = Arc::new(AtomicBool::new(true));

    // 发送线程：消费命令（输入帧/停止）+ 周期心跳
    let mut send_conn = conn.try_clone()?;
    let s_cipher = cipher.clone();
    let s_running = running.clone();
    let s_tx = tx.clone();
    let s_rx = rx.clone();
    thread::Builder::new()
        .name("viewer-send".into())
        .spawn(move || -> Result<()> {
            let mut last_hb = Instant::now();
            loop {
                match s_rx.lock().unwrap().recv_timeout(Duration::from_secs(1)) {
                    Ok(ControlCommand::Stop) => {
                        let _ = send_conn.send_frame(FrameType::Goodbye, &[]);
                        let _ = send_conn.shutdown();
                        s_running.store(false, Ordering::SeqCst);
                        let _ = s_tx.send(CoreEvent::PeerDisconnected);
                        break;
                    }
                    Ok(ControlCommand::SendInput(m)) => {
                        let payload = s_cipher.lock().unwrap().seal(&m.encode())?;
                        if let Err(e) = send_conn.send_frame(FrameType::Input, &payload) {
                            if s_running.load(Ordering::SeqCst) {
                                let _ = s_tx.send(CoreEvent::Error(format!("发送失败: {}", e)));
                            }
                            break;
                        }
                    }
                    Ok(ControlCommand::SetQuality(_)) => {} // V1: 画质由被控端决定
                    Err(_) => {
                        if last_hb.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
                            let _ = send_conn.send_frame(FrameType::Heartbeat, &[]);
                            last_hb = Instant::now();
                        }
                        if !s_running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
            Ok(())
        })?;

    // 主线程：接收帧 → UI 事件
    let mut timeouts = 0u32;
    while running.load(Ordering::SeqCst) {
        match conn.recv_frame()? {
            Some(frame) => {
                timeouts = 0;
                match frame.ty {
                    FrameType::Frame => {
                        let payload = cipher.lock().unwrap().open(&frame.payload)?;
                        let m = FrameMsg::decode(&payload)?;
                        let _ = tx.send(CoreEvent::ScreenFrame(m.jpeg));
                    }
                    FrameType::Size => {
                        let payload = cipher.lock().unwrap().open(&frame.payload)?;
                        let m = SizeMsg::decode(&payload)?;
                        let _ = tx.send(CoreEvent::Size { w: m.w, h: m.h });
                    }
                    FrameType::Heartbeat => {}
                    FrameType::Goodbye => {
                        let _ = tx.send(CoreEvent::PeerDisconnected);
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                    _ => {}
                }
            }
            None => {
                timeouts += 1;
                if timeouts >= READ_TIMEOUT_TO_DEAD {
                    let _ = tx.send(CoreEvent::PeerDisconnected);
                    running.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    }
    Ok(())
}

// ---------- 握手辅助 ----------

/// 接收指定类型的一帧并解码（明文握手阶段用）。
fn recv_typed<C: FrameStream, T: HandshakeMsg>(conn: &mut C, ty: FrameType) -> Result<T>
{
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if Instant::now() > deadline {
            bail!("握手超时（等待 {:?}）", ty);
        }
        match conn.recv_frame()? {
            Some(frame) => {
                if frame.ty != ty {
                    bail!("握手顺序错误：期望 {:?} 收到 {:?}", ty, frame.ty);
                }
                return T::decode(&frame.payload);
            }
            None => {}
        }
    }
}

/// 握手消息可解码约束（由各消息类型实现）。
pub trait HandshakeMsg {
    fn decode(b: &[u8]) -> Result<Self>
    where
        Self: Sized;
}

impl HandshakeMsg for HelloMsg {
    fn decode(b: &[u8]) -> Result<Self> {
        HelloMsg::decode(b)
    }
}
impl HandshakeMsg for HelloAckMsg {
    fn decode(b: &[u8]) -> Result<Self> {
        HelloAckMsg::decode(b)
    }
}
impl HandshakeMsg for AuthMsg {
    fn decode(b: &[u8]) -> Result<Self> {
        AuthMsg::decode(b)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_free_udp_port_顺延到空闲端口() {
        // 占住 start 端口，顺延应返回能成功 bind 的空闲端口且 >= start
        let s = std::net::UdpSocket::bind(("0.0.0.0", 0)).unwrap();
        let start = s.local_addr().unwrap().port();
        let got = find_free_udp_port(start);
        assert!(got > start, "被占端口应向后顺延查找空闲端口");
        // 验证返回的端口确实可以成功 bind
        assert!(std::net::UdpSocket::bind(("0.0.0.0", got)).is_ok());
    }

    #[test]
    fn find_free_udp_port_空闲端口直接返回() {
        // 随机选择一个未占用的高位端口段测试，验证 find_free_udp_port 返回可用端口
        let port = find_free_udp_port(45000);
        assert!(port >= 45000 && port <= 45020);
        assert!(std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok());
    }
}
