# launch2dashboard

<!-- impeccable:product-schema 1 -->

## Platform
web

## Stack
Rust, Axum, Askama, JavaScript vanilla et CSS. Un binaire autonome, aucun build frontend, npm, YAML ou base de données.

## Users
Utilisateur macOS gérant ses LaunchAgents locaux depuis un navigateur.

## Product Purpose
Lister, configurer, démarrer, arrêter et diagnostiquer les services launchd du namespace `launch2dashboard.*`.

## Operating Context
`launchd` reste la source de vérité. Plists dans `~/Library/LaunchAgents`, logs dans `~/Library/Logs/launch2dashboard`.

## Capabilities and Constraints
V1 : CRUD, actions, statuts, PID, uptime et nombre de relances quand disponibles, logs et SSE. Écoute exclusivement sur `127.0.0.1:9090`, décision utilisateur du 8 octobre 2026. CPU/RAM et multi-host exclus de V1.

## Brand Commitments
Reprendre le dashboard sombre violet de `design/dashboard-mockup.png`, adapté aux capacités réelles de V1. Le plan `launch2dashboard-plan.md` fait autorité pour le périmètre.

## Evidence on Hand
Plan et maquette fournis. Aucun service ni chiffre de démonstration ne sera présenté comme réel.
