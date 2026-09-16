use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_FRAME: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    Hello { device_id: u64, screen_w: u16, screen_h: u16 },
    CursorEnter { x: f64, y: f64 },
    CursorLeave,
    PointerMove { rel_dx: f64, rel_dy: f64 },
    PointerButton { button: u8, pressed: bool },
    Key { code: u16, pressed: bool },
    Wheel { delta_y: f64 },
    Clipboard { digest: [u8; 32], content: String },
    Ping { ts: u64 },
    Pong { ts: u64 },
    Quit,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("encodage: {0}")]
    Encode(#[from] bincode::Error),
    #[error("décodage: {0}")]
    Decode(bincode::Error),
    #[error("frame trop grande ({0} octets, max {MAX_FRAME})")]
    TooLarge(usize),
    #[error("flux tronqué")]
    Truncated,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub fn encode(msg: &Msg) -> Result<Vec<u8>, ProtocolError> {
    Ok(bincode::serialize(msg)?)
}

pub fn decode(bytes: &[u8]) -> Result<Msg, ProtocolError> {
    bincode::deserialize(bytes).map_err(ProtocolError::Decode)
}

pub fn frame(payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if payload.len() > MAX_FRAME {
        return Err(ProtocolError::TooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub async fn write_frame<S: AsyncWrite + Unpin>(s: &mut S, msg: &Msg) -> Result<(), ProtocolError> {
    let payload = encode(msg)?;
    let framed = frame(&payload)?;
    s.write_all(&framed).await?;
    Ok(())
}

pub async fn read_frame<S: AsyncRead + Unpin>(s: &mut S) -> Result<Msg, ProtocolError> {
    let mut len = [0u8; 4];
    s.read_exact(&mut len).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ProtocolError::Truncated
        } else {
            ProtocolError::Io(e)
        }
    })?;
    let n = u32::from_be_bytes(len) as usize;
    if n > MAX_FRAME {
        return Err(ProtocolError::TooLarge(n));
    }
    let mut payload = vec![0u8; n];
    s.read_exact(&mut payload).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ProtocolError::Truncated
        } else {
            ProtocolError::Io(e)
        }
    })?;
    decode(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_tous_messages() {
        let msgs = vec![
            Msg::Hello { device_id: 7, screen_w: 1920, screen_h: 1080 },
            Msg::CursorEnter { x: 0.0, y: 540.5 },
            Msg::CursorLeave,
            Msg::PointerMove { rel_dx: -1.25, rel_dy: 2.5 },
            Msg::PointerButton { button: 1, pressed: true },
            Msg::Key { code: 38, pressed: false },
            Msg::Wheel { delta_y: -120.0 },
            Msg::Clipboard { digest: [7u8; 32], content: "bonjour".into() },
            Msg::Ping { ts: 123 },
            Msg::Pong { ts: 123 },
            Msg::Quit,
        ];
        for m in &msgs {
            let bytes = encode(m).unwrap();
            assert_eq!(decode(&bytes).unwrap(), *m);
        }
    }

    #[tokio::test]
    async fn frame_roundtrip_et_taille() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let msg = Msg::PointerMove { rel_dx: 1.0, rel_dy: 1.0 };
        write_frame(&mut a, &msg).await.unwrap();
        assert_eq!(read_frame(&mut b).await.unwrap(), msg);
    }

    #[tokio::test]
    async fn frame_trop_grande_fermee() {
        let payload = vec![0u8; MAX_FRAME + 1];
        let err = frame(&payload).unwrap_err();
        assert!(matches!(err, ProtocolError::TooLarge(_)));
    }
}