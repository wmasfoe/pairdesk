//! 传输层：TCP 读/写分离的连接封装。
//!
//! 会话层会把连接拆成"发送端"与"接收端"两个实例（[`Connection::try_clone`]），
//! 分别供采集线程写、接收线程读——TcpStream::try_clone 共享同一 socket，
//! 读写方向并发安全。
//!
//! 帧读取用缓冲累积；网络空闲超时返回 `Ok(None)`，由调用方决定心跳/断线策略。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use anyhow::{Result, bail};

use crate::protocol::{
    Frame, FrameType, HEADER_LEN, MAX_FRAME, encode_header, parse_header,
};
use crate::quic_frame::FrameStream;

pub struct Connection {
    stream: TcpStream,
    rx_buf: Vec<u8>,
}

/// 让 TCP 连接也走统一的 [`FrameStream`] 接口（QUIC 与 TCP 在同一会话逻辑下互操作）。
impl FrameStream for Connection {
    fn recv_frame(&mut self) -> Result<Option<Frame>> {
        Connection::recv_frame(self)
    }
    fn send_frame(&mut self, ty: FrameType, payload: &[u8]) -> Result<()> {
        Connection::send_frame(self, ty, payload)
    }
    fn set_read_timeout(&mut self, d: Duration) -> Result<()> {
        Connection::set_read_timeout(self, d)
    }
    fn try_clone(&self) -> Result<Self> {
        Connection::try_clone(self)
    }
    fn peer_addr(&self) -> Result<std::net::SocketAddr> {
        Connection::peer_addr(self)
    }
}

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream,
            rx_buf: Vec::with_capacity(64 * 1024),
        }
    }

    /// 克隆出共享同一 socket 的另一个连接（读/写方向分离用）。
    pub fn try_clone(&self) -> Result<Connection> {
        Ok(Connection::new(self.stream.try_clone()?))
    }

    /// 设置读超时（`recv_frame` 空闲超过此时长返回 `Ok(None)`）。
    pub fn set_read_timeout(&self, d: Duration) -> Result<()> {
        self.stream.set_read_timeout(Some(d))?;
        Ok(())
    }

    /// 发送一帧（payload 已加密或明文均可）。
    pub fn send_frame(&mut self, ty: FrameType, payload: &[u8]) -> Result<()> {
        let mut buf = Vec::with_capacity(HEADER_LEN + payload.len());
        buf.extend_from_slice(&encode_header(ty, payload.len()));
        buf.extend_from_slice(payload);
        self.stream.write_all(&buf)?;
        Ok(())
    }

    /// 阻塞式读取下一帧。
    /// - `Ok(Some(frame))`：拿到一帧
    /// - `Ok(None)`：读超时（网络空闲，非错误）
    /// - `Err`：连接错误/协议错误
    pub fn recv_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            // 缓冲中已有完整帧？
            if self.rx_buf.len() >= HEADER_LEN {
                let (ty, len) = parse_header(&self.rx_buf)?;
                if self.rx_buf.len() >= HEADER_LEN + len {
                    let payload = self.rx_buf[HEADER_LEN..HEADER_LEN + len].to_vec();
                    self.rx_buf.drain(..HEADER_LEN + len);
                    return Ok(Some(Frame { ty, payload }));
                }
            }
            if self.rx_buf.len() > HEADER_LEN + MAX_FRAME {
                bail!("接收缓冲超限，协议错乱");
            }
            let mut chunk = [0u8; 16 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => bail!("连接被对端关闭"),
                Ok(n) => self.rx_buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // 读超时：外层判断心跳
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    pub fn peer_addr(&self) -> Result<std::net::SocketAddr> {
        Ok(self.stream.peer_addr()?)
    }
}

/// 监听并接受一个连接（被控端）。
pub fn accept_once(port: u16) -> Result<Connection> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    let (stream, _) = listener.accept()?;
    stream.set_nodelay(true)?;
    Ok(Connection::new(stream))
}

/// 主动连接（控制端）。
pub fn connect(addr: std::net::SocketAddr) -> Result<Connection> {
    let stream = TcpStream::connect(addr)?;
    stream.set_nodelay(true)?;
    Ok(Connection::new(stream))
}