# traveller

KVM logiciel Rust par-dessus internet : le curseur du PC **contrôleur** traverse le
bord de son écran et le contrôle passe à l'autre PC (le **client**), comme Synergy —
sans solution tierce ni runtime à installer.

## Topologie

- **Contrôleur** (fixe) — machine qui possède la souris/le clavier physiques « vivants ».
  Elle capture ses périphériques et, quand le curseur franchit le bord partagé, envoie
  les événements au client.
- **Client** — machine contrôlée à distance. Elle **écoute** sur `listen`, reçoit les
  événements et les injecte localement. Ses périphériques physiques sont ignorés.
- Navigation **bidirectionnelle** : retour au contrôleur en ramenant le curseur vers
  l'intérieur depuis le bord opposé.
- Une seule machine a `role="controller"`. Pas de rotation de rôle.

## Prérequis

- Contrôleur : Linux (X11) ou Windows 10/11. Client : Linux (X11) ou Windows 10/11.
  **Wayland n'est pas supporté.**
- **Redirection de port** sur la machine qui écoute (le client par défaut) : le port
  `7722` (ou celui configuré) doit être accessible depuis l'Internet vers le client.
- Côté Windows, la capture et l'injection nécessitent une session interactive
  (pas de service système).

## Build

```bash
cargo build --release
```

Binaires produit :

- `x86_64-unknown-linux-gnu` (build natif Linux)
- `x86_64-pc-windows-msvc` (cross-compilation `cargo-xwin`/CI ou build natif Windows)

Aucun runtime à installer sur les machines cibles.

## Installation

1. Copier le binaire `traveller` sur chaque machine.
2. Copier la config d'exemple adaptée au rôle :
   - `config.example-controller.toml` sur la machine contrôleur,
   - `config.example-client.toml` sur la machine client.
3. Renommer en `config.toml` et ajuster (rôles, écrans, `side`, adresse `peer_addr`).

## Premier lancement

1. Sur le client : `./traveller config.toml` → génère `cert.pem`/`key.pem` et écoute.
2. Sur le contrôleur : `./traveller config.toml` → se connecte. En l'absence
   d'empreinte configurée, la connexion est acceptée et l'empreinte du client est
   **affichée par l'app**.
3. Sur le contrôleur : copier cette empreinte dans `peer_fingerprint` de la config.
4. Sur le client : procéder de même avec l'empreinte du contrôleur (affichée à
   l'établissement de la première session).
5. Redémarrer les deux bouts. Toute connexion à empreinte différente est désormais refusée.

## Configuration de référence

```toml
role = "controller"            # "controller" | "client"
peer_addr = "mondyndns.ddns.net:7722"   # si cette machine se connecte
listen = "0.0.0.0:7722"        # si cette machine écoute (mutuellement exclusif)

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

Chemin de config par défaut : `./config.toml` (modifiable via l'argument positionnel :
`traveller <chemin>`). `cert_path`/`key_path` sont relatifs au fichier de config.

## Démarrage

```bash
# machine cliente
./traveller config.toml
# machine contrôleur
./traveller config.toml
```

Déplacez le curseur du contrôleur vers le côté configuré (`side`) pour passer sur le
client. Ramenez-le vers l'intérieur pour revenir.

## Dépannage

- **Wayland** : non supporté. Utiliser une session X11 (ex. GNOME, passer en
  « Xorg ») sur les deux machines.
- **`fingerprint mismatch` / connexion refusée** : l'empreinte reçue diffère de
  `peer_fingerprint`. Vérifier que les `peer_fingerprint` des deux configs
  correspondent aux empreintes **de l'autre machine** (toujours l'empreinte du pair).
- **Latence élevée** : la latence réseau directe s'ajoute à chaque événement.
  Vérifier le ping, préférer une redirection de port directe (pas de double NAT).
  Le transport utilise `TCP_NODELAY`.
- **Arrêt d'urgence** : `Ctrl+Alt+F10` (configurable dans `[keys] ` `emergency_stop`)
  coupe immédiatement la capture et repasse en focus local.
- **Presse-papiers** : la synchronisation est pollée (300 ms par défaut) et plafonnée
  à `max_kb`. Désactivable via `clipboard.enabled = false`.

## Périmètre V1

**Inclus** : Linux X11, Windows 10/11, 2 machines / 1 pair, résolutions déclarées en
config, presse-papiers ≤ 4 Mo, contrôleur fixe, port forwarding, TLS 1.3 + pinning
d'empreintes, reconnexion automatique.

**Exclu** : Linux Wayland, plus de 2 machines, découverte automatique, UDP/QUIC,
résolutions dynamiques, partage de fichiers, souris absolue multi-moniteurs complexes.

**Roadmap** : UDP/QUIC, Wayland, > 2 machines, résolutions dynamiques.