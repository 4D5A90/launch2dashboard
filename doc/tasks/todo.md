# launch2dashboard V1

Périmètre : plan fourni et maquette, demande d'implémentation utilisateur ; écoute locale confirmée.

- [x] Lire plan et maquette, fixer le périmètre V1 et l'écoute locale.
- [x] Définir les frontières et critères d'acceptation.
- [x] Implémenter domaine, adapter launchd et tests isolés.
- [x] Implémenter API, protections HTTP et flux de logs.
- [x] Implémenter dashboard, formulaire et états accessibles.
- [x] Exécuter tests, compilation, formatage, lint et vérification navigateur.
- [x] Documenter utilisation et limitations vérifiées.

## Revue
24 tests Rust/HTTP verts ; cargo check, fmt, clippy -D warnings et build release verts. 13 scénarios navigateur verts, zéro erreur console. Revue backend : état conservé lors d'édition, rollback, isolation des plists invalides, refus des politiques non représentables et permissions privées corrigés/testés. Revue visuelle : verdict ship sur corrections SVG et focus pendant polling. Aucun LaunchAgent réel modifié. Voir doc/validation.md pour portée et limites.

## Renommage launch2dashboard

- [x] Aligner crate, binaire, header API, logs, interface et documents.
- [x] Regrouper les Markdown dans doc/ et corriger leurs références.
- [x] Repasser tests, compilation, formatage, lint et vérifier le serveur renommé.
- [x] Remplacer le namespace d’exemple par `launch2dashboard.*`, confirmé par l’utilisateur.

## Documentation et sources UI

- [x] Ajouter au README les prérequis macOS, le build release, l’installation Cargo et le lancement.
- [x] Déplacer les templates et assets dans src/ ; configurer askama.toml et include_str.
- [x] Actualiser les chemins documentés ; vérifier 24 tests, check, fmt, clippy, syntaxe JS et build release.

## Correction SSH / Background

- [x] Reproduire le code 125 sur le domaine GUI distant et vérifier le domaine utilisateur.
- [x] Définir les critères dans stories/ssh-launch-domain.md.
- [x] Corriger la sélection du domaine, les plists et les lectures d’état avec tests de régression.
- [x] Valider les gates et un service temporaire distant.
- [x] Livrer le binaire corrigé et documenter la preuve.
