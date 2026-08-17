//! 测试辅助：在 X 屏幕上创建并绘制一个纯色窗口。
//!
//! 用 GC + PolyFillRectangle 主动填充颜色（仅背景属性在某些 X 实现下
//! 不会真正落屏，主动绘制最可靠）。

use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext, Rectangle, Window, WindowClass,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let color_str = args.next().unwrap_or_else(|| "336699".into());
    let w: u16 = args.next().unwrap_or_else(|| "640".into()).parse()?;
    let h: u16 = args.next().unwrap_or_else(|| "480".into()).parse()?;
    let color = u32::from_str_radix(&color_str, 16)?;

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let win: Window = conn.generate_id()?.into();
    conn.create_window(
        screen.root_depth,
        win,
        screen.root,
        0,
        0,
        w,
        h,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixel(color)
            .event_mask(EventMask::EXPOSURE),
    )?;

    // 创建 GC 并填充全窗口
    let gc: Gcontext = conn.generate_id()?.into();
    conn.create_gc(gc, win, &CreateGCAux::new().foreground(color))?;
    conn.poly_fill_rectangle(
        win,
        gc,
        &[Rectangle {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }],
    )?;
    conn.map_window(win)?;
    conn.flush()?;
    // 等 map 完成
    std::thread::sleep(Duration::from_millis(300));
    // 刷新一次绘制（确保前台缓冲）
    conn.poly_fill_rectangle(
        win,
        gc,
        &[Rectangle {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }],
    )?;
    conn.flush()?;
    eprintln!("paint: 窗口 {}x{} 颜色 #{:06x} 已绘制", w, h, color);
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}