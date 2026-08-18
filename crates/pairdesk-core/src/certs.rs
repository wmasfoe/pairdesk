//! P2P 身份证书管理。
//!
//! P2P 无可信 CA，端与端之间需要一个"互相认识"的方式。方案：
//!  - 每台机器首次生成一份自签证书(公钥+私钥)，持久化到本地文件
//!    （后续跨机场景，被控端把【公钥证书】经信令发给控制端即可信任，
//!      身份认证仍由应用层的密码握手承担——证书只负责传输加密）。
//!  - 同一台机器上的两端（本机自测）共享同一证书文件，天然互认。
//!
//! 不用硬编码证书：避免 DER 抄错、避免私钥进代码（易错且不利于安全）。

use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, bail};

/// P2P 身份：公钥证书 + 私钥。
pub struct P2pIdentity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

/// 在指定目录加载或生成身份证书（生成后即持久化，之后复用）。
pub fn ensure_identity(dir: &Path) -> Result<P2pIdentity> {
    std::fs::create_dir_all(dir)?;
    let cert_path = dir.join("pairdesk.cert.der");
    let key_path = dir.join("pairdesk.key.der");

    if cert_path.exists() && key_path.exists() {
        Ok(P2pIdentity {
            cert_der: std::fs::read(cert_path)?,
            key_der: std::fs::read(key_path)?,
        })
    } else {
        let cert = rcgen::generate_simple_self_signed(vec![
            "pairdesk.local".to_string(),
            "localhost".to_string(),
        ])?;
        let cert_der = cert.cert.der().clone().to_vec();
        let key_der = cert.key_pair.serialize_der();
        std::fs::write(&cert_path, &cert_der)?;
        std::fs::write(&key_path, &key_der)?;
        Ok(P2pIdentity { cert_der, key_der })
    }
}

/// 由身份私钥构建 QUIC 服务端配置（被控端）。
pub fn server_quic_config(id: &P2pIdentity) -> Result<quinn::ServerConfig> {
    // 幂等：确保 rustls 加密提供者已就绪（任一配置入口都能自动装上）
    let _ = rustls::crypto::ring::default_provider().install_default();
    if id.cert_der.is_empty() || id.key_der.is_empty() {
        bail!("身份证书为空");
    }
    let cert = rustls::pki_types::CertificateDer::from(id.cert_der.clone());
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        id.key_der.clone(),
    ));
    let tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?;
    let crypto = Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(tls)?);
    let mut cfg = quinn::ServerConfig::with_crypto(crypto);
    cfg.transport = Arc::new(quinn::TransportConfig::default());
    Ok(cfg)
}

/// 由身份公钥构建 QUIC 客户端配置（控制端信任对端公钥）。
pub fn client_quic_config(id: &P2pIdentity) -> Result<quinn::ClientConfig> {
    // 幂等：确保 rustls 加密提供者已就绪
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = rustls::pki_types::CertificateDer::from(id.cert_der.clone());
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert)?;
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let crypto = Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(tls)?);
    Ok(quinn::ClientConfig::new(crypto))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_persistent_and_consistent() {
        let dir = std::env::temp_dir().join(format!("pd-cert-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = ensure_identity(&dir).unwrap();
        let b = ensure_identity(&dir).unwrap();
        assert_eq!(a.cert_der, b.cert_der, "同目录两次应得到同一证书");
        assert!(!a.cert_der.is_empty() && !a.key_der.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
