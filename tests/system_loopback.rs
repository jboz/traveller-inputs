use std::sync::{Arc, Mutex};
use std::time::Duration;

use traveller::clipboard::{ClipboardBridge, ClipboardSource};
use traveller::client::run_client;
use traveller::config::{ClipboardConfig, Config, KeysConfig, Role, Screen, Side};
use traveller::controller::run_controller;
use traveller::input::{InputCapture, InputError, InputEvent, InputInjector};
use traveller::security::ensure_cert;
use traveller::transport::protocol::Msg;
use traveller::transport::{connect_loop, serve_loop};

/// Capture scriptée : émet un mouvement absolu puis s'arrête.
#[derive(Debug, Default)]
struct ScriptInput;

impl InputCapture for ScriptInput {
    fn spawn_capture(
        &self,
        tx: tokio::sync::mpsc::UnboundedSender<InputEvent>,
    ) -> Result<(), InputError> {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let _ = tx.send(InputEvent::Move { delta: (1.0, 0.0) });
            std::thread::sleep(Duration::from_millis(50));
            let _ = tx.send(InputEvent::Move { delta: (1.0, 0.0) });
        });
        Ok(())
    }
    fn position(&self) -> Result<(f64, f64), InputError> { Ok((1919.0, 500.0)) }
    fn set_position(&self, _x: f64, _y: f64) -> Result<(), InputError> { Ok(()) }
}

#[derive(Debug, Default)]
struct FakeInjector {
    calls: Arc<Mutex<Vec<String>>>,
}

impl InputInjector for FakeInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InputError> {
        self.calls.lock().unwrap().push(format!("move {dx:?} {dy:?}"));
        Ok(())
    }
    fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError> {
        self.calls.lock().unwrap().push(format!("pos {x:?} {y:?}"));
        Ok(())
    }
    fn button(&mut self, button: u8, pressed: bool) -> Result<(), InputError> {
        self.calls.lock().unwrap().push(format!("btn {button} {pressed}"));
        Ok(())
    }
    fn key(&mut self, code: u16, pressed: bool) -> Result<(), InputError> {
        self.calls.lock().unwrap().push(format!("key {code} {pressed}"));
        Ok(())
    }
    fn wheel(&mut self, delta_y: f64) -> Result<(), InputError> {
        self.calls.lock().unwrap().push(format!("wheel {delta_y:?}"));
        Ok(())
    }
}

#[derive(Debug, Default)]
struct NoopSource(Arc<Mutex<String>>);

impl ClipboardSource for NoopSource {
    fn get(&mut self) -> anyhow::Result<String> { Ok(self.0.lock().unwrap().clone()) }
    fn set(&mut self, s: &str) -> anyhow::Result<()> {
        *self.0.lock().unwrap() = s.to_string(); Ok(())
    }
}

fn cfg_pair() -> (Config, Config) {
    let dir = tempfile::tempdir().unwrap();
    let (cc, ck) = (dir.path().join("ctrl.pem"), dir.path().join("ctrl.key"));
    let (sc, sk) = (dir.path().join("cli.pem"), dir.path().join("cli.key"));
    ensure_cert(&cc, &ck).unwrap();
    ensure_cert(&sc, &sk).unwrap();

    let disabled = ClipboardConfig { enabled: false, sync_interval_ms: 1, max_kb: 1024 };
    let keys = KeysConfig { emergency_stop: "Ctrl+Alt+F10".into() };
    let s1920 = Screen { width: 1920, height: 1080 };

    let ctrl = Config {
        role: Role::Controller, listen: None, peer_addr: Some("127.0.0.1:19801".into()),
        screen: s1920, peer_screen: s1920, side: Side::Right, peer_fingerprint: None,
        cert_path: cc, key_path: ck, clipboard: disabled.clone(), keys: keys.clone(),
    };
    let cli = Config {
        role: Role::Client, listen: Some("127.0.0.1:19801".into()), peer_addr: None,
        screen: s1920, peer_screen: s1920, side: Side::Right, peer_fingerprint: None,
        cert_path: sc, key_path: sk, clipboard: disabled, keys,
    };
    (ctrl, cli)
}

#[tokio::test]
async fn traversee_e2e_loopback() {
    let (ctrl_cfg, cli_cfg) = cfg_pair();
    let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

    // Serveur côté « machine contrôlée ».
    let cli_cfg2 = cli_cfg.clone();
    let srv_task = tokio::spawn(async move { serve_loop(&cli_cfg2, None).await });

    // Contrôleur se connecte.
    let (ctl_handle, ctl_events) = connect_loop(&ctrl_cfg, None).await.unwrap();
    let (srv_handle, srv_events) = srv_task.await.unwrap().unwrap();

    let ctrl_bridge = ClipboardBridge::new(NoopSource::default(), &ctrl_cfg.clipboard);
    let cli_bridge = ClipboardBridge::new(NoopSource::default(), &cli_cfg.clipboard);

    // Clone les senders avant de déplacer les handles dans les tasks.
    let ctrl_send = ctl_handle.send.clone();
    let srv_send = srv_handle.send.clone();

    let client_task = tokio::spawn(run_client(
        srv_handle, srv_events,
        FakeInjector { calls: logs.clone() }, cli_bridge,
    ));
    let ctrl_task = tokio::spawn(run_controller(
        Arc::new(ctrl_cfg), ctl_handle, ctl_events,
        ScriptInput, ctrl_bridge,
    ));

    // Attendre l'injection (2 mouvements + park de 25 ms ≤ 200 ms).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let now = logs.lock().unwrap().clone();
        if now.iter().any(|l| l.starts_with("pos")) && now.iter().any(|l| l.starts_with("move")) {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "injection non reçue: {now:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let logs = logs.lock().unwrap().clone();
    assert!(logs.contains(&"pos 0.0 500.0".to_string()), "{logs:?}");
    assert!(logs.contains(&"move 1.0 0.0".to_string()), "{logs:?}");

    // Quit propre des deux côtés.
    let _ = ctrl_send.send(Msg::Quit);
    let _ = srv_send.send(Msg::Quit);

    tokio::time::timeout(Duration::from_secs(3), ctrl_task).await.unwrap().unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(3), client_task).await.unwrap().unwrap().unwrap();
}