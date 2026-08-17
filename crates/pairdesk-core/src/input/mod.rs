//! 输入注入：把远端传来的鼠标/键盘事件落到本机。
//!
//! 平台实现按 `#[cfg(target_os)]` 门控：
//! - macOS: `macos.rs`（CGEvent）
//! - Windows: `windows.rs`（SendInput）
//! - Linux: `linux.rs`（XTest）

use anyhow::Result;

/// 鼠标按键（协议层定义，平台实现映射）。
pub const BTN_LEFT: u8 = 1;
pub const BTN_MIDDLE: u8 = 2;
pub const BTN_RIGHT: u8 = 3;
pub const BTN_WHEEL_UP: u8 = 4;
pub const BTN_WHEEL_DOWN: u8 = 5;

/// 修饰键位（按位或组合）。
pub const MOD_SHIFT: u32 = 1;
pub const MOD_CTRL: u32 = 2;
pub const MOD_ALT: u32 = 4;
pub const MOD_META: u32 = 8;

/// 按键的跨平台表示：用 X11 keysym（UCS/ASCII 直接对应）。
/// 平台实现做 keysym→平台键码的映射。
pub type KeySym = u32;

/// 输入注入器抽象：被控端收到远端输入后调用。
pub trait InputInjector: Send {
    /// 移动鼠标到屏幕坐标（远端分辨率坐标，会话层负责等比换算）。
    fn move_mouse(&mut self, x: f64, y: f64) -> Result<()>;
    /// 按下/松开鼠标按键。
    fn button(&mut self, btn: u8, down: bool) -> Result<()>;
    /// 滚动滚轮。
    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()>;
    /// 按下/松开按键（keysym 表示）。
    fn key(&mut self, sym: KeySym, down: bool, mods: u32) -> Result<()>;
}

/// 常用 keysym 常量（X11 定义值）。
pub mod keysym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const DELETE: u32 = 0xffff;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const HOME: u32 = 0xff50;
    pub const END: u32 = 0xff57;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const CTRL_L: u32 = 0xffe3;
    pub const ALT_L: u32 = 0xffe9;
    pub const META_L: u32 = 0xffe7;
    pub const SUPER_L: u32 = 0xffeb;
    pub const SPACE: u32 = 0x20;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::X11Injector;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacInjector;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WinInjector;

#[cfg(target_os = "linux")]
pub type PlatformInjector = X11Injector;
#[cfg(target_os = "macos")]
pub type PlatformInjector = MacInjector;
#[cfg(target_os = "windows")]
pub type PlatformInjector = WinInjector;