# Icône de service

En tant qu’utilisateur, je veux associer une image PNG ou SVG à un service pour le reconnaître d’un coup d’œil.

## Décisions

- Téléversement local uniquement ; pas de catalogue distant (macosicons.com exige une clé, 50 requêtes/mois et l’affichage des crédits).
- 5 Mo maximum. Format détecté par le contenu (signature PNG, ou UTF-8 contenant `<svg`), jamais par le nom ni le `Content-Type`.
- Stockage hors plist : `~/Library/Application Support/launch2dashboard/icons/<id>.{png,svg}`, fichiers 0600, symlinks refusés.
- Port `IconStore` distinct de `ServiceManager` ; adapter fichiers `FsIconStore`.
- L’icône est servie avec sa propre CSP `default-src 'none'; style-src 'unsafe-inline'; sandbox` : un SVG ouvert directement ne peut rien exécuter dans l’origine du dashboard.
- `GET /api/services` et `GET /api/services/{id}` exposent `icon`, une version opaque (ou `null`) ; l’URL `…/icon?v=<version>` est mise en cache, la version change à chaque remplacement.
- À la création, si le service est créé mais l’envoi de l’icône échoue, le service est conservé et l’erreur affichée.

## API

`PUT /api/services/{id}/icon` (corps = image brute) → 204 · `GET` → image · `DELETE` → 204.

## Critères

```gherkin
Scénario: téléverser une icône
  Étant donné un service "demo"
  Quand j'envoie un PNG ou un SVG de 5 Mo maximum
  Alors l'icône s'affiche sur la carte, dans la navigation et dans le détail

Scénario: fichier refusé
  Quand j'envoie plus de 5 Mo, ou un fichier qui n'est ni PNG ni SVG
  Alors je reçois une erreur claire et l'icône précédente est conservée

Scénario: service inconnu
  Quand j'envoie une icône pour un service inexistant
  Alors je reçois 404 et rien n'est écrit

Scénario: retirer l'icône
  Quand je retire l'icône
  Alors l'icône par défaut revient

Scénario: suppression du service
  Quand je supprime le service
  Alors son icône est supprimée
```

## Hors périmètre

Recadrage ou redimensionnement, nettoyage du SVG, icône dans le rendu HTML initial sans JavaScript.
