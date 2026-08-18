//! PairDesk 桌面壳入口。
//!
//! Tauri 2 壳：打开窗口、承载内嵌前端，并通过 `invoke_handler` 把前端的动作
//! 粘到 `pairdesk-core`（起被控端 / 起控制端 / 收帧 / 发输入），见 [`bridge`]。
//!
//! 用户模型：被控端生成会话码 + 设密码 + 开「允许远程控制」开关；
//! 控制端输入 会话码 + 密码，程序自动择一（同网直连 / QUIC 打洞 / 中继兜底）。

mod bridge;

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
        .run(tauri::generate_context!())
        .expect("运行 PairDesk 桌面壳出错");
}
