//! 平台权限检测与引导（macOS 屏幕录制与辅助功能权限）。
//!
//! 在 macOS 上：
//! - 屏幕录制（Screen Recording）：被控端截屏需要
//! - 辅助功能（Accessibility）：控制端注入鼠标/键盘需要
//! 在 Linux / Windows 上默认认为均已授权（无需此类特殊弹窗权限）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionStatus {
    /// 屏幕录制权限（macOS）
    pub screen_recording: bool,
    /// 辅助功能权限（macOS）
    pub accessibility: bool,
    /// 是否需要进行权限引导（仅 macOS 为 true）
    pub need_guidance: bool,
}

#[cfg(target_os = "macos")]
mod sys {
    use super::PermissionStatus;
    use std::process::Command;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn check_permissions() -> PermissionStatus {
        let screen_recording = unsafe { CGPreflightScreenCaptureAccess() };
        let accessibility = unsafe { AXIsProcessTrusted() };
        PermissionStatus {
            screen_recording,
            accessibility,
            need_guidance: true,
        }
    }

    pub fn request_permission(permission_type: &str) -> bool {
        match permission_type {
            "screen" => unsafe { CGRequestScreenCaptureAccess() },
            _ => false,
        }
    }

    pub fn open_permission_settings(permission_type: &str) {
        let target = match permission_type {
            "screen" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            _ => "x-apple.systempreferences:com.apple.preference.security",
        };
        let _ = Command::new("open").arg(target).spawn();
    }
}

#[cfg(not(target_os = "macos"))]
mod sys {
    use super::PermissionStatus;

    pub fn check_permissions() -> PermissionStatus {
        PermissionStatus {
            screen_recording: true,
            accessibility: true,
            need_guidance: false,
        }
    }

    pub fn request_permission(_permission_type: &str) -> bool {
        true
    }

    pub fn open_permission_settings(_permission_type: &str) {}
}

pub fn check_permissions() -> PermissionStatus {
    sys::check_permissions()
}

pub fn request_permission(permission_type: &str) -> bool {
    sys::request_permission(permission_type)
}

pub fn open_permission_settings(permission_type: &str) {
    sys::open_permission_settings(permission_type);
}
