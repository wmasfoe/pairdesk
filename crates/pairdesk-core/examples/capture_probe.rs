//! 诊断工具：连接 X server 并打印 root 指定坐标的像素 RGB。
//! 用法: capture_probe [x] [y]   (默认 320 240)

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let px: i16 = args.next().unwrap_or_else(|| "320".into()).parse()?;
    let py: i16 = args.next().unwrap_or_else(|| "240".into()).parse()?;
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;
    let geo = conn.get_geometry(root)?.reply()?;
    println!("root 尺寸 {}x{} depth {}", geo.width, geo.height, geo.depth);
    let reply = conn
        .get_image(ImageFormat::Z_PIXMAP, root, px, py, 1, 1, !0)?
        .reply()?;
    println!("GetImage 返回 {} 字节 depth {}", reply.data.len(), reply.depth);
    // 打印原始字节（4BPP 下前 4 字节即该像素）
    for i in 0..reply.data.len().min(16) {
        print!("{:02x} ", reply.data[i]);
    }
    println!();
    Ok(())
}