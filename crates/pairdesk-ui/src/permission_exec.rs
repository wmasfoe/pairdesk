//! 权限动作的执行器。
//!
//! # 为什么单独成文件
//!
//! 领域层（`pairdesk-shell`）只产出 [`PermissionAction`] 意图，不执行——
//! 因为执行有副作用（拉起系统设置、杀掉进程），会让单测污染真实环境。
//!
//! 本模块是适配器层的对应部分：把意图翻译成真实的系统调用。
//! 这样职责清晰：
//!
//! - `pairdesk-shell::permissions`  纯函数，可测无副作用
//! - `pairdesk-ui::permission_exec` 真实执行，只在用户点击时调用
//!
//! 历史上这两者混在领域层，导致 `cargo test -p pairdesk-shell` 每跑一次
//! 就弹出两次 macOS 系统设置窗口。

use pairdesk_shell::permissions::{PermissionAction, PermissionKind};

/// 执行一个权限动作。
///
/// 返回值来自 [`PermissionAction::needs_recheck_after`]，表示是否需要在稍后
/// 重新检测权限（`Request` 的系统对话框是异步的）。
pub fn execute(action: PermissionAction) -> bool {
    match action {
        PermissionAction::Request(kind) => request(kind),
        PermissionAction::OpenSettings(kind) => open_settings(kind),
        PermissionAction::Recheck => {}
        PermissionAction::Restart => restart_app(),
    }
    action.needs_recheck_after()
}

/// 触发系统权限授权对话框。
///
/// macOS 上屏幕录制会弹系统对话框；辅助功能没有 API 能主动弹窗，
/// 只能引导用户去设置面板（`request_permission` 对它返回 false）。
fn request(kind: PermissionKind) {
    let _ = std::panic::catch_unwind(|| {
        pairdesk_core::permissions::request_permission(kind.as_str())
    });
}

/// 打开系统隐私与安全设置面板。
fn open_settings(kind: PermissionKind) {
    let _ = std::panic::catch_unwind(|| {
        pairdesk_core::permissions::open_permission_settings(kind.as_str())
    });
}

/// 重启应用。
///
/// macOS 的 TCC 权限结果按进程缓存：`AXIsProcessTrusted` /
/// `CGPreflightScreenCaptureAccess` 一旦在本进程读到 false 就持续返回 false。
/// 用户授权后必须用全新进程重新查询 tccd 才能读到新状态。
///
/// 注意：本函数**不返回**（spawn 新进程后 exit）。
fn restart_app() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            // 拿不到自身路径 → 不退出，否则应用会凭空消失且用户无法恢复
            eprintln!("[perm] 重启失败，无法获取自身路径: {e}");
            return;
        }
    };

    match std::process::Command::new(&exe).spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            // spawn 失败（权限、路径、资源不足）→ 保持当前进程存活，
            // 让用户还能继续操作，而不是窗口突然消失。
            eprintln!("[perm] 重启失败，无法启动新进程: {e}");
        }
    }
}

// 本模块不做单元测试：所有函数都会产生真实副作用（拉起系统设置、
// 杀掉进程），测试它们等于污染开发机环境——这正是本次修复要消除的问题。
//
// 「该做什么」的判定逻辑全部在 pairdesk-shell::permissions 里，有单测覆盖。
// 这里只剩「怎么执行」的翻译，没有可测的判断分支。
