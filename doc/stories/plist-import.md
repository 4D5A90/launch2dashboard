# Import d’un plist

En tant qu’utilisateur, je veux partir d’un fichier `.plist` existant pour créer un service sans ressaisir sa commande, puis lui choisir un nom et une icône.

## Décisions

- L’import **pré-remplit** le formulaire d’ajout ; la création passe par le flux existant (`POST /api/services`). Aucun nouveau chemin d’écriture.
- Le plist d’origine n’est **ni modifié, ni déchargé, ni supprimé**. L’utilisateur gère lui-même un éventuel doublon.
- Le serveur lit le plist (XML ou binaire, via la crate `plist`) : le navigateur ne sait pas lire le binaire. Rien n’est écrit sur disque.
- Lecture exposée par le port `ServiceManager::parse_definition(bytes)`, car le format appartient à l’adapter launchd et `web.rs` ne dépend que des ports (ADR 001). Implémentation pure via `definition::to_config`.
- Nom suggéré dérivé du `Label` : caractères hors `[A-Za-z0-9_-]` remplacés par `-`, tronqué à 100 (ex. `com.foo.bar` → `com-foo-bar`). Toujours modifiable avant de créer.
- Les clés que le formulaire ne représente pas (`StartInterval`, `WatchPaths`…) sont **perdues** et listées dans la réponse (`ignored`), puis affichées à l’utilisateur avant la création. Sont exclues de cette liste les clés qu’L2D réécrit lui-même (`Label`, `Program`, `StandardOutPath`, `StandardErrorPath`, `LimitLoadToSessionType`).
- Corps limité à la limite globale de 64 Ko.

## API

`POST /api/services/import` (corps = plist brut, header `X-L2D-Request: 1`) → 200 :

```json
{ "config": { "id": "com-foo-bar", "executable": "…", "arguments": [], "working_directory": null,
              "environment": {}, "autostart": false, "restart": "never" },
  "ignored": ["StartInterval"] }
```

Plist illisible, ou sans `ProgramArguments` exploitable → 400 avec message clair.

## Critères

```gherkin
Scénario: pré-remplir depuis un plist
  Étant donné la fenêtre d'ajout ouverte
  Quand je choisis un fichier .plist valide (XML ou binaire)
  Alors exécutable, arguments, dossier, environnement, démarrage auto et redémarrage sont remplis
  Et le nom proposé est dérivé du Label
  Et rien n'est créé tant que je n'ai pas validé

Scénario: personnaliser puis créer
  Étant donné un formulaire pré-rempli par import
  Quand je modifie le nom, ajoute une icône et valide
  Alors le service launch2dashboard.<nom> est créé avec cette icône

Scénario: clés non prises en charge
  Quand le plist contient StartInterval ou WatchPaths
  Alors le formulaire affiche la liste des clés qui ne seront pas reprises

Scénario: fichier invalide
  Quand je choisis un fichier qui n'est pas un plist, ou sans ProgramArguments
  Alors un message d'erreur s'affiche et le formulaire est inchangé

Scénario: original intact
  Quand j'importe ~/Library/LaunchAgents/com.foo.bar.plist puis crée le service
  Alors com.foo.bar.plist n'est ni modifié ni déchargé
```

## Hors périmètre

Gestion en place de plists tiers (labels arbitraires), migration ou déchargement de l’original, conservation des clés non gérées, import multiple, glisser-déposer.
