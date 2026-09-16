use traveller::client::run_client;
use traveller::input::{InputError, InputInjector};
use traveller::transport::protocol::Msg;
use traveller::transport::{ConnEvent, TransportHandle};
use tokio::sync::mpsc;

#[derive(Debug, Default)]
struct FakeInjector {
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
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

#[tokio::test]
async fn injection_depuis_messages() {
    let (tx, rx) = mpsc::unbounded_channel::<ConnEvent>();
    let (tx_msg, _rx_msg) = mpsc::unbounded_channel::<Msg>();
    let handle = TransportHandle { send: tx_msg };
    let calls = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let injector = FakeInjector { calls: calls.clone() };
    let task = tokio::spawn(run_client(handle, rx, injector));

    tx.send(ConnEvent::Msg(Msg::CursorEnter { x: 100.0, y: 200.0 })).unwrap();
    tx.send(ConnEvent::Msg(Msg::PointerMove { rel_dx: 5.0, rel_dy: 3.0 })).unwrap();
    tx.send(ConnEvent::Msg(Msg::PointerButton { button: 1, pressed: true })).unwrap();
    tx.send(ConnEvent::Msg(Msg::Key { code: 1, pressed: false })).unwrap();
    tx.send(ConnEvent::Msg(Msg::Wheel { delta_y: -120.0 })).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let logs = calls.lock().unwrap().clone();
    assert!(logs.contains(&"pos 100.0 200.0".to_string()));
    assert!(logs.contains(&"move 5.0 3.0".to_string()));
    assert!(logs.contains(&"btn 1 true".to_string()));
    assert!(logs.contains(&"key 1 false".to_string()));
    assert!(logs.contains(&"wheel -120.0".to_string()));

    tx.send(ConnEvent::Msg(Msg::Quit)).unwrap();
    task.await.unwrap().unwrap();
}