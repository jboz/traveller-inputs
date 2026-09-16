use tokio::sync::mpsc;

use crate::input::InputInjector;
use crate::transport::protocol::Msg;
use crate::transport::{ConnEvent, TransportHandle};

/// Boucle du nœud « client » (machine contrôlée) : réceptionne les messages
/// applicatifs du contrôleur et les injecte localement.
pub async fn run_client<J: InputInjector>(
    handle: TransportHandle,
    mut events: mpsc::UnboundedReceiver<ConnEvent>,
    mut injector: J,
) -> anyhow::Result<()> {
    while let Some(ce) = events.recv().await {
        match ce {
            ConnEvent::Msg(Msg::CursorEnter { x, y }) => injector.set_position(x, y)?,
            ConnEvent::Msg(Msg::CursorLeave) => {}
            ConnEvent::Msg(Msg::PointerMove { rel_dx, rel_dy }) => {
                injector.move_relative(rel_dx, rel_dy)?
            }
            ConnEvent::Msg(Msg::PointerButton { button, pressed }) => {
                injector.button(button, pressed)?
            }
            ConnEvent::Msg(Msg::Key { code, pressed }) => injector.key(code, pressed)?,
            ConnEvent::Msg(Msg::Wheel { delta_y }) => injector.wheel(delta_y)?,
            // Clipboard géré par le service presse-papiers (Task 14).
            ConnEvent::Msg(Msg::Clipboard { .. }) => {}
            ConnEvent::Msg(Msg::Ping { ts }) => {
                let _ = handle.send.send(Msg::Pong { ts });
            }
            ConnEvent::Msg(Msg::Pong { .. }) => {}
            ConnEvent::Msg(Msg::Hello { .. }) => tracing::info!("pair déclaré"),
            ConnEvent::Msg(Msg::Quit) => break,
            ConnEvent::Closed(e) => {
                tracing::info!("session fermée: {e}");
                break;
            }
        }
    }
    Ok(())
}