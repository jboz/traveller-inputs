//! Transport TCP+TLS : serveur et client, heartbeat, heartbeat timeout,
//! vérification post-handshake de l'empreinte du pair.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::connect_loop;
pub use server::serve_loop;

use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::security::peer_fingerprint_from_common;
use crate::transport::protocol::{read_frame, write_frame, Msg};

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct TransportHandle {
    pub send: mpsc::UnboundedSender<Msg>,
}

#[derive(Debug)]
pub enum ConnEvent {
    Msg(Msg),
    Closed(String),
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) struct SessionParts {
    pub tx_events: mpsc::UnboundedSender<ConnEvent>,
    pub rx_events: mpsc::UnboundedReceiver<ConnEvent>,
    pub handle: TransportHandle,
}

/// Vérifie l'empreinte du pair après handshake. `None` = first-run : on
/// accepte et on logue l'empreinte ; `Some(attendue)` = mismatch → refuse.
pub(crate) fn verify_peer_fp(
    common: &rustls::CommonState,
    expected_fp: Option<[u8; 32]>,
) -> anyhow::Result<()> {
    let fp = peer_fingerprint_from_common(common)?;
    match expected_fp {
        None => tracing::info!("first-run : empreinte du pair = sha256:{}", hex::encode(fp)),
        Some(exp) if exp != fp => {
            anyhow::bail!(
                "empreinte du pair sha256:{} ≠ attendue sha256:{}",
                hex::encode(fp),
                hex::encode(exp)
            )
        }
        Some(_) => tracing::debug!("empreinte du pair vérifiée"),
    }
    Ok(())
}

/// Identifiant stable de la machine, dérivé du cert local (8 premiers octets
/// du SHA-256 du DER) — utilisé dans le `Hello`.
pub(crate) fn device_id_of(cert_pem: &[u8]) -> u64 {
    let mut pem = cert_pem;
    let mut hasher = Sha256::new();
    if let Ok(Some(der)) = rustls_pemfile::read_one(&mut pem) {
        if let rustls_pemfile::Item::X509Certificate(der) = der {
            hasher.update(der.as_ref());
        }
    }
    let h = hasher.finalize();
    u64::from_be_bytes(h[..8].try_into().unwrap_or([0u8; 8]))
}

/// Spawn les tâches lecteur/écrivain partagées et retourne les poignées.
///
/// - lecteur : pousse chaque message dans `ConnEvent::Msg`, auto-répond
///   `Pong` aux `Ping` reçus, `Closed` à la fermeture.
/// - écrivain : multiplexe la file applicative + heartbeat `Ping` périodique
///   (seulement si `heartbeat` est vrai, côté contrôleur).
pub(crate) fn spawn_session<S>(
    tls_stream: S,
    heartbeat: bool,
) -> SessionParts
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (tx_events, rx_events) = mpsc::unbounded_channel();
    let (tx_send, mut rx_send) = mpsc::unbounded_channel();
    let (mut rd, mut wr) = tokio::io::split(tls_stream);

    let tx_events2 = tx_events.clone();
    let tx_send2 = tx_send.clone();
    tokio::spawn(async move {
        loop {
            match read_frame(&mut rd).await {
                Ok(Msg::Quit) => {
                    let _ = tx_events2.send(ConnEvent::Closed("quit".into()));
                    break;
                }
                Ok(Msg::Ping { ts }) => {
                    let _ = tx_events2.send(ConnEvent::Msg(Msg::Ping { ts }));
                    let _ = tx_send2.send(Msg::Pong { ts });
                }
                Ok(msg) => {
                    let _ = tx_events2.send(ConnEvent::Msg(msg));
                }
                Err(e) => {
                    tracing::debug!("lecture fermée : {e}");
                    let _ = tx_events2.send(ConnEvent::Closed(e.to_string()));
                    break;
                }
            }
        }
    });

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        interval.tick().await;
        loop {
            tokio::select! {
                maybe = rx_send.recv() => match maybe {
                    Some(msg) => {
                        if write_frame(&mut wr, &msg).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
                _ = interval.tick(), if heartbeat => {
                    if write_frame(&mut wr, &Msg::Ping { ts: now_ms() }).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    SessionParts {
        tx_events,
        rx_events,
        handle: TransportHandle { send: tx_send },
    }
}