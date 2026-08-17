//! 屏幕采集：平台差异收敛于 [`ScreenCapturer`] trait。
//!
//! 平台实现按 `#[cfg(target_os)]` 门控：
//! - macOS: `macos.rs`（ScreenCaptureKit）
//! - Windows: `windows.rs`（DXGI）
//! - Linux: `linux.rs`（X11）

use anyhow::Result;

/// 一帧原始画面（RGB 字节序，`w*h*3`）。
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub rgb: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// 屏幕采集器抽象：会话层只依赖此 trait。
pub trait ScreenCapturer: Send {
    /// 采集当前屏幕，返回一帧 RGB。
    fn capture(&mut self) -> Result<CapturedFrame>;
    /// 当前屏幕分辨率。
    fn display_size(&self) -> (u32, u32);
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::X11Capturer;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacCapturer;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WinCapturer;

/// 当前平台可用的采集器类型。
#[cfg(target_os = "linux")]
pub type PlatformCapturer = X11Capturer;
#[cfg(target_os = "macos")]
pub type PlatformCapturer = MacCapturer;
#[cfg(target_os = "windows")]
pub type PlatformCapturer = WinCapturer;