//! Windows 屏幕采集（DXGI）—— M4 里程碑实现。
//! 当前为占位，仅在 Windows 编译时启用。

use anyhow::bail;

use super::{CapturedFrame, ScreenCapturer};

pub struct WinCapturer;

impl ScreenCapturer for WinCapturer {
    fn capture(&mut self) -> anyhow::Result<CapturedFrame> {
        bail!("Windows 采集尚未实现（M4 里程碑）");
    }
    fn display_size(&self) -> (u32, u32) {
        (0, 0)
    }
}