//! 画面编码：RGB → JPEG。
//!
//! V1 用纯 Rust 软编（jpeg-encoder），全量帧。
//! V1.5 计划：差分块只发变化区域，降低带宽与编码时间。

use anyhow::Result;
use jpeg_encoder::{ColorType, Encoder as JpegEncoder};

/// 编码一帧 RGB 像素为 JPEG。
/// - `rgba`：严格按 `w*h*3` 长度的 RGB 字节序
/// - 质量 0-100
pub fn encode_jpeg(rgb: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>> {
    if rgb.len() != (w as usize) * (h as usize) * 3 {
        anyhow::bail!(
            "像素长度不符: 期望 {} 实际 {}",
            (w as usize) * (h as usize) * 3,
            rgb.len()
        );
    }
    let mut out: Vec<u8> = Vec::with_capacity((w as usize) * (h as usize) / 2);
    let enc = JpegEncoder::new(&mut out, quality.min(100).max(1));
    enc.encode(rgb, w as u16, h as u16, ColorType::Rgb)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_small_frame() {
        // 16x16 纯红
        let mut rgb = vec![0u8; 16 * 16 * 3];
        for px in rgb.chunks_mut(3) {
            px[0] = 255;
        }
        let jpeg = encode_jpeg(&rgb, 16, 16, 80).unwrap();
        assert!(jpeg.len() > 50, "JPEG 不应为空");
        // JPEG 文件头 SOI
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn bad_size_rejected() {
        assert!(encode_jpeg(&vec![0u8; 10], 16, 16, 80).is_err());
    }
}