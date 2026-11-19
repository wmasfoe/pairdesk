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
        /// 带提示参数查询辅助功能权限。传 `kAXTrustedCheckOptionPrompt = true`
        /// 时，若未授权，系统会主动弹出「XXX 想控制这台电脑」授权对话框，
        /// 并把应用加入系统设置的辅助功能列表。
        ///
        /// 这是 Apple 为辅助功能提供的**唯一**可主动触发授权的 API
        /// （屏幕录制有 `CGRequestScreenCaptureAccess`，辅助功能没有对应物）。
        fn AXIsProcessTrustedWithOptions(options: core_foundation::base::CFTypeRef) -> bool;
    }

    // kAXTrustedCheckOptionPrompt 的字符串常量值，由 ApplicationServices 导出。
    // 这里直接写死字符串字面量而不链接符号，避免 core-foundation 类型体操。
    const AX_TRUSTED_CHECK_OPTION_PROMPT: &str = "AXTrustedCheckOptionPrompt";

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
            // 屏幕录制：系统会弹「想录制屏幕」对话框
            "screen" => unsafe { CGRequestScreenCaptureAccess() },
            // 辅助功能：系统会弹「想控制这台电脑」对话框，并把本应用加入
            // 系统设置 → 隐私与安全性 → 辅助功能 列表（用户还需手动勾选）
            "accessibility" => request_accessibility_prompt(),
            _ => false,
        }
    }

    /// 触发辅助功能授权对话框。
    ///
    /// 返回值：调用**之后**是否已获得授权。首次调用几乎必然返回 false
    /// （对话框刚弹出，用户还没点），需要用户授权后重启应用才会变 true。
    ///
    /// 实现说明：`AXIsProcessTrustedWithOptions` 需要一个 CFDictionary，
    /// 内含 `{AXTrustedCheckOptionPrompt: true}`。用 core-foundation 构造
    /// 字典要引入一堆类型，这里用最小化的 CoreFoundation C API 直接构造。
    fn request_accessibility_prompt() -> bool {
        use core_foundation::base::{CFTypeRef, TCFType};
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::string::CFString;

        let key = CFString::new(AX_TRUSTED_CHECK_OPTION_PROMPT);
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);

        unsafe {
            AXIsProcessTrustedWithOptions(options.as_CFTypeRef() as CFTypeRef)
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
