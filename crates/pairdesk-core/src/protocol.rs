//! 协议层：帧编解码、握手消息、XChaCha20-Poly1305 加密。
//!
//! 帧格式（8 字节头 + payload）：
//! ```text
//! MAGIC(2B "PD") | VERSION(1B) | TYPE(1B) | LEN(4B 大端)
//! ```
//! 握手阶段（HELLO~AUTH_DENIED）payload 明文；之后所有帧 payload 加密。

use anyhow::{Result, bail};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 2] = b"PD";
pub const VERSION: u8 = 1;
pub const DEFAULT_PORT: u16 = 8888;
/// 单帧上限（JPEG 全量帧 1080p 高质量一般 < 2MB，留足余量）
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
/// 心跳间隔
pub const HEARTBEAT_SECS: u64 = 15;
/// 连续失联判死次数
pub const HEARTBEAT_TIMEOUT: u64 = 3;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Hello = 1,
    HelloAck = 2,
    Auth = 3,
    AuthOk = 4,
    AuthDenied = 5,
    Size = 6,
    Frame = 7,
    Input = 8,
    Heartbeat = 9,
    Goodbye = 10,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<FrameType> {
        Some(match v {
            1 => FrameType::Hello,
            2 => FrameType::HelloAck,
            3 => FrameType::Auth,
            4 => FrameType::AuthOk,
            5 => FrameType::AuthDenied,
            6 => FrameType::Size,
            7 => FrameType::Frame,
            8 => FrameType::Input,
            9 => FrameType::Heartbeat,
            10 => FrameType::Goodbye,
            _ => return None,
        })
    }
}

pub struct Frame {
    pub ty: FrameType,
    pub payload: Vec<u8>,
}

pub const HEADER_LEN: usize = 8;

/// 编码 8 字节帧头。
pub fn encode_header(ty: FrameType, len: usize) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..2].copy_from_slice(MAGIC);
    h[2] = VERSION;
    h[3] = ty as u8;
    h[4..8].copy_from_slice(&(len as u32).to_be_bytes());
    h
}

/// 解析帧头，校验 magic/version。
pub fn parse_header(buf: &[u8]) -> Result<(FrameType, usize)> {
    if buf.len() < HEADER_LEN {
        bail!("帧头不足");
    }
    if &buf[0..2] != MAGIC {
        bail!("magic 不符(非 PairDesk 协议)");
    }
    if buf[2] != VERSION {
        bail!("协议版本不符: 期望 {} 实际 {}", VERSION, buf[2]);
    }
    let ty = FrameType::from_u8(buf[3]).ok_or_else(|| anyhow::anyhow!("未知帧类型 {}", buf[3]))?;
    let len = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    if len > MAX_FRAME {
        bail!("帧过长 {} > {}", len, MAX_FRAME);
    }
    Ok((ty, len))
}

// ---------- 握手消息（手写定长编解码，零依赖） ----------

/// HELLO：控制端 → 被控端
pub struct HelloMsg {
    pub viewer_random: [u8; 16],
}
impl HelloMsg {
    pub fn encode(&self) -> Vec<u8> {
        self.viewer_random.to_vec()
    }
    pub fn decode(b: &[u8]) -> Result<HelloMsg> {
        if b.len() != 16 {
            bail!("HELLO 载荷长度错误");
        }
        let mut viewer_random = [0u8; 16];
        viewer_random.copy_from_slice(b);
        Ok(HelloMsg { viewer_random })
    }
}

/// HELLO_ACK：被控端 → 控制端（带随机数与密码盐）
pub struct HelloAckMsg {
    pub host_random: [u8; 16],
    pub salt: [u8; 32],
}
impl HelloAckMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(48);
        v.extend_from_slice(&self.host_random);
        v.extend_from_slice(&self.salt);
        v
    }
    pub fn decode(b: &[u8]) -> Result<HelloAckMsg> {
        if b.len() != 48 {
            bail!("HELLO_ACK 载荷长度错误");
        }
        let mut host_random = [0u8; 16];
        let mut salt = [0u8; 32];
        host_random.copy_from_slice(&b[0..16]);
        salt.copy_from_slice(&b[16..48]);
        Ok(HelloAckMsg { host_random, salt })
    }
}

/// AUTH：控制端 → 被控端，password_hash = sha256(salt || password)
pub struct AuthMsg {
    pub hash: [u8; 32],
}
impl AuthMsg {
    pub fn encode(&self) -> Vec<u8> {
        self.hash.to_vec()
    }
    pub fn decode(b: &[u8]) -> Result<AuthMsg> {
        if b.len() != 32 {
            bail!("AUTH 载荷长度错误");
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(b);
        Ok(AuthMsg { hash })
    }
}

/// AUTH_DENIED：被控端 → 控制端，附加原因说明
pub struct AuthDeniedMsg {
    pub reason: String,
}
impl AuthDeniedMsg {
    pub fn encode(&self) -> Vec<u8> {
        let b = self.reason.as_bytes();
        let mut v = Vec::with_capacity(2 + b.len());
        v.extend_from_slice(&(b.len() as u16).to_be_bytes());
        v.extend_from_slice(b);
        v
    }
    pub fn decode(b: &[u8]) -> Result<AuthDeniedMsg> {
        if b.len() < 2 {
            bail!("AUTH_DENIED 载荷长度错误");
        }
        let n = u16::from_be_bytes([b[0], b[1]]) as usize;
        let reason = String::from_utf8_lossy(&b[2..2 + n]).to_string();
        Ok(AuthDeniedMsg { reason })
    }
}

/// SIZE：被控端 → 控制端，屏幕分辨率
pub struct SizeMsg {
    pub w: u32,
    pub h: u32,
}
impl SizeMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&self.w.to_be_bytes());
        v.extend_from_slice(&self.h.to_be_bytes());
        v
    }
    pub fn decode(b: &[u8]) -> Result<SizeMsg> {
        if b.len() != 8 {
            bail!("SIZE 载荷长度错误");
        }
        Ok(SizeMsg {
            w: u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            h: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
        })
    }
}

/// FRAME：被控端 → 控制端，JPEG 画面帧
pub struct FrameMsg {
    pub seq: u32,
    pub jpeg: Vec<u8>,
}
impl FrameMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + self.jpeg.len());
        v.extend_from_slice(&self.seq.to_be_bytes());
        v.extend_from_slice(&self.jpeg);
        v
    }
    pub fn decode(b: &[u8]) -> Result<FrameMsg> {
        if b.len() < 4 {
            bail!("FRAME 载荷长度错误");
        }
        Ok(FrameMsg {
            seq: u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            jpeg: b[4..].to_vec(),
        })
    }
}

/// INPUT：控制端 → 被控端，输入事件
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMsg {
    MouseMove { x: f64, y: f64 },
    Button { btn: u8, down: bool },
    Scroll { dx: f64, dy: f64 },
    Key { keycode: u32, down: bool, mods: u32 },
}
impl InputMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(20);
        match self {
            InputMsg::MouseMove { x, y } => {
                v.push(0);
                v.extend_from_slice(&x.to_bits().to_be_bytes());
                v.extend_from_slice(&y.to_bits().to_be_bytes());
            }
            InputMsg::Button { btn, down } => {
                v.push(1);
                v.push(*btn);
                v.push(*down as u8);
            }
            InputMsg::Scroll { dx, dy } => {
                v.push(2);
                v.extend_from_slice(&dx.to_bits().to_be_bytes());
                v.extend_from_slice(&dy.to_bits().to_be_bytes());
            }
            InputMsg::Key { keycode, down, mods } => {
                v.push(3);
                v.extend_from_slice(&keycode.to_be_bytes());
                v.push(*down as u8);
                v.extend_from_slice(&mods.to_be_bytes());
            }
        }
        v
    }
    pub fn decode(b: &[u8]) -> Result<InputMsg> {
        if b.is_empty() {
            bail!("INPUT 载荷为空");
        }
        Ok(match b[0] {
            0 => {
                if b.len() != 17 {
                    bail!("INPUT/MouseMove 长度错误");
                }
                InputMsg::MouseMove {
                    x: f64::from_bits(u64::from_be_bytes(b[1..9].try_into()?)),
                    y: f64::from_bits(u64::from_be_bytes(b[9..17].try_into()?)),
                }
            }
            1 => {
                if b.len() != 3 {
                    bail!("INPUT/Button 长度错误");
                }
                InputMsg::Button { btn: b[1], down: b[2] != 0 }
            }
            2 => {
                if b.len() != 17 {
                    bail!("INPUT/Scroll 长度错误");
                }
                InputMsg::Scroll {
                    dx: f64::from_bits(u64::from_be_bytes(b[1..9].try_into()?)),
                    dy: f64::from_bits(u64::from_be_bytes(b[9..17].try_into()?)),
                }
            }
            3 => {
                if b.len() != 10 {
                    bail!("INPUT/Key 长度错误");
                }
                InputMsg::Key {
                    keycode: u32::from_be_bytes(b[1..5].try_into()?),
                    down: b[5] != 0,
                    mods: u32::from_be_bytes(b[6..10].try_into()?),
                }
            }
            _ => bail!("未知 INPUT 子类型 {}", b[0]),
        })
    }
}

// ---------- 密码哈希与会话密钥 ----------

/// 密码校验哈希：sha256(salt || password)。
pub fn password_hash(salt: &[u8], password: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(password.as_bytes());
    h.finalize().into()
}

/// 会话密钥：sha256(viewer_random || host_random || password)。
/// 双方各自用同样的输入推导出相同密钥，后续帧据此加密。
pub fn session_key(viewer_random: &[u8; 16], host_random: &[u8; 16], password: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(viewer_random);
    h.update(host_random);
    h.update(password.as_bytes());
    h.finalize().into()
}

// ---------- 加密层 ----------

/// XChaCha20-Poly1305 双向加密器。
///
/// nonce = 发送计数(12B 大端) + 12B 零，收发各自独立计数，永不重复。
pub struct Cipher {
    enc: XChaCha20Poly1305,
    dec: XChaCha20Poly1305,
    send_seq: u64,
    recv_seq: u64,
}

impl Cipher {
    pub fn new(key: [u8; 32]) -> Cipher {
        Cipher {
            enc: XChaCha20Poly1305::new((&key).into()),
            dec: XChaCha20Poly1305::new((&key).into()),
            send_seq: 0,
            recv_seq: 0,
        }
    }

    fn nonce(seq: u64) -> [u8; 24] {
        // 24 字节 XChaCha nonce：未字节为发送计数（大端），其余补零。
        // 每帧计数唯一，杜绝 nonce 重用。
        let mut n = [0u8; 24];
        n[16..24].copy_from_slice(&seq.to_be_bytes());
        n
    }

    /// 加密（payload 明文 → 密文+tag）。
    pub fn seal(&mut self, plain: &[u8]) -> Result<Vec<u8>> {
        self.send_seq += 1;
        let seq_nonce = Self::nonce(self.send_seq);
        let nonce = XNonce::from_slice(&seq_nonce);
        let ct = self
            .enc
            .encrypt(nonce, plain)
            .map_err(|e| anyhow::anyhow!("加密失败: {}", e))?;
        Ok(ct)
    }

    /// 解密（密文+tag → 明文），验证失败返回错误。
    pub fn open(&mut self, ct: &[u8]) -> Result<Vec<u8>> {
        self.recv_seq += 1;
        let seq_nonce = Self::nonce(self.recv_seq);
        let nonce = XNonce::from_slice(&seq_nonce);
        let pt = self
            .dec
            .decrypt(nonce, ct)
            .map_err(|_| anyhow::anyhow!("解密失败(密钥不符或数据被篡改)"))?;
        Ok(pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let h = encode_header(FrameType::Frame, 12345);
        let (ty, len) = parse_header(&h).unwrap();
        assert_eq!(ty, FrameType::Frame);
        assert_eq!(len, 12345);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut h = encode_header(FrameType::Frame, 1);
        h[0] = b'X';
        assert!(parse_header(&h).is_err());
    }

    #[test]
    fn cipher_roundtrip() {
        let mut c1 = Cipher::new([7u8; 32]);
        let mut c2 = Cipher::new([7u8; 32]);
        let ct = c1.seal(b"hello pairdesk").unwrap();
        let pt = c2.open(&ct).unwrap();
        assert_eq!(pt, b"hello pairdesk");
    }

    #[test]
    fn cipher_wrong_key_fails() {
        let mut c1 = Cipher::new([7u8; 32]);
        let mut c2 = Cipher::new([8u8; 32]);
        let ct = c1.seal(b"secret").unwrap();
        assert!(c2.open(&ct).is_err());
    }

    #[test]
    fn input_roundtrip() {
        for msg in [
            InputMsg::MouseMove { x: 12.5, y: -3.0 },
            InputMsg::Button { btn: 1, down: true },
            InputMsg::Scroll { dx: 0.0, dy: 2.0 },
            InputMsg::Key { keycode: 38, down: false, mods: 4 },
        ] {
            assert_eq!(InputMsg::decode(&msg.encode()).unwrap(), msg);
        }
    }

    #[test]
    fn key_derivation_agrees() {
        let vr = [1u8; 16];
        let hr = [2u8; 16];
        let k1 = session_key(&vr, &hr, "pw");
        let k2 = session_key(&vr, &hr, "pw");
        assert_eq!(k1, k2);
        let k3 = session_key(&vr, &hr, "pw-x");
        assert_ne!(k1, k3);
    }
}