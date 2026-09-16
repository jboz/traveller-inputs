use std::sync::Arc;

use crate::config::Config;
use crate::security::{client_tls_config, ensure_cert};
use crate::transport::protocol::Msg;
use crate::transport::{ConnEvent, SessionParts, TransportHandle, device_id_of, verify_peer_fp};
use rustls::pki_types::ServerName;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// Se connecte au serveur (nœud « client »). Boucle de reconnexion avec
/// backoff exponentiel (1 s, 2 s, … cap 30 s) tant que la session échoue.
/// À l'ouverture d'une session valide, envoie le `Hello` (géométrie locale),
/// active le heartbeat `Ping` et retourne handle + récepteur d'événements.
pub async fn connect_loop(
    cfg: &Config,
    expected_fp: Option<[u8; 32]>,
) -> anyhow::Result<(TransportHandle, tokio::sync::mpsc::UnboundedReceiver<ConnEvent>)> {
    let store = ensure_cert(&cfg.cert_path, &cfg.key_path)?;
    let tls = Arc::new(client_tls_config(&store.cert_pem, &store.key_pem)?);
    let connector = TlsConnector::from(tls);
    let addr = cfg
        .peer_addr
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("peer_addr requis côté contrôleur"))?;
    let device_id = device_id_of(&store.cert_pem);
    let screen = cfg.screen;

    let mut backoff = Duration::from_secs(1);
    loop {
        let tcp = match TcpStream::connect(addr).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("connexion à {addr} échouée : {e}; retry dans {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        tcp.set_nodelay(true).ok();
        let server_name = match ServerName::try_from("traveller.local") {
            Ok(n) => n,
            Err(_) => ServerName::try_from("localhost").unwrap(),
        };
        let tls_stream = match connector.clone().connect(server_name, tcp).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("handshake TLS échoué : {e}; retry dans {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        let common = tls_stream.get_ref().1;
        if let Err(e) = verify_peer_fp(common, expected_fp) {
            tracing::warn!("empreinte refusée : {e}; retry dans {backoff:?}");
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }
        tracing::info!("session TLS établie avec {addr}");

        let SessionParts {
            tx_events: _,
            rx_events,
            handle,
        } = crate::transport::spawn_session(tls_stream, true);

        handle.send.send(Msg::Hello {
            device_id,
            screen_w: screen.width as u16,
            screen_h: screen.height as u16,
        })?;

        return Ok((handle, rx_events));
    }
}