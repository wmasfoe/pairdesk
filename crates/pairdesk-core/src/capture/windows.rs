//! Windows 屏幕采集 —— GDI BitBlt 全屏截屏（M4 里程碑真实实现）。
//!
//! 用最简可靠的 GDI 路径：GetDC(全屏) → BitBlt 拷到内存位图 → GetDIBits 取 32bpp
//! 像素 → 转 RGB(w*h*3)。先打通功能，DXGI Desktop Duplication 的高效流式留作优化。
//!
//! ⚠️ 真机约束：普通窗口会话即可截屏；若在锁屏/安全桌面(UAC)上画面为空属系统行为。

use anyhow::{bail, Result};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, HBITMAP, HDC, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDC, GetSystemMetrics, ReleaseDC, SM_CXSCREEN, SM_CYSCREEN,
};

use super::{CapturedFrame, ScreenCapturer};

pub struct WinCapturer {
    w: u32,
    h: u32,
}

impl WinCapturer {
    pub fn new() -> Result<WinCapturer> {
        let w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as u32;
        let h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as u32;
        if w == 0 || h == 0 {
            bail!("无法取得屏幕尺寸");
        }
        Ok(WinCapturer { w, h })
    }
}

impl ScreenCapturer for WinCapturer {
    fn capture(&mut self) -> Result<CapturedFrame> {
        let w = self.w as i32;
        let h = self.h as i32;
        unsafe {
            let screen = GetDC(HWND(0)); // 0 = 全屏 DC
            if screen.0 == 0 {
                bail!("GetDC 失败");
            }
            let mem = CreateCompatibleDC(screen);
            if mem.0 == 0 {
                let _ = ReleaseDC(HWND(0), screen);
                bail!("CreateCompatibleDC 失败");
            }
            let bmp = CreateCompatibleBitmap(screen, w, h);
            if bmp.0 == 0 {
                DeleteDC(mem);
                let _ = ReleaseDC(HWND(0), screen);
                bail!("CreateCompatibleBitmap 失败");
            }
            let old = SelectObject(mem, bmp);
            // 全屏拷贝
            BitBlt(mem, 0, 0, w, h, screen, 0, 0, SRCCOPY);

            // 32bpp 像素（biHeight 为负 → 自上而下行序，与后续 RGB 行序一致）
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0, // BI_RGB
                    ..Default::default()
                },
                bmiColors: Default::default(),
            };
            let mut px = vec![0u8; (self.w * self.h * 4) as usize];
            let copied = GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(px.as_mut_ptr() as *mut _),
                &mut bmi,
                0, // DIB_RGB_COLORS
            );

            // 还原并清理
            SelectObject(mem, old);
            DeleteObject(bmp);
            DeleteDC(mem);
            let _ = ReleaseDC(HWND(0), screen);

            if copied == 0 {
                bail!("GetDIBits 失败");
            }
            // BGRA → RGB
            let mut rgb = vec![0u8; (self.w * self.h * 3) as usize];
            for i in 0..(self.w * self.h) as usize {
                let o = i * 4;
                rgb[i * 3] = px[o + 2];
                rgb[i * 3 + 1] = px[o + 1];
                rgb[i * 3 + 2] = px[o];
            }
            Ok(CapturedFrame {
                rgb,
                w: self.w,
                h: self.h,
            })
        }
    }

    fn display_size(&self) -> (u32, u32) {
        (self.w, self.h)
    }
}
