# traveller — KVM logiciel Rust sur internet

Date : 2026-09-16
Statut : design validé — spec autonome, implémentation-ready.

## 1. Objectif

`traveller` est un partage de souris/clavier entre **deux** PC (un Linux X11, un Windows), par-dessus **internet**. Le curseur du PC "contrôleur" traverse le bord de son écran et apparaît sur l'autre PC, comme Synergy — mais 100 % maison, sans dépendance à une solution tierce, ni runtime à installer.

Cas d'usage : PC Windows + PC Linux côte à côte. La souris/clavier physiques sont branchés sur Windows ; quand le curseur arrive au bord droit de l'écran Windows, le contrôle passe au Linux (qui affiche le curseur et reçoit clics + clavier). Retour par le bord opposé.

## 2. Décisions verrouillées (contexte pour l'agent)

- **Contrôleur fixe** : un seul PC possède les périphériques physiques "vivants" (le contrôleur). L'autre (le client) est contrôlé à distance ; ses périphériques physiques existent mais sont **ignorés**. Pas de rotation de rôle, pas d'arbitrage de grab.
- Navigation **bidirectionnelle** par croisement de bord.
- Connexion **via internet** avec redirection de port (port forwarding) sur la machine qui écoute.
- Tech : **Rust**, un binaire unique cross-platform (Linux X11 + Windows 10/11).
- Transport V1 : **TCP + TLS** avec `TCP_NODELAY`. UDP/QUIC = hors périmètre.
- Nom du projet et du binaire : `traveller`.
- Périmètre strict V1 : 2 machines, 1 pair, résolutions déclarées en config. Wayland exclu (verrouillé par l'OS).

## 3. Modèle d'interaction (détaillé)

Le contrôleur capture ses périphériques locaux en continu. Selon l'état de focus :

- **Focus local** : le curseur est dans l'écran du contrôleur → comportement normal, aucun paquet réseau d'entrée.
- **Crossing sortant** : le curseur atteint le bord partagé (config `side`) ET son déplacement continue vers l'extérieur → le contrôleur envoie `CursorEnter{x,y}`, cache son curseur local, et bascule en focus distant.
- **Focus distant** : le contrôleur traduit les événements de ses périphériques en messages réseau ; le client les injecte localement (déplacements relatifs + événements absolus pour entrée/sortie).
- **Retour** : le contrôleur suit un **curseur virtuel** sur l'écran du client. Quand ce curseur virtuel atteint le bord partagé côté client et continue vers l'intérieur → envoi de `CursorLeave`, retour en focus local, curseur réel repositionné au bord avec Y mis à l'échelle.

**Anti-boucle (critique, non négociable)** : un événement injecté ne doit jamais être recapturé par la capture locale, sinon ping-pong infini.

- X11 : filtrer les événements portant le drapeau `XTEST` (drapeau présent sur les événements synthétisés).
- Windows : filtrer via le bit "injected" de `GetMessageExtraInfo()`.
- À défaut de pouvoir détecter la provenance : comportement en échec-ouvert documenté (log warning + pas d'envoi quand le drapeau est présent).

## 4. Architecture du code (crate unique, modules)

```
Cargo.toml
src/
  main.rs          — point d'entrée : charge la config, lance le rôle
  config.rs        — parsing + validation TOML (fail-fast : config invalide → erreur claire + exit ≠ 0)
  transport/
    protocol.rs    — enum des messages + encode/decode (bincode), frames length-prefixed
    server.rs      — listener TCP+TLS (tokio)
    client.rs      — connecteur TCP+TLS (tokio)
  controller.rs    — machine à états du focus (Local ⇄ Remote) + routage capture→local/réseau
  client.rs        — rôle client : réception + injection, périphériques locaux ignorés
  input/
    mod.rs         — trait PlatformInput { capture, synthèse, filtrage_provenance }
    rdev_backend.rs
  clipboard/
    mod.rs         — trait ClipboardSync { poll et applique avec anti-boucle digest }
    arboard_backend.rs
  security.rs      — TLS rustls, génération cert auto-signé (rcgen), pinning d'empreinte
  geometry.rs      — mapping d'écran, détection de bord, mise à l'échelle Y
```

Règles de conception :
- La logique pure (géométrie, état de focus, coalescing, anti-boucle) est **indépendante de l'OS** : testable sans matériel, sans `rdev`/`arboard`.
- Chaque unité a un rôle unique, communique par interfaces (traits), testable seule.
- Pas de dépendance à une solution KVM tierce. Les crates utilisés sont des blocs de construction (voir §6).

## 5. Machine à états du contrôleur

```
               crossing sortant du bord partagé
   ┌───────────┐  (envoie CursorEnter{x,y},     ┌───────────┐
   │   Local   │   cache curseur)                │  Remote   │
   └───────────┘ ─────────────────────────────▶ └───────────┘
          ▲                                            │
          │                                            │
          │  crossing du bord partagé côté client      │
          │  (envoie CursorLeave, resto apporte        │
          │   curseur au bord, Y mappé)                ▼
```

- `Local` : les événements locaux n'engendrent aucun paquet. Seule la détection de bord est active.
- Transition vers `Remote` : envoi `CursorEnter{x,y}` **avant** le flux de mouvements ; le client affiche le curseur à `(x,y)` (absolu) puis suit les déplacements relatifs.
- `Remote` : `PointerMove` en relatifs, `PointerButton`, `Key` relayés tels quels. Le contrôleur maintient la position virtuelle du curseur côté client pour détecter le bord de retour.
- Transition vers `Local` : envoi `CursorLeave` ; le contrôleur ré-affiche son curseur au bord mappé et repasse en capture locale.
- **Ordonnancement** : tout message sortant est précédé du résultat de l'état courant (jamais deux `CursorEnter` consécutifs sans `CursorLeave`, et inversement).
- État rejouable à tout moment : en cas de reconnexion, l'état revient à `Local` et le `Hello` est ré-échangé.

## 6. Dépendances Rust

| Crate                  | Usage                                                   |
|------------------------|---------------------------------------------------------|
| `tokio`                | runtime async, `TcpListener`/`TcpStream`, time, sync    |
| `rustls`               | TLS 1.3 (chargement cert, handshake, custom verifier)   |
| `rcgen`                | génération certificat auto-signé au 1er lancement       |
| `rustls-pemfile`       | parsing PEM du cert/clé                                 |
| `rdev`                 | capture souris/clavier + synthèse (X11 + Windows)       |
| `arboard`              | lecture/écriture presse-papiers                         |
| `bincode`              | sérialisation binaire compacte des messages             |
| `serde` + `toml`       | config                                                   |
| `sha2` + `hex`         | digest anti-boucle clipboard + empreintes               |
| `tracing` / `tracing-subscriber` | logs console + fichier rotatif                 |
| `anyhow`               | erreurs en couches hautes                               |
| `thiserror`            | erreurs typées des modules                              |

La couche OS est 100 % derrière traits : remplacer `rdev`/`arboard` ne touche pas le reste.

## 7. Protocole

### Encadrement
- Frame : `u32` big-endian (longueur payload) + payload encodé `bincode`.
- Longueur max 16 Mo (presse-papiers). Au-delà : frame rejetée + connexion fermée (anti-crash malveillant).
- Un seul flux TCP+TLS, bidirectionnel.
- `TCP_NODELAY` obligatoire sur le socket (désactiver Nagle).
- Heartbeat : contrôleur envoie `Ping` toutes les ~3 s ; l'autre répond `Pong`. Silence > 15 s → déconnexion.

### Messages

| Message          | Sens                  | Couche | Champs                                                       |
|------------------|-----------------------|--------|--------------------------------------------------------------|
| `Hello`          | contrôleur → client   | empreinte locale + géométrie locale | `device_id: u64`, `screen_w: u16`, `screen_h: u16` |
| `CursorEnter`    | contrôleur → client   | focus  | `x: f64` (abs, écran client), `y: f64`                       |
| `CursorLeave`    | contrôleur → client   | focus  | —                                                            |
| `PointerMove`    | contrôleur → client   | input  | `rel_dx: f64`, `rel_dy: f64`                                 |
| `PointerButton`  | contrôleur → client   | input  | `button: u8`, `pressed: bool`                                |
| `Key`            | contrôleur → client   | input  | `code: u16`, `pressed: bool`                                 |
| `Clipboard`      | bidirectionnel        | app    | `digest: [u8;32]` (SHA-256), `content: String`               |
| `Ping` / `Pong`  | bidirectionnel        | —      | `ts: u64` (ms monotonic)                                     |
| `Quit`           | bidirectionnel        | —      | —                                                            |

- `button` : mappi comme `rdev::Button` (boutons gauche/droit/molette/4/5, ordinal plateforme).
- `code` : code brut `rdev::Key` côté contrôleur, injecté tel quel côté client (les deux sont la même plateforme ? NON — voir §9 : le code doit être mappe par plateforme).
- **Coalescing mouvement** : si le buffer d'envoi sature, jeter les `PointerMove` intermédiaires et n'envoyer que le dernier (somme des deltas restée fidèle au déplacement réel). Clics et touches : jamais jetés.

## 8. Mapping de codes clavier (point d'attention)

Le contrôleur capture via `rdev` une valeur de touche de **sa** plateforme, le client injecte sur **une autre** plateforme (ex: contrôleur Windows → client Linux X11, ou inverse). `rdev` n'expose pas un virtual-key universel standard :

- Cross-platform de confiance : utiliser le **layout physique** via un mapping par position (ex: touche `Key::KeyA` → position physique "A" → code X11 `a` / VK `0x41`).
- Décision : définir un **canonique par position** (type `PhysicalKey`) dans le protocole (u16), avec tables de traduction : contrôleur (rdev) → canonique → plateforme du client.
- V1 : couvrir les touches présentes sur un clavier standard ANSI/ISO (lettres, chiffres, ponctuation, modifiers, navigation). Hors couverture : touche transmise telle quelle + log warning (fallback dégradé acceptable en V1).

## 9. Géométrie & mapping

- Deux écrans côte à côte. Config : résolution locale (`screen`) et résolution du pair (`peer_screen`), `side = "right" | "left"` (position du client vue du contrôleur).
- Entrée (contrôleur → client), `side="right"` :
  `client_x = 0` (bord gauche du client), `client_y = round(local_y * (peer_h-1) / (local_h-1))`.
- Suivi côté contrôleur (curseur virtuel sur le client) : `virt += (rel_dx, rel_dy)`.
- Retour (client → contrôleur), `side="right"` : quand `virt_x <= 0` ET `rel_dx < 0` :
  `local_x = local_w - 1`, `local_y = round(virt_y * (local_h-1) / (peer_h-1))`.
- `side="left"` : symétrie exacte (entrée par le bord droit du client ; retour quand `virt_x >= peer_w - 1` ET `rel_dx > 0`).
- Détection de bord local : `x >= local_w - 1` (right) ou `x <= 0` (left), combinée à la direction du mouvement (`dy`/`dx`). Tolérance : bord à 1 px.
- Les formules vivent dans `geometry.rs`, pures et unit-testées.

## 10. Presse-papiers

- Synchronisation **bidirectionnelle continue** (focus local ou distant), poll à `sync_interval_ms` (défaut 300 ms) des deux côtés.
- Anti-boucle : digest SHA-256 du contenu local comparé au dernier digest reçu avant application ; pas d'application si identiques.
- Plafond : contenu > `max_kb` (défaut 4096) → non transmis (log). `enabled = false` désactive totalement.
- Race au changement pendant la transmission : le digest est comparé à l'application, un contenu obsolète n'écrase pas un contenu plus récent.

## 11. Robustesse & erreurs

- Démarrage **fail-fast** : config absente/invalide, empreinte pair absente (sauf 1er lancement), certificat illisible → message clair, exit code ≠ 0.
- 1er lancement : génération cert auto-signé localement (rcgen), affichage de l'empreinte SHA-256 à reporter dans la config de l'autre machine.
- Connexion : `peer_fingerprint` vide au 1er run autorisé (empreinte affichée), puis exigé — la vérification de l'empreinte pair est **toujours** active quand le champ est renseigné.
- Perte de connexion en session → log + reconnexion auto, backoff exponentiel (1 s, puis 2, 4, … cap 30 s). État du contrôleur forcé à `Local` à la reconnexion, `Hello` ré-échangé.
- Raccourci `emergency_stop` (défaut `Ctrl+Alt+F10`) : coupe la capture immédiatement (restaure le curseur, arrête l'envoi), sans quitter l'app.
- Logging `tracing` : console + fichier rotatif (1 Mo × 5).

## 12. Tests

**Unitaires (sans OS, sans matériel) :**
- Roundtrip encode/decode de tous les messages (bincode).
- Détection de bord + direction (Local→Remote, et retour), les deux `side`.
- Mapping Y proportionnel (entrée et retour, résolutions différentes).
- Machine à états : séquences valides et invalides (`CursorEnter` sans `CursorLeave`, etc.) rejetées par la contrainte d'ordonnancement.
- Coalescing : buffer saturé → dernier mouvement gagne, somme des relatifs conservée.
- Anti-boucle clipboard : digest identique → pas d'application.
- Plafond clipboard : contenu trop gros → pas d'envoi.
- Encadrement : frame trop grande → rejet + fermeture.

**Intégration réseau (localhost, tokio) :**
- server↔client, Hello/Handshake, latence simulée (sleep > seuil) → timeout heartbeat détecté.
- Reconnexion : coupe du flux → backoff → re-Hello → reprise à l'état `Local`.

**Manuel final (checklist, machines réelles) :**
- Traversée gauche↔droite dans les deux sens, mapping Y correct sur des résolutions différentes.
- Clics gauche/droit/molette, molette scroll, clavier (dont combos Ctrl+C/V), caractères accentués.
- Copier-coller bidirectionnel sans boucle.
- Coupure réseau → reconnexion automatique, curseurs restaurés.
- `emergency_stop` : capture coupée immédiatement.
- Anti-boucle vérifié (aucun événement dupliqué — observer les logs).

## 13. Configuration (`config.toml`)

```toml
role = "controller"            # "controller" | "client"
listen = "0.0.0.0:7722"        # si cette machine écoute (port forwarding)
peer_addr = "mondyndns.ddns.net:7722"  # si cette machine se connecte

screen = { width = 1920, height = 1080 }    # écran de CETTE machine
peer_screen = { width = 2560, height = 1440 }
side = "right"                 # position du client vue du contrôleur

peer_fingerprint = "sha256:..."   # exigé après le 1er échange d'empreintes

cert_path = "data/cert.pem"    # généré au 1er lancement
key_path  = "data/key.pem"

[clipboard]
enabled = true
sync_interval_ms = 300
max_kb = 4096

[keys]
emergency_stop = "Ctrl+Alt+F10"
```

- Chemin de config par défaut : `./config.toml` (modifiable via `--config <path>`). Les chemins `cert_path`/`key_path` sont relatifs au fichier de config.
- Une seule machine a `role="controller"`. L'autre `role="client"`.

## 14. Périmètre V1 (explicite)

**Inclus** : Linux X11, Windows 10/11, 2 machines / 1 pair, résolutions en config, presse-papiers ≤ 4 Mo, contrôleur fixe, port forwarding.

**Exclu** : Linux Wayland, > 2 machines, découverte automatique, UDP/QUIC, résolutions dynamiques, partage de fichiers, souris absolue multi-moniteurs complexes.

## 15. Critères d'acceptation (fin de V1)

1. Le curseur traverse contrôleur→client et client→contrôleur, mapping Y exact avec résolutions différentes.
2. Clics, molette et clavier fonctionnent sur le client (y compris combos).
3. Aucun événement dupliqué (anti-boucle vérifié).
4. Presse-papiers synchro bidirectionnelle sans boucle.
5. TLS 1.3 + pinning : une empreinte différente → refus de connexion.
6. Reconnexion auto après coupure, à l'état `Local`.
7. `emergency_stop` fonctionne.
8. Binaire release unique sur chaque plateforme, sans runtime.

## 16. Livrable & build

- Binaire `traveller` en `cargo build --release` pour :
  - `x86_64-unknown-linux-gnu`
  - `x86_64-pc-windows-msvc` (cross-compilation `cargo-xwin` ou CI, ou build natif).
- Aucun runtime à installer sur les machines cibles.
- Deux `config.toml` fournis en exemple : un contrôleur, un client.