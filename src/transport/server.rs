use std::sync::Arc;

use crate::config::Config;
use crate::security::{ensure_cert, server_tls_config};
use crate::transport::{ConnEvent, SessionParts, TransportHandle, verify_peer_fp};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// Écoute en boucle, renvoie la première session TLS validée (empreinte OK).
/// Le serveur accepte aussi les sessions dont la vérification échoue et
/// boucle jusqu'à la première connexion légitime (pour le test V1).
pub async fn serve_loop(
    cfg: &Config,
    expected_fp: Option<[u8; 32]>,
) -> anyhow::Result<(TransportHandle, tokio::sync::mpsc::UnboundedReceiver<ConnEvent>)> {
    let store = ensure_cert(&cfg.cert_path, &cfg.key_path)?;
    let tls = Arc::new(server_tls_config(&store.cert_pem, &store.key_pem)?);
    let acceptor = TlsAcceptor::from(tls);
    let listener = TcpListener::bind(cfg.listen.as_deref().unwrap()).await?;
    tracing::info!("serveur à l'écoute sur {}", cfg.listen.as_deref().unwrap());

    loop {
        let (tcp, peer) = listener.accept().await?;
        tcp.set_nodelay(true).ok();
        tracing::info!("connexion entrante de {peer}");
        let acceptor = acceptor.clone();
        let exp = expected_fp;
        match acceptor.accept(tcp).await {
            Ok(tls_stream) => {
                let common = tls_stream.get_ref().1;
                if let Err(e) = verify_peer_fp(common, exp) {
                    tracing::warn!("empreinte refusée : {e}");
                    continue;
                }
                let SessionParts {
                    tx_events: _,
                    rx_events,
                    handle,
                } = crate::transport::spawn_session(tls_stream, false);
                // Le serveur (nœud « client ») n'émet pas de `Hello` :
                // seul le contrôleur s'annonce (connect_loop).
                return Ok((handle, rx_events));
            }
            Err(e) => {
                tracing::debug!("TLS handshake refusé : {e}");
                continue;
            }
        }
    }
}