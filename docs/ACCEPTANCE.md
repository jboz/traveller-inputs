# Checklist d'acceptation — traveller V1

Procédure manuelle sur **deux machines réelles** (contrôleur + client), portions
1.0 → 1.3, 2.0 → 2.2. Cocher chaque critère une fois validé.

## 1. Traversée du curseur (spec §15.1)

- [ ] Curseur contrôleur → client : franchir le bord partagé (`side = "right"` → bord
      droit) ; le curseur apparaît sur le client et y est contrôlable.
- [ ] Retour client → contrôleur : depuis l'écran du client, ramener le curseur vers
      le bord partagé (gauche) ; il réapparaît sur le contrôleur au bord, au même Y
      mis à l'échelle.
- [ ] Résolutions **différentes** (ex. 1920×1080 contrôleur / 2560×1440 client) :
      le Y d'entrée/sortie est mappé proportionnellement (milieu → milieu, haut →
      haut, bas → bas).
- [ ] Aller-retour répété plusieurs fois sans dérive de position.

## 2. Clics, molette, clavier (spec §15.2)

- [ ] Clic gauche, clic droit, clic molette fonctionnent sur le client.
- [ ] Molette (scroll) fonctionne sur le client dans les deux sens.
- [ ] Clavier : frappe simple, combos `Ctrl+C` / `Ctrl+V`, caractères accentués.

## 3. Anti-boucle (spec §15.3)

- [ ] Aucun événement dupliqué : dans les logs du contrôleur, ne pas voir un même
      événement émis deux fois ni d'écho recapturé par la capture locale.
- [ ] Pendant le focus distant, déplacer la souris physique du client ne provoque
      aucun mouvement sur le contrôleur (périphériques du client ignorés).

## 4. Presse-papiers (spec §15.4)

- [ ] Copier sur le contrôleur → coller sur le client (synchronisation < 1 s avec
      `sync_interval_ms = 300`).
- [ ] Copier sur le client → coller sur le contrôleur.
- [ ] Pas de boucle : après l'application réseau, le poll local ne ré-émet pas le
      même contenu (observer les logs, un seul `Clipboard` par changement).
- [ ] Contenu > `max_kb` : non transmis (log), rien ne plante.

## 5. TLS + pinning (spec §15.5)

- [ ] Premier lancement sans `peer_fingerprint` : connexion acceptée, empreinte
      affichée.
- [ ] `peer_fingerprint` renseigné et correct : connexion établie.
- [ ] `peer_fingerprint` renseigné mais **différent** (ex. changer le cert d'un
      côté) : connexion refusée, message « empreinte refusée » côté serveur,
      le contrôleur retente avec backoff.

## 6. Reconnexion auto (spec §15.6)

- [ ] Couper le réseau (câble/Wi-Fi ou firewall) pendant le focus distant : le
      contrôleur repasse en focus local (curseur restauré à sa position).
- [ ] Rétablir le réseau : reconnexion automatique sans relancer les binaires,
      l'état repart à `Local`.

## 7. Arrêt d'urgence (spec §15.7)

- [ ] En focus distant, appuyer `Ctrl+Alt+F10` (config `[keys] emergency_stop`) :
      la capture s'arrête immédiatement, retour en focus local, pas de pression
      résiduelle transmise.
- [ ] La combinaison est reconfigurable dans la config (test avec une autre combinaison).

## 8. Binaire unique par plateforme (spec §15.8)

- [ ] Contrôleur Windows : binaire `.exe` native, fonctionne sans runtime.
- [ ] Client Linux : binaire ELF `x86_64-unknown-linux-gnu`, fonctionne sans runtime.
- [ ] Lancer `cargo build --release` et vérifier l'absence de dépendances système
      tierces à installer.