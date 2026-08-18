//! Tauri IPC 桥：把前端动作粘到 `pairdesk-core`，并把内核事件推回前端。
//!
//! 命令（前端 `app/src/bridge/tauri.ts` 对应调用）：
//!  - `pd_set_allowed`   允许远程控制总开关（关掉则拒绝起被控端）
//!  - `pd_start_host_auto` 起被控端（自动就绪：QUIC 打洞 + 中继）
//!  - `pd_connect_auto`   起控制端（自动择一：QUIC 打洞优先 → 中继兜底）
//!  - `pd_stop`           停止当前会话
//!  - `pd_send_input`     控制端注入输入
//!
//! 事件：内核 `CoreEvent` 统一以 `core://event` 推给前端。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use pairdesk_core::protocol::InputMsg;
use pairdesk_core::{CoreEvent, CoreHandle, Quality};

/// 应用级共享状态。
pub struct AppState {
    /// 当前活动会话句柄（一次只跑一个会话）
    pub handle: Mutex<Option<CoreHandle>>,
    /// 允许远程控制总开关（默认关，需用户手动打开）
    pub allowed: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            handle: Mutex::new(None),
            allowed: AtomicBool::new(false),
        }
    }
}

fn parse_addr(s: &str) -> Result<SocketAddr, String> {
    s.parse().map_err(|e| format!("地址无效 {}: {e}", s))
}

/// 设置"允许远程控制"开关。
#[tauri::command]
pub fn pd_set_allowed(state: State<'_, AppState>, allowed: bool) {
    state.allowed.store(allowed, Ordering::SeqCst);
}

/// 启动一个会话（被控/控制端都走这里）：存句柄 + 起事件转发线程。
fn start_session(
    state: &AppState,
    app: &AppHandle,
    handle: CoreHandle,
    rx: std::sync::mpsc::Receiver<CoreEvent>,
) -> Result<(), String> {
    // 若已有会话，先停旧的
    if let Some(old) = state.handle.lock().unwrap().take() {
        old.stop();
    }
    *state.handle.lock().unwrap() = Some(handle);
    // 事件泵：内核事件 → 前端
    let app = app.clone();
    std::thread::Builder::new()
        .name("pd-event-fwd".into())
        .spawn(move || {
            while let Ok(ev) = rx.recv() {
                let payload = core_event_to_json(&ev);
                let _ = app.emit("core://event", payload);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 起被控端（自动就绪：QUIC 打洞 + 中继兜底）。
#[tauri::command]
pub fn pd_start_host_auto(
    state: State<'_, AppState>,
    app: AppHandle,
    relay: String,
    sid: String,
    hole_port: u16,
    password: String,
) -> Result<(), String> {
    if !state.allowed.load(Ordering::SeqCst) {
        return Err("「允许远程控制」开关是关闭的".into());
    }
    let addr = parse_addr(&relay)?;
    let (h, rx) = CoreHandle::start_host_auto(addr, sid, hole_port, password)
        .map_err(|e| e.to_string())?;
    h.set_quality(Quality { jpeg: 80, fps: 20 });
    start_session(&state, &app, h, rx)
}

/// 起控制端（自动择一：QUIC 打洞优先 → 中继兜底）。
#[tauri::command]
pub fn pd_connect_auto(
    state: State<'_, AppState>,
    app: AppHandle,
    relay: String,
    sid: String,
    password: String,
) -> Result<(), String> {
    let addr = parse_addr(&relay)?;
    let (h, rx) = CoreHandle::connect_auto(addr, sid, password).map_err(|e| e.to_string())?;
    start_session(&state, &app, h, rx)
}

/// 停止当前会话。
#[tauri::command]
pub fn pd_stop(state: State<'_, AppState>) {
    if let Some(h) = state.handle.lock().unwrap().take() {
        h.stop();
    }
}

/// 控制端注入输入事件。
#[tauri::command]
pub fn pd_send_input(state: State<'_, AppState>, msg: InputCmd) {
    if let Some(h) = state.handle.lock().unwrap().as_ref() {
        match msg {
            InputCmd::MouseMove { x, y } => h.send_input(InputMsg::MouseMove { x, y }),
            InputCmd::Button { btn, down } => h.send_input(InputMsg::Button { btn, down }),
            InputCmd::Scroll { dx, dy } => h.send_input(InputMsg::Scroll { dx, dy }),
            InputCmd::Key {
                keycode,
                down,
                mods,
            } => h.send_input(InputMsg::Key {
                keycode,
                down,
                mods,
            }),
        }
    }
}

/// 前端输入序列化的扁平结构（`kind` 为 Rust 变体名，如 MouseMove/Button/Scroll/Key）。
#[derive(Deserialize)]
#[serde(tag = "kind")]
pub enum InputCmd {
    MouseMove { x: f64, y: f64 },
    Button { btn: u8, down: bool },
    Scroll { dx: f64, dy: f64 },
    Key { keycode: u32, down: bool, mods: u32 },
}

/// 把内核事件映射为前端可消费的 JSON（`type` 字段区分事件类别）。
fn core_event_to_json(ev: &CoreEvent) -> serde_json::Value {
    use serde_json::json;
    match ev {
        CoreEvent::ScreenFrame(jpeg) => json!({ "type": "frame", "jpeg": jpeg }),
        CoreEvent::Size { w, h } => json!({ "type": "size", "w": w, "h": h }),
        CoreEvent::Stats { fps, kbps, ping_ms } => {
            json!({ "type": "stats", "fps": fps, "kbps": kbps, "pingMs": ping_ms })
        }
        CoreEvent::Error(e) => json!({ "type": "error", "message": e }),
        CoreEvent::PeerConnected => json!({ "type": "peerConnected" }),
        CoreEvent::PeerDisconnected => json!({ "type": "peerDisconnected" }),
        CoreEvent::AuthResult { ok, reason } => {
            json!({ "type": "authResult", "ok": ok, "reason": reason })
        }
        CoreEvent::SignalHole(addr) => json!({ "type": "signalHole", "addr": addr.to_string() }),
        CoreEvent::Transport(path) => json!({ "type": "transport", "path": path }),
    }
}
