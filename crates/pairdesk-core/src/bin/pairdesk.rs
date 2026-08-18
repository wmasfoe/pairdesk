//! PairDesk CLI — M0 阶段的无界面验证工具。
//!
//! 用法：
//! ```text
//!   pairdesk serve --port 8888 --password 123456
//!   pairdesk connect 127.0.0.1:8888 --password 123456 --frames 5 --dump-dir /tmp/frames
//! ```
//! `connect` 收满 N 帧后自动退出；`serve` 运行至 Ctrl+C。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use pairdesk_core::protocol::InputMsg;
use pairdesk_core::{CoreEvent, CoreHandle, Quality};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }
    let result = match args[1].as_str() {
        "serve" => run_host(&args[2..]),
        "connect" => run_viewer(&args[2..]),
        "help" | "--help" | "-h" => {
            print_usage();
            return;
        }
        "version" | "--version" | "-V" => {
            println!("pairdesk {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        other => {
            eprintln!("未知子命令: {}", other);
            print_usage();
            return;
        }
    };
    if let Err(e) = result {
        eprintln!("错误: {:#}", e);
        std::process::exit(1);
    }
}

fn print_usage() {
    println!(
        "PairDesk CLI v{}
用法:
  pairdesk serve [--port 8888] [--password xxx] [--fps 20] [--jpeg 75]
     启动被控端,监听端口等待连接
  pairdesk connect <ip:port> [--password xxx] [--frames 10] [--dump-dir DIR]
     连接被控端,收 N 帧画面;带 --test-input 时发送一次鼠标移动+点击验证输入链路
     例: pairdesk connect 127.0.0.1:8888 --password 123456 --frames 5 --test-input 100,100
  pairdesk version | help",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let k = args[i].clone();
        if k.starts_with("--") {
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                m.insert(k.trim_start_matches("--").to_string(), args[i + 1].clone());
                i += 2;
            } else {
                m.insert(k.trim_start_matches("--").to_string(), "true".to_string());
                i += 1;
            }
        } else {
            m.insert("pos".to_string(), k);
            i += 1;
        }
    }
    m
}

/// 被控端。
fn run_host(args: &[String]) -> anyhow::Result<()> {
    let a = parse_args(args);
    let port: u16 = a.get("port").and_then(|v| v.parse().ok()).unwrap_or(8888);
    let password = a.get("password").cloned().unwrap_or_else(|| "123456".into());
    let fps: u32 = a.get("fps").and_then(|v| v.parse().ok()).unwrap_or(20);
    let jpeg: u8 = a.get("jpeg").and_then(|v| v.parse().ok()).unwrap_or(75);
    // 中继模式：提供 --relay <地址> 与 --sid <会话码> 时走中继，否则直连
    let relay_opt = a
        .get("relay")
        .map(|s| s.parse::<std::net::SocketAddr>().map_err(|e| anyhow::anyhow!(e)));
    let sid_opt = a.get("sid").cloned();
    // 打洞 QUIC 端口（被控端用它接收异网直连；可选，默认 8889）
    let hole_port: u16 = a.get("hole-port").and_then(|v| v.parse().ok()).unwrap_or(8889);

    // 自动就绪模式：serve --relay --sid --auto（同起 QUIC 打洞 + 中继兜底）
    if a.contains_key("auto") {
        if let (Some(Ok(relay)), Some(sid)) = (&relay_opt, &sid_opt) {
            println!("[被控端] 自动就绪(QUIC打洞+中继) relay {} sid {} hole {} 密码:{} 品质:jpeg={} fps={}", relay, sid, hole_port, password, jpeg, fps);
            let (handle, rx) = CoreHandle::start_host_auto(*relay, sid.clone(), hole_port, password)?;
            handle.set_quality(Quality { jpeg, fps });
            return event_pump(&rx);
        }
    }
    if let (Some(Ok(relay)), Some(sid)) = (relay_opt, sid_opt) {
        println!("[被控端] 经中继 {} 会话 {} 打洞端口 {} 密码:{} 品质:jpeg={} fps={}", relay, sid, hole_port, password, jpeg, fps);
        let (handle, rx) = CoreHandle::start_host_via_relay(relay, sid, hole_port, password)?;
        handle.set_quality(Quality { jpeg, fps });
        return event_pump(&rx);
    }
    // QUIC 打洞直连模式：serve --quic-port <port>（被控端在异网打洞端口等 viewer）
    if let Some(qp) = a.get("quic-port").and_then(|v| v.parse::<u16>().ok()) {
        println!("[被控端] QUIC 打洞直连 端口 {} 密码:{} 品质:jpeg={} fps={}", qp, password, jpeg, fps);
        let (handle, rx) = CoreHandle::start_host_via_quic(qp, password)?;
        handle.set_quality(Quality { jpeg, fps });
        return event_pump(&rx);
    }
    println!("[被控端] 直连监听 0.0.0.0:{} 密码:{} 品质:jpeg={} fps={}", port, password, jpeg, fps);
    let (handle, rx) = CoreHandle::start_host(port, password)?;
    handle.set_quality(Quality { jpeg, fps });
    event_pump(&rx)
}

/// 事件泵：打印关键事件直到通道断开。
fn event_pump(rx: &std::sync::mpsc::Receiver<CoreEvent>) -> anyhow::Result<()> {
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(ev) => match ev {
                CoreEvent::PeerConnected => println!("[被控端] ◀ 对端已连接"),
                CoreEvent::PeerDisconnected => println!("[被控端] ⏹ 对端断开"),
                CoreEvent::AuthResult { ok, reason } => {
                    println!("[被控端] 🔑 认证 {}", if ok { "成功" } else { "失败" });
                    if let Some(r) = reason {
                        println!("[被控端]   原因: {}", r);
                    }
                }
                CoreEvent::Error(e) => println!("[被控端] ❌ {}", e),
                CoreEvent::Transport(t) => println!("[被控端] 传输路径: {}", t),
                _ => {}
            },
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => {}
        }
    }
    Ok(())
}

/// 控制端。
fn run_viewer(args: &[String]) -> anyhow::Result<()> {
    let mut a = parse_args(args);
    let addr: SocketAddr = a
        .remove("pos")
        .ok_or_else(|| anyhow::anyhow!("缺少目标地址,例: connect 127.0.0.1:8888"))?
        .parse()?;
    let password = a.get("password").cloned().unwrap_or_else(|| "123456".into());
    let frames: u32 = a.get("frames").and_then(|v| v.parse().ok()).unwrap_or(10);
    let dump_dir = a.get("dump-dir").map(PathBuf::from);
    let test_input = a.get("test-input").cloned();
    // 中继相关参数（auto/quic 模式都可用）
    let relay_opt = a.get("relay").map(|s| {
        s.parse::<std::net::SocketAddr>()
            .map_err(|e| anyhow::anyhow!(e))
    });
    let sid_opt = a.get("sid").cloned();
    // 自动择一模式：connect --relay --sid --auto（先 QUIC 打洞，失败自动中继兜底）
    if a.contains_key("auto") {
        if let (Some(Ok(relay)), Some(sid)) = (&relay_opt, &sid_opt) {
            println!("[控制端] 自动择一(QUIC打洞→中继) relay {} sid {} 密码:{} 收 {} 帧", relay, sid, password, frames);
            let (handle, rx) = CoreHandle::connect_auto(*relay, sid.clone(), password)?;
            return run_viewer_loop(&handle, &rx, frames, dump_dir, test_input);
        }
    }
    // QUIC 打洞直连模式：connect --quic <hole-addr>（pos 给占位即可）
    if let Some(hole_str) = a.get("quic").cloned() {
        let hole: SocketAddr = hole_str.parse()?;
        println!("[控制端] QUIC 直连 {} 密码:{} 收 {} 帧", hole, password, frames);
        let (handle, rx) = CoreHandle::connect_via_quic(hole, password)?;
        return run_viewer_loop(&handle, &rx, frames, dump_dir, test_input);
    }
    // 中继模式：提供 --relay 与 --sid 时经中继
    if let (Some(Ok(relay)), Some(sid)) = (relay_opt, sid_opt) {
        println!("[控制端] 经中继 {} 会话 {} 密码:{} 收 {} 帧", relay, sid, password, frames);
        let (handle, rx) = CoreHandle::connect_via_relay(relay, sid, password)?;
        return run_viewer_loop(&handle, &rx, frames, dump_dir, test_input);
    }
    println!("[控制端] 直连 {} 密码:{} 收 {} 帧", addr, password, frames);
    let (handle, rx) = CoreHandle::connect(addr, password)?;
    run_viewer_loop(&handle, &rx, frames, dump_dir, test_input)
}

/// 控制端会话主循环：收帧/验证输入/错误处理（直连与中继共用）。
fn run_viewer_loop(
    handle: &CoreHandle,
    rx: &std::sync::mpsc::Receiver<CoreEvent>,
    frames: u32,
    dump_dir: Option<PathBuf>,
    test_input: Option<String>,
) -> anyhow::Result<()> {
    let mut got = 0u32;
    let mut authed = false;
    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(ev) => match ev {
                CoreEvent::AuthResult { ok, reason } => {
                    authed = ok;
                    println!("[控制端] 🔑 认证 {}", if ok { "成功" } else { "失败" });
                    if let Some(r) = reason {
                        println!("[控制端]   原因: {}", r);
                        return Ok(());
                    }
                    // 认证成功后,可选发一次测试输入验证反向链路
                    if let Some(ti) = &test_input {
                        if let Some((x, y)) = ti.split_once(',') {
                            let x: f64 = x.trim().parse()?;
                            let y: f64 = y.trim().parse()?;
                            println!("[控制端] 🖱 发送测试输入: 移动到 ({},{}) + 左键点击", x, y);
                            handle.send_input(InputMsg::MouseMove { x, y });
                            std::thread::sleep(Duration::from_millis(200));
                            handle.send_input(InputMsg::Button { btn: 1, down: true });
                            std::thread::sleep(Duration::from_millis(50));
                            handle.send_input(InputMsg::Button { btn: 1, down: false });
                        }
                    }
                }
                CoreEvent::PeerConnected => println!("[控制端] ▶ 已建立连接"),
                CoreEvent::Size { w, h } => println!("[控制端] 📐 远端屏幕 {}x{}", w, h),
                CoreEvent::ScreenFrame(jpeg) => {
                    got += 1;
                    if let Some(dir) = &dump_dir {
                        std::fs::create_dir_all(dir)?;
                        let p = dir.join(format!("frame-{:04}.jpg", got));
                        std::fs::write(&p, &jpeg)?;
                        println!("[控制端] 📸 第 {} 帧已存: {}", got, p.display());
                    } else {
                        println!("[控制端] 📸 第 {} 帧 ({:.1} KB)", got, jpeg.len() as f64 / 1024.0);
                    }
                    if got >= frames {
                        println!("[控制端] ✅ 收到 {} 帧,测试完成", got);
                        handle.stop();
                        return Ok(());
                    }
                }
                CoreEvent::PeerDisconnected => {
                    println!("[控制端] ⏹ 远端断开");
                    break;
                }
                CoreEvent::Error(e) => println!("[控制端] ❌ {}", e),
                CoreEvent::Transport(t) => println!("[控制端] 传输路径: {}", t),
                _ => {}
            },
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !authed {
                    println!("[控制端] ⏳ 等待认证结果超时…");
                }
            }
        }
    }
    Ok(())
}