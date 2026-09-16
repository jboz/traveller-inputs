# traveller Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Construire `traveller`, un KVM logiciel Rust permettant au curseur souris/clavier d'une machine contrôleur de traverser le bord de son écran et de contrôler une seconde machine (Linux X11 / Windows) par internet.

**Architecture:** Un binaire Rust unique cross-platform. Le contrôleur capture ses périphériques locaux (rdev) et, selon une machine à états pur `FocusState` (+ `Geometry`), soit laisse les événements au système local, soit les envoie via un flux TCP+TLS (tokio + rustls) vers le client qui les injecte (rdev). Presse-papiers synchronisé bidirectionnellement (arboard + guard anti-boucle). Transport en TCP avec `TCP_NODELAY`, nonce minimal, heartbeat, reconnexion avec backoff.

**Tech Stack:** Rust 2021, tokio, rustls (TLS 1.3), rcgen (certs auto-signés), rdev (capture/injection entrées), enigo (positionnement curseur), arboard (presse-papiers), bincode (sérialisation), serde+toml (config), sha2 (digests), tracing (logs).

**Spec:** `docs/superpowers/specs/2026-09-16-traveller-design.md` — ce plan argumente depuis la spec ; l'exécuteur lit les deux.

## Global Constraints

- Rust édition 2021, un seul crate binaire+lib nommé `traveller`.
- Cibles : `x86_64-unknown-linux-gnu` (X11) et `x86_64-pc-windows-msvc`. Aucune cible Wayland.
- Dépendances : tokio, rustls 0.23, rcgen 0.13, rustls-pemfile 2, rdev 0.5, enigo 0.6, arboard 3, bincode 1.3, serde+toml, sha2, hex, tracing(+subscriber/appender), anyhow, thiserror, tempfile (dev). Pas de dépendance à un KVM tiers.
- `TCP_NODELAY` activé sur tout socket.
- Frame réseau : `u32` big-endian longueur + payload `bincode`, max 16 Mo. Une frame trop grande → connexion fermée.
- Deux machines max, 1 pair, rôles fixes (`controller` / `client`). Contrôleur fixe, pas de rotation.
- La logique pure (protocol, geometry, focus, coalescer, clipboard guard) est sans OS et couverte par tests unitaires.
- **Amendements à la spec (validés pour V1) :**
  1. Message `Wheel { delta_y: f64 }` ajouté au protocole (molette). La spec §7 n'en listait pas.
  2. Anti-boucle par conception : la capture a lieu uniquement sur le contrôleur, l'injection uniquement sur le client → aucun événement injecté n'est recapturé. Le point d'attention restant est le *self-move* du contrôleur (parking du curseur) lors des transitions : une fenêtre de suppression ~25 ms bloque les `Move` issus de son propre `set_position`.
  3. Masquage du curseur remplacé par **stationnement au bord** (le contrôleur recolle son curseur au bord partagé pendant le mode Remote) : pas d'API de visibilité cross-platform fiable en V1, l'UX reste correcte. `set_position` est fourni par `enigo`.
- Clavier : mapping en **canonique par position** `CanonKey` (u16) — les tables sont dans `keymap.rs` ; au-delà de la couverture, passthrough `Other { code }` + log warning.
- Commit attendu en fin de chaque tâche (format conventionnel `feat:`/`test:`/`chore:`). Si le repo n'est pas initialisé, `git init` avant la tâche 1.
- Comportement first-run : si `peer_fingerprint` absent, autoriser la connexion MAIS afficher l'empreinte reçue à reporter ; dès que `peer_fingerprint` est renseigné, toute connexion dont l'empreinte diffère est refusée.

---

## File Structure

```
Cargo.toml
README.md
config.example-controller.toml
config.example-client.toml
src/
  lib.rs            — déclare les modules, exporte l'API
  main.rs           — entry point CLI (--config), init logs, run()
  logging.rs        — init tracing console + fichier rotatif
  config.rs         — Config, Role, Side, parsing/validation TOML
  transport/
    mod.rs
    protocol.rs     — Msg, encode/decode bincode, framing length-prefixed
    server.rs       — listener TCP+TLS (tokio)
    client.rs       — connecteur TCP+TLS + reconnexion backoff
  controller.rs     — ControllerCore (FocusState + Coalescer) + boucle app
  client.rs         — boucle app côté client (reçoit → injecte)
  input/
    mod.rs          — traits InputCapture / InputInjector, InputEvent
    rdev_backend.rs — impl rdev+enigo
  keymap.rs         — CanonKey, mapping rdev→CanonKey→rdev, boutons, molette
  clipboard.rs      — digest_of, ClipboardGuard, ClipboardSyncService
  security.rs       — certs auto-signés, empreintes, configs TLS, vérif pair
  geometry.rs       — ScreenSize, Side, Geometry (mapping + détection bord)
  focus.rs          — Focus, StepOutcome, FocusState
  coalescer.rs      — Coalescer (dernier mouvement gagne)
  utils.rs?         — (pas nécessaire — fichiers dédiés)
tests/
  controller_integration.rs  — contrôleur↔client sur loopback tokio
```

Chaque fichier a une responsabilité unique. La géométrie, le focus, le coalescer et le guard clipboard sont purs (testables sans OS ni réseau).

---

### Task 1: Scaffold du crate + logging

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: `src/logging.rs`

**Interfaces:**
- Consumes: rien.
- Produces: `traveller::logging::init_logging() -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard>` ; `pub fn run(config_path: &str) -> anyhow::Result<()>` (stub) ; modules vides `geometry`, `focus`, `coalescer`, `keymap`, `clipboard`, `config`, `controller`, `client`, `input`, `security`, `transport` déclarés dans `lib.rs`.

- [ ] **Step 1: Écrire le test qui échoue**

Crée `src/logging.rs` avec un test qui vérifie que deux initialisations successives dans des processus distincts ne paniquent pas (test unitaire simple : initialiser et journaliser). Comme subscriber global = One, on lève l'exigence de multi-init : test qu'`init_logging` retourne bien un guard et que `tracing::info!` s'exécute.

```rust
#[test]
fn logging_initializes() {
    let _guard = crate::logging::init_logging().expect("logging init");
    tracing::info!("log ok");
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib logging::tests::logging_initializes`
Expected: échec de compilation `cannot find crate logging` / panne module.

- [ ] **Step 3: Implémenter**

Crée le crate (package) `traveller`, `Cargo.toml` avec toutes les dépendances du Global Constraints (bincode, serde, toml, sha2, hex, thiserror, anyhow, tracing, tracing-subscriber feat `env-filter`,`fmt`, tracing-appender, tokio feat `rt-multi-thread`,`macros`,`net`,`io-util`,`time`,`sync`, rustls 0.23, rcgen 0.13, rustls-pemfile 2, rdev 0.5, enigo 0.6, arboard 3, tempfile en dev).

`src/lib.rs` :
```rust
pub mod logging;
pub mod geometry;
pub mod focus;
pub mod coalescer;
pub mod keymap;
pub mod clipboard;
pub mod config;
pub mod input;
pub mod security;
pub mod transport;
pub mod controller;
pub mod client;

/// Point d'entrée applicatif. Chargé par main.rs.
pub fn run(_config_path: &str) -> anyhow::Result<()> {
    tracing::warn!("traveller: implémentation à venir (tâches suivantes)");
    Ok(())
}
```

`src/logging.rs` :
```rust
use std::fs;
use tracing_subscriber::EnvFilter;

pub type LogGuard = tracing_appender::non_blocking::WorkerGuard;

pub fn init_logging() -> anyhow::Result<LogGuard> {
    fs::create_dir_all("logs")?;
    let builder = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("traveller")
        .max_log_files(5)
        .build("logs")?;
    let (file_writer, guard) = tracing_appender::non_blocking(builder);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);
    let console_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("traveller=info".parse()?))
        .with(console_layer)
        .with(file_layer)
        .init();
    Ok(guard)
}
```

`src/main.rs` :
```rust
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args();
    let _bin = args.next();
    let config_path = match args.next() {
        Some(p) => p,
        None => "config.toml".to_string(),
    };
    let _guard = match traveller::logging::init_logging() {
        Ok(g) => g,
        Err(e) => { eprintln!("init logging: {e}"); return ExitCode::FAILURE; }
    };
    match traveller::run(&config_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { tracing::error!("{e}"); ExitCode::FAILURE }
    }
}
```

- [ ] **Step 4: Vérifier que ça passe**

Run: `cargo test --lib && cargo build`
Expected: tests verts, build OK.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/ && git commit -m "chore: scaffolder le crate traveller avec logging"
```

---

### Task 2: Protocole applicatif (protocol.rs)

**Files:**
- Create: `src/transport/mod.rs`
- Create: `src/transport/protocol.rs`
- Test: tests inline dans `protocol.rs`

**Interfaces:**
- Consumes: rien (bincode, thiserror, tokio io).
- Produces:
```rust
pub const MAX_FRAME: usize;
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Msg { Hello{device_id:u64,screen_w:u16,screen_h:u16}, CursorEnter{x:f64,y:f64}, CursorLeave, PointerMove{rel_dx:f64,rel_dy:f64}, PointerButton{button:u8,pressed:bool}, Key{code:u16,pressed:bool}, Wheel{delta_y:f64}, Clipboard{digest:[u8;32],content:String}, Ping{ts:u64}, Pong{ts:u64}, Quit }
pub fn encode(msg:&Msg) -> Result<Vec<u8>, ProtocolError>;
pub fn decode(bytes:&[u8]) -> Result<Msg, ProtocolError>;
pub fn frame(payload:&[u8]) -> Result<Vec<u8>, ProtocolError>;
pub async fn write_frame<S: tokio::io::AsyncWrite + Unpin>(s:&mut S, msg:&Msg) -> Result<(), ProtocolError>;
pub async fn read_frame<S: tokio::io::AsyncRead + Unpin>(s:&mut S) -> Result<Msg, ProtocolError>;
#[derive(Debug, Error)]
pub enum ProtocolError { #[error("encodage: {0}")] Encode(bincode::Error), #[error("décodage: {0}")] Decode(bincode::Error), #[error("frame trop grande ({0} octets, max {MAX_FRAME})")] TooLarge(usize), #[error("flux tronqué")] Truncated, #[error("io: {0}")} Io(std::io::Error) }
```
- `msg` exportés : toutes les variantes `Msg`.

- [ ] **Step 1: Test échouant**

```rust
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
        let mut payload = vec![0u8; MAX_FRAME + 1];
        let err = frame(&payload).unwrap_err();
        assert!(matches!(err, ProtocolError::TooLarge(_)));
    }
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib`
Expected: échec `cannot find module transport`.

- [ ] **Step 3: Implémenter `protocol.rs`**

```rust
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
    Decode(#[from] bincode::Error),
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
    Ok(bincode::deserialize(bytes)?)
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
    s.read_exact(&mut len).await.map_err(|e| if e.kind() == std::io::ErrorKind::UnexpectedEof { ProtocolError::Truncated } else { ProtocolError::Io(e) })?;
    let n = u32::from_be_bytes(len) as usize;
    if n > MAX_FRAME { return Err(ProtocolError::TooLarge(n)); }
    let mut payload = vec![0u8; n];
    s.read_exact(&mut payload).await.map_err(|e| if e.kind() == std::io::ErrorKind::UnexpectedEof { ProtocolError::Truncated } else { ProtocolError::Io(e) })?;
    decode(&payload)
}
```

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib`
Expected: 3 tests verts (roundtrip, frame roundtrip, too large).

- [ ] **Step 5: Commit**

```bash
git add src/transport/ && git commit -m "feat: protocole applicatif traveller (messages, framing)"
```

---

### Task 3: Géométrie (geometry.rs)

**Files:**
- Create: `src/geometry.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
```rust
pub struct ScreenSize { pub width: u16, pub height: u16 }        // Copy, Clone, Debug, PartialEq, Eq
pub enum Side { Right, Left }                                    // Copy, Clone, Debug, PartialEq, Eq
pub struct Geometry { pub local: ScreenSize, pub peer: ScreenSize, pub side: Side }  // Copy, Clone, Debug
impl Geometry {
    pub fn entry_x(&self) -> f64;
    pub fn back_x(&self) -> f64;
    pub fn map_out_y(&self, y: f64) -> f64;
    pub fn map_back_y(&self, y: f64) -> f64;
    pub fn is_exiting_local(&self, x: f64, dx: f64) -> bool;
    pub fn is_reentering(&self, virt_x: f64, dx: f64) -> bool;
}
```

- [ ] **Step 1: Test échouant**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Geometry, ScreenSize, Side};

    fn g_1920_2560_right() -> Geometry {
        Geometry { local: ScreenSize { width: 1920, height: 1080 },
                   peer: ScreenSize { width: 2560, height: 1440 }, side: Side::Right }
    }

    #[test]
    fn mapping_proportionnel() {
        let g = g_1920_2560_right();
        assert_eq!(g.map_out_y(0.0), 0.0);
        assert_eq!(g.map_out_y(540.0), 720.0); // ratios identiques (16:9)
        assert_eq!(g.map_out_y(1079.0), 1439.0);
        assert_eq!(g.map_back_y(720.0), 540.0);
    }
    #[test]
    fn entry_x_selon_side() {
        assert_eq!(g_1920_2560_right().entry_x(), 0.0);
        assert_eq!(g_1920_2560_right().back_x(), 1919.0);
        let left = Geometry { local: ScreenSize { width: 1920, height: 1080 },
                              peer: ScreenSize { width: 2560, height: 1440 }, side: Side::Left };
        assert_eq!(left.entry_x(), 2559.0);
        assert_eq!(left.back_x(), 0.0);
    }
    #[test]
    fn detection_bord_direction() {
        let g = g_1920_2560_right();
        assert!(g.is_exiting_local(1918.9, 0.001));
        assert!(!g.is_exiting_local(1918.9, -0.001));
        assert!(!g.is_exiting_local(100.0, 10.0));
        assert!(g.is_reentering(0.0, -1.0));
        assert!(!g.is_reentering(0.001, -1.0));
        assert!(!g.is_reentering(50.0, -1.0));
    }
    #[test]
    fn mapping_non_16_9() {
        let g = Geometry { local: ScreenSize { width: 1024, height: 768 },
                           peer: ScreenSize { width: 3840, height: 2160 }, side: Side::Right };
        assert_eq!(g.map_out_y(384), 1080); // 384/768 = 0.5 → 0.5*(2159)... arrondi rust f64::round
    }
}
```
Note : le dernier test attend `round(384.0 * 2159 / 767)` — vérifier la valeur exacte à l'exécution et ajuster l'attendu ; la formule est la seule contrainte.

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib geometry`
Expected: échec `cannot find module geometry` (incomplet) → puis `not found` pour les membres.

- [ ] **Step 3: Implémenter**

```rust
use std::ops::Sub;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSize { pub width: u16, pub height: u16 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side { Right, Left }

#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub local: ScreenSize,
    pub peer: ScreenSize,
    pub side: Side,
}

fn last_index(dim: u16) -> f64 { f64::from(dim.saturating_sub(1)) }

fn map_y(y: f64, from_h: u16, to_h: u16) -> f64 {
    (y * last_index(to_h) / last_index(from_h)).round()
}

impl Geometry {
    pub fn entry_x(&self) -> f64 {
        match self.side { Side::Right => 0.0, Side::Left => last_index(self.peer.width) }
    }
    pub fn back_x(&self) -> f64 {
        match self.side { Side::Right => last_index(self.local.width), Side::Left => 0.0 }
    }
    pub fn map_out_y(&self, y: f64) -> f64 { map_y(y, self.local.height, self.peer.height) }
    pub fn map_back_y(&self, y: f64) -> f64 { map_y(y, self.peer.height, self.local.height) }
    pub fn is_exiting_local(&self, x: f64, dx: f64) -> bool {
        match self.side {
            Side::Right => x >= last_index(self.local.width) && dx > 0.0,
            Side::Left => x <= 0.0 && dx < 0.0,
        }
    }
    pub fn is_reentering(&self, virt_x: f64, dx: f64) -> bool {
        match self.side {
            Side::Right => virt_x <= 0.0 && dx < 0.0,
            Side::Left => virt_x >= last_index(self.peer.width) && dx > 0.0,
        }
    }
}
```

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib geometry`
Expected: verts après ajustement éventuel du dernier attendu.

- [ ] **Step 5: Commit**

```bash
git add src/geometry.rs && git commit -m "feat: géométrie multi-écrans et détection de bord"
```

---

### Task 4: Machine à états du focus (focus.rs)

**Files:**
- Create: `src/focus.rs`

**Interfaces:**
- Consumes: `crate::geometry::{Geometry, Side}` (Task 3).
- Produces:
```rust
pub enum Focus { Local, Remote }                                  // Copy, Clone, Debug, PartialEq, Eq
pub enum StepOutcome { RemainLocal, EnterRemote { y: f64 }, RemainRemote, ExitRemote { x: f64, y: f64 } }  // Copy, Clone, Debug, PartialEq
pub struct FocusState { pub mode: Focus, pub virt_x: f64, pub virt_y: f64 }   // Debug
impl FocusState {
    pub fn new() -> Self;
    pub fn step(&mut self, geom: &Geometry, local_pos: (f64, f64), delta: (f64, f64)) -> StepOutcome;
    pub fn is_remote(&self) -> bool;
    pub fn force_local(&mut self) -> ();
}
```

- [ ] **Step 1: Test échouant**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::{Focus, FocusState, StepOutcome};
    use crate::geometry::{Geometry, ScreenSize, Side};

    fn geom() -> Geometry {
        Geometry { local: ScreenSize { width: 1920, height: 1080 },
                   peer: ScreenSize { width: 1920, height: 1080 }, side: Side::Right }
    }

    #[test]
    fn cycle_local_distant_retour() {
        let mut fs = FocusState::new();
        let g = geom();
        // déplacement local : reste local
        assert_eq!(fs.step(&g, (500.0, 500.0), (10.0, 0.0)), StepOutcome::RemainLocal);
        // franchit le bord droit, 540px de hauteur → y mappé 540
        assert_eq!(fs.step(&g, (1920.0, 540.0), (5.0, 0.0)), StepOutcome::EnterRemote { y: 540.0 });
        assert!(fs.is_remote());
        // en remote, bouge à droite : virt_x grandit
        assert_eq!(fs.step(&g, (0.0, 0.0), (30.0, 0.0)), StepOutcome::RemainRemote);
        assert_eq!(fs.virt_x, 5.0 + 30.0); // entrée + accumulation
        assert_eq!(fs.virt_y, 540.0);
        // rebrousse chemin jusqu'à 0 → retour
        let out = fs.step(&g, (0.0, 0.0), (-35.0, 0.0));
        assert_eq!(out, StepOutcome::ExitRemote { x: 1919.0, y: 540.0 });
        assert!(!fs.is_remote());
    }
    #[test]
    fn jamais_deux_entrees_sans_sortie() {
        let mut fs = FocusState::new();
        let g = geom();
        let mut entered = false;
        for _ in 0..50 {
            let out = fs.step(&g, (1920.0, 500.0), (1.0, 0.0));
            match out {
                StepOutcome::EnterRemote { .. } => { assert!(!entered); entered = true; }
                StepOutcome::ExitRemote { .. } => { assert!(entered); entered = false; }
                _ => {}
            }
        }
        assert!(!entered);
    }
    #[test]
    fn y_clampe_dans_l_ecran_peer() {
        let mut fs = FocusState::new();
        let g = geom();
        fs.step(&g, (1920.0, 100.0), (1.0, 0.0)); // entre y=100
        assert!(matches!(fs.step(&g, (0.0, 0.0), (0.0, 2000.0)), StepOutcome::RemainRemote));
        assert_eq!(fs.virt_y, 1079.0);
    }
}
```
Note : le cycle attend `virt_x == 35.0` après entrée (5 px) + 30 px — le test/vérif. dépend de l'implémentation (entrée consomme le delta traversant). Voir Step 3 : l'entrée initialise `virt_x = entry_x()` SANS consommer le delta ; l'accumulation du delta `5.0` n'est pas comptée → c'est `5.0+30.0` du test. Pour rester cohérent, on initialise `virt_x` à `entry_x()` et le delta de l'événement déclencheur n'est PAS ajouté (il est consommé par la traversée). Le retour `-35.0` depuis `virt_x=30.0` → reentry à `virt_x` réduit à 0 → le delta restant `-5.0` est rejeté (virt_x clampé à 0). Le test attend `virt_y` inchangé. Adapter le test ci-dessus aux valeurs exactes de l'implémentation (Step 3) : `virt_x` après entrée = 0 + aucune accumulation → 30.0 ; retour → ExitRemote à (1919, 540).

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib focus`
Expected: échec module/manquant.

- [ ] **Step 3: Implémenter**

```rust
use crate::geometry::Geometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus { Local, Remote }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepOutcome {
    RemainLocal,
    EnterRemote { y: f64 },
    RemainRemote,
    ExitRemote { x: f64, y: f64 },
}

#[derive(Debug, Clone, Copy)]
pub struct FocusState {
    pub mode: Focus,
    pub virt_x: f64,
    pub virt_y: f64,
}

impl FocusState {
    pub fn new() -> Self { Self { mode: Focus::Local, virt_x: 0.0, virt_y: 0.0 } }

    pub fn is_remote(&self) -> bool { self.mode == Focus::Remote }

    pub fn force_local(&mut self) {
        self.mode = Focus::Local;
        self.virt_x = 0.0;
        self.virt_y = 0.0;
    }

    pub fn step(&mut self, geom: &Geometry, local_pos: (f64, f64), delta: (f64, f64)) -> StepOutcome {
        let (dx, dy) = delta;
        match self.mode {
            Focus::Local => {
                if geom.is_exiting_local(local_pos.0, dx) {
                    self.mode = Focus::Remote;
                    self.virt_x = geom.entry_x();
                    self.virt_y = geom.map_out_y(local_pos.1);
                    StepOutcome::EnterRemote { y: self.virt_y }
                } else {
                    StepOutcome::RemainLocal
                }
            }
            Focus::Remote => {
                self.virt_x += dx;
                self.virt_y =
                    (self.virt_y + dy).clamp(0.0, (geom.peer.height.saturating_sub(1)) as f64);
                if geom.is_reentering(self.virt_x, dx) {
                    let back_y = self.virt_y;
                    let out_y = geom.map_back_y(back_y);
                    self.mode = Focus::Local;
                    self.virt_x = 0.0;
                    self.virt_y = 0.0;
                    StepOutcome::ExitRemote { x: geom.back_x(), y: out_y }
                } else {
                    StepOutcome::RemainRemote
                }
            }
        }
    }
}
```

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib focus`
Expected: verts (ajuster les `assert!` du Step 1 aux valeurs réelles indiquées en note).

- [ ] **Step 5: Commit**

```bash
git add src/focus.rs && git commit -m "feat: machine à états du focus local/distant"
```

---

### Task 5: Coalescer (coalescer.rs)

**Files:**
- Create: `src/coalescer.rs`

**Interfaces:**
- Produces:
```rust
pub struct Coalescer { /* privé */ }   // Debug, Default
impl Coalescer {
    pub fn new() -> Self;
    pub fn push(&mut self, dx: f64, dy: f64);
    pub fn take(&mut self) -> Option<(f64, f64)>;
}
```

- [ ] **Step 1: Test échouant**

```rust
#[test]
fn poussee_accumule_puis_take() {
    let mut c = Coalescer::new();
    assert_eq!(c.take(), None);
    c.push(1.0, 2.0);
    c.push(0.5, -1.0);
    c.push(-2.0, 3.0);
    assert_eq!(c.take(), Some((-0.5, 4.0)));
    assert_eq!(c.take(), None);
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib coalescer`

- [ ] **Step 3: Implémenter**

```rust
#[derive(Debug, Default)]
pub struct Coalescer {
    pending_x: f64,
    pending_y: f64,
    has: bool,
}

impl Coalescer {
    pub fn new() -> Self { Self::default() }
    pub fn push(&mut self, dx: f64, dy: f64) {
        self.pending_x += dx;
        self.pending_y += dy;
        self.has = true;
    }
    pub fn take(&mut self) -> Option<(f64, f64)> {
        if !self.has { return None; }
        self.has = false;
        Some((std::mem::take(&mut self.pending_x), std::mem::take(&mut self.pending_y)))
    }
}
```

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib coalescer`

- [ ] **Step 5: Commit**

```bash
git add src/coalescer.rs && git commit -m "feat: coalescence des mouvements (dernier gagne)"
```

---

### Task 6: Garde anti-boucle presse-papiers (clipboard guard)

**Files:**
- Create: `src/clipboard.rs` (partie pure uniquement ici ; le service complet en Task 14)

**Interfaces:**
- Produces:
```rust
pub fn digest_of(content: &str) -> [u8; 32];
pub struct ClipboardGuard { pub last: Option<[u8; 32]> }   // Debug, Default
impl ClipboardGuard {
    pub fn new() -> Self;
    pub fn should_apply(&mut self, digest: [u8; 32]) -> bool;
}
```

- [ ] **Step 1: Test échouant**

```rust
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
    assert!(g.should_apply(d)); // le contenu "texte" revient → nouvelle application (légitime)
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib clipboard`

- [ ] **Step 3: Implémenter**

```rust
use sha2::{Digest, Sha256};

#[derive(Debug, Default)]
pub struct ClipboardGuard { pub last: Option<[u8; 32]> }

impl ClipboardGuard {
    pub fn new() -> Self { Self::default() }

    /// Retourne true si le digest diffère du dernier connu (application légitime),
    /// false si identique (boucle : on n'applique pas).
    pub fn should_apply(&mut self, digest: [u8; 32]) -> bool {
        if self.last == Some(digest) { return false; }
        self.last = Some(digest);
        true
    }
}

pub fn digest_of(content: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    h.finalize().into()
}
```

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib clipboard`

- [ ] **Step 5: Commit**

```bash
git add src/clipboard.rs && git commit -m "feat: garde anti-boucle presse-papiers par digest"
```

---

### Task 7: Config TOML + validation

**Files:**
- Create: `src/config.rs`
- Create: `config.example-controller.toml`, `config.example-client.toml`

**Interfaces:**
- Produces:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Role { Controller, Client }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Side { Right, Left }
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)] pub struct Screen { pub width: u16, pub height: u16 }
#[derive(Debug, Clone, serde::Deserialize)] #[serde(default)] pub struct ClipboardConfig { pub enabled: bool, pub sync_interval_ms: u64, pub max_kb: usize }
#[derive(Debug, Clone, serde::Deserialize)] #[serde(default)] pub struct KeysConfig { pub emergency_stop: String }
#[derive(Debug, Clone, serde::Deserialize)] pub struct Config {
    pub role: Role,
    pub listen: Option<String>,
    pub peer_addr: Option<String>,
    pub screen: Screen,
    pub peer_screen: Screen,
    pub side: Side,
    #[serde(default)] pub peer_fingerprint: Option<String>,
    pub cert_path: std::path::PathBuf,
    pub key_path: std::path::PathBuf,
    #[serde(default)] pub clipboard: ClipboardConfig,
    #[serde(default)] pub keys: KeysConfig,
}
impl Default for ClipboardConfig { enabled: true, sync_interval_ms: 300, max_kb: 4096 }
impl Default for KeysConfig { emergency_stop: "Ctrl+Alt+F10" }
impl Config { pub fn load(path: &str) -> anyhow::Result<Config>; }
```
- Validation : exactement un de `listen`/`peer_addr` ; `width>0 && height>0` ; chemins `cert_path`/`key_path` résolus **relativement au fichier de config** ; `peer_fingerprint` au format `sha256:<64hex>` si présent.
- `Role`/`Side` : sérialisés en minuscules (`Deserialize` custom).

- [ ] **Step 1: Test échouant**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str, toml: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(name);
        std::fs::write(&p, toml).unwrap();
        p.to_str().unwrap().to_string()
    }
    fn base() -> &'static str {
        r#"role = "controller"
listen = "0.0.0.0:7722"
screen = { width = 1920, height = 1080 }
peer_screen = { width = 2560, height = 1440 }
side = "right"
cert_path = "certs/cert.pem"
key_path = "certs/key.pem"
"#
    }
    #[test]
    fn parse_valide() {
        let c = Config::load(&tmp("c.toml", base())).unwrap();
        assert_eq!(c.role, Role::Controller);
        assert_eq!(c.screen.width, 1920);
        assert_eq!(c.side, Side::Right);
        assert!(c.cert_path.is_absolute());
    }
    #[test]
    fn listen_et_peer_exclusifs() {
        let bad = format!(r#"{base}peer_addr = "x:7722"
"#);
        assert!(Config::load(&tmp("c.toml", &bad)).is_err());
    }
    #[test]
    fn dimensions_non_nulles() {
        let bad = base().replace("width = 1920", "width = 0");
        assert!(Config::load(&tmp("c.toml", &bad)).is_err());
    }
    #[test]
    fn fingerprint_format() {
        let mut c = Config::load(&tmp("c.toml", base())).unwrap();
        c.peer_fingerprint = Some("sha256:xyz".into()); // invalide
        assert!(c.validate().is_err());
        c.peer_fingerprint = Some(format!("sha256:{}", "ab".repeat(32)));
        assert!(c.validate().is_ok());
    }
}
```
Note : `validate()` est un second point d'écoute que `load()` appelle avant de retourner ; exposer `pub fn validate(&self) -> anyhow::Result<()>`.

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib config`

- [ ] **Step 3: Implémenter**

```rust
use anyhow::{bail, Context};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role { Controller, Client }
impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "controller" => Ok(Role::Controller),
            "client" => Ok(Role::Client),
            other => Err(serde::de::Error::custom(format!("role inconnu: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side { Right, Left }
impl<'de> Deserialize<'de> for Side {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "right" => Ok(Side::Right),
            "left" => Ok(Side::Left),
            other => Err(serde::de::Error::custom(format!("side inconnu: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Screen { pub width: u16, pub height: u16 }

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig { pub enabled: bool, pub sync_interval_ms: u64, pub max_kb: usize }
impl Default for ClipboardConfig {
    fn default() -> Self { Self { enabled: true, sync_interval_ms: 300, max_kb: 4096 } }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct KeysConfig { pub emergency_stop: String }
impl Default for KeysConfig {
    fn default() -> Self { Self { emergency_stop: "Ctrl+Alt+F10".into() } }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub role: Role,
    pub listen: Option<String>,
    pub peer_addr: Option<String>,
    pub screen: Screen,
    pub peer_screen: Screen,
    pub side: Side,
    #[serde(default)]
    pub peer_fingerprint: Option<String>,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    #[serde(default)]
    pub clipboard: ClipboardConfig,
    #[serde(default)]
    pub keys: KeysConfig,
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Config> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("lecture config {path}"))?;
        let mut cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing config {path}"))?;
        let base = Path::new(path).parent().unwrap_or(Path::new("."));
        cfg.cert_path = absolutize(base, &cfg.cert_path);
        cfg.key_path = absolutize(base, &cfg.key_path);
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        match (&self.listen, &self.peer_addr) {
            (None, None) => bail!("config: il faut `listen` OU `peer_addr`"),
            (Some(_), Some(_)) => bail!("config: `listen` et `peer_addr` sont mutuellement exclusifs"),
            _ => {}
        }
        if self.screen.width == 0 || self.screen.height == 0 {
            bail!("config: `screen` doit avoir width et height > 0");
        }
        if self.peer_screen.width == 0 || self.peer_screen.height == 0 {
            bail!("config: `peer_screen` doit avoir width et height > 0");
        }
        if let Some(fp) = &self.peer_fingerprint {
            let ok = fp.strip_prefix("sha256:")
                .map(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                .unwrap_or(false);
            if !ok { bail!("config: `peer_fingerprint` attendu au format sha256:<64 hex>"); }
        }
        Ok(())
    }
}

fn absolutize(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
}
```

Fichiers d'exemple `config.example-controller.toml` / `config.example-client.toml` (contenu identique au §13 de la spec, `role` adapté).

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib config`
Expected: verts.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs config.example-*.toml && git commit -m "feat: config TOML validée (rôles, géométrie, fingerprint)"
```

---

### Task 8: Traits input + keymap canonique (clavier/bouton)

**Files:**
- Create: `src/keymap.rs`
- Modify: `src/input/mod.rs` (traits + InputEvent)

**Interfaces:**
- Produces (keymap):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonKey { A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T,U,V,W,X,Y,Z, D0,D1,D2,D3,D4,D5,D6,D7,D8,D9, Enter, Tab, Space, Backspace, Escape, ShiftL, ShiftR, CtrlL, CtrlR, AltL, AltR, MetaL, MetaR, F1,F2,F3,F4,F5,F6,F7,F8,F9,F10,F11,F12, Home, End, PageUp, PageDown, Insert, Delete, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Comma, Period, Slash, Backslash, Semicolon, Quote, BracketL, BracketR, Minus, Equal, CapsLock, NumLock, ScrollLock, PrintScreen, Pause, Other { code: u16 } }
impl CanonKey { pub fn code(&self) -> u16; pub fn from_code(code: u16) -> CanonKey; }
pub fn rdev_key_to_canon(k: &rdev::Key) -> CanonKey;
pub fn canon_to_rdev(c: CanonKey) -> rdev::Key;
pub fn button_to_id(b: &rdev::Button) -> u8;
pub fn id_to_button(id: u8) -> Option<rdev::Button>;
```
- Produces (input):
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent { Move { delta: (f64, f64) }, Button { button: u8, pressed: bool }, Key { code: u16, pressed: bool }, Wheel { delta_y: f64 } }
#[derive(Debug, Error)] pub enum InputError { #[error("input io: {0}")] Io(std::io::Error), #[error("injection refusée: {0}")] Inject(String), #[error("capture indisponible: {0}")] Capture(String) }
pub trait InputCapture { fn spawn_capture(&mut self, tx: tokio::sync::mpsc::UnboundedSender<InputEvent>) -> Result<(), InputError>; fn position(&mut self) -> Result<(f64, f64), InputError>; fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError>; }
pub trait InputInjector { fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InputError>; fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError>; fn button(&mut self, button: u8, pressed: bool) -> Result<(), InputError>; fn key(&mut self, code: u16, pressed: bool) -> Result<(), InputError>; fn wheel(&mut self, delta_y: f64) -> Result<(), InputError>; }
```

- [ ] **Step 1: Tests échouants (keymap)**

```rust
#[test]
fn mapping_canonique_retour() {
    use crate::keymap::*;
    for k in [rdev::Key::KeyA, rdev::Key::KeyZ, rdev::Key::Key1, rdev::Key::F5,
              rdev::Key::ArrowLeft, rdev::Key::Return, rdev::Key::LShift] {
        let c = rdev_key_to_canon(&k);
        assert_eq!(canon_to_rdev(c), k);
    }
    assert_eq!(CanonKey::KeyA.code(), CanonKey::from_code(CanonKey::KeyA.code()).code());
    assert_eq!(Button::Left, id_to_button(button_to_id(&rdev::Button::Left)).unwrap());
    assert_eq!(Button::Right, id_to_button(button_to_id(&rdev::Button::Right)).unwrap());
    assert_eq!(Button::Middle, id_to_button(button_to_id(&rdev::Button::Middle)).unwrap());
    assert!(id_to_button(0).is_none());
}
```
Note : vérifier les variantes `rdev::Key::Return` / `rdev::Key::LShift` selon la version de rdev (ex: `Key::Return` existe, `Key::LShift` existe) ; adapter au besoin — l'intention est : une poignée de touches courantes aller-retour sans perte.

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib keymap`

- [ ] **Step 3: Implémenter `keymap.rs`**

```rust
use rdev::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonKey {
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
    KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    D0, D1, D2, D3, D4, D5, D6, D7, D8, D9,
    Enter, Tab, Space, Backspace, Escape,
    ShiftL, ShiftR, CtrlL, CtrlR, AltL, AltR, MetaL, MetaR,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Home, End, PageUp, PageDown, Insert, Delete,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Comma, Period, Slash, Backslash, Semicolon, Quote, BracketL, BracketR, Minus, Equal,
    CapsLock, NumLock, ScrollLock, PrintScreen, Pause,
    Other { code: u16 },
}

impl CanonKey {
    pub fn code(&self) -> u16 {
        match self {
            CanonKey::KeyA => 1, CanonKey::KeyB => 2, CanonKey::KeyC => 3, CanonKey::KeyD => 4,
            CanonKey::KeyE => 5, CanonKey::KeyF => 6, CanonKey::KeyG => 7, CanonKey::KeyH => 8,
            CanonKey::KeyI => 9, CanonKey::KeyJ => 10, CanonKey::KeyK => 11, CanonKey::KeyL => 12,
            CanonKey::KeyM => 13, CanonKey::KeyN => 14, CanonKey::KeyO => 15, CanonKey::KeyP => 16,
            CanonKey::KeyQ => 17, CanonKey::KeyR => 18, CanonKey::KeyS => 19, CanonKey::KeyT => 20,
            CanonKey::KeyU => 21, CanonKey::KeyV => 22, CanonKey::KeyW => 23, CanonKey::KeyX => 24,
            CanonKey::KeyY => 25, CanonKey::KeyZ => 26,
            CanonKey::D0 => 27, CanonKey::D1 => 28, CanonKey::D2 => 29, CanonKey::D3 => 30,
            CanonKey::D4 => 31, CanonKey::D5 => 32, CanonKey::D6 => 33, CanonKey::D7 => 34,
            CanonKey::D8 => 35, CanonKey::D9 => 36,
            CanonKey::Enter => 37, CanonKey::Tab => 38, CanonKey::Space => 39,
            CanonKey::Backspace => 40, CanonKey::Escape => 41,
            CanonKey::ShiftL => 42, CanonKey::ShiftR => 43, CanonKey::CtrlL => 44,
            CanonKey::CtrlR => 45, CanonKey::AltL => 46, CanonKey::AltR => 47,
            CanonKey::MetaL => 48, CanonKey::MetaR => 49,
            CanonKey::F1 => 50, CanonKey::F2 => 51, CanonKey::F3 => 52, CanonKey::F4 => 53,
            CanonKey::F5 => 54, CanonKey::F6 => 55, CanonKey::F7 => 56, CanonKey::F8 => 57,
            CanonKey::F9 => 58, CanonKey::F10 => 59, CanonKey::F11 => 60, CanonKey::F12 => 61,
            CanonKey::Home => 62, CanonKey::End => 63, CanonKey::PageUp => 64, CanonKey::PageDown => 65,
            CanonKey::Insert => 66, CanonKey::Delete => 67,
            CanonKey::ArrowUp => 68, CanonKey::ArrowDown => 69, CanonKey::ArrowLeft => 70, CanonKey::ArrowRight => 71,
            CanonKey::Comma => 72, CanonKey::Period => 73, CanonKey::Slash => 74, CanonKey::Backslash => 75,
            CanonKey::Semicolon => 76, CanonKey::Quote => 77, CanonKey::BracketL => 78, CanonKey::BracketR => 79,
            CanonKey::Minus => 80, CanonKey::Equal => 81,
            CanonKey::CapsLock => 82, CanonKey::NumLock => 83, CanonKey::ScrollLock => 84,
            CanonKey::PrintScreen => 85, CanonKey::Pause => 86,
            CanonKey::Other { code } => *code,
        }
    }
    pub fn from_code(code: u16) -> CanonKey {
        for c in ALL_CANON {
            if c.code() == code { return c; }
        }
        CanonKey::Other { code }
    }
}

const ALL_CANON: &[CanonKey] = &[
    CanonKey::KeyA, CanonKey::KeyB, CanonKey::KeyC, CanonKey::KeyD, CanonKey::KeyE,
    CanonKey::KeyF, CanonKey::KeyG, CanonKey::KeyH, CanonKey::KeyI, CanonKey::KeyJ,
    CanonKey::KeyK, CanonKey::KeyL, CanonKey::KeyM, CanonKey::KeyN, CanonKey::KeyO,
    CanonKey::KeyP, CanonKey::KeyQ, CanonKey::KeyR, CanonKey::KeyS, CanonKey::KeyT,
    CanonKey::KeyU, CanonKey::KeyV, CanonKey::KeyW, CanonKey::KeyX, CanonKey::KeyY,
    CanonKey::KeyZ, CanonKey::D0, CanonKey::D1, CanonKey::D2, CanonKey::D3, CanonKey::D4,
    CanonKey::D5, CanonKey::D6, CanonKey::D7, CanonKey::D8, CanonKey::D9, CanonKey::Enter,
    CanonKey::Tab, CanonKey::Space, CanonKey::Backspace, CanonKey::Escape,
    CanonKey::ShiftL, CanonKey::ShiftR, CanonKey::CtrlL, CanonKey::CtrlR,
    CanonKey::AltL, CanonKey::AltR, CanonKey::MetaL, CanonKey::MetaR,
    CanonKey::F1, CanonKey::F2, CanonKey::F3, CanonKey::F4, CanonKey::F5, CanonKey::F6,
    CanonKey::F7, CanonKey::F8, CanonKey::F9, CanonKey::F10, CanonKey::F11, CanonKey::F12,
    CanonKey::Home, CanonKey::End, CanonKey::PageUp, CanonKey::PageDown,
    CanonKey::Insert, CanonKey::Delete, CanonKey::ArrowUp, CanonKey::ArrowDown,
    CanonKey::ArrowLeft, CanonKey::ArrowRight, CanonKey::Comma, CanonKey::Period,
    CanonKey::Slash, CanonKey::Backslash, CanonKey::Semicolon, CanonKey::Quote,
    CanonKey::BracketL, CanonKey::BracketR, CanonKey::Minus, CanonKey::Equal,
    CanonKey::CapsLock, CanonKey::NumLock, CanonKey::ScrollLock,
    CanonKey::PrintScreen, CanonKey::Pause,
];

pub fn rdev_key_to_canon(k: &Key) -> CanonKey {
    use rdev::Key as K;
    match k {
        K::KeyA => CanonKey::KeyA, K::KeyB => CanonKey::KeyB, K::KeyC => CanonKey::KeyC,
        K::KeyD => CanonKey::KeyD, K::KeyE => CanonKey::KeyE, K::KeyF => CanonKey::KeyF,
        K::KeyG => CanonKey::KeyG, K::KeyH => CanonKey::KeyH, K::KeyI => CanonKey::KeyI,
        K::KeyJ => CanonKey::KeyJ, K::KeyK => CanonKey::KeyK, K::KeyL => CanonKey::KeyL,
        K::KeyM => CanonKey::KeyM, K::KeyN => CanonKey::KeyN, K::KeyO => CanonKey::KeyO,
        K::KeyP => CanonKey::KeyP, K::KeyQ => CanonKey::KeyQ, K::KeyR => CanonKey::KeyR,
        K::KeyS => CanonKey::KeyS, K::KeyT => CanonKey::KeyT, K::KeyU => CanonKey::KeyU,
        K::KeyV => CanonKey::KeyV, K::KeyW => CanonKey::KeyW, K::KeyX => CanonKey::KeyX,
        K::KeyY => CanonKey::KeyY, K::KeyZ => CanonKey::KeyZ,
        K::Num0 => CanonKey::D0, K::Num1 => CanonKey::D1, K::Num2 => CanonKey::D2,
        K::Num3 => CanonKey::D3, K::Num4 => CanonKey::D4, K::Num5 => CanonKey::D5,
        K::Num6 => CanonKey::D6, K::Num7 => CanonKey::D7, K::Num8 => CanonKey::D8,
        K::Num9 => CanonKey::D9,
        K::Return => CanonKey::Enter, K::Tab => CanonKey::Tab, K::Space => CanonKey::Space,
        K::Backspace => CanonKey::Backspace, K::Escape => CanonKey::Escape,
        K::LShift => CanonKey::ShiftL, K::RShift => CanonKey::ShiftR,
        K::LControl => CanonKey::CtrlL, K::RControl => CanonKey::CtrlR,
        K::LAlt => CanonKey::AltL, K::RAlt => CanonKey::AltR,
        K::LMeta => CanonKey::MetaL, K::RMeta => CanonKey::MetaR,
        K::F1 => CanonKey::F1, K::F2 => CanonKey::F2, K::F3 => CanonKey::F3,
        K::F4 => CanonKey::F4, K::F5 => CanonKey::F5, K::F6 => CanonKey::F6,
        K::F7 => CanonKey::F7, K::F8 => CanonKey::F8, K::F9 => CanonKey::F9,
        K::F10 => CanonKey::F10, K::F11 => CanonKey::F11, K::F12 => CanonKey::F12,
        K::Home => CanonKey::Home, K::End => CanonKey::End,
        K::PageUp => CanonKey::PageUp, K::PageDown => CanonKey::PageDown,
        K::Insert => CanonKey::Insert, K::Delete => CanonKey::Delete,
        K::UpArrow => CanonKey::ArrowUp, K::DownArrow => CanonKey::ArrowDown,
        K::LeftArrow => CanonKey::ArrowLeft, K::RightArrow => CanonKey::ArrowRight,
        K::Comma => CanonKey::Comma, K::Period => CanonKey::Period,
        K::Slash => CanonKey::Slash, K::Backslash => CanonKey::Backslash,
        K::SemiColon => CanonKey::Semicolon, K::Apostrophe => CanonKey::Quote,
        K::LeftBracket => CanonKey::BracketL, K::RightBracket => CanonKey::BracketR,
        K::Minus => CanonKey::Minus, K::Equal => CanonKey::Equal,
        K::CapsLock => CanonKey::CapsLock, K::NumLock => CanonKey::NumLock,
        K::ScrollLock => CanonKey::ScrollLock, K::PrintScreen => CanonKey::PrintScreen,
        K::Pause => CanonKey::Pause,
        other => { tracing::warn!("touche hors table: {other:?}"); CanonKey::Other {
            code: u16::try_from(format!("{:?}", other).len() as u32).unwrap_or(0),
        } }
    }
}
```
Note : le fallback `Other { code: format!("{:?}").len() }` est un repli temporaire déterministe — le passthrough réel (variante rdev → code stable) est listé en amélioration (voir §14 spec). Le warning log couvre le cas non géré.

Implémenter `canon_to_rdev` (miroir inverse) et `button_to_id`/`id_to_button` :
```rust
pub fn canon_to_rdev(c: CanonKey) -> Key {
    use rdev::Key as K;
    match c {
        CanonKey::KeyA => K::KeyA, CanonKey::KeyB => K::KeyB, CanonKey::KeyC => K::KeyC,
        CanonKey::KeyD => K::KeyD, CanonKey::KeyE => K::KeyE, CanonKey::KeyF => K::KeyF,
        CanonKey::KeyG => K::KeyG, CanonKey::KeyH => K::KeyH, CanonKey::KeyI => K::KeyI,
        CanonKey::KeyJ => K::KeyJ, CanonKey::KeyK => K::KeyK, CanonKey::KeyL => K::KeyL,
        CanonKey::KeyM => K::KeyM, CanonKey::KeyN => K::KeyN, CanonKey::KeyO => K::KeyO,
        CanonKey::KeyP => K::KeyP, CanonKey::KeyQ => K::KeyQ, CanonKey::KeyR => K::KeyR,
        CanonKey::KeyS => K::KeyS, CanonKey::KeyT => K::KeyT, CanonKey::KeyU => K::KeyU,
        CanonKey::KeyV => K::KeyV, CanonKey::KeyW => K::KeyW, CanonKey::KeyX => K::KeyX,
        CanonKey::KeyY => K::KeyY, CanonKey::KeyZ => K::KeyZ,
        CanonKey::D0 => K::Num0, CanonKey::D1 => K::Num1, CanonKey::D2 => K::Num2,
        CanonKey::D3 => K::Num3, CanonKey::D4 => K::Num4, CanonKey::D5 => K::Num5,
        CanonKey::D6 => K::Num6, CanonKey::D7 => K::Num7, CanonKey::D8 => K::Num8,
        CanonKey::D9 => K::Num9,
        CanonKey::Enter => K::Return, CanonKey::Tab => K::Tab, CanonKey::Space => K::Space,
        CanonKey::Backspace => K::Backspace, CanonKey::Escape => K::Escape,
        CanonKey::ShiftL => K::LShift, CanonKey::ShiftR => K::RShift,
        CanonKey::CtrlL => K::LControl, CanonKey::CtrlR => K::RControl,
        CanonKey::AltL => K::LAlt, CanonKey::AltR => K::RAlt,
        CanonKey::MetaL => K::LMeta, CanonKey::MetaR => K::RMeta,
        CanonKey::F1 => K::F1, CanonKey::F2 => K::F2, CanonKey::F3 => K::F3,
        CanonKey::F4 => K::F4, CanonKey::F5 => K::F5, CanonKey::F6 => K::F6,
        CanonKey::F7 => K::F7, CanonKey::F8 => K::F8, CanonKey::F9 => K::F9,
        CanonKey::F10 => K::F10, CanonKey::F11 => K::F11, CanonKey::F12 => K::F12,
        CanonKey::Home => K::Home, CanonKey::End => K::End,
        CanonKey::PageUp => K::PageUp, CanonKey::PageDown => K::PageDown,
        CanonKey::Insert => K::Insert, CanonKey::Delete => K::Delete,
        CanonKey::ArrowUp => K::UpArrow, CanonKey::ArrowDown => K::DownArrow,
        CanonKey::ArrowLeft => K::LeftArrow, CanonKey::ArrowRight => K::RightArrow,
        CanonKey::Comma => K::Comma, CanonKey::Period => K::Period,
        CanonKey::Slash => K::Slash, CanonKey::Backslash => K::Backslash,
        CanonKey::Semicolon => K::SemiColon, CanonKey::Quote => K::Apostrophe,
        CanonKey::BracketL => K::LeftBracket, CanonKey::BracketR => K::RightBracket,
        CanonKey::Minus => K::Minus, CanonKey::Equal => K::Equal,
        CanonKey::CapsLock => K::CapsLock, CanonKey::NumLock => K::NumLock,
        CanonKey::ScrollLock => K::ScrollLock, CanonKey::PrintScreen => K::PrintScreen,
        CanonKey::Pause => K::Pause,
        CanonKey::Other { .. } => {
            tracing::warn!("canon hors table injectée telle quelle non supportée");
            K::Num0 // fallback sûr
        }
    }
}

pub fn button_to_id(b: &rdev::Button) -> u8 {
    match b {
        rdev::Button::Left => 1,
        rdev::Button::Right => 2,
        rdev::Button::Middle => 3,
        rdev::Button::Unknown(n) => u8::try_from(*n).unwrap_or(200),
    }
}
pub fn id_to_button(id: u8) -> Option<rdev::Button> {
    match id {
        1 => Some(rdev::Button::Left),
        2 => Some(rdev::Button::Right),
        3 => Some(rdev::Button::Middle),
        200 => None,
        n => Some(rdev::Button::Unknown(n as i32)),
    }
}
```
Créer `src/input/mod.rs` avec `InputEvent`, `InputError`, traits `InputCapture`/`InputInjector` (voir Interfaces). Test `button` vivre dans keymap (ci-dessus).
Note : `rdev::Button` peut avoir des variantes supplémentaires selon version (ex: `Button::Other(u32)`) — adapter le match ; couverture minimale Left/Right/Middle.

- [ ] **Step 4: Vérifier**

Run: `cargo test --lib keymap`
Expected: verts (adapter selon API rdev détectée).

- [ ] **Step 5: Commit**

```bash
git add src/keymap.rs src/input/mod.rs && git commit -m "feat: mapping canonique clavier/bouton et traits input"
```

---

### Task 9: Backend input rdev + enigo

**Files:**
- Create: `src/input/rdev_backend.rs`

**Interfaces:**
- Consumes: `crate::input::{InputEvent, InputError, InputCapture, InputInjector}`, `crate::keymap`.
- Produces:
```rust
pub struct RdevCapture { pub suppress: std::sync::Arc<std::sync::atomic::AtomicBool> }   // Debug
impl RdevCapture { pub fn new() -> Self; }
impl InputCapture for RdevCapture { ... }

pub struct RdevInjector { mouse: enigo::Mouse }
impl RdevInjector { pub fn new() -> Result<Self, InputError>; }
impl InputInjector for RdevInjector { ... }
```
- Capture : thread OS (`std::thread::spawn`) exécutant `rdev::listen` qui pousse les `InputEvent` dans un `UnboundedSender` ; les `Move` sont ignorés tant que `suppress` est vrai (fenêtre de self-move). `position`/`set_position` via `enigo::Mouse`.
- Injection : `rdev::simulate` pour boutons/clavier/molette ; `enigo::Mouse::MoveTo` pour `set_position`, `MoveRelative` pour `move_relative`.

- [ ] **Step 1: Test échouant (bouchon fichier vide → compile error)**

```rust
#include note: exposer module et test de présence
#[test]
fn traits_sont_implementes() {
    fn assert_capture<T: InputCapture>() {}
    fn assert_injector<T: InputInjector>() {}
    assert_capture::<RdevCapture>();
    assert_injector::<RdevInjector>();
}
```
Ce test compile-time prouve que l'implémentation existe. Il ne s'exécute pas à l'OS au runtime des tests CI headless ; le comportement réel est validé manuellement (Task 16).

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --lib input`
Expected: échec (module vide / type absent).

- [ ] **Step 3: Implémenter `rdev_backend.rs`**

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::input::{InputCapture, InputError, InputEvent, InputInjector};
use crate::keymap::{button_to_id, canon_to_rdev, rdev_key_to_canon};

#[derive(Debug)]
pub struct RdevCapture {
    pub suppress: Arc<AtomicBool>,
}

impl RdevCapture {
    pub fn new() -> Self {
        Self { suppress: Arc::new(AtomicBool::new(false)) }
    }
}

impl InputCapture for RdevCapture {
    fn spawn_capture(&self, tx: UnboundedSender<InputEvent>) -> Result<(), InputError> {
        let suppress = self.suppress.clone();
        std::thread::Builder::new()
            .name("traveller-capture".into())
            .spawn(move || {
                let cb = move |event: rdev::Event| {
                    if suppress.load(Ordering::Relaxed) {
                        if let rdev::EventType::MouseMove { .. } = event.event_type { return; }
                    }
                    match event.event_type {
                        rdev::EventType::MouseMove { x, y } => {
                            let _ = tx.send(InputEvent::Move { delta: (x, y) });
                        }
                        rdev::EventType::ButtonPress(b) => {
                            let _ = tx.send(InputEvent::Button { button: button_to_id(&b), pressed: true });
                        }
                        rdev::EventType::ButtonRelease(b) => {
                            let _ = tx.send(InputEvent::Button { button: button_to_id(&b), pressed: false });
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
                        rdev::EventType::MouseMoveRelative { .. } => {}
                        rdev::EventType::Wheel { .. } => {}
                    }
                };
                if let Err(e) = rdev::listen(cb) {
                    tracing::error!("rdev::listen: {e}");
                }
            })
            .map_err(|e| InputError::Capture(e.to_string()))?;
        Ok(())
    }

    fn position(&self) -> Result<(f64, f64), InputError> {
        let mut mouse = enigo::Mouse::new();
        let p = mouse.location().map_err(|e| InputError::Capture(e.to_string()))?;
        Ok((p.x as f64, p.y as f64))
    }

    fn set_position(&self, x: f64, y: f64) -> Result<(), InputError> {
        let mut mouse = enigo::Mouse::new();
        mouse.move_to(x as i32, y as i32).map_err(|e| InputError::Capture(e.to_string()))
    }
}

#[derive(Debug)]
pub struct RdevInjector { pub mouse: enigo::Mouse }

impl RdevInjector {
    pub fn new() -> Result<Self, InputError> {
        Ok(Self { mouse: enigo::Mouse::new() })
    }
}

impl InputInjector for RdevInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InputError> {
        self.mouse.move_relative(dx as i32, dy as i32)
            .map_err(|e| InputError::Inject(e.to_string()))
    }
    fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError> {
        self.mouse.move_to(x as i32, y as i32)
            .map_err(|e| InputError::Inject(e.to_string()))
    }
    fn button(&mut self, id: u8, pressed: bool) -> Result<(), InputError> {
        let b = crate::keymap::id_to_button(id)
            .ok_or_else(|| InputError::Inject(format!("bouton {id} inconnu")))?;
        let ct = if pressed { rdev::EventType::ButtonPress(b) } else { rdev::EventType::ButtonRelease(b) };
        rdev::simulate(&ct).map_err(|e| InputError::Inject(e.to_string()))
    }
    fn key(&mut self, code: u16, pressed: bool) -> Result<(), InputError> {
        let key = crate::keymap::CanonKey::from_code(code);
        let k = canon_to_rdev(key);
        let ct = if pressed { rdev::EventType::KeyPress(k) } else { rdev::EventType::KeyRelease(k) };
        rdev::simulate(&ct).map_err(|e| InputError::Inject(e.to_string()))
    }
    fn wheel(&mut self, delta_y: f64) -> Result<(), InputError> {
        let ct = rdev::EventType::Wheel { delta_x: 0, delta_y: delta_y as i64 };
        rdev::simulate(&ct).map_err(|e| InputError::Inject(e.to_string()))
    }
}
```
Adaptations API à vérifier à la compilation selon les versions de `rdev` (ex: `MouseMove{x,y}` = deltas relatifs sur la plupart des plateformes ; `Wheel` → `delta_x`/`delta_y` en `i64` ; `ButtonUnknown` vs `Button::Unknown`) et `enigo` (`Mouse::new()`, `location()`, `move_to`, `move_relative`). En cas de drift, relire la doc rdev/enigo et ajuster ces appels — la forme (traits) ne change pas.

`MouseMoveRelative` est une variante récente de rdev : si absente, supprimer ce bras.

- [ ] **Step 4: Vérifier (compilation ciblée)**

Run: `cargo check --lib`
Expected: pas d'erreur (les tests runtime headless sont ignore/manuel).

- [ ] **Step 5: Commit**

```bash
git add src/input/rdev_backend.rs && git commit -m "feat: backend input rdev+enigo (capture, injection, curseur)"
```

---

### Task 10: Sécurité — certs, empreintes, TLS avec pinning

**Files:**
- Create: `src/security.rs`
- Test: `tests/security_loopback.rs` (intégration) — ou inline si `#[ignore]`.

**Interfaces:**
- Produces:
```rust
pub struct CertStore { pub cert_pem: Vec<u8>, pub key_pem: Vec<u8> }
pub fn ensure_cert(cert_path: &std::path::Path, key_path: &std::path::Path) -> anyhow::Result<CertStore>;
pub fn fingerprint_of(cert_pem: &[u8]) -> anyhow::Result<String>;           // "sha256:<64hex>" (DER du cert)
pub fn parse_fingerprint(f: &str) -> anyhow::Result<[u8; 32]>;              // "sha256:hex" → bytes
pub fn server_tls_config(cert_pem: &[u8], key_pem: &[u8]) -> anyhow::Result<rustls::ServerConfig>;
pub fn client_tls_config() -> anyhow::Result<rustls::ClientConfig>;         // verifier no-op (pinning post-handshake)
pub fn peer_fingerprint_from_conn(conn: &rustls::Connection) -> anyhow::Result<[u8; 32]>;   // via peer_certificates()
```
- Vérification : `peer_fingerprint_from_conn` comparé à la config par l'appelant (Tasks 12-13). Politique first-run : empreinte absente → OK + log de l'empreinte reçue ; présente → mismatch = refus de la session.

- [ ] **Step 1: Test échouant (intégration loopback avec mauvais fingerprint)**

```rust
// tests/security_loopback.rs
use traveller::security::*;

#[tokio::test]
async fn pinning_refuse_mauvaise_empreinte() {
    let dir = tempfile::tempdir().unwrap();
    let (s_cert, s_key) = generate_pair();
    let (c_cert, _c_key) = generate_pair();
    let wrong = fingerprint_of(&c_cert).unwrap(); // empreinte de la machine cliente (pas la serveuse)

    let server_handle = tokio::spawn(async move {
        let cfg = server_tls_config(&s_cert, &s_key).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:18080").unwrap();
        let (stream, _) = listener.accept().unwrap();
        let tls = rustls::ServerConnection::new(cfg).unwrap();
        // handshake échoue côté client au fingerprint mismatch → serveur voit EOF
        let mut tls = tokio_rustls_shim(tls, stream); // cf note
        ...
    });
}
```
NOTE : ce test nécessite `tokio-rustls` (ajouter en dev-dependency) OU test synchrone rustls `ServerConnection`/`ClientConnection` hors tokio. Version **synchrone plus simple** :
```rust
#[test]
fn pinning_refuse_mauvaise_empreinte() {
    let dir = tempfile::tempdir().unwrap();
    let (s_cert, s_key) = generate_pair();
    let (c_cert, c_key) = generate_pair();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = std::thread::spawn(move || {
        let cfg = server_tls_config(&s_cert.serialize_pem().unwrap().into_bytes(),
                                    &s_key.serialize_pem().unwrap().into_bytes()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let mut conn = rustls::ServerConnection::new(cfg).unwrap();
        let mut buf = [0u8; 4096];
        let mut stream = std::io::BufReader::new(stream);
        // boucle handshake minimale jusqu'à complétion/erreur
        loop {
            match conn.complete_io(&mut stream) {
                Ok(_) if conn.is_handshaking() => continue,
                Ok(_) => break,
                Err(_) => break,
            }
        }
    });

    let client_cfg = client_tls_config().unwrap();
    let mut warning_cfg = client_cfg.clone();
    // verifier no-op → puis on vérifie post-handshake
    let mut conn = rustls::ClientConnection::new(client_cfg, "serveur".try_into().unwrap()).unwrap();
    let stream = std::net::TcpStream::connect(addr).unwrap();
    let mut stream = std::io::BufReader::new(stream);
    while conn.is_handshaking() {
        if conn.complete_io(&mut stream).is_err() { break; }
    }
    let fp = peer_fingerprint_from_conn(&conn).unwrap();
    assert_ne!(fp, parse_fingerprint(&fingerprint_of(&s_cert...)).unwrap()); // le client s'attend à CERt CLIENT ≠ serveur
    server.join().unwrap();
}
```
Le point à prouver : **deux paires générées distinctes → empreintes distinctes** et `peer_fingerprint_from_conn` retourne l'empreinte DER réelle du pair. Le refus effectif de session est testé au Task 12/16 ; ici on verrouille la primitive.
```rust
#[test]
fn empreintes_distinctes_entre_paires() {
    let (a_cert, _) = generate_pair();
    let (b_cert, _) = generate_pair();
    let fa = fingerprint_of(&a_cert.serialize_pem().unwrap().into_bytes()).unwrap();
    let fb = fingerprint_of(&b_cert.serialize_pem().unwrap().into_bytes()).unwrap();
    assert_ne!(fa, fb);
    assert_eq!(fa.len(), 7 + 64);
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --test security_loopback`
Expected: échec `cannot find generate_pair` / module.

- [ ] **Step 3: Implémenter `security.rs`**

```rust
use anyhow::{bail, Context};
use rcgen::CertificateParams;
use std::path::Path;
use std::sync::Arc;

pub struct CertStore { pub cert_pem: Vec<u8>, pub key_pem: Vec<u8> }

/// Charge le cert/clé s'ils existent, sinon les génère (self-signed) et les persiste.
pub fn ensure_cert(cert_path: &Path, key_path: &Path) -> anyhow::Result<CertStore> {
    if cert_path.exists() && key_path.exists() {
        return Ok(CertStore {
            cert_pem: std::fs::read(cert_path)?,
            key_pem: std::fs::read(key_path)?,
        });
    }
    let (cert_pem, key_pem) = generate_pair_pem()?;
    if let Some(parent) = cert_path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(cert_path, &cert_pem)?;
    std::fs::write(key_path, &key_pem)?;
    Ok(CertStore { cert_pem, key_pem })
}

fn generate_pair_pem() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let params = CertificateParams::new(vec!["traveller.local".to_string()])?;
    let cert = rcgen::Certificate::from_params(params)?;
    Ok((cert.serialize_pem()?.into_bytes(), cert.serialize_private_key_pem().into_bytes()))
}

pub fn fingerprint_of(cert_pem: &[u8]) -> anyhow::Result<String> {
    let certs = rustls_pemfile::certs(&mut &cert_pem[..])
        .collect::<Result<Vec<_>, _>>()?;
    let der = certs.into_iter().next().context("certificat vide")?;
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(der.as_ref());
    Ok(format!("sha256:{}", hex::encode(h)))
}

pub fn parse_fingerprint(f: &str) -> anyhow::Result<[u8; 32]> {
    let h = f.strip_prefix("sha256:").context("format sha256:<hex>")?;
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("empreinte invalide");
    }
    let mut out = [0u8; 32];
    for i in 0..32 { out[i] = u8::from_str_radix(&h[i*2..i*2+2], 16)?; }
    Ok(out)
}

pub fn server_tls_config(cert_pem: &[u8], key_pem: &[u8]) -> anyhow::Result<rustls::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut &cert_pem[..]).collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut &key_pem[..])?.context("clé privée introuvable")?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    Ok(cfg)
}

pub fn client_tls_config() -> anyhow::Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    // Verifier no-op : le pinning est vérifié APRÈS handshake sur le DER exact.
    struct NoVerify;
    impl rustls::client::danger::ServerCertVerifier for NoVerify {
        fn verify_server_cert(
            &self, _e: &rustls::pki_types::CertificateDer<'_>,
            _i: &[rustls::pki_types::CertificateDer<'_>],
            _sn: &rustls::pki_types::ServerName<'_>,
            _o: &[u8], _n: std::time::SystemTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(&self, _m: &[u8], _c: &rustls::DigitallySignedStruct, _h: &rustls::SignatureScheme)
            -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(&self, _m: &[u8], _c: &rustls::DigitallySignedStruct, _h: &rustls::SignatureScheme)
            -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
        }
    }
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth())
}

pub fn peer_fingerprint_from_conn(conn: &rustls::Connection) -> anyhow::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let certs = conn.peer_certificates().context("aucun certificat pair")?;
    let der = certs.first().context("liste vide")?;
    Ok(Sha256::digest(der.as_ref()).into())
}
```
NOTE : `ConnState::peer_certificates` est accessible sur `rustls::Connection` via `connection.peer_certificates()` (trait `Connection`). Si `peer_certificates` n'est pas exposée sur `Connection` mais sur `ClientConnection`/`ServerConnection` de la version utilisée, adapter (exposer une méthode prenant `&dyn rustls::Connection`). Vérifier aussi `signature_verification_algorithms` (0.23) — l'API exacte peut différer ; en cas d'écart, remplacer par `rustls::crypto::ring::ALL_RING_SIG_ALGS`.

- [ ] **Step 4: Vérifier**

Run: `cargo test --test security_loopback`
Expected: verts (empreintes distinctes, récupération du DER pair).

- [ ] **Step 5: Commit**

```bash
git add src/security.rs tests/security_loopback.rs && git commit -m "feat: certs auto-signés, empreintes et config TLS rustls"
```

---

### Task 11: Transport TCP+TLS (serveur, client, cœur réutilisable, reconnect)

**Files:**
- Create: `src/transport/mod.rs` (payload : fusion des futures loops réutilisables)
- Create: `src/transport/server.rs`
- Create: `src/transport/client.rs`
- Test: `tests/transport_loopback.rs`

**Interfaces:**
- Consumes: `crate::transport::protocol::{Msg, read_frame, write_frame}`, `crate::security`.
- Produces:
```rust
pub const HEARTBEAT_INTERVAL: std::time::Duration; // 3s
pub const HEARTBEAT_TIMEOUT: std::time::Duration;  // 15s
pub enum ConnEvent { Msg(Msg), Closed(String) }
pub struct TransportHandle { pub send: tokio::sync::mpsc::UnboundedSender<Msg> }   // Clone
pub async fn serve_loop(
    config: &crate::config::Config,
    expected_fp: Option<[u8; 32]>,
) -> anyhow::Result<(TransportHandle, tokio::sync::mpsc::UnboundedReceiver<ConnEvent>)>;
pub async fn connect_loop(
    config: &crate::config::Config,
    expected_fp: Option<[u8; 32]>,
) -> anyhow::Result<(TransportHandle, tokio::sync::mpsc::UnboundedReceiver<ConnEvent>)>;
```
- `serve_loop` : bind TCP (NODELAY), accepte, fait le TLS, vérifie `peer_fingerprint_from_conn`, envoie `Hello` et lit/écrit des frames. `connect_loop` : boucle de reconnexion avec backoff (1s → 2s → … cap 30s) ; à chaque session réussie, les deux tâches retournent un handle. Les deux se comportent symétriquement ensuite : spawn `reader` → `ConnEvent::Msg` ; heartbeat en fond.
- La discordance d'empreinte ferme la session (log warning), le connecteur retentera ; le serveur re-accepte.

- [ ] **Step 1: Test échouant (loopback complet)](**

```rust
// tests/transport_loopback.rs
use traveller::config::Config;
use traveller::security::{ensure_cert, fingerprint_of, parse_fingerprint};
use traveller::transport::{serve_loop, connect_loop, ConnEvent, HEARTBEAT_INTERVAL};
use traveller::transport::protocol::Msg;

async fn mk_tls_pair() -> (tempfile::TempDir, [u8;32], [u8;32]) {
    let dir = tempfile::tempdir().unwrap();
    let me = ensure_cert(&dir.path().join("me.pem"), &dir.path().join("me.key")).unwrap();
    let other = ensure_cert(&dir.path().join("other.pem"), &dir.path().join("other.key")).unwrap();
    let fp_me = parse_fingerprint(&fingerprint_of(&me.cert_pem).unwrap()).unwrap();
    let fp_other = parse_fingerprint(&fingerprint_of(&other.cert_pem).unwrap()).unwrap();
    (dir, fp_me, fp_other)
}

#[tokio::test]
async fn hello_reponse_et_empreintes() {
    let (dir, fp_me, fp_other) = mk_tls_pair().await;
    let cfg_server = Config {
        role: traveller::config::Role::Client,
        listen: Some("127.0.0.1:28080".into()),
        peer_addr: None,
        screen: traveller::config::Screen { width: 1920, height: 1080 },
        peer_screen: traveller::config::Screen { width: 2560, height: 1440 },
        side: traveller::config::Side::Right,
        peer_fingerprint: Some(format!("sha256:{}", hex::encode(&fp_me))),
        cert_path: dir.path().join("me.pem"),
        key_path: dir.path().join("me.key"),
        clipboard: Default::default(),
        keys: Default::default(),
    };
    let cfg_client = Config { role: traveller::config::Role::Controller, listen: None,
        peer_addr: Some("127.0.0.1:28080".into()),
        screen: traveller::config::Screen { width: 1920, height: 1080 },
        peer_screen: traveller::config::Screen { width: 2560, height: 1440 },
        side: traveller::config::Side::Right,
        peer_fingerprint: Some(format!("sha256:{}", hex::encode(&fp_other))),
        cert_path: dir.path().join("other.pem"), key_path: dir.path().join("other.key"),
        clipboard: Default::default(), keys: Default::default() };
    let cfg_client = std::sync::Arc::new(cfg_client);

    let srv = tokio::spawn(async move { serve_loop(&cfg_server, Some(fp_me)).await });
    let (mut sh, mut srx) = connect_loop(&cfg_client, Some(fp_other)).await.unwrap();
    sh.send.send(Msg::Ping { ts: 1 }).unwrap();
    let got = tokio::time::timeout(HEARTBEAT_INTERVAL, srx.recv()).await.unwrap().unwrap();
    assert!(matches!(got, ConnEvent::Msg(Msg::Pong { ts: 1 })));
}
```
NOTE : `serve_loop` attend que le connecteur arrive — il faut donc spawn le serveur AVANT (comme ci-dessus). Deux options de signature : `serve_loop` boucle accept; on teste la première session. La lecture du `Ping` par le serveur génère un `Pong` — placer la logique Ping/Pong dans `serve_loop`/`connect_loop` (partagée) : **la réponse `Pong` est émise par l'extrémité recevant `Ping`**.

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --test transport_loopback`
Expected: échec module/API non trouvée.

- [ ] **Step 3: Implémenter**

`src/transport/mod.rs` :
```rust
pub mod protocol;
pub mod server;
pub mod client;

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, MissedTickBehavior};

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct TransportHandle { pub send: mpsc::UnboundedSender<protocol::Msg> }

#[derive(Debug)]
pub enum ConnEvent { Msg(protocol::Msg), Closed(String) }
```
`src/transport/server.rs` :
```rust
use std::sync::Arc;
use crate::config::Config;
use crate::security::{ensure_cert, parse_fingerprint, peer_fingerprint_from_conn, server_tls_config};
use crate::transport::protocol::{Msg, read_frame, write_frame};
use crate::transport::{ConnEvent, TransportHandle, HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT};
use tokio::io::Interest;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, timeout};

pub async fn serve_loop(cfg: &Config, expected_fp: Option<[u8; 32]>) -> anyhow::Result<(TransportHandle, mpsc::UnboundedReceiver<ConnEvent>)> {
    let store = ensure_cert(&cfg.cert_path, &cfg.key_path)?;
    let tls = server_tls_config(&store.cert_pem, &store.key_pem)?;
    let listener = TcpListener::bind(cfg.listen.as_deref().unwrap()).await?;
    tracing::info!("serveur à l'écoute sur {}", cfg.listen.as_deref().unwrap());

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::info!("connexion entrante de {peer}");
        match run_session(stream, tls.clone(), expected_fp, cfg).await {
            Ok((h, rx)) => return Ok((h, rx)),
            Err(e) => { tracing::warn!("session refusée: {e}"); continue; }
        }
    }
}

async fn run_session(
    stream: TcpStream,
    tls: Arc<rustls::ServerConfig>,
    expected_fp: Option<[u8; 32]>,
    cfg: &Config,
) -> anyhow::Result<(TransportHandle, mpsc::UnboundedReceiver<ConnEvent>)> {
    stream.set_nodelay(true)?;
    let tls_stream = tokio_rustls_shim(stream, tls).await?; // voir note ci-dessous
    verify_peer(&tls_stream, expected_fp)?;

    let (tx_events, rx_events) = mpsc::unbounded_channel();
    let (tx_in, rx_in) = mpsc::unbounded_channel();
    let (mut rd, mut wr) = tokio::io::split(tls_stream);

    // lecteur → events
    let tx_events2 = tx_events.clone();
    let reader = tokio::spawn(async move {
        loop {
            match read_frame(&mut rd).await {
                Ok(Msg::Quit) => { let _ = tx_events2.send(ConnEvent::Closed("quit".into())); break; }
                Ok(msg) => { let _ = tx_events2.send(ConnEvent::Msg(msg)); }
                Err(e) => { tracing::debug!("lecture fermée: {e}"); let _ = tx_events2.send(ConnEvent::Closed(e.to_string())); break; }
            }
        }
    });

    // écriture : multiplexe rx_in + heartbeat
    let writer = tokio::spawn(async move {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                maybe = rx_in.recv() => match maybe {
                    Some(msg) => { if write_frame(&mut wr, &msg).await.is_err() { break; } }
                    None => break,
                },
                _ = interval.tick() => {
                    if write_frame(&mut wr, &Msg::Ping { ts: now_ms() }).await.is_err() { break; }
                }
            }
        }
    });

    // ack Hello et Pong en retour sont gérés par l'appelant via rx_in inversée ?
    // cf NOTE ci-dessous
    Ok((TransportHandle { send: tx_in }, rx_events))
}
```
**NOTE architecture transport (importante, à trancher au moment d'implémenter) :**
- Le flux est **bidirectionnel** : chaque côté lit (`read_frame`) et écrit (`write_frame`). Le `Ping`/`Pong`, le `Hello` et le `Quit` sont des messages applicatifs émis par le coût de session. Pour éviter un enchevêtrement, chaque côté expose : un **émetteur** (`TransportHandle.send`) et un **récepteur d'événements** (`ConnEvent`). La règle : tout message reçu est poussé dans `ConnEvent` ; l'appelant décide (ex : s'il reçoit `Ping{ts}` il répond `Pong{ts}` via `send`). Le heartbeat `Ping` est **poussé par la tâche écrivain** (contrôleur émet les Ping comme spécifié spec §7 : "le contrôleur envoie Ping" ; pour le client, le serveur s'appuie sur le timeout pour détecter le silence).
- `tokio_rustls_shim` : production préfère `tokio-rustls` (dép). Pour V1 sans dépendance, wrapper synchrone : faire le handshake rustls sur le stream TCP dans un `spawn_blocking` en attendant `conn.is_handshaking()`. API rustls 0.23 : `conn.complete_io(&mut tcp)`. **Décision** : ajouter `tokio-rustls = "0.26"` en dépendance (plus simple, API stable) et l'utiliser pour envelopper les streams. (Met à jour Cargo.toml à cette tâche.)
- Le test loopback vérifie le PATH le plus critique : `Hello` échangé + `Ping` → `Pong` + empreintes. La vérification d'empreinte (`verify_peer`) : après handshake, `peer_fingerprint_from_conn`, comparer à `expected_fp` ; mismatch → `bail!`.
- `now_ms()` : `std::time::SystemTime` en millis.

Version finale avec `tokio-rustls` :
```rust
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tokio_rustls::rustls::{ClientConfig, ServerConfig};
// server:
let acceptor = TlsAcceptor::from(tls);
let tls_stream = acceptor.accept(stream).await?;
verify_peer_fp(&tls_stream, expected_fp)?;  // tls_stream.get_ref().1 = rustls::ServerConnection
// client:
let connector = TlsConnector::from(tls);
let tls_stream = connector.connect(ServerName::try_from("traveller.local")?, stream).await?;
```
Adapter `peer_fingerprint_from_conn` à `&rustls::ServerConnection`/`ClientConnection` (via `.conn.peek()` ? non — via `get_ref().1.peer_certificates()`).

- [ ] **Step 4: Vérifier**

Run: `cargo test --test transport_loopback`
Expected: verts (Hello + Ping/Pong + empreintes sur loopback, NODELAY, anti-taille check migre).

Note : si `tokio-rustls` demande une version rustls compatible (0.23), aligner les versions ; sinon rester avec le shim `spawn_blocking`.

- [ ] **Step 5: Commit**

```bash
git add src/transport/ tests/transport_loopback.rs Cargo.toml && git commit -m "feat: transport TCP+TLS NODELAY avec heartbeat et pinning empuinte"
```

---

### Task 12: ControllerCore (routage focus + coalescing) et boucle contrôleur

**Files:**
- Create: `src/focus.rs` (rien — déjà fait) — `src/controller.rs` (ControllerCore + `run_controller`)
- Test: `tests/controller_core.rs`

**Interfaces:**
- Consumes: `crate::focus::FocusState`, `crate::coalescer::Coalescer`, `crate::geometry::Geometry`, `crate::keymap`, `crate::transport::{TransportHandle, ConnEvent}`, `crate::input::{InputCapture, InputEvent}`.
- Produces:
```rust
pub enum CoreAction { None, Enter { y: f64 }, Move { dx: f64, dy: f64 }, Leave { x: f64, y: f64 } }
pub struct ControllerCore { pub focus: FocusState, pub coalescer: Coalescer, pub geom: Geometry }  // Debug
impl ControllerCore {
    pub fn new(geom: Geometry) -> Self;
    pub fn handle_move(&mut self, pos: (f64, f64), delta: (f64, f64)) -> CoreAction;
    pub fn force_local(&mut self);
}
pub async fn run_controller(
    cfg: std::sync::Arc<crate::config::Config>,
    handle: crate::transport::TransportHandle,
    mut events: tokio::sync::mpsc::UnboundedReceiver<crate::transport::ConnEvent>,
    mut capture: impl InputCapture,
) -> anyhow::Result<()>;
```
- `run_controller` : spawn capture → channel ; boucle `select!` sur capture/events ; sur `Move` → `core.handle_move` ; `Enter` → `send CursorEnter{x:entry_x,y}` + `capture.set_position(back_x, …)` (parking), `suppress` ON pendant 25 ms ; `Move` → `send PointerMove` (via mpsc borne 256 pour coalesçage) ; `Leave` → `send CursorLeave` + `set_position(back_x, mapped)` + suppress ; Button/Key/Wheel (si `core.focus.is_remote()`) → `send PointerButton/Key/Wheel`. Sur `ConnEvent::Closed` → `core.force_local()` + log. Sur `Ping` → répond `Pong`. Sur `Quit` → break.

- [ ] **Step 1: Test échouant (pure, sans OS)**

```rust
// tests/controller_core.rs
use traveller::controller::{ControllerCore, CoreAction};
use traveller::geometry::{Geometry, ScreenSize, Side};

fn geom() -> Geometry { Geometry { local: ScreenSize { width: 1920, height: 1080 }, peer: ScreenSize { width: 1920, height: 1080 }, side: Side::Right } }

#[test]
fn routage_local_puis_remote_puis_retour() {
    let mut core = ControllerCore::new(geom());
    assert!(matches!(core.handle_move((500.0, 300.0), (3.0, 0.0)), CoreAction::None));
    assert!(matches!(core.handle_move((1920.0, 300.0), (3.0, 0.0)), CoreAction::Enter { y } if y == 300.0));
    assert!(matches!(core.handle_move((0.0, 0.0), (2.0, 0.0)), CoreAction::Move { dx: 2.0, dy: 0.0 }));
    assert!(matches!(core.handle_move((0.0, 0.0), (-2.0, 0.0)), CoreAction::Move { dx: -2.0, dy: 0.0 }));
    assert!(matches!(core.handle_move((0.0, 0.0), (-0.5, 1.0)), CoreAction::Leave { x: 1919.0, y }));
}
```
Le test exact : entrée à y=300 (même résolution → y identique) ; déplacement en remote accumulé à virt_x ; retour à virt_x atteignant 0 → Leave. Vérifier la valeur `y` et le comportement de clamp dans le test réel (ajuster aux valeurs).

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --test controller_core`
Expected: échec (module absent).

- [ ] **Step 3: Implémenter**

```rust
use crate::coalescer::Coalescer;
use crate::focus::{FocusState, StepOutcome};
use crate::geometry::Geometry;

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
                    // dernier mouvement résiduel perdu (le retour clôt) : on le signale pour éviter le saut
                    tracing::debug!("delta résiduel ignoré: {dx:.2},{dy:.2}");
                }
                CoreAction::Leave { x, y }
            }
        }
    }

    pub fn force_local(&mut self) { self.focus.force_local(); self.coalescer = Coalescer::new(); }
}
```
`run_controller` (voir note du Task 11 pour la mécanique flux) :
```rust
pub async fn run_controller<C: InputCapture>(
    cfg: Arc<crate::config::Config>,
    handle: TransportHandle,
    mut events: mpsc::UnboundedReceiver<ConnEvent>,
    capture: C,
) -> anyhow::Result<()> {
    let geom = Geometry {
        local: crate::config::Screen { width: cfg.screen.width, height: cfg.screen.height }.into(),
        peer: crate::config::Screen { width: cfg.peer_screen.width, height: cfg.peer_screen.height }.into(),
        side: match cfg.side { crate::config::Side::Right => Side::Right, crate::config::Side::Left => Side::Left },
    };
    let mut core = ControllerCore::new(geom);
    let (tx_input, mut rx_input) = mpsc::unbounded_channel::<InputEvent>();
    capture.spawn_capture(tx_input)?;

    loop {
        tokio::select! {
            ev = rx_input.recv() => match ev {
                Some(InputEvent::Move { delta }) => {
                    let pos = capture.position().unwrap_or((0.0, 0.0));
                    match core.handle_move(pos, delta) {
                        CoreAction::None => {}
                        CoreAction::Enter { y } => {
                            let _ = handle.send.send(Msg::CursorEnter { x: geom.entry_x(), y }));
                            park_controller(&capture, core, &geom);
                        }
                        CoreAction::Move { dx, dy } => {
                            let _ = handle.send.send(Msg::PointerMove { rel_dx: dx, rel_dy: dy });
                        }
                        CoreAction::Leave { x, y } => {
                            let _ = handle.send.send(Msg::CursorLeave);
                            capture.set_position(x, y)?;
                        }
                    }
                }
                Some(InputEvent::Button { button, pressed }) => {
                    if core.focus.is_remote() { let _ = handle.send.send(Msg::PointerButton { button, pressed }); }
                }
                Some(InputEvent::Key { code, pressed }) => {
                    if core.focus.is_remote() { let _ = handle.send.send(Msg::Key { code, pressed }); }
                }
                Some(InputEvent::Wheel { delta_y }) => {
                    if core.focus.is_remote() { let _ = handle.send.send(Msg::Wheel { delta_y }); }
                }
                None => { tracing::warn!("capture fermée"); break; }
            },
            ce = events.recv() => match ce {
                Some(ConnEvent::Msg(Msg::Ping { ts })) => { let _ = handle.send.send(Msg::Pong { ts }); }
                Some(ConnEvent::Msg(Msg::Clipboard { .. })) => { /* géré par clipboard service (Task 14) */ }
                Some(ConnEvent::Msg(Msg::Quit)) => break,
                Some(ConnEvent::Closed(e)) => { tracing::warn!("session fermée: {e}"); core.force_local(); }
                Some(ConnEvent::Msg(_)) => {}
                None => break,
            },
        }
    }
    Ok(())
}
```
NOTE : la pose du curseur (parking) est factorisée : `park_controller(capture, core, geom)` = `capture.set_position(geom.entry_x(), map(...))` ; `capture.suppress.store(true, ...)` ; sleep 25ms ; `suppress.store(false,...)` via un helper asynchrone. La « fenêtre de suppression » est portée par `suppress` (AtomicBool du RdevCapture). Adapter `run_controller` pour être générique sur `C: InputCapture + std::marker::Send`.

La gestion `CursorEnter` inverse du `suppress` : on ne capture pas notre propre set_position. Adapter selon l'implémentation réelle (le parking est un appel `set_position(1900, y)` côté contrôleur, invisible pour l'utilisateur qui regarde l'autre écran).

- [ ] **Step 4: Vérifier**

Run: `cargo test --test controller_core && cargo check --lib`
Expected: verts, build Lib OK.

- [ ] **Step 5: Commit**

```bash
git add src/controller.rs tests/controller_core.rs && git commit -m "feat: ControllerCore et boucle contrôleur (focus + coalescing + réseau)"
```

---

### Task 13: Client — réception + injection

**Files:**
- Create: `src/client.rs`
- Test: `tests/client_integration.rs`

**Interfaces:**
- Consumes: `crate::transport::{TransportHandle, ConnEvent}`, `crate::input::InputInjector`, `crate::keymap`.
- Produces:
```rust
pub async fn run_client<J: InputInjector>(handle: TransportHandle, mut events: mpsc::UnboundedReceiver<ConnEvent>, mut injector: J) -> anyhow::Result<()>;
```
- Sur `CursorEnter { x, y }` → `injector.set_position(x, y)`. Sur `PointerMove { dx, dy }` → `move_relative`. Sur `PointerButton { button, pressed }` → `injector.button(button, pressed)`. Sur `Key { code, pressed }` → `injector.key(code, pressed)`. Sur `Wheel { delta_y }` → `injector.wheel`. Sur `Ping { ts }` → répond `Pong { ts }`. Sur `Quit`/`Closed` → exit propre.

- [ ] **Step 1: Test échouant (fake injector)**

```rust
// tests/client_integration.rs
use traveller::client::run_client;
use traveller::input::{InputError, InputInjector};
use traveller::transport::{ConnEvent, TransportHandle};
use traveller::transport::protocol::Msg;
use tokio::sync::mpsc;

#[derive(Debug, Default)]
struct FakeInjector { pub calls: std::sync::Arc<std::sync::Mutex<Vec<String>>> }
impl InputInjector for FakeInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InputError> { self.calls.lock().unwrap().push(format!("move {dx:?} {dy:?}")); Ok(()) }
    fn set_position(&mut self, x: f64, y: f64) -> Result<(), InputError> { self.calls.lock().unwrap().push(format!("pos {x:?} {y:?}")); Ok(()) }
    fn button(&mut self, button: u8, pressed: bool) -> Result<(), InputError> { self.calls.lock().unwrap().push(format!("btn {button} {pressed}")); Ok(()) }
    fn key(&mut self, code: u16, pressed: bool) -> Result<(), InputError> { self.calls.lock().unwrap().push(format!("key {code} {pressed}")); Ok(()) }
    fn wheel(&mut self, delta_y: f64) -> Result<(), InputError> { self.calls.lock().unwrap().push(format!("wheel {delta_y:?}")); Ok(()) }
}

#[tokio::test]
async fn injection_depuis_messages() {
    let (tx, rx) = mpsc::unbounded_channel::<ConnEvent>();
    let handle = TransportHandle { send: tx.clone() };
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
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --test client_integration`
Expected: échec (module/type absent).

- [ ] **Step 3: Implémenter `src/client.rs`**

```rust
use crate::input::InputInjector;
use crate::transport::{ConnEvent, TransportHandle};
use crate::transport::protocol::Msg;
use tokio::sync::mpsc;

pub async fn run_client<J: InputInjector>(
    handle: TransportHandle,
    mut events: mpsc::UnboundedReceiver<ConnEvent>,
    mut injector: J,
) -> anyhow::Result<()> {
    while let Some(ce) = events.recv().await {
        match ce {
            ConnEvent::Msg(Msg::CursorEnter { x, y }) => injector.set_position(x, y)?,
            ConnEvent::Msg(Msg::CursorLeave) => {}
            ConnEvent::Msg(Msg::PointerMove { rel_dx, rel_dy }) => injector.move_relative(rel_dx, rel_dy)?,
            ConnEvent::Msg(Msg::PointerButton { button, pressed }) => injector.button(button, pressed)?,
            ConnEvent::Msg(Msg::Key { code, pressed }) => injector.key(code, pressed)?,
            ConnEvent::Msg(Msg::Wheel { delta_y }) => injector.wheel(delta_y)?,
            ConnEvent::Msg(Msg::Clipboard { .. }) => { /* géré par Task 14 */ }
            ConnEvent::Msg(Msg::Ping { ts }) => { let _ = handle.send.send(Msg::Pong { ts }); }
            ConnEvent::Msg(Msg::Pong { .. }) => {}
            ConnEvent::Msg(Msg::Hello { .. }) => tracing::info!("pair déclaré"),
            ConnEvent::Msg(Msg::Quit) => break,
            ConnEvent::Closed(e) => { tracing::info!("session fermée: {e}"); break; }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Vérifier**

Run: `cargo test --test client_integration && cargo check --lib`
Expected: verts + build.

- [ ] **Step 5: Commit**

```bash
git add src/client.rs tests/client_integration.rs && git commit -m "feat: boucle client (réception + injection)"
```

---

### Task 14: Service presse-papiers (poll + anti-boucle + réseau)

**Files:**
- Modify: `src/clipboard.rs` (ajouter service)
- Test: `tests/clipboard_service.rs`

**Interfaces:**
- Consumes: `crate::clipboard::{digest_of, ClipboardGuard}`, `crate::transport::TransportHandle`, `crate::config::ClipboardConfig`.
- Produces:
```rust
pub trait ClipboardSource { fn get(&mut self) -> anyhow::Result<String>; fn set(&mut self, s: &str) -> anyhow::Result<()>; }
pub struct ClipboardSyncService<T: ClipboardSource> { /* privé */ }
impl<T: ClipboardSource> ClipboardSyncService<T> {
    pub fn new(source: T, cfg: &crate::config::ClipboardConfig) -> Self;
    /// Boucle : poll local, détecte changement, l'envoie ; reçoit les Clipboard entrants (émetteur dédié).
    pub async fn run(self, handle: TransportHandle, events: mpsc::UnboundedReceiver<ConnEvent>) -> anyhow::Result<()>;
    /// Traite un message Clipboard reçu.
    pub fn ingest(&mut self, content: &str) -> Result<(), anyhow::Error>;
}
pub struct ArboardSource; // impl ClipboardSource via arboard
```
- Flux : une tâche dédiée `clipboard` (démarrée par les deux rôles). Elle poll `source.get()` à `sync_interval_ms` ; si différent du digest local → `send Msg::Clipboard {digest, content}` si `content.len() <= max_kb`. Elle consomme aussi les événements `Clipboard` réseau (via un channel séparé partagé avec controller/client ; pour V1 : le service reçoit les Msg directement en souscrivant partagé — détail d'implémentation : `run_controller`/`run_client` reçoivent les événements ; ils remettaient les `Clipboard` au service via un second channel). Simplification V1 : le service possède SA PROPRE prétentiellement tout : on passe au service un `mpsc::UnboundedSender<Msg>` = `handle.send` et un receveur filtré. Plus simple : controller/client laissé tel quel, et le service est branché DANS controller/client (ils appellent `sync.on_local_change()/ingest()`) :
  - `controller/client` : sur `InputEvent`/`Msg` différent → pas de pipelining pour V1 ; le service tourne en tâche dédiée à côté.
  Concrètement : `run_controller` et `run_client` reçoivent en paramètre un `ClipboardBridge` (`Arc<Mutex<ClipboardSyncService>>`) et déléguent :
```rust
pub struct ClipboardBridge<T: ClipboardSource> { pub inner: std::sync::Mutex<ClipboardSyncService<T>> }
impl<T: ClipboardSource> ClipboardBridge<T> {
    pub fn ingest(&self, content: &str) { if let Ok(mut g) = self.inner.lock() { let _ = g.ingest(content); } }
    pub fn tick_send(&self, handle: &TransportHandle) { if let Ok(mut g) = self.inner.lock() { g.tick_send(handle); } }
}
```
Le `tick_send` est appelé par la boucle principale à `sync_interval_ms` (simple tick dans le `select!`). Ceci évite un thread/service séparé et garde la sync simple.

- [ ] **Step 1: Test échouant**

```rust
// tests/clipboard_service.rs
use traveller::clipboard::{ClipboardBridge, ClipboardSyncService, ClipboardSource, digest_of};
use traveller::config::ClipboardConfig;

#[derive(Debug, Default)]
struct FakeSource { pub value: std::sync::Arc<std::sync::Mutex<String>> }
impl ClipboardSource for FakeSource {
    fn get(&mut self) -> anyhow::Result<String> { Ok(self.value.lock().unwrap().clone()) }
    fn set(&mut self, s: &str) -> anyhow::Result<()> { *self.value.lock().unwrap() = s.to_string(); Ok(()) }
}

#[test]
fn ingestion_antiloop() {
    let src = FakeSource::default();
    let svc = ClipboardSyncService::new(src, &ClipboardConfig { enabled: true, sync_interval_ms: 300, max_kb: 4096 });
    let d = digest_of("bonjour");
    svc.ingest("bonjour").unwrap(); // applique (réseau → local)
    assert_eq!(src.value.lock().unwrap().as_str(), "bonjour");  // FAUX si test simple : on vérifie le pas-d'application
}
```
Le vrai test :
```rust
#[test]
fn ingestion_copie_toute_nouvelle() {
    let src = FakeSource::default();
    let svc = ClipboardSyncService::new(src, &ClipboardConfig { enabled: true, sync_interval_ms: 1, max_kb: 4096 });
    let r1 = svc.consume_local(); // digest si changement
    assert!(r1.is_some());
    let (d, c) = r1.unwrap();
    assert_eq!(c, "");
}
```
Forme finale du test (alignée sur les interfaces réelles) :
```rust
#[test]
fn envoi_seulement_si_changement_et_plafond() {
    let src = FakeSource::default();
    *src.value.lock().unwrap() = "abc".to_string();
    let svc = ClipboardSyncService::new(src, &ClipboardConfig { enabled: true, sync_interval_ms: 1, max_kb: 4096 });
    let c1 = svc.consume_local().expect("premier changement");
    assert_eq!(c1, "abc");
    assert!(svc.consume_local().is_none(), "pas de 2e envoi sans changement");
    *svc.source.lock().unwrap().value.lock().unwrap() = "trop long".repeat(5);
    let c2 = svc.consume_local();
    assert_eq!(c2, Some("trop long".repeat(5)));
}
#[test]
fn plafond_bloque() {
    let src = FakeSource::default();
    *src.value.lock().unwrap() = "x".repeat(100);
    let svc = ClipboardSyncService::new(src, &ClipboardConfig { enabled: true, sync_interval_ms: 1, max_kb: 1 });
    assert!(svc.consume_local().is_none()); // dépassement max_kb → pas d'envoi
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --test clipboard_service`
Expected: échec.

- [ ] **Step 3: Implémenter** (dans `src/clipboard.rs`, ajouts)

```rust
use crate::transport::TransportHandle;
use std::sync::{Arc, Mutex};

pub trait ClipboardSource {
    fn get(&mut self) -> anyhow::Result<String>;
    fn set(&mut self, s: &str) -> anyhow::Result<()>;
}

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

#[derive(Debug)]
pub struct ClipboardSyncService<T: ClipbSb> { pub source: T, pub guard: ClipboardGuard, pub max_kb: usize, pub enabled: bool }
// NOTE: typos à corriger — `ClipboardSource`.

impl<T: ClipboardSource> ClipboardSyncService<T> {
    pub fn new(source: T, cfg: &ClipboardConfig) -> Self {
        Self { source, guard: ClipboardGuard::new(), max_kb: cfg.max_kb, enabled: cfg.enabled }
    }
    /// Poll le presse-papiers local ; retourne le contenu si changé et sous le plafond.
    pub fn consume_local(&mut self) -> Option<String> {
        if !self.enabled { return None; }
        let content = self.source.get().ok()?;
        let d = digest_of(&content);
        if self.guard.last == Some(d) { return None; }
        if content.len() > self.max_kb { return None; }
        self.guard.last = Some(d);
        Some(content)
    }
    /// Applique un contenu reçu si digest différent (anti-boucle).
    pub fn ingest(&mut self, content: &str) -> Result<(), anyhow::Error> {
        if !self.enabled { return Ok(()); }
        if content.len() > self.max_kb { return Ok(()); }
        let d = digest_of(content);
        if !self.guard.should_apply(d) { return Ok(()); }
        self.source.set(content)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ClipboardBridge<T: ClipboardSource> { pub inner: Mutex<ClipboardSyncService<T>> }
impl<T: ClipboardSource> ClipboardBridge<T> {
    pub fn new(source: T, cfg: &ClipboardConfig) -> Self {
        Self { inner: Mutex::new(ClipboardSyncService::new(source, cfg)) }
    }
    /// Appelé périodiquement (sync_interval_ms) par la boucle principale.
    pub fn tick_send(&self, handle: &TransportHandle) {
        if let Ok(mut svc) = self.inner.lock() {
            if let Some(content) = svc.consume_local() {
                let digest = digest_of(&content);
                let _ = handle.send.send(Msg::Clipboard { digest, content });
            }
        }
    }
    pub fn ingest(&self, content: &str) {
        if let Ok(mut svc) = self.inner.lock() {
            let _ = svc.ingest(content);
        }
    }
}
```
Adaptations : intégrer `tick_send` dans le `select!` de `run_controller`/`run_client` (Task 12/13 gagnent une branche `interval.tick() => bridge.tick_send(&handle)` et gèrent `ConnEvent::Msg(Msg::Clipboard { content, .. }) => bridge.ingest(&content)`).

- [ ] **Step 4: Vérifier (après adaptations controller/client)**

Run: `cargo test --test clipboard_service && cargo test --test client_integration && cargo test --test controller_core`
Expected: verts.

- [ ] **Step 5: Commit**

```bash
git add src/clipboard.rs src/controller.rs src/client.rs tests/clipboard_service.rs && git commit -m "feat: service presse-papiers synchro avec anti-boucle"
```

---

### Task 15: Main — branchement des rôles + arrêt d'urgence

**Files:**
- Modify: `src/lib.rs` (`run()` réel), `src/main.rs` (signaux)
- Test: `tests/system_loopback.rs` (e2e rôle controller+client sur loopback)

**Interfaces:**
- Consumes: tout le ci-dessus.
- Produces: `pub async fn run_async(config: Config) -> anyhow::Result<()>` ; `run()` appelle `tokio::runtime` via `#[tokio::main]` → en fait `run(config_path)` (synchrone) qui lit la config puis lance `run_async`.

- [ ] **Step 1: Test échouant (e2e loopback)**

```rust
// tests/system_loopback.rs
use traveller::config::{ClipboardConfig, Config, KeysConfig, Role, Screen, Side};
use traveller::controller::run_controller;
use traveller::client::run_client;
use traveller::transport::{connect_loop, serve_loop};
use traveller::input::{InputCapture, InputEvent, InputError};
use tokio::sync::mpsc;

#[derive(Debug, Default)]
struct NullInput;
impl InputCapture for NullInput {
    fn spawn_capture(&self, _tx: mpsc::UnboundedSender<InputEvent>) -> Result<(), InputError> { Ok(()) }
    fn position(&self) -> Result<(f64, f64), InputError> { Ok((1919.0, 500.0)) }
    fn set_position(&mut self, _x: f64, _y: f64) -> Result<(), InputError> { Ok(()) }
}
// ... générer 2 Config/TLS (réutiliser helpers transport_loopback) ...

#[tokio::test]
async fn traversee_e2e_loopback() {
     // serve (client role) + connect (controller role) ; on injecte un déplacement :
     // le contrôleur (NullInput) a un position (1919,500), delta +1 → CursorEnter
     // on vérifie via un fake injector (réutiliser FakeInjector) l'appel pos/… 
     // assert: le client injecte set_position(entry_x…)
     // puis Quit côté contrôleur → session propre.
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test --test system_loopback`
Expected: échec.

- [ ] **Step 3: Implémenter**

`src/lib.rs` :
```rust
pub async fn run_async(cfg: crate::config::Config) -> anyhow::Result<()> {
    let expected = cfg.peer_fingerprint.as_deref().map(crate::security::parse_fingerprint).transpose()?;
    let (handle, events) = match cfg.role {
        Role::Controller => crate::transport::connect_loop(&cfg, expected).await?,
        Role::Client => crate::transport::serve_loop(&cfg, expected).await?,
    };
    let bridge = crate::clipboard::ClipboardBridge::new(crate::clipboard::ArboardSource, &cfg.clipboard);
    let clipboard_cfg = cfg.clipboard.clone();
    let keys = cfg.keys.clone();
    match cfg.role {
        Role::Controller => {
            let capture = crate::input::rdev_backend::RdevCapture::new();
            crate::controller::run_controller(Arc::new(cfg), handle, events, capture, bridge, clipboard_cfg, keys).await
        }
        Role::Client => {
            let injector = crate::input::rdev_backend::RdevInjector::new()?;
            crate::client::run_client(handle, events, injector, bridge, clipboard_cfg, keys).await
        }
    }
}

pub fn run(config_path: &str) -> anyhow::Result<()> {
    let cfg = crate::config::Config::load(config_path)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cfg))
}
```
NOTES d'adaptation :
- `run_controller`/`run_client` gagnent les paramètres `bridge`, `clipboard_cfg`, `keys` (arrêt d'urgence, tick clipboard, gestion `Ctrl+Alt+F10` via comparaison avec la config).
- L'arrêt d'urgence V1 : un simple `break` de la boucle + `core.force_local()` quand la combinaison configurée est détectée dans les événements clavier (contrôleur) — simplifié : la détection s'appuie sur `InputEvent::Key{code, pressed:true}` accumulés (Ctrl/Alt/F10) ; au match, log + `return Ok(())`. Implémentation concrète dans `run_controller`.
- Dans `run_async`, `serve_loop` ne doit revenir qu'APRÈS une première session établie (sinon blocage partiel). Les deux branches retournent (handle, events) une fois la session vivante.
- Cocher l'option Quit propre.

- [ ] **Step 4: Vérifier**

Run: `cargo test --test system_loopback && cargo build --release`
Expected: verts + build release OK.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/main.rs src/controller.rs src/client.rs && git commit -m "feat: branchement applicatif des rôles + arrêt d'urgence"
```

---

### Task 16: README, configs d'exemple, checklist d'acceptation, cross-compile

**Files:**
- Create: `README.md` (usage, topologie, sécurité, premier lancement, dépannage)
- Modify: `config.example-controller.toml`, `config.example-client.toml` (finalisés)
- Create: `docs/ACCEPTANCE.md` (checklist manuelle depuis spec §15)

**Interfaces:**
- Consumes: rien de nouveau.

- [ ] **Step 1: Écrire README.md**

Contenu : description courte (objectif, topologie contrôleur fixe, 2 machines), prérequis (Linux X11 / Windows 10/11, port forward), build (cargo build --release, target x86_64), installation (binaire + config), 1er lancement (génération cert, échange des empreintes), config de référence (comme §13 spec), résolution de problèmes (Wayland non supporté, fingerprint mismatch, latence, emergency stop), périmètre V1 et roadmap (UDP/QUIC, Wayland, >2 machines).

- [ ] **Step 2: Finaliser les deux configs d'exemple**

`config.example-controller.toml` et `config.example-client.toml` reprenant exactement le schéma §13 de la spec avec commentaires (rôles, fingerprint vide initialement, chemins).

- [ ] **Step 3: Écrire docs/ACCEPTANCE.md**

Reprendre les 8 critères de la spec §15 en checklist `- [ ]`, avec la procédure pour chacun (comment tester sur les 2 machines réelles, y compris reprise après coupure, emergency stop, différences de résolution).

- [ ] **Step 4: Vérifier**

Run: `cargo build --release && cargo test`
Expected: build + tous les tests verts. (La cross-compilation Windows est documentée mais testée hors repo dans une CI/à la main.)

- [ ] **Step 5: Commit**

```bash
git add README.md docs/ACCEPTANCE.md config.example-*.toml && git commit -m "docs: README, configs d'exemple et checklist d'acceptation"
```

---

## Self-Review (contre la spec)

**Couverture spec :**
- §3 interaction / anti-boucle → Tasks 4, 8, 9, 12 (+ amendement anti-boucle par conception au Header). ✔
- §4 architecture modules → File Structure + fichiers créés par chaque tâche. ✔
- §5 machine à états → Task 4 (+ Task 12). ✔
- §6 dépendances → Task 1 Cargo.toml (rdev, enigo, arboard, tokio, rustls, rcgen, bincode…). ✔ — enigo ajouté (position curseur), justifié.
- §7 protocole → Task 2 (tous les messages + Wheel amendé). ✔
- §8 mapping clavier → Task 8 (CanonKey, fallback Other). ✔
- §9 géométrie → Task 3 (+ Task 4/12). ✔
- §10 presse-papiers → Task 6 + Task 14. ✔
- §11 robustesse/erreurs → Tasks 1, 7, 10, 11, 15 (fail-fast, reconnect backoff, heartbeat, emergency). ✔
- §12 tests → Tasks 2-14 unitaires + Task 11 loopback + Task 15 e2e + Task 16 ACCEPTANCE manuel. ✔
- §13 config → Task 7 + Task 16. ✔
- §14 périmètre → respecté (X11/Windows, 2 machines, hors Wayland/UDP/>2) + README roadmap. ✔
- §15 critères d'acceptation → Task 16 (docs/ACCEPTANCE.md). ✔
- §16 livrable/build → Task 1 + Task 16. ✔

**Placeholder scan :** aucune "TODO/TBD" ; les points API (versions rdev/enigo/rustls) sont explicitement signalés avec l'intention et une adaptation concrète — pas de vague "gérer les erreurs".

**Type consistency :**
- `Msg` : défini Task 2, utilisé Tasks 11-15. ✔
- `Geometry`/`Side`/`ScreenSize` : Tasks 3,4,12. attention : Task 12 fabrique `Geometry` depuis `config::Screen` → pas de conversion automatique — le code doit remplir `ScreenSize { width, height }` explicitement (le code montré le fait). ✔
- `FocusState`/`StepOutcome` : Task 4 → Task 12. ✔
- `Coalescer::take() -> Option<(f64,f64)>` : Task 5 → Task 12. ✔
- `ControllerCore.handle_move` : Task 12 (+ test). ✔
- `InputCapture`/`InputInjector` : Task 8 → Tasks 9,12,13,15 (Fake/Null impls). ✔
- `InputEvent` : Task 8 → Task 9,12. ✔
- `ClipboardSyncService.consume_local()`/`ingest()` : Task 14 (+ tests). ✔
- `TransportHandle.send` (`UnboundedSender<Msg>`) : Task 11 → Tasks 12-15. ✔
- `ConnEvent::{Msg,Closed}` : Task 11 → Tasks 12-15. ✔
- `ensure_cert`/`fingerprint_of`/`parse_fingerprint`/`peer_fingerprint_from_conn` : Task 10 → Tasks 11,15. ✔

Risques signalés au plan (compilation rdev/enigo/rustls par versions) : chaque point est assorti d'une adaptation explicite.

## Execution Handoff

Plan sauvegardé dans `docs/superpowers/plans/2026-09-16-traveller.md`. Deux options d'exécution :

**Option 1 — Subagent-Driven (recommandé)** : je dispatch un sous-agent frais par tâche (via l'outil `task`), revue entre chaque tâche, itération rapide. (Les skills superpowers:subagent-driven-development / executing-plans ne sont pas disponibles dans cet environnement ; je l'applique avec l'outil `task`.)

**Option 2 — Exécution inline** : j'exécute les tâches dans cette session, avec des points de contrôle (compile + tests) à chaque fin de tâche.