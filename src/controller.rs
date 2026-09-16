use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::clipboard::{ClipboardBridge, ClipboardSource};
use crate::coalescer::Coalescer;
use crate::config::Config;
use crate::focus::{FocusState, StepOutcome};
use crate::geometry::{Geometry, Side};
use crate::input::{InputCapture, InputEvent};
use crate::keymap::{CanonKey, parse_emergency_combo};
use crate::transport::protocol::Msg;
use crate::transport::{ConnEvent, TransportHandle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoreAction {
    None,
    Enter { y: f64 },
    Move { dx: f64, dy: f64 },
    Leave { x: f64, y: f64 },
}

#[derive(Debug)]
pub struct ControllerCore {
    pub focus: FocusState,
    pub coalescer: Coalescer,
    pub geom: Geometry,
}

impl ControllerCore {
    pub fn new(geom: Geometry) -> Self {
        Self { focus: FocusState::new(), coalescer: Coalescer::new(), geom }
    }

    pub fn handle_move(&mut self, pos: (f64, f64), delta: (f64, f64)) -> CoreAction {
        match self.focus.step(&self.geom, pos, delta) {
            StepOutcome::RemainLocal => CoreAction::None,
            StepOutcome::EnterRemote { y } => CoreAction::Enter { y },
            StepOutcome::RemainRemote => {
                self.coalescer.push(delta.0, delta.1);
                CoreAction::Move { dx: delta.0, dy: delta.1 }
            }
            StepOutcome::ExitRemote { x, y } => {
                if let Some((dx, dy)) = self.coalescer.take() {
                    tracing::debug!("delta résiduel ignoré: {dx:.2},{dy:.2}");
                }
                CoreAction::Leave { x, y }
            }
        }
    }

    pub fn force_local(&mut self) {
        self.focus.force_local();
        self.coalescer = Coalescer::new();
    }
}

/// Pose le curseur (parking) en supprimant la capture de nos propres
/// `set_position` pendant ~25 ms.
async fn park<C: InputCapture>(capture: &C, x: f64, y: f64) {
    capture.set_suppress(true);
    let _ = capture.set_position(x, y);
    tokio::time::sleep(Duration::from_millis(25)).await;
    capture.set_suppress(false);
}

fn geom_from(cfg: &Config) -> Geometry {
    Geometry {
        local: crate::geometry::ScreenSize { width: cfg.screen.width, height: cfg.screen.height },
        peer: crate::geometry::ScreenSize {
            width: cfg.peer_screen.width,
            height: cfg.peer_screen.height,
        },
        side: match cfg.side {
            crate::config::Side::Right => Side::Right,
            crate::config::Side::Left => Side::Left,
        },
    }
}

pub async fn run_controller<C: InputCapture, T: ClipboardSource>(
    cfg: Arc<Config>,
    handle: TransportHandle,
    mut events: mpsc::UnboundedReceiver<ConnEvent>,
    capture: C,
    clipboard: ClipboardBridge<T>,
) -> anyhow::Result<()> {
    let geom = geom_from(&cfg);
    let mut core = ControllerCore::new(geom);
    let (tx_input, mut rx_input) = mpsc::unbounded_channel::<InputEvent>();
    capture.spawn_capture(tx_input)?;
    let mut cb_interval = tokio::time::interval(clipboard.interval());
    cb_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    cb_interval.tick().await;
    let emergency = parse_emergency_combo(&cfg.keys.emergency_stop)?;
    let mut pressed_keys: HashSet<CanonKey> = HashSet::new();
    tracing::info!("contrôleur actif (focus local)");

    loop {
        tokio::select! {
            ev = rx_input.recv() => match ev {
                Some(InputEvent::Move { delta }) => {
                    let pos = capture.position().unwrap_or((0.0, 0.0));
                    match core.handle_move(pos, delta) {
                        CoreAction::None => {}
                        CoreAction::Enter { y } => {
                            let _ = handle.send.send(Msg::CursorEnter {
                                x: geom.entry_x(),
                                y,
                            });
                            park(&capture, geom.back_x(), geom.map_back_y(y)).await;
                        }
                        CoreAction::Move { dx, dy } => {
                            let _ = handle.send.send(Msg::PointerMove { rel_dx: dx, rel_dy: dy });
                        }
                        CoreAction::Leave { x, y } => {
                            let _ = handle.send.send(Msg::CursorLeave);
                            park(&capture, x, y).await;
                        }
                    }
                }
                Some(InputEvent::Button { button, pressed }) => {
                    if core.focus.is_remote() {
                        let _ = handle.send.send(Msg::PointerButton { button, pressed });
                    }
                }
                Some(InputEvent::Key { code, pressed: down }) => {
                    let canon = CanonKey::from_code(code);
                    if down { pressed_keys.insert(canon); } else { pressed_keys.remove(&canon); }
                    if down && emergency.iter().all(|k| pressed_keys.contains(k)) {
                        tracing::warn!("arrêt d'urgence détecté ({})", cfg.keys.emergency_stop);
                        core.force_local();
                        return Ok(());
                    }
                    if core.focus.is_remote() {
                        let _ = handle.send.send(Msg::Key { code, pressed: down });
                    }
                }
                Some(InputEvent::Wheel { delta_y }) => {
                    if core.focus.is_remote() {
                        let _ = handle.send.send(Msg::Wheel { delta_y });
                    }
                }
                None => {
                    tracing::warn!("capture fermée");
                    break;
                }
            },
            ce = events.recv() => match ce {
                Some(ConnEvent::Msg(Msg::Ping { .. })) => {
                    // le transport répond déjà Pong automatiquement
                }
                Some(ConnEvent::Msg(Msg::Clipboard { content, .. })) => {
                    clipboard.ingest(&content);
                }
                Some(ConnEvent::Msg(Msg::Quit)) => break,
                Some(ConnEvent::Closed(e)) => {
                    tracing::warn!("session fermée: {e}");
                    core.force_local();
                }
                // Autres messages ignorés.
                Some(ConnEvent::Msg(_)) => {}
                None => break,
            },
            _ = cb_interval.tick() => {
                clipboard.tick_send(&handle);
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::ScreenSize;

    fn geom() -> Geometry {
        Geometry {
            local: ScreenSize { width: 1920, height: 1080 },
            peer: ScreenSize { width: 1920, height: 1080 },
            side: Side::Right,
        }
    }

    #[test]
    fn routage_local_puis_remote_puis_retour() {
        let mut core = ControllerCore::new(geom());
        assert!(matches!(core.handle_move((500.0, 300.0), (3.0, 0.0)), CoreAction::None));
        assert!(matches!(core.handle_move((1920.0, 300.0), (3.0, 0.0)), CoreAction::Enter { y } if y == 300.0));
        assert!(core.focus.is_remote());
        assert!(matches!(core.handle_move((0.0, 0.0), (3.0, 0.0)), CoreAction::Move { dx: 3.0, dy: 0.0 }));
        assert!(matches!(core.handle_move((0.0, 0.0), (-1.0, 0.0)), CoreAction::Move { dx: -1.0, dy: 0.0 }));
        // virt_x = 2 → -2 → 0 avec dx<0 → retour
        let out = core.handle_move((0.0, 0.0), (-2.0, 0.0));
        assert!(matches!(out, CoreAction::Leave { x, y } if x == 1919.0 && y == 300.0));
        assert!(!core.focus.is_remote());
    }

    #[test]
    fn force_local_remet_a_zero() {
        let mut core = ControllerCore::new(geom());
        core.handle_move((1920.0, 540.0), (1.0, 0.0));
        assert!(core.focus.is_remote());
        core.coalescer.push(0.5, 0.5);
        core.force_local();
        assert!(!core.focus.is_remote());
        assert_eq!(core.coalescer.take(), None);
    }

    #[test]
    fn clamp_vertical_remote() {
        let mut core = ControllerCore::new(geom());
        core.handle_move((1920.0, 100.0), (1.0, 0.0));
        let _ = core.handle_move((0.0, 0.0), (0.0, 2000.0));
        assert_eq!(core.focus.virt_y, 1079.0);
        let _ = core.handle_move((0.0, 0.0), (0.0, -4000.0));
        assert_eq!(core.focus.virt_y, 0.0);
    }
}