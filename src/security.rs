//! Sécurité : certificats auto-signés, empreintes SHA-256 (pin à la main) et
//! configuration TLS rustls. Le pinning est vérifié APRÈS le handshake sur
//! l'empreinte DER exacte du pair (politique first-run gérée par l'appelant).

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};

/// Charge le certificat/clé s'ils existent, sinon les génère (self-signed) et
/// les persiste. Renvoie les PEM bruts.
pub fn ensure_cert(cert_path: &Path, key_path: &Path) -> anyhow::Result<CertStore> {
    if cert_path.exists() && key_path.exists() {
        return Ok(CertStore {
            cert_pem: std::fs::read(cert_path)?,
            key_pem: std::fs::read(key_path)?,
        });
    }
    let (cert_pem, key_pem) = generate_pair_pem()?;
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(cert_path, &cert_pem)?;
    std::fs::write(key_path, &key_pem)?;
    Ok(CertStore { cert_pem, key_pem })
}

pub struct CertStore {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
}

/// Génère une paire auto-signée (rcgen) et retourne (cert_pem, key_pem).
pub fn generate_pair_pem() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let kc = generate_simple_self_signed(vec!["traveller.local".to_string()])?;
    Ok((
        kc.cert.pem().into_bytes(),
        kc.key_pair.serialize_pem().into_bytes(),
    ))
}

/// Empreinte "sha256:<64 hex>" calculée sur le DER du certificat.
pub fn fingerprint_of(cert_pem: &[u8]) -> anyhow::Result<String> {
    let cert = first_cert_der(cert_pem)?;
    let h = Sha256::digest(cert.as_ref());
    Ok(format!("sha256:{}", hex::encode(h)))
}

/// "sha256:hex" → tableau de 32 octets.
pub fn parse_fingerprint(f: &str) -> anyhow::Result<[u8; 32]> {
    let h = f.strip_prefix("sha256:").context("format sha256:<hex>")?;
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("empreinte invalide");
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}

/// Empreinte SHA-256 brute du DER du pair, après handshake.
pub fn peer_fingerprint_from_conn(conn: &rustls::Connection) -> anyhow::Result<[u8; 32]> {
    let certs = conn.peer_certificates().context("aucun certificat pair")?;
    let der = certs.first().context("liste vide")?;
    Ok(Sha256::digest(der.as_ref()).into())
}

pub fn server_tls_config(cert_pem: &[u8], key_pem: &[u8]) -> anyhow::Result<rustls::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut &cert_pem[..]).collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut &key_pem[..])?.context("clé privée introuvable")?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Ok(rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)?)
}

/// Verifier no-op : le pinning est vérifié APRÈS le handshake sur le DER exact.
#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn client_tls_config() -> anyhow::Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth())
}

fn first_cert_der(pem: &[u8]) -> anyhow::Result<CertificateDer<'static>> {
    let mut certs = rustls_pemfile::certs(&mut &pem[..]).collect::<Result<Vec<_>, _>>()?;
    certs.pop().context("certificat vide")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empreintes_distinctes_entre_paires() {
        let (a_cert, _) = generate_pair_pem().unwrap();
        let (b_cert, _) = generate_pair_pem().unwrap();
        let fa = fingerprint_of(&a_cert).unwrap();
        let fb = fingerprint_of(&b_cert).unwrap();
        assert_ne!(fa, fb);
        assert_eq!(fa.len(), 7 + 64);
        assert!(fa.starts_with("sha256:"));
    }

    #[test]
    fn parse_fingerprint_roundtrip() {
        let (a_cert, _) = generate_pair_pem().unwrap();
        let s = fingerprint_of(&a_cert).unwrap();
        let bytes = parse_fingerprint(&s).unwrap();
        assert_eq!(bytes.len(), 32);
        assert!(parse_fingerprint("md5:abcd").is_err());
        assert!(parse_fingerprint("sha256:zz").is_err());
    }

    #[test]
    fn ensure_cert_generates_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("data").join("cert.pem");
        let key_path = dir.path().join("data").join("key.pem");
        let first = ensure_cert(&cert_path, &key_path).unwrap();
        let second = ensure_cert(&cert_path, &key_path).unwrap();
        assert_eq!(first.cert_pem, second.cert_pem);
        assert_eq!(first.key_pem, second.key_pem);
        assert!(cert_path.exists());
    }

    /// Handshake TLS synchrone complet : le client récupère l'empreinte du
    /// serveur via peer_fingerprint_from_conn, et elle égale fingerprint_of.
    #[test]
    fn loopback_handshake_retourne_empreinte_serveur() {
        let (s_cert, s_key) = generate_pair_pem().unwrap();
        let (c_cert, _c_key) = generate_pair_pem().unwrap();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let expected = fingerprint_of(&s_cert).unwrap();
        let wrong = fingerprint_of(&c_cert).unwrap();
        assert_ne!(expected, wrong);

        let server = std::thread::spawn(move || {
            let cfg = server_tls_config(&s_cert, &s_key).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut conn = rustls::ServerConnection::new(Arc::new(cfg)).unwrap();
            let _ = conn.complete_io(&mut stream);
        });

        let client_conn = rustls::ClientConnection::new(
            client_tls_config().unwrap().into(),
            "traveller.local".try_into().unwrap(),
        )
        .unwrap();
        let mut conn = rustls::Connection::Client(client_conn);
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let _ = conn.complete_io(&mut stream);

        let fp = peer_fingerprint_from_conn(&conn).unwrap();
        assert_eq!(hex::encode(fp), expected.trim_start_matches("sha256:"));
        assert_ne!(hex::encode(fp), wrong.trim_start_matches("sha256:"));
        server.join().unwrap();
    }
}