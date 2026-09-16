use std::sync::Mutex;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::config::ClipboardConfig;
use crate::transport::protocol::Msg;
use crate::transport::TransportHandle;

#[derive(Debug, Default)]
pub struct ClipboardGuard {
    pub last: Option<[u8; 32]>,
}

impl ClipboardGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retourne true si le digest diffère du dernier connu (application légitime),
    /// false si identique (boucle : on n'applique pas).
    pub fn should_apply(&mut self, digest: [u8; 32]) -> bool {
        if self.last == Some(digest) {
            return false;
        }
        self.last = Some(digest);
        true
    }
}

pub fn digest_of(content: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    h.finalize().into()
}

pub trait ClipboardSource {
    fn get(&mut self) -> anyhow::Result<String>;
    fn set(&mut self, s: &str) -> anyhow::Result<()>;
}

/// Source presse-papiers réelle via arboard.
#[derive(Debug)]
pub struct ArboardSource;

impl ClipboardSource for ArboardSource {
    fn get(&mut self) -> anyhow::Result<String> {
        let mut cb = arboard::Clipboard::new()?;
        Ok(cb.get_text()?)
    }
    fn set(&mut self, s: &str) -> anyhow::Result<()> {
        let mut cb = arboard::Clipboard::new()?;
        cb.set_text(s.to_string())?;
        Ok(())
    }
}

/// Service de synchronisation : poll local + ingestion réseau, garde par digest.
#[derive(Debug)]
pub struct ClipboardSyncService<T: ClipboardSource> {
    pub source: T,
    pub guard: ClipboardGuard,
    pub max_kb: usize,
    pub enabled: bool,
    pub interval: Duration,
}

impl<T: ClipboardSource> ClipboardSyncService<T> {
    pub fn new(source: T, cfg: &ClipboardConfig) -> Self {
        Self {
            source,
            guard: ClipboardGuard::new(),
            max_kb: cfg.max_kb,
            enabled: cfg.enabled,
            interval: Duration::from_millis(cfg.sync_interval_ms.max(1)),
        }
    }

    /// Poll le presse-papiers local ; renvoie le contenu si changé et sous le
    /// plafond. Ne met pas à jour le garde en cas de dépassement.
    pub fn consume_local(&mut self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let content = self.source.get().ok()?;
        let d = digest_of(&content);
        if self.guard.last == Some(d) {
            return None;
        }
        if content.len() > self.max_kb {
            return None;
        }
        self.guard.last = Some(d);
        Some(content)
    }

    /// Applique un contenu reçu si digest différent (anti-boucle).
    pub fn ingest(&mut self, content: &str) -> Result<(), anyhow::Error> {
        if !self.enabled {
            return Ok(());
        }
        if content.len() > self.max_kb {
            return Ok(());
        }
        let d = digest_of(content);
        if !self.guard.should_apply(d) {
            return Ok(());
        }
        self.source.set(content)?;
        Ok(())
    }
}

/// Pont thread-safe que les boucles principale intègrent dans leur `select!`.
#[derive(Debug)]
pub struct ClipboardBridge<T: ClipboardSource> {
    pub inner: Mutex<ClipboardSyncService<T>>,
}

impl<T: ClipboardSource> ClipboardBridge<T> {
    pub fn new(source: T, cfg: &ClipboardConfig) -> Self {
        Self { inner: Mutex::new(ClipboardSyncService::new(source, cfg)) }
    }

    pub fn interval(&self) -> Duration {
        self.inner
            .lock()
            .map(|svc| svc.interval)
            .unwrap_or(Duration::from_millis(300))
    }

    /// Poll local et envoie le changement sur le réseau.
    pub fn tick_send(&self, handle: &TransportHandle) {
        if let Ok(mut svc) = self.inner.lock() {
            if let Some(content) = svc.consume_local() {
                let digest = digest_of(&content);
                let _ = handle.send.send(Msg::Clipboard { digest, content });
            }
        }
    }

    /// Applique un contenu reçu depuis le réseau.
    pub fn ingest(&self, content: &str) {
        if let Ok(mut svc) = self.inner.lock() {
            let _ = svc.ingest(content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_stables_et_distincts() {
        assert_eq!(digest_of("abc"), digest_of("abc"));
        assert_ne!(digest_of("abc"), digest_of("abd"));
        assert_eq!(hex::encode(&digest_of("")[..4]), "e3b0c442");
    }

    #[test]
    fn double_applique_refusee() {
        let mut g = ClipboardGuard::new();
        let d = digest_of("texte");
        assert!(g.should_apply(d));
        assert!(!g.should_apply(d));
        assert!(g.should_apply(digest_of("autre")));
        assert!(g.should_apply(d));
    }
}