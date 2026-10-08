# launch2dashboard (L2D)

Dashboard macOS local pour gérer les services `launchd` du namespace `launch2dashboard.*`. Rust, Axum, Askama et JavaScript vanilla, sans build frontend.

## Prérequis macOS

Utiliser une session utilisateur macOS ouverte, sans `sudo` pour compiler ou lancer L2D.

1. Installer les outils de compilation Apple s’ils ne sont pas déjà présents, puis terminer l’installation dans la fenêtre macOS :

   ```sh
   xcode-select --install
   ```

2. Installer Rust stable et Cargo avec [rustup, l’installateur officiel](https://rust-lang.org/tools/install/) :

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   . "$HOME/.cargo/env"
   rustc --version
   cargo --version
   ```

   Si Rust est déjà installé via rustup, le mettre à jour avec `rustup update stable`.

## Compiler

Depuis le dossier du projet contenant `Cargo.toml` :

```sh
cargo build --release --locked
```

Le binaire est généré dans `target/release/launch2dashboard`. La compilation cible l’architecture du Mac utilisé : Apple Silicon (`arm64`) ou Intel (`x86_64`). Pour un autre Mac, compiler sur celui-ci est le plus simple.

HTML, CSS et JavaScript sont embarqués dans le binaire : aucun Node/npm ni dossier `src/templates/` ou `src/static/` à copier pour l’exécution.

## Installer

Depuis le même dossier, installer le programme pour l’utilisateur courant :

```sh
cargo install --path . --locked
```

Cargo compile en mode release et installe `launch2dashboard` dans `~/.cargo/bin/` par défaut, déjà ajouté au `PATH` par rustup. Pour réinstaller après une mise à jour du code, ajouter `--force` à cette commande.

## Lancer

```sh
launch2dashboard
```

Ouvrir [127.0.0.1:9090](http://127.0.0.1:9090). Le serveur écoute uniquement sur cette adresse locale. `Ctrl+C` arrête L2D sans arrêter les services gérés par launchd.

Pour essayer sans installer : `./target/release/launch2dashboard`. Pour développer : `cargo run --locked`. Ne lancer qu’une instance à la fois ; le port `9090` doit être disponible.

L’installation du binaire ne configure pas de démarrage automatique à l’ouverture de session. Rust est nécessaire pour compiler, pas pour exécuter le binaire compilé.

### Mac distant via SSH

L2D fonctionne aussi sans session graphique : il choisit le domaine launchd `user/<uid>` (Background) lorsque `gui/<uid>` est indisponible. Le domaine choisi apparaît au démarrage.

Lancer `launch2dashboard` sur le Mac distant, puis créer un tunnel depuis le Mac qui ouvre le navigateur :

```sh
ssh -N -L 9090:127.0.0.1:9090 utilisateur@mac-distant
```

Ouvrir ensuite [127.0.0.1:9090](http://127.0.0.1:9090) sur le Mac local. Remplacer `utilisateur@mac-distant` par la destination SSH et garder le tunnel ouvert.

## Vérifier et consulter la documentation

```sh
sh scripts/check.sh
```

La documentation est regroupée dans [doc/](doc/README.md) : [utilisation et API](doc/usage.md), [plan V1](doc/launch2dashboard-plan.md), [design](doc/DESIGN.md) et [validation](doc/validation.md).
