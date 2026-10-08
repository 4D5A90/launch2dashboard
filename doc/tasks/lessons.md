# Leçons

- 2026-10-08 : utilisateur confirme `127.0.0.1` au lieu de `0.0.0.0`. Une interface locale capable de lancer des exécutables écoute sur loopback par défaut ; une exposition réseau exige un besoin et une protection explicites.

- 2026-10-08 : correction utilisateur : le produit se nomme `launch2dashboard`, abrégé `L2D` dans les espaces courts. Aligner binaire, protocole, chemins et interface ; regrouper la documentation dans `doc/` en conservant seulement un README d’entrée à la racine.

- 2026-10-08 : ne pas reprendre un namespace organisationnel provenant d’un exemple comme identité du produit. L’utilisateur confirme que seuls les services `launch2dashboard.*` doivent être gérés ; conserver ce filtre et ne pas élargir aux autres LaunchAgents.

- 2026-10-08 : préférence utilisateur : regrouper les sources de l’interface dans `src/templates/` et `src/static/`, avec Askama configuré explicitement. Garder les routes HTTP `/static/` indépendantes de l’arborescence disque.

- 2026-10-08 : un daemon macOS peut être lancé via SSH sans session Aqua. Ne jamais déduire la disponibilité du domaine GUI depuis le seul UID ; sélectionner explicitement un domaine accessible et l’utiliser pour toutes les commandes, avec le type de session plist correspondant.
