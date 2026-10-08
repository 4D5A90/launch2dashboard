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
- L'état Starting n'est pas inféré à partir de `launchctl list` ; états Running/Stopped/Error fondés sur PID et exit code.
- CPU/RAM et multi-host restent hors V1 selon le plan.
- Les politiques launchd avancées non représentables dans le formulaire ne sont pas éditables.
- Stop et l'édition d'un service arrêté le laissent déchargé jusqu'à Start ou la prochaine ouverture de session.
- Les logs doivent utiliser les chemins launch2dashboard attendus ; pas de rotation automatique.

## Renommage et namespace

Nom du crate et du binaire : `launch2dashboard`. Interface : `L2D`. Services gérés : `launch2dashboard.*`, choix confirmé par l’utilisateur. Header de mutation : `X-L2D-Request: 1`. Logs : `~/Library/Logs/launch2dashboard/`. Documentation regroupée dans `doc/`, README d’entrée conservé à la racine.

Après ces changements : 24 tests verts, cargo check/fmt/clippy et build release verts, 13 scénarios navigateur verts sans erreur console. Recherche des anciennes références dans sources et documents : aucune occurrence. Liens Markdown et sidecar de design valides. Marque et namespace confirmés sur le serveur réel, API 200 avec zéro service.

## Sources UI dans src/

Templates déplacés dans `src/templates/`, ressources dans `src/static/`. Askama configuré par `askama.toml` ; routes HTTP inchangées. Après déplacement : 24 tests verts, cargo check/fmt/clippy et build release verts, syntaxe JavaScript valide. README complété avec les prérequis et les commandes de compilation, installation et lancement sur macOS.
