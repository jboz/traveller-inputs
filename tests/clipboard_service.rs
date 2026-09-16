use std::sync::{Arc, Mutex};

use traveller::clipboard::{
    ClipboardBridge, ClipboardSource, ClipboardSyncService, digest_of,
};
use traveller::config::ClipboardConfig;

#[derive(Debug, Default)]
struct FakeSource {
    value: Arc<Mutex<String>>,
}

impl ClipboardSource for FakeSource {
    fn get(&mut self) -> anyhow::Result<String> {
        Ok(self.value.lock().unwrap().clone())
    }
    fn set(&mut self, s: &str) -> anyhow::Result<()> {
        *self.value.lock().unwrap() = s.to_string();
        Ok(())
    }
}

fn cfg(max_kb: usize) -> ClipboardConfig {
    ClipboardConfig { enabled: true, sync_interval_ms: 1, max_kb }
}

#[test]
fn envoi_seulement_si_changement_et_plafond() {
    let src = FakeSource::default();
    *src.value.lock().unwrap() = "abc".to_string();
    let mut svc = ClipboardSyncService::new(src, &cfg(4096));
    let c1 = svc.consume_local().expect("premier changement");
    assert_eq!(c1, "abc");
    assert!(svc.consume_local().is_none(), "pas de 2e envoi sans changement");
    let big = "trop long".repeat(5);
    *svc.source.value.lock().unwrap() = big.clone();
    assert_eq!(svc.consume_local(), Some(big));
}

#[test]
fn plafond_bloque() {
    let src = FakeSource::default();
    *src.value.lock().unwrap() = "x".repeat(100);
    let mut svc = ClipboardSyncService::new(src, &cfg(1));
    assert!(svc.consume_local().is_none());
    assert_eq!(svc.guard.last, None, "on ne mémorise pas un contenu trop long");
}

#[test]
fn ingestion_antiloop() {
    let src = FakeSource::default();
    let mut svc = ClipboardSyncService::new(src, &cfg(4096));
    svc.ingest("bonjour").unwrap();
    assert_eq!(svc.source.value.lock().unwrap().as_str(), "bonjour");
    // 2e réception identique : boucle → pas d'application.
    svc.ingest("bonjour").unwrap();
    assert_eq!(svc.source.value.lock().unwrap().as_str(), "bonjour");
}

#[test]
fn bridge_tick_envoie_et_ingest_applique() {
    let src = FakeSource::default();
    *src.value.lock().unwrap() = "contenu local".to_string();
    let bridge = ClipboardBridge::new(src, &cfg(4096));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = traveller::transport::TransportHandle { send: tx };
    bridge.tick_send(&handle);
    bridge.tick_send(&handle);
    bridge.ingest("depuis reseau");
    assert_eq!(bridge.inner.lock().unwrap().source.value.lock().unwrap().as_str(), "depuis reseau");
}

#[test]
fn digest_stable() {
    assert_eq!(digest_of("x"), digest_of("x"));
}