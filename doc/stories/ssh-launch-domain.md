# Lancement depuis SSH sans session graphique

En tant qu’utilisateur d’un Mac distant, je veux créer et piloter mes services L2D depuis une session SSH, même sans session Aqua ouverte.

## Cause observée

Sur le Mac distant, `launchctl print gui/501` renvoie le code 125 alors que `launchctl print user/501` réussit et décrit une session Background. L’adapter imposait le domaine GUI et utilisait une lecture des statuts dans le contexte implicite de son processus.

## Critères

- Au démarrage, choisir GUI lorsqu’il est disponible, sinon le domaine utilisateur Background ; échouer clairement si aucun domaine n’est accessible.
- Ne pas changer de domaine à la suite d’un échec de mutation.
- Cibler ce domaine pour bootstrap, bootout, kickstart, présence et état.
- Utiliser `LimitLoadToSessionType=Background` dans le domaine utilisateur, y compris pour un plist existant créé sans cette clé.
- Distinguer service absent, erreur d’accès et sortie illisible ; ne pas annoncer Stopped sur un échec de lecture.
- Conserver le namespace `launch2dashboard.*`, l’écoute loopback et les garanties de rollback.

## Preuve attendue

Test de régression rouge puis vert, suite Rust/HTTP et Clippy verts, puis création/démarrage/arrêt/suppression d’un service temporaire dédié sur le Mac distant. Ne pas modifier les autres services.
