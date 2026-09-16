pub mod rdev_backend;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Move { delta: (f64, f64) },
    Button { button: u8, pressed: bool },
    Key { code: u16, pressed: bool },
    Wheel { delta_y: f64 },
}

#[derive(Debug, Error)]
pub enum InputError {
    #[error("input io: {0}")]
    Io(#[from] std::io::Error),
    #[error("injection refusée: {0}")]
    Inject(String),
    #[error("capture indisponible: {0}")]
    Capture(String),
}

pub trait InputCapture: Send + 'static {
    fn spawn_capture(
        &self,
        tx: tokio::sync::mpsc::UnboundedSender<InputEvent>,
    ) -> Result<(), InputError>;
    fn position(&self) -> Result<(f64, f64), InputError>;
    fn set_position(&self, x: f64, y: f64) -> Result<(), InputError>;
}

pub trait InputInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InputError>;
    fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError>;
    fn button(&mut self, button: u8, pressed: bool) -> Result<(), InputError>;
    fn key(&mut self, code: u16, pressed: bool) -> Result<(), InputError>;
    fn wheel(&mut self, delta_y: f64) -> Result<(), InputError>;
}