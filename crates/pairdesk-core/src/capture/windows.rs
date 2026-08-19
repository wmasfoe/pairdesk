//! Windows 屏幕采集（DXGI）—— M4 里程碑实现。
//! 当前为占位，仅在 Windows 编译时启用。

use anyhow::{bail, Result};

use super::{CapturedFrame, ScreenCapturer};

pub struct WinCapturer;

impl WinCapturer {
    /// 占位构造：仅保证 Windows 分支可编译；真实采集实现在 M4 里程碑
    pub fn new() -> Result<WinCapturer> {
        Ok(WinCapturer)
    }
}

impl ScreenCapturer for WinCapturer {
    fn capture(&mut self) -> anyhow::Result<CapturedFrame> {
        bail!("Windows 采集尚未实现（M4 里程碑）");
    }
    fn display_size(&self) -> (u32, u32) {
        (0, 0)
    }
}