//! Linux X11 屏幕采集（XGetImage）。
//!
//! 通过 x11rb 连接 X server，对根窗口发 GetImage 请求拉取全屏像素，
//! 并按 visual 的 color mask 转换为 RGB 字节序。
//! 注意：需要系统已安装 libx11（开发环境已装）。

use anyhow::{Result, bail};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, Visualtype};

use super::{CapturedFrame, ScreenCapturer};

pub struct X11Capturer {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
    root: x11rb::protocol::xproto::Window,
    w: u32,
    h: u32,
    /// 像素字节数/行（GetImage 返回）。
    stride: u32,
    /// RGB 通道在 32 位像素中的移位（由 visual mask 推导）。
    r_shift: u32,
    g_shift: u32,
    b_shift: u32,
}

impl X11Capturer {
    /// 连接默认 X display（取 `DISPLAY` 环境变量）。
    pub fn new() -> Result<X11Capturer> {
        Self::with_display(None)
    }

    pub fn with_display(display: Option<&str>) -> Result<X11Capturer> {
        let (conn, screen_num) = x11rb::connect(display)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        let geo = conn.get_geometry(root)?.reply()?;
        let w = geo.width as u32;
        let h = geo.height as u32;
        if w == 0 || h == 0 {
            bail!("屏幕尺寸为 0，无法采集");
        }

        // 由 root_visual 找 color mask 推导通道移位
        let visual = find_visual(&conn, screen_num, screen.root_visual)
            .ok_or_else(|| anyhow::anyhow!("找不到 root visual"))?;

        Ok(X11Capturer {
            conn,
            screen_num,
            root,
            w,
            h,
            stride: w * 4,
            r_shift: visual.red_mask.trailing_zeros(),
            g_shift: visual.green_mask.trailing_zeros(),
            b_shift: visual.blue_mask.trailing_zeros(),
        })
    }
}

impl ScreenCapturer for X11Capturer {
    fn display_size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    fn capture(&mut self) -> Result<CapturedFrame> {
        let reply = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                0,
                0,
                self.w as u16,
                self.h as u16,
                !0, // plane mask 全选
            )?
            .reply()?;

        let data = reply.data;
        if data.len() < (self.stride as usize) * (self.h as usize) {
            bail!("GetImage 返回数据不足");
        }

        let mut rgb = Vec::with_capacity((self.w * self.h * 3) as usize);
        for y in 0..self.h {
            let row = y as usize * self.stride as usize;
            for x in 0..self.w {
                let px = u32::from_le_bytes([
                    data[row + x as usize * 4],
                    data[row + x as usize * 4 + 1],
                    data[row + x as usize * 4 + 2],
                    data[row + x as usize * 4 + 3],
                ]);
                rgb.push(((px >> self.r_shift) & 0xFF) as u8);
                rgb.push(((px >> self.g_shift) & 0xFF) as u8);
                rgb.push(((px >> self.b_shift) & 0xFF) as u8);
            }
        }
        Ok(CapturedFrame {
            rgb,
            w: self.w,
            h: self.h,
        })
    }
}

/// 按 visual id 在屏幕的 allowed_depths 中查找 VisualType。
fn find_visual<C: Connection>(
    conn: &C,
    screen_num: usize,
    visual_id: u32,
) -> Option<Visualtype> {
    let screen = &conn.setup().roots[screen_num];
    for depth in &screen.allowed_depths {
        for v in &depth.visuals {
            if v.visual_id == visual_id {
                return Some(v.clone());
            }
        }
    }
    None
}