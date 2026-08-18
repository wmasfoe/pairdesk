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
use crate::transport::{accept_once, connect, Connection};
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

/// 被控端经中继登记：连 relay → 注册 sid(+打洞端口) → 等 viewer，随后走单会话。
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
            let result = (|| -> Result<()> {
                let conn = relay::register_host(relay, &sid, hole_port)?;
                let _ = host_session_once(conn, &password, &event_tx, &cmd_rx);
                Ok(())
            })();
            if let Err(e) = result {
                let _ = event_tx.send(CoreEvent::Error(format!("中继被控端: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

/// 控制端经中继匹配：连 relay → 按 sid 匹配 host → 拿打洞端点 → 走握手。
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
                let (conn, hole) = relay::connect_viewer(relay, &sid)?;
                let _ = event_tx.send(CoreEvent::SignalHole(hole)); // 上报打洞端点(供后续 QUIC 直连)
                viewer_session_with_conn(conn, &password, &event_tx, &cmd_rx)
            })();
            if let Err(e) = result {
                let _ = event_tx.send(CoreEvent::Error(format!("中继控制端: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

// ---------- QUIC 直连（异网打洞后的 P2P 传输） ----------

/// 被控端 QUIC 直连：在 hole_port 起 QUIC server，等 viewer 连接后跑会话。
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
            let result = (|| -> Result<()> {
                let id = crate::certs::ensure_identity(&identity_dir())?;
                let rt = Arc::new(tokio::runtime::Runtime::new()?);
                let (send, recv) = rt.block_on(async move {
                    let server = quinn::Endpoint::server(
                        crate::certs::server_quic_config(&id)?,
                        (std::net::Ipv4Addr::UNSPECIFIED, hole_port).into(),
                    )?;
                    let inc = server
                        .accept()
                        .await
                        .ok_or_else(|| anyhow::anyhow!("QUIC server 关闭"))?;
                    let conn = inc.await?;
                    conn.accept_bi().await.map_err(anyhow::Error::from)
                })?;
                let fs = crate::quic_frame::QuicFrameStream::new(rt, send, recv);
                let _ = host_session_once(fs, &password, &event_tx, &cmd_rx);
                Ok(())
            })();
            if let Err(e) = result {
                let _ = event_tx.send(CoreEvent::Error(format!("QUIC 被控端: {}", e)));
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

// ---------- 自动择一（用户只给 relay+sid+密码，系统无感择优） ----------

/// 被控端自动就绪：同时起 QUIC 打洞 server + 中继注册，任一被连即成为会话。
/// 返回主句柄(命令广播到各路子会话)与聚合事件流。
pub fn spawn_host_auto(
    relay: SocketAddr,
    sid: String,
    hole_port: u16,
    password: String,
) -> Result<(crate::CoreHandle, Receiver<CoreEvent>)> {
    let (event_tx, event_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    let cmd_rx = Arc::new(Mutex::new(cmd_rx));
    let h = crate::CoreHandle::from_tx(cmd_tx);

    // 两路子会话各自就绪（QUIC 打洞 + 中继兜底）
    let (qh, qrx) = spawn_host_via_quic(hole_port, password.clone())?;
    let (rh, rrx) = spawn_host_via_relay(relay, sid, hole_port, password)?;

    // 子事件 → 主事件聚合
    let e1 = event_tx.clone();
    thread::Builder::new().name("host-auto-fwd-q".into()).spawn(move || {
        for e in qrx {
            if e1.send(e).is_err() {
                break;
            }
        }
    })?;
    thread::Builder::new().name("host-auto-fwd-r".into()).spawn(move || {
        for e in rrx {
            if event_tx.send(e).is_err() {
                break;
            }
        }
    })?;
    // 主命令 → 广播到两路
    let cmd_rx = Arc::clone(&cmd_rx);
    thread::Builder::new().name("host-auto-cmd".into()).spawn(move || loop {
        let c = match cmd_rx.lock().unwrap().recv() {
            Ok(c) => c,
            Err(_) => break,
        };
        let _ = qh.tx().send(c.clone());
        let _ = rh.tx().send(c);
    })?;

    Ok((h, event_rx))
}

/// 控制端自动择一：经 relay 信令拿打洞端点 → 先试 QUIC 打洞直连，
/// 失败自动降级到中继兜底（对用户无感）。
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
                let (relay_conn, hole) = relay::connect_viewer(relay, &sid)?;
                // ① 先试 QUIC 打洞直连
                match try_quic_connect(hole) {
                    Ok(fs) => {
                        let _ = event_tx.send(CoreEvent::Transport("QUIC 打洞直连".into()));
                        viewer_session_with_conn(fs, &password, &event_tx, &cmd_rx)
                    }
                    Err(e) => {
                        let _ = event_tx.send(CoreEvent::Transport(format!("中继兜底 ({})", e)));
                        viewer_session_with_conn(relay_conn, &password, &event_tx, &cmd_rx)
                    }
                }
            })();
            if let Err(e) = result {
                let _ = event_tx.send(CoreEvent::Error(format!("自动择一控制端: {}", e)));
            }
        })?;
    Ok((h, event_rx))
}

/// 尝试以 QUIC 直连到打洞端点（带超时），成功返回同步帧流。
fn try_quic_connect(hole: SocketAddr) -> Result<crate::quic_frame::QuicFrameStream> {
    let id = crate::certs::ensure_identity(&identity_dir())?;
    let cfg = crate::certs::client_quic_config(&id)?;
    let rt = Arc::new(tokio::runtime::Runtime::new()?);
    let res = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(4), async {
            let mut ep = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
            ep.set_default_client_config(cfg);
            let conn = ep.connect(hole, "pairdesk.local")?.await?;
            conn.open_bi().await.map_err(anyhow::Error::from)
        })
        .await
    });
    let (send, recv) = res.map_err(|_| anyhow::anyhow!("QUIC 打洞超时"))??;
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
    let hello = recv_typed::<_, HelloMsg>(&mut conn, FrameType::Hello)?;
    let host_random: [u8; 16] = rand::random();
    let salt: [u8; 32] = rand::random();
    conn.send_frame(FrameType::HelloAck, &HelloAckMsg { host_random, salt }.encode())?;

    let auth = recv_typed::<_, AuthMsg>(&mut conn, FrameType::Auth)?;
    let expect = password_hash(&salt, password);
    if auth.hash != expect {
        conn.send_frame(FrameType::AuthDenied, &AuthDeniedMsg { reason: "密码错误".into() }.encode())?;
        let _ = tx.send(CoreEvent::AuthResult { ok: false, reason: Some("密码错误".into()) });
        return Ok(());
    }
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
                        let payload = r_cipher.lock().unwrap().open(&frame.payload)?;
                        match frame.ty {
                            FrameType::Input => {
                                let msg = InputMsg::decode(&payload)?;
                                apply_input(&mut injector, msg)?;
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
                match c_rx.lock().unwrap().recv_timeout(Duration::from_secs(5)) {
                    Ok(ControlCommand::Stop) => {
                        let _ = c_conn.send_frame(FrameType::Goodbye, &[]);
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
    let viewer_random: [u8; 16] = rand::random();
    conn.send_frame(FrameType::Hello, &HelloMsg { viewer_random }.encode())?;
    let ack = recv_typed::<_, HelloAckMsg>(&mut conn, FrameType::HelloAck)?;
    let auth = AuthMsg { hash: password_hash(&ack.salt, password) };
    conn.send_frame(FrameType::Auth, &auth.encode())?;

    // 等待 AUTH_OK 或 AUTH_DENIED（最多 20 秒）
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
        let _ = tx.send(CoreEvent::AuthResult { ok: false, reason: Some("认证超时".into()) });
        return Ok(());
    }
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
                let payload = cipher.lock().unwrap().open(&frame.payload)?;
                match frame.ty {
                    FrameType::Frame => {
                        let m = FrameMsg::decode(&payload)?;
                        let _ = tx.send(CoreEvent::ScreenFrame(m.jpeg));
                    }
                    FrameType::Size => {
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