//! QUIC 帧流适配：把 QUIC 双向流适配成"同步帧流"，供会话层使用。
//!
//! 背景：会话层的握手/加密/画面逻辑全部基于**同步 + 帧**（`transport::Connection`，
//! TCP 专用）。要让 QUIC 直连也能跑同一套逻辑，需要把 QUIC（异步 tokio）的字节流
//! 桥接回同步的"读一整帧 / 写一整帧"接口——本模块用 `Runtime::block_on` 做这个桥，
//! 并按 PairDesk 帧格式（8B 头 + payload）在 QUIC 流上收发。
//!
//! 帧格式与 TCP 一致（见 `protocol.rs`），保证两端无论走 TCP 还是 QUIC 协议互通。

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Result, bail};

use crate::protocol::{
    Frame, FrameType, HEADER_LEN, MAX_FRAME, encode_header, parse_header,
};

/// 帧流抽象：会话层通过它读/写"完整帧"（TCP 与 QUIC 共用同一接口）。
pub trait FrameStream {
    /// 阻塞读一帧；对端干净关闭返回 None。
    fn recv_frame(&mut self) -> Result<Option<Frame>>;
    /// 发送一帧。
    fn send_frame(&mut self, ty: FrameType, payload: &[u8]) -> Result<()>;
    /// 设置读超时（用于心跳断线检测）。QUIC 默认 no-op（其自带 idle timeout 兜底）。
    fn set_read_timeout(&mut self, _d: std::time::Duration) -> Result<()> {
        Ok(())
    }
    /// 复制一份可独立读写的句柄，供会话多线程（recv/ctrl/send）各自使用。
    /// TCP 用 `try_clone`（共享 socket）；QUIC 用共享内部 Arc 克隆。
    fn try_clone(&self) -> Result<Self>
    where
        Self: Sized;
    /// 对端地址（仅日志用；QUIC 默认返回占位）。
    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(([0, 0, 0, 0], 0u16).into())
    }
}

/// 基于 QUIC 双向流的同步帧流。
#[derive(Clone)]
pub struct QuicFrameStream {
    rt: Arc<tokio::runtime::Runtime>, // 与 QUIC endpoint 共享的运行时（驱动 IO）
    send: Arc<tokio::sync::Mutex<quinn::SendStream>>,
    recv: Arc<tokio::sync::Mutex<quinn::RecvStream>>,
}

impl QuicFrameStream {
    /// 由已建立的 QUIC 双向流构造（rt 需与承载该连接的 endpoint 同一运行时）。
    pub fn new(
        rt: Arc<tokio::runtime::Runtime>,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
    ) -> QuicFrameStream {
        QuicFrameStream {
            rt,
            send: Arc::new(tokio::sync::Mutex::new(send)),
            recv: Arc::new(tokio::sync::Mutex::new(recv)),
        }
    }
}

impl FrameStream for QuicFrameStream {
    fn try_clone(&self) -> Result<Self> {
        Ok(self.clone()) // send/recv 是内部 Arc，克隆共享同一对流
    }

    fn recv_frame(&mut self) -> Result<Option<Frame>> {
        let rt = self.rt.clone();
        let recv = self.recv.clone();
        rt.block_on(async move {
            let mut recv = recv.lock().await;
            let mut hdr = [0u8; HEADER_LEN];
            read_exact(&mut *recv, &mut hdr).await?;
            let (ty, len) = parse_header(&hdr)?;
            if len > MAX_FRAME {
                bail!("帧过大: {}", len);
            }
            let mut payload = vec![0u8; len];
            read_exact(&mut *recv, &mut payload).await?;
            Ok(Some(Frame { ty, payload }))
        })
    }

    fn send_frame(&mut self, ty: FrameType, payload: &[u8]) -> Result<()> {
        let header = encode_header(ty, payload.len());
        let rt = self.rt.clone();
        let send = self.send.clone();
        rt.block_on(async move {
            let mut send = send.lock().await;
            send.write_all(&header).await?;
            send.write_all(payload).await?;
            Ok(())
        })
    }
}

/// 从 RecvStream 精确读够 n 字节。
async fn read_exact(recv: &mut quinn::RecvStream, buf: &mut [u8]) -> Result<()> {
    let mut got = 0usize;
    while got < buf.len() {
        let n = match recv.read(&mut buf[got..]).await {
            Ok(Some(n)) => n,
            Ok(None) => bail!("对端关闭（帧不完整）"),
            Err(e) => bail!("QUIC 读错误: {}", e),
        };
        got += n;
    }
    Ok(())
}
