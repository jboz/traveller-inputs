use std::time::Duration;

use traveller::config::{Config, Role, Screen, Side};
use traveller::security::{ensure_cert, fingerprint_of, parse_fingerprint};
use traveller::transport::protocol::Msg;
use traveller::transport::{ConnEvent, HEARTBEAT_INTERVAL, connect_loop, serve_loop};

fn cfg(
    role: Role,
    listen: Option<String>,
    peer_addr: Option<String>,
    cert: &std::path::Path,
    key: &std::path::Path,
    fp: Option<String>,
) -> Config {
    Config {
        role,
        listen,
        peer_addr,
        screen: Screen { width: 1920, height: 1080 },
        peer_screen: Screen { width: 2560, height: 1440 },
        side: Side::Right,
        peer_fingerprint: fp,
        cert_path: cert.to_path_buf(),
        key_path: key.to_path_buf(),
        clipboard: Default::default(),
        keys: Default::default(),
    }
}

async fn mk_pair() -> (tempfile::TempDir, [u8; 32], [u8; 32]) {
    let dir = tempfile::tempdir().unwrap();
    let controller = ensure_cert(&dir.path().join("controller.pem"), &dir.path().join("controller.key")).unwrap();
    let station = ensure_cert(&dir.path().join("station.pem"), &dir.path().join("station.key")).unwrap();
    let fp_controller = parse_fingerprint(&fingerprint_of(&controller.cert_pem).unwrap()).unwrap();
    let fp_station = parse_fingerprint(&fingerprint_of(&station.cert_pem).unwrap()).unwrap();
    (dir, fp_controller, fp_station)
}

#[tokio::test]
async fn hello_ping_pong_empreintes() {
    let (dir, fp_controller, fp_station) = mk_pair().await;

    // Port éphémère : on réserve, on récupère, on laisse.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);

    let cfg_server = cfg(
        Role::Client,
        Some(addr.clone()),
        None,
        &dir.path().join("station.pem"),
        &dir.path().join("station.key"),
        Some(format!("sha256:{}", hex::encode(fp_controller))),
    );
    let cfg_controller = cfg(
        Role::Controller,
        None,
        Some(addr),
        &dir.path().join("controller.pem"),
        &dir.path().join("controller.key"),
        Some(format!("sha256:{}", hex::encode(fp_station))),
    );

    let srv = tokio::spawn(async move {
        serve_loop(&cfg_server, Some(fp_controller)).await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let (handle, mut events) =
        connect_loop(&cfg_controller, Some(fp_station)).await.expect("session contrôleur");
    let (srv_handle, mut srv_events) = srv.await.unwrap().expect("session serveur");

    // Le contrôleur a envoyé Hello à la station.
    let hello = tokio::time::timeout(Duration::from_secs(2), srv_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(hello, ConnEvent::Msg(Msg::Hello { device_id, screen_w, screen_h })
        if screen_w == 1920 && screen_h == 1080 && device_id != 0));

    // Ping → Pong automatique.
    handle.send.send(Msg::Ping { ts: 42 }).unwrap();
    let got = tokio::time::timeout(HEARTBEAT_INTERVAL, events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(got, ConnEvent::Msg(Msg::Pong { ts: 42 })));
    // La station a reçu le Ping (relayé) et y a répondu.
    let got = tokio::time::timeout(HEARTBEAT_INTERVAL, srv_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(got, ConnEvent::Msg(Msg::Ping { ts: 42 })));

    // La station peut envoyer vers le contrôleur (bidirectionnel) : le Ping
    // arrive côté contrôleur, qui y répond automatiquement en Pong.
    srv_handle.send.send(Msg::Ping { ts: 7 }).unwrap();
    let got = tokio::time::timeout(HEARTBEAT_INTERVAL, events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(got, ConnEvent::Msg(Msg::Ping { ts: 7 })));
    let got = tokio::time::timeout(HEARTBEAT_INTERVAL, srv_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(got, ConnEvent::Msg(Msg::Pong { ts: 7 })));

    // Aucune fermeture en 200 ms.
    assert!(
        tokio::time::timeout(Duration::from_millis(200), events.recv())
            .await
            .is_err()
    );
}
