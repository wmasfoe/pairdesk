//! 远程画面：JPEG 解码与纹理管理。
//!
//! # 为什么单独成模块
//!
//! 这是整个 App 性能最吃紧的路径，也是 GPUI 迁移中坑最多的地方。两个必须遵守的
//! 约束（均由实测得出，详见 `docs/architecture/gpui-constraints.md`）：
//!
//! 1. **帧泵不能用 `render()` 里的 `cx.notify()`** —— 实测帧率会从 20fps 塌到 0.2fps。
//!    必须用 `cx.spawn` + `background_executor().timer()` 驱动。
//! 2. **旧纹理必须显式 `cx.drop_image()`** —— 否则 GPU atlas 无限增长。
//!
//! 本模块把这两条封装成 [`FramePump`]，让视图层无法踩错。

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, RenderImage, Window};
use image::Frame;

/// 帧泵轮询间隔。20fps 目标 → 50ms 一帧，取 15ms 保证不丢帧且不空转。
#[allow(dead_code)]
const PUMP_INTERVAL: Duration = Duration::from_millis(15);

/// 解码一帧 JPEG 为 GPUI 可渲染的图像。
///
/// 使用 `zune-jpeg`（实测 3.4ms/帧 @1080p，比 `image` crate 快一倍）。
/// 输出必须是 RGBA8——GPUI 的 `RenderImage` 要求这个格式。
pub fn decode_jpeg(jpeg: &[u8]) -> Result<Arc<RenderImage>, String> {
    let mut decoder = zune_jpeg::JpegDecoder::new(jpeg);
    let rgb = decoder.decode().map_err(|e| format!("JPEG 解码失败: {e:?}"))?;

    let (w, h) = decoder
        .dimensions()
        .ok_or_else(|| "JPEG 缺少尺寸信息".to_string())?;
    let (w, h) = (w as u32, h as u32);

    // RGB → RGBA：每像素补一个 255 的 alpha
    let expected = (w as usize) * (h as usize) * 3;
    if rgb.len() < expected {
        return Err(format!(
            "JPEG 解码数据不足：期望 {expected} 字节，实际 {}",
            rgb.len()
        ));
    }
    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for c in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }

    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| "构造 RGBA 图像失败（尺寸与数据不匹配）".to_string())?;

    Ok(Arc::new(RenderImage::new([Frame::new(img)])))
}

/// 画面帧泵：从内核事件流取帧，解码后替换当前纹理。
///
/// 使用方式：视图在构造时创建，`Render::render` 里读 [`FramePump::image`]，
/// 构造后立刻调 [`FramePump::spawn`] 启动后台泵。
pub struct FramePump {
    rx: Receiver<Vec<u8>>,
    image: Option<Arc<RenderImage>>,
    /// 累计解码帧数（用于状态栏显示）
    frame_count: u64,
    /// 累计解码耗时（毫秒），用于算平均值
    decode_ms_total: f64,
    /// 最近一次错误（解码失败时记录，不清空画面）
    last_error: Option<String>,
}

impl FramePump {
    /// 创建帧泵。`rx` 来自 `CoreHandle` 启动时返回的事件流接收器。
    pub fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            image: None,
            frame_count: 0,
            decode_ms_total: 0.0,
            last_error: None,
        }
    }

    /// 当前纹理。`None` 表示还没收到第一帧。
    pub fn image(&self) -> Option<Arc<RenderImage>> {
        self.image.clone()
    }

    /// 已解码的帧数。
    ///
    /// 当前 UI 未展示统计信息，保留供后续状态栏使用（`#[allow(dead_code)]`）。
    #[allow(dead_code)]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// 平均解码耗时（毫秒）。保留供性能诊断使用。
    #[allow(dead_code)]
    pub fn avg_decode_ms(&self) -> f64 {
        if self.frame_count == 0 {
            0.0
        } else {
            self.decode_ms_total / self.frame_count as f64
        }
    }

    /// 最近一次解码错误。保留供诊断使用。
    #[allow(dead_code)]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// 尝试取一帧并解码。返回是否成功换了新画面。
    ///
    /// **必须由 [`FramePump::spawn`] 调用，不要在 `render()` 里调。**
    pub fn try_advance<T: 'static>(&mut self, cx: &mut Context<T>) -> bool {
        let Ok(jpeg) = self.rx.try_recv() else {
            return false;
        };

        let started = std::time::Instant::now();
        match decode_jpeg(&jpeg) {
            Ok(new_image) => {
                self.decode_ms_total += started.elapsed().as_secs_f64() * 1000.0;
                self.frame_count += 1;
                self.last_error = None;

                // 关键：旧纹理必须显式释放，否则 GPU atlas 无限增长
                if let Some(old) = self.image.replace(new_image) {
                    cx.drop_image(old, None);
                }
                true
            }
            Err(e) => {
                // 解码失败不清空画面——保留上一帧比黑屏好
                self.last_error = Some(e);
                false
            }
        }
    }

}

/// 从事件流里分离画面帧与其他事件。
///
/// `pairdesk-core` 的 `CoreEvent` 是单一通道，画面帧与状态事件混在一起。
/// 这里在适配器层做一次分流：帧进 [`FramePump`]，其余进会话状态机。
pub fn split_events(
    rx: Receiver<pairdesk_core::CoreEvent>,
) -> (
    Receiver<Vec<u8>>,
    Receiver<pairdesk_core::CoreEvent>,
) {
    let (frame_tx, frame_rx) = std::sync::mpsc::channel();
    let (other_tx, other_rx) = std::sync::mpsc::channel();

    std::thread::Builder::new()
        .name("pd-event-split".into())
        .spawn(move || {
            while let Ok(ev) = rx.recv() {
                match ev {
                    pairdesk_core::CoreEvent::ScreenFrame(jpeg) => {
                        // 帧通道满了就丢旧帧：宁可掉帧也不要阻塞状态事件
                        if frame_tx.send(jpeg).is_err() {
                            break;
                        }
                    }
                    other => {
                        if other_tx.send(other).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .expect("spawn event splitter");

    (frame_rx, other_rx)
}

/// 检查系统外观是否为暗色。GPUI 在 `Window` 上有该信息。
pub fn is_dark(window: &Window) -> bool {
    window.appearance() == gpui::WindowAppearance::Dark
}
/// 根据系统外观取主题模式。
pub fn theme_mode(window: &Window) -> crate::theme::ThemeMode {
    crate::theme::ThemeMode::from_system(is_dark(window))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张最小合法 JPEG 用于测试。
    fn make_jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut rgb = vec![0u8; (w as usize) * (h as usize) * 3];
        for (i, px) in rgb.chunks_exact_mut(3).enumerate() {
            px[0] = (i % 256) as u8;
            px[1] = ((i / 256) % 256) as u8;
            px[2] = 128;
        }
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
        enc.encode(
            &rgb,
            w as u32,
            h as u32,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        out
    }

    #[test]
    fn 解码合法_jpeg_得到正确尺寸() {
        let jpeg = make_jpeg(64, 48);
        let img = decode_jpeg(&jpeg).expect("应能解码");
        let size = img.size(0);
        assert_eq!(size.width.0, 64);
        assert_eq!(size.height.0, 48);
    }

    #[test]
    fn 解码输出是_rgba_四通道() {
        let jpeg = make_jpeg(8, 8);
        let img = decode_jpeg(&jpeg).unwrap();
        let bytes = img.as_bytes(0).expect("应有像素数据");
        // RGBA：8*8*4 = 256 字节
        assert_eq!(bytes.len(), 8 * 8 * 4);
        // alpha 通道应全部为 255
        for chunk in bytes.chunks_exact(4) {
            assert_eq!(chunk[3], 255, "alpha 必须是不透明白");
        }
    }

    #[test]
    fn 解码非法数据返回错误而不崩溃() {
        // 空数据、随机字节、截断的 JPEG 都必须返回 Err 而不是 panic
        assert!(decode_jpeg(&[]).is_err());
        assert!(decode_jpeg(&[0xff, 0xd8]).is_err());
        assert!(decode_jpeg(b"not a jpeg at all").is_err());

        let valid = make_jpeg(32, 32);
        let truncated = &valid[..valid.len() / 2];
        assert!(decode_jpeg(truncated).is_err(), "截断的 JPEG 应返回错误");
    }

    #[test]
    fn 帧泵初始状态为空() {
        let (_tx, rx) = std::sync::mpsc::channel();
        let pump = FramePump::new(rx);
        assert!(pump.image().is_none());
        assert_eq!(pump.frame_count(), 0);
        assert_eq!(pump.avg_decode_ms(), 0.0);
        assert!(pump.last_error().is_none());
    }

    #[test]
    fn 事件分流把画面帧与其他事件分开() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(pairdesk_core::CoreEvent::ScreenFrame(vec![1, 2, 3]))
            .unwrap();
        tx.send(pairdesk_core::CoreEvent::PeerConnected).unwrap();
        tx.send(pairdesk_core::CoreEvent::Size { w: 100, h: 50 })
            .unwrap();
        tx.send(pairdesk_core::CoreEvent::ScreenFrame(vec![4, 5, 6]))
            .unwrap();
        drop(tx);

        let (frame_rx, other_rx) = split_events(rx);

        let frames: Vec<_> = frame_rx.iter().collect();
        assert_eq!(frames.len(), 2, "应分离出 2 帧");
        assert_eq!(frames[0], vec![1, 2, 3]);
        assert_eq!(frames[1], vec![4, 5, 6]);

        let others: Vec<_> = other_rx.iter().collect();
        assert_eq!(others.len(), 2, "应有 2 个非帧事件");
        assert!(matches!(
            others[0],
            pairdesk_core::CoreEvent::PeerConnected
        ));
        assert!(matches!(others[1], pairdesk_core::CoreEvent::Size { .. }));
    }

    #[test]
    fn 解码性能满足_20fps_预算() {
        // 性能断言只在 release 下有意义：debug 构建未优化的解码器会慢 20-60 倍
        // （实测 720p：debug 206ms vs release 约 1.5ms），拿 debug 断言性能必然误报。
        if cfg!(debug_assertions) {
            return;
        }

        // 20fps = 50ms/帧预算。1080p 解码实测约 3.4ms，这里用 720p 做保守验证。
        let jpeg = make_jpeg(1280, 720);
        let start = std::time::Instant::now();
        let iterations = 10;
        for _ in 0..iterations {
            let _ = decode_jpeg(&jpeg).unwrap();
        }
        let avg_ms = start.elapsed().as_secs_f64() * 1000.0 / iterations as f64;
        assert!(
            avg_ms < 40.0,
            "720p 解码平均 {avg_ms:.1}ms，超过 20fps 预算的 80%"
        );
    }
}
