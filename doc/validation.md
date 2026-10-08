# Validation — 8 octobre 2026

## Résultats

```text
cargo fmt --all -- --check                   PASS
cargo check --locked --all-targets           PASS
cargo test --locked                         24 passed; 0 failed
cargo clippy --locked --all-targets -- -D warnings  PASS
cargo build --release --offline --locked    PASS
node --check src/static/app.js                  PASS
```

Rust 1.99.0 utilisé avec une installation temporaire hors du projet, sans changement permanent du PATH. Avec Rust disponible, les vérifications se relancent depuis la racine :

```sh
sh scripts/check.sh
```

Le binaire `target/release/launch2dashboard` n'a pas besoin de la toolchain pour s'exécuter.

## Couverture

16 tests domaine/adapter : validation, namespace, plists, arguments, KeepAlive, bootstrap échoué, rollback, état après édition, préservation clés inconnues, permissions 0600, refus symlinks, limites de logs, isolement plist invalide, uptime.

8 tests HTTP : Host/Origin/header, lecture et statuts, CRUD/actions, erreurs 400/404/409, ressources et CSP, SSE initial et erreur ultérieure, échappement HTML.

13 scénarios Chrome headless : filtres, recherche, start/stop, création, édition, confirmation de suppression, validation locale, erreur HTTP visible, logs hostiles affichés comme texte, SSE, absence de débordement à 390px, état vide et conservation du focus des cartes/terminaux/panneau après polling. Zéro erreur console inattendue. Le cas HTTP 400 volontaire est compté comme scénario d'erreur attendu.

Captures desktop 1536px et mobile 390px, création desktop/mobile et état vide dans `.impeccable/review/`. Les services représentés sont des fixtures de test, explicitement signalées comme synthétiques dans les logs. Les mutations navigateur ont été interceptées, jamais envoyées à launchd. Le script de validation navigateur est temporaire ; Playwright provient du runtime Codex, aucune dépendance npm ajoutée au projet.

## Vérification réelle en lecture seule

```text
Live read-only API: 200 services: 0
Live dashboard: 200 HTML bytes: 6039
Listener: 127.0.0.1:9090
```

Aucun LaunchAgent réel créé, démarré, arrêté, modifié ou supprimé. La connexion HTTP et le démarrage du binaire sont vérifiés sur macOS ; les mutations launchctl sont couvertes par des doubles, pas par un test bout en bout sur des processus réels.

## Revue

Revue backend : quatre défauts détectés puis corrigés — conservation de l'état après édition, isolement des plists invalides, refus de conversion silencieuse des politiques KeepAlive avancées, permissions privées. Mutex conservé dans le worker pour que l'annulation d'une requête HTTP ne libère pas prématurément une mutation.

Revue visuelle Impeccable : structure conforme au périmètre de la maquette, adaptations V1 documentées. Verdict final `ship` limité aux corrections évaluées : icônes SVG et maintien du focus. Détecteur exécuté une fois ; contraste hover, petits libellés et ombres corrigés. Signal de padding des dialogues non retenu : les enfants fournissent les marges internes de 20–25px.

## Limites explicites

- Pas de compteur de redémarrages historique fiable sans stockage : affiché `—`.
- Les états sont lus dans le domaine choisi via `launchctl print` ; le format texte macOS reste une limite de compatibilité couverte par fixtures.
- CPU/RAM et multi-host restent hors V1 selon le plan.
- Les politiques launchd avancées non représentables dans le formulaire ne sont pas éditables.
- Stop et l'édition d'un service arrêté le laissent déchargé jusqu'à Start ou la prochaine ouverture de session.
- Les logs doivent utiliser les chemins launch2dashboard attendus ; pas de rotation automatique.

## Renommage et namespace

Nom du crate et du binaire : `launch2dashboard`. Interface : `L2D`. Services gérés : `launch2dashboard.*`, choix confirmé par l’utilisateur. Header de mutation : `X-L2D-Request: 1`. Logs : `~/Library/Logs/launch2dashboard/`. Documentation regroupée dans `doc/`, README d’entrée conservé à la racine.

Après ces changements : 24 tests verts, cargo check/fmt/clippy et build release verts, 13 scénarios navigateur verts sans erreur console. Recherche des anciennes références dans sources et documents : aucune occurrence. Liens Markdown et sidecar de design valides. Marque et namespace confirmés sur le serveur réel, API 200 avec zéro service.

## Sources UI dans src/

Templates déplacés dans `src/templates/`, ressources dans `src/static/`. Askama configuré par `askama.toml` ; routes HTTP inchangées. Après déplacement : 24 tests verts, cargo check/fmt/clippy et build release verts, syntaxe JavaScript valide. README complété avec les prérequis et les commandes de compilation, installation et lancement sur macOS.

## Correctif SSH sans session graphique

Cause confirmée dans le compte exécutant L2D : `launchctl print gui/502` échoue avec le code 125 ; `launchctl print user/502` réussit. Le domaine GUI était imposé et les lectures d’état utilisaient le contexte ambiant.

Régression TDD : attente `user/501/launch2dashboard.demo`, résultat initial `gui/501/launch2dashboard.demo`. Deux régressions supplémentaires d’analyse du texte ont aussi échoué avant correction : accolade dans un argument et terminaison par signal.

Après correction : **32 tests verts**, formatage, Clippy et build release verts. Revue des deux cas de parsing : résolus. La lecture utilise les champs directs (une tabulation) ; `last terminating signal = Terminated: 15` produit Error avec code -15.

Binaire installé dans `/Users/hermes/.cargo/bin/launch2dashboard` sur `mini.local`, SHA-256 `350fc3298717d5f042949b04eb24187338ff191152b20b031005c7e1c5a5a1e1`. Copie de secours : `/Users/hermes/.cargo/bin/.launch2dashboard.before-domain-fix-0aa7dc40f9`. Relancé sous le même compte, sans installation de démarrage automatique. Le journal `~/Library/Logs/launch2dashboard/server.log` confirme :

```text
Managing launchd domain user/502
launch2dashboard is listening at http://127.0.0.1:9090
```

Vérification réelle via l’API sur un service sleep temporaire dédié, avec `LimitLoadToSessionType=Background` :

```text
CREATE: 201 stopped
START: 200 running
RESTART: 200 running
EDIT RUNNING: 200 running
LOGS: 200
SSE: 200
STOP: 200 stopped
DELETE: 204
Temporary test artifacts cleaned
```

Le service temporaire a été déchargé, son plist supprimé et ses logs de test mis à la corbeille. Aucun autre service existant modifié. Les sources corrigées sont dans ce projet ; la mise à jour distante porte sur le binaire installé.
