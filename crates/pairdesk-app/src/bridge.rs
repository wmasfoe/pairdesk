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
//!
//! 自测模式：App 以 `PD_ROLE=host|viewer` 等环境变量启动时，不经 UI 直接起会话，
//! 并把控制端收到的画面帧写盘（`PD_DUMP_DIR`），用于无头验证"壳 + 内核"全链路。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};

use pairdesk_core::protocol::InputMsg;
use pairdesk_core::{CoreEvent, CoreHandle, Quality};

/// 应用级共享状态。
pub struct AppState {
    /// 当前活动会话句柄（一次只跑一个会话）
    pub handle: Mutex<Option<CoreHandle>>,
    /// 允许远程控制总开关（默认关，需用户手动打开）
    pub allowed: AtomicBool,
    /// 画面帧落盘目录（自测用；None 则只发事件不写盘）
    pub dump: Mutex<Option<PathBuf>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            handle: Mutex::new(None),
            allowed: AtomicBool::new(false),
            dump: Mutex::new(None),
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

/// 事件泵：内核事件 → 前端事件流；可选的画面帧落盘（自测）。
pub fn pump_events(app: &AppHandle, rx: std::sync::mpsc::Receiver<CoreEvent>, dump: Option<PathBuf>) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("pd-event-fwd".into())
        .spawn(move || {
            let mut n = 0u32;
            while let Ok(ev) = rx.recv() {
                if let Some(dir) = &dump {
                    if let CoreEvent::ScreenFrame(jpeg) = &ev {
                        let _ = std::fs::write(dir.join(format!("frame-{:04}.jpg", n + 1)), jpeg);
                        n += 1;
                    }
                }
                let _ = app.emit("core://event", core_event_to_json(&ev));
            }
        })
        .expect("spawn event forwarder");
}

/// 启动一个会话：存句柄 + 起事件转发线程。
fn start_session(
    state: &AppState,
    app: &AppHandle,
    handle: CoreHandle,
    rx: std::sync::mpsc::Receiver<CoreEvent>,
) -> Result<(), String> {
    if let Some(old) = state.handle.lock().unwrap().take() {
        old.stop();
    }
    *state.handle.lock().unwrap() = Some(handle);
    let dump = state.dump.lock().unwrap().clone();
    pump_events(app, rx, dump);
    Ok(())
}

/// 起被控端（模式：relay / quic / auto / direct）。
#[tauri::command]
pub fn pd_start_host(
    state: State<'_, AppState>,
    app: AppHandle,
    mode: String,
    relay: String,
    sid: String,
    hole_port: u16,
    password: String,
) -> Result<(), String> {
    if !state.allowed.load(Ordering::SeqCst) {
        return Err("「允许远程控制」开关是关闭的".into());
    }
    let (h, rx) = match mode.as_str() {
        "quic" => CoreHandle::start_host_via_quic(hole_port, password).map_err(|e| e.to_string())?,
        "direct" => CoreHandle::start_host(hole_port, password).map_err(|e| e.to_string())?,
        "auto" => {
            let addr = parse_addr(&relay)?;
            CoreHandle::start_host_auto(addr, sid, hole_port, password).map_err(|e| e.to_string())?
        }
        _ => {
            // 默认 relay
            let addr = parse_addr(&relay)?;
            CoreHandle::start_host_via_relay(addr, sid, hole_port, password).map_err(|e| e.to_string())?
        }
    };
    h.set_quality(Quality { jpeg: 80, fps: 20 });
    start_session(&state, &app, h, rx)
}

/// 兼容旧命令
#[tauri::command]
pub fn pd_start_host_auto(
    state: State<'_, AppState>,
    app: AppHandle,
    relay: String,
    sid: String,
    hole_port: u16,
    password: String,
) -> Result<(), String> {
    pd_start_host(state, app, "auto".into(), relay, sid, hole_port, password)
}

/// 起控制端（模式：relay / quic / auto / direct）。
#[tauri::command]
pub fn pd_connect(
    state: State<'_, AppState>,
    app: AppHandle,
    mode: String,
    target: String,
    sid: String,
    password: String,
) -> Result<(), String> {
    let (h, rx) = match mode.as_str() {
        "quic" => {
            let addr = parse_addr(&target)?;
            CoreHandle::connect_via_quic(addr, password).map_err(|e| e.to_string())?
        }
        "direct" => {
            let addr = parse_addr(&target)?;
            CoreHandle::connect(addr, password).map_err(|e| e.to_string())?
        }
        "auto" => {
            let addr = parse_addr(&target)?;
            CoreHandle::connect_auto(addr, sid, password).map_err(|e| e.to_string())?
        }
        _ => {
            // 默认 relay
            let addr = parse_addr(&target)?;
            CoreHandle::connect_via_relay(addr, sid, password).map_err(|e| e.to_string())?
        }
    };
    start_session(&state, &app, h, rx)
}

/// 兼容旧命令
#[tauri::command]
pub fn pd_connect_auto(
    state: State<'_, AppState>,
    app: AppHandle,
    relay: String,
    sid: String,
    password: String,
) -> Result<(), String> {
    pd_connect(state, app, "auto".into(), relay, sid, password)
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

/// 检查平台权限状态（macOS 屏幕录制 / 辅助功能权限）。
#[tauri::command]
pub fn pd_check_permissions() -> pairdesk_core::permissions::PermissionStatus {
    pairdesk_core::permissions::check_permissions()
}

/// 请求权限（如触发 macOS 屏幕录制系统弹窗）。
#[tauri::command]
pub fn pd_request_permission(permission_type: String) -> bool {
    pairdesk_core::permissions::request_permission(&permission_type)
}

/// 打开系统隐私与安全设置面板。
#[tauri::command]
pub fn pd_open_permission_settings(permission_type: String) {
    pairdesk_core::permissions::open_permission_settings(&permission_type);
}

/// 检查更新信息
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_notes: String,
    pub download_url: String,
}

/// 检查是否有新版本（请求 GitHub Releases API）
#[tauri::command]
pub async fn pd_check_update() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let url = "https://api.github.com/repos/wmasfoe/pairdesk/releases/latest";
    let client = reqwest::Client::builder()
        .user_agent("PairDesk-App")
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("GitHub API 返回状态码: {}", res.status()));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("解析更新响应失败: {e}"))?;

    let tag_name = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    let release_notes = json["body"].as_str().unwrap_or("").to_string();

    let has_update = !tag_name.is_empty() && tag_name != current_version;

    // 匹配当前平台下载资产
    let target_pattern = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "aarch64.dmg"
        } else {
            "x86_64.dmg"
        }
    } else if cfg!(target_os = "windows") {
        ".msi"
    } else {
        ".AppImage"
    };

    let download_url = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str().unwrap_or("");
                if name.contains(target_pattern) {
                    a["browser_download_url"].as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| json["html_url"].as_str().unwrap_or("").to_string());

    Ok(UpdateInfo {
        current_version,
        latest_version: tag_name,
        has_update,
        release_notes,
        download_url,
    })
}

/// 打开系统默认浏览器访问下载/发布页
#[tauri::command]
pub fn pd_open_url(url: String) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", &url]).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
}

/// 重启应用：macOS 的 TCC 权限结果按进程缓存（AXIsProcessTrusted /
/// CGPreflightScreenCaptureAccess 一旦在本进程读到 false 就持续返回 false），
/// 授权后必须用全新进程重新查询 tccd 才能读到新状态。
#[tauri::command]
pub fn pd_restart_app(app: AppHandle) {
    tauri::process::restart(&app.env());
}

// ---------- 自测模式（无头验证"壳 + 内核"全链路） ----------

/// 自测：设置画面帧落盘目录。
pub fn selftest_set_dump(app: &AppHandle, dir: PathBuf) {
    *app.state::<AppState>().dump.lock().unwrap() = Some(dir);
}

/// 自测：起被控端（绕过 UI 允许开关，代表用户已允许）。
pub fn selftest_host(
    app: &AppHandle,
    relay: SocketAddr,
    sid: String,
    hole: u16,
    password: String,
) {
    let state = app.state::<AppState>();
    state.allowed.store(true, Ordering::SeqCst);
    let dump = state.dump.lock().unwrap().clone();
    if let Some(old) = state.handle.lock().unwrap().take() {
        old.stop();
    }
    match CoreHandle::start_host_auto(relay, sid.clone(), hole, password) {
        Ok((h, rx)) => {
            h.set_quality(Quality { jpeg: 80, fps: 20 });
            *state.handle.lock().unwrap() = Some(h);
            pump_events(app, rx, dump);
            eprintln!("[selftest] 被控端已启动 sid={sid}");
        }
        Err(e) => eprintln!("[selftest-host] 失败: {e}"),
    }
}

/// 自测：起控制端（自动择一）。
pub fn selftest_viewer(app: &AppHandle, relay: SocketAddr, sid: String, password: String) {
    let state = app.state::<AppState>();
    let dump = state.dump.lock().unwrap().clone();
    if let Some(old) = state.handle.lock().unwrap().take() {
        old.stop();
    }
    match CoreHandle::connect_auto(relay, sid.clone(), password) {
        Ok((h, rx)) => {
            *state.handle.lock().unwrap() = Some(h);
            pump_events(app, rx, dump);
            eprintln!("[selftest] 控制端已启动 sid={sid}");
        }
        Err(e) => eprintln!("[selftest-viewer] 失败: {e}"),
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
        CoreEvent::Notice(msg) => json!({ "type": "notice", "message": msg }),
        CoreEvent::PeerConnected => json!({ "type": "peerConnected" }),
        CoreEvent::PeerDisconnected => json!({ "type": "peerDisconnected" }),
        CoreEvent::AuthResult { ok, reason } => {
            json!({ "type": "authResult", "ok": ok, "reason": reason })
        }
        CoreEvent::SignalHole(addr) => json!({ "type": "signalHole", "addr": addr.to_string() }),
        CoreEvent::Transport(path) => json!({ "type": "transport", "path": path }),
    }
}
