//! macOS 屏幕采集 —— core-graphics 逐帧截屏（M2 里程碑真实实现）。
//!
//! 用 `CGDisplay::image()` 抓主显示器 → 取像素字节 → 转成 RGB(w*h*3)。
//! 简单可靠、单机可自测；ScreenCaptureKit 的高效流式采集留作后续优化。
//!
//! ⚠️ 真机约束：采集需「屏幕录制」权限（系统设置→隐私与安全→屏幕录制，
//! 授予运行 PairDesk 的终端/应用），否则 macOS 会返回全黑画面。

use anyhow::{bail, Result};
use core_graphics::display::CGDisplay;
use core_graphics::image::CGImage;

use super::{CapturedFrame, ScreenCapturer};

pub struct MacCapturer {
    display: CGDisplay,
    w: u32,
    h: u32,
}

impl MacCapturer {
    pub fn new() -> Result<MacCapturer> {
        let display = CGDisplay::main();
        // 用一次截屏探得分辨率（简单，不依赖额外 API）
        let (w, h) = display
            .image()
            .map(|img| (img.width() as u32, img.height() as u32))
            .unwrap_or((0, 0));
        if w == 0 || h == 0 {
            bail!("无法取得主显示器分辨率");
        }
        Ok(MacCapturer { display, w, h })
    }
}

impl ScreenCapturer for MacCapturer {
    fn capture(&mut self) -> Result<CapturedFrame> {
        let cg: CGImage = self
            .display
            .image()
            .ok_or_else(|| anyhow::anyhow!("截屏失败(检查「屏幕录制」权限)"))?;
        let w = cg.width() as u32;
        let h = cg.height() as u32;
        if w == 0 || h == 0 {
            bail!("截屏失败(宽度或高度为 0；检查「屏幕录制」权限)");
        }

        let data = cg.data();
        let bytes = data.bytes();
        let bpr = cg.bytes_per_row() as usize;

        // CGDisplayCreateImage 惯例为 4 字节/像素(BGRA)。逐像素取 B,G,R 拼成 RGB。
        // 若真机出现红蓝颠倒，把下面 r/b 两行互换即可。
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        let wu = w as usize;
        for row in 0..h as usize {
            let base = row * bpr;
            for col in 0..wu {
                let o = base + col * 4;
                let b = bytes[o];
                let g = bytes[o + 1];
                let r = bytes[o + 2];
                let so = (row * wu + col) * 3;
                rgb[so] = r;
                rgb[so + 1] = g;
                rgb[so + 2] = b;
            }
        }
        Ok(CapturedFrame { rgb, w, h })
    }

    fn display_size(&self) -> (u32, u32) {
        (self.w, self.h)
    }
}
