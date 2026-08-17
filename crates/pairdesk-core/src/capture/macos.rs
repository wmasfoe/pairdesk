//! macOS 屏幕采集（ScreenCaptureKit）—— M2 里程碑实现。
//! 当前为占位，仅在 macOS 编译时启用。

use anyhow::bail;

use super::{CapturedFrame, ScreenCapturer};

pub struct MacCapturer;

impl ScreenCapturer for MacCapturer {
    fn capture(&mut self) -> anyhow::Result<CapturedFrame> {
        bail!("macOS 采集尚未实现（M2 里程碑）");
    }
    fn display_size(&self) -> (u32, u32) {
        (0, 0)
    }
}