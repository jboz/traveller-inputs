use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

use enigo::{Coordinate, Enigo, Mouse, Settings};

use crate::input::{InputCapture, InputError, InputEvent, InputInjector};
use crate::keymap::{button_to_id, canon_to_rdev, rdev_key_to_canon};

pub struct RdevCapture {
    pub suppress: Arc<AtomicBool>,
    enigo: Mutex<Enigo>,
}

impl RdevCapture {
    pub fn new() -> Result<Self, InputError> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| InputError::Capture(format!("enigo: {e}")))?;
        Ok(Self {
            suppress: Arc::new(AtomicBool::new(false)),
            enigo: Mutex::new(enigo),
        })
    }
}

impl InputCapture for RdevCapture {
    fn spawn_capture(&self, tx: UnboundedSender<InputEvent>) -> Result<(), InputError> {
        let suppress = self.suppress.clone();
        std::thread::Builder::new()
            .name("traveller-capture".into())
            .spawn(move || {
                let mut last: Option<(f64, f64)> = None;
                let cb = move |event: rdev::Event| {
                    match event.event_type {
                        rdev::EventType::MouseMove { x, y } => {
                            let delta = match last {
                                Some((px, py)) => (x - px, y - py),
                                None => (0.0, 0.0),
                            };
                            last = Some((x, y));
                            if suppress.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = tx.send(InputEvent::Move { delta });
                        }
                        rdev::EventType::ButtonPress(b) => {
                            let _ = tx.send(InputEvent::Button {
                                button: button_to_id(&b),
                                pressed: true,
                            });
                        }
                        rdev::EventType::ButtonRelease(b) => {
                            let _ = tx.send(InputEvent::Button {
                                button: button_to_id(&b),
                                pressed: false,
                            });
                        }
                        rdev::EventType::KeyPress(k) => {
                            let code = rdev_key_to_canon(&k).code();
                            let _ = tx.send(InputEvent::Key { code, pressed: true });
                        }
                        rdev::EventType::KeyRelease(k) => {
                            let code = rdev_key_to_canon(&k).code();
                            let _ = tx.send(InputEvent::Key { code, pressed: false });
                        }
                        rdev::EventType::Wheel { delta_y, .. } => {
                            let _ = tx.send(InputEvent::Wheel { delta_y: delta_y as f64 });
                        }
                    }
                };
                if let Err(e) = rdev::listen(cb) {
                    tracing::error!("rdev::listen: {e:?}");
                }
            })
            .map_err(|e| InputError::Capture(e.to_string()))?;
        Ok(())
    }

    fn position(&self) -> Result<(f64, f64), InputError> {
        let enigo = self
            .enigo
            .lock()
            .map_err(|e| InputError::Capture(e.to_string()))?;
        let p = enigo.location().map_err(|e| InputError::Capture(e.to_string()))?;
        Ok((p.0 as f64, p.1 as f64))
    }

    fn set_position(&self, x: f64, y: f64) -> Result<(), InputError> {
        let mut enigo = self
            .enigo
            .lock()
            .map_err(|e| InputError::Capture(e.to_string()))?;
        enigo
            .move_mouse(x as i32, y as i32, Coordinate::Abs)
            .map_err(|e| InputError::Capture(e.to_string()))
    }
}

pub struct RdevInjector {
    mouse: Enigo,
}

impl RdevInjector {
    pub fn new() -> Result<Self, InputError> {
        let mouse = Enigo::new(&Settings::default())
            .map_err(|e| InputError::Inject(format!("enigo: {e}")))?;
        Ok(Self { mouse })
    }
}

impl InputInjector for RdevInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InputError> {
        self.mouse
            .move_mouse(dx as i32, dy as i32, Coordinate::Rel)
            .map_err(|e| InputError::Inject(e.to_string()))
    }

    fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError> {
        self.mouse
            .move_mouse(x as i32, y as i32, Coordinate::Abs)
            .map_err(|e| InputError::Inject(e.to_string()))
    }

    fn button(&mut self, id: u8, pressed: bool) -> Result<(), InputError> {
        let b = crate::keymap::id_to_button(id)
            .ok_or_else(|| InputError::Inject(format!("bouton {id} inconnu")))?;
        let ct = if pressed {
            rdev::EventType::ButtonPress(b)
        } else {
            rdev::EventType::ButtonRelease(b)
        };
        rdev::simulate(&ct).map_err(|e| InputError::Inject(e.to_string()))
    }

    fn key(&mut self, code: u16, pressed: bool) -> Result<(), InputError> {
        let key = crate::keymap::CanonKey::from_code(code);
        let k = canon_to_rdev(key);
        let ct = if pressed {
            rdev::EventType::KeyPress(k)
        } else {
            rdev::EventType::KeyRelease(k)
        };
        rdev::simulate(&ct).map_err(|e| InputError::Inject(e.to_string()))
    }

    fn wheel(&mut self, delta_y: f64) -> Result<(), InputError> {
        let ct = rdev::EventType::Wheel {
            delta_x: 0,
            delta_y: delta_y as i64,
        };
        rdev::simulate(&ct).map_err(|e| InputError::Inject(e.to_string()))
    }
}

impl fmt::Debug for RdevCapture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RdevCapture").field("suppress", &self.suppress).finish()
    }
}

impl fmt::Debug for RdevInjector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RdevInjector").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traits_sont_implementes() {
        fn assert_capture<T: InputCapture>() {}
        fn assert_injector<T: InputInjector>() {}
        assert_capture::<RdevCapture>();
        assert_injector::<RdevInjector>();
    }
}