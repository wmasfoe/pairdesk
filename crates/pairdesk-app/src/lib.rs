//! PairDesk 桌面壳入口。
//!
//! Tauri 2 壳：打开窗口、承载内嵌前端，并通过 `invoke_handler` 把前端的动作
//! 粘到 `pairdesk-core`（起被控端 / 起控制端 / 收帧 / 发输入），见 [`bridge`]。
//!
//! 用户模型：被控端生成会话码 + 设密码 + 开「允许远程控制」开关；
//! 控制端输入 会话码 + 密码，程序自动择一（同网直连 / QUIC 打洞 / 中继兜底）。
//!
//! 自测模式：`PD_ROLE=host|viewer`（配 `PD_RELAY`/`PD_SID`/`PD_PASSWORD`/
//! `PD_HOLE`/`PD_DUMP_DIR`）启动时，不经 UI 直接起会话并把画面帧落盘，
//! 用于无头验证"壳 + 内核"全链路。

mod bridge;

use std::path::PathBuf;
use tauri::Manager;

fn selftest_from_env(app: &tauri::AppHandle) {
    use std::env;
    let role = match env::var("PD_ROLE") {
        Ok(r) => r,
        Err(_) => return,
    };
    // 自测模式：隐藏主窗口，避免 App 自己的窗口盖住被采集的"远程屏幕"内容
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    let relay = match env::var("PD_RELAY")
        .ok()
        .and_then(|s| s.parse::<std::net::SocketAddr>().ok())
    {
        Some(a) => a,
        _ => return,
    };
    let sid = env::var("PD_SID").unwrap_or_default();
    let pwd = env::var("PD_PASSWORD").unwrap_or_default();
    if let Some(dir) = env::var("PD_DUMP_DIR").ok() {
        std::fs::create_dir_all(&dir).ok();
        bridge::selftest_set_dump(app, PathBuf::from(dir));
    }
    match role.as_str() {
        "host" => {
            let hole = env::var("PD_HOLE").ok().and_then(|v| v.parse().ok()).unwrap_or(23517);
            bridge::selftest_host(app, relay, sid, hole, pwd);
        }
        "viewer" => bridge::selftest_viewer(app, relay, sid, pwd),
        _ => {}
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(bridge::AppState::default())
        .invoke_handler(tauri::generate_handler![
            bridge::pd_set_allowed,
            bridge::pd_start_host_auto,
            bridge::pd_connect_auto,
            bridge::pd_stop,
            bridge::pd_send_input
        ])
        .setup(|app| {
            selftest_from_env(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("运行 PairDesk 桌面壳出错");
}
