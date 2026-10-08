---
name: "launch2dashboard"
description: "Interface sombre violette pour le contrôle des services locaux."
colors:
  bg: "#0e1119"
  surface: "#161b26"
  surface-raised: "#1d2330"
  line: "#292f3d"
  text: "#f3f3fa"
  muted: "#a7afc3"
  violet: "#9c8aff"
  primary: "#6450f5"
  primary-hover: "#654bf5"
  green: "#35d68a"
  red: "#ff7c86"
  amber: "#e9bb60"
  white: "#fff"
  button-hover: "#2b3042"
  running-bg: "#15362e"
  running-text: "#70e9ae"
  stopped-bg: "#262b38"
  stopped-text: "#c4cbdd"
  error-bg: "#3c222d"
  error-text: "#ff9ca5"
  starting-bg: "#3a3020"
  starting-text: "#f0cf8f"
  danger-bg: "#502633"
  danger-text: "#ffb6bc"
  danger-hover: "#65303b"
  field-bg: "#10151f"
  nav-active-bg: "#29234f"
  nav-active-text: "#f0eaff"
typography:
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "14px"
  body-copy:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "14px"
    lineHeight: 1.55
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "12px"
  section-title:
    fontSize: "14px"
    fontWeight: 600
  service-title:
    fontSize: "16px"
    fontWeight: 600
    letterSpacing: "-.015em"
  detail-title:
    fontSize: "18px"
    letterSpacing: "-.015em"
  metric:
    fontSize: "19px"
    fontWeight: 500
  page-title:
    fontSize: "30px"
    lineHeight: 1.2
    letterSpacing: "-.025em"
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "12px"
rounded:
  compact: "7px"
  control: "8px"
  inset: "10px"
  panel: "12px"
  dialog: "16px"
spacing:
  tight: "6px"
  small: "8px"
  control: "10px"
  compact: "12px"
  icon-gap: "14px"
  panel-gap: "16px"
  section: "20px"
  roomy: "24px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.white}"
    rounded: "{rounded.control}"
    padding: "10px 14px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
  button-secondary:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "10px 14px"
  button-danger:
    backgroundColor: "{colors.danger-bg}"
    textColor: "{colors.danger-text}"
    rounded: "{rounded.control}"
    padding: "10px 14px"
  input:
    backgroundColor: "{colors.field-bg}"
    textColor: "{colors.text}"
    rounded: "{rounded.compact}"
    padding: "10px 12px"
  nav-active:
    backgroundColor: "{colors.nav-active-bg}"
    textColor: "{colors.nav-active-text}"
    rounded: "{rounded.control}"
    padding: "13px"
  badge-running:
    backgroundColor: "{colors.running-bg}"
    textColor: "{colors.running-text}"
    rounded: "{rounded.compact}"
    padding: "6px 9px"
    typography: "{typography.label}"
  service-card:
    rounded: "{rounded.panel}"
    padding: "20px"
---

# Design System: launch2dashboard

## Overview

**Creative North Star: "Console locale sombre et violette"**

Le dashboard reprend la maquette launch2dashboard fournie : surfaces bleu nuit, accents violets et informations compactes. Les contours et les variations de fond séparent les zones sans ombre portée.

Les actions, la sélection et les états restent lisibles dans une interface dense. Les icônes de services sont des tracés SVG ; les valeurs techniques utilisent une police monospace.

**Key Characteristics:**
- Surfaces sombres avec contours fins.
- Violet pour action et sélection.
- États nommés et accompagnés d’un point coloré.

Source : `../src/static/style.css`, `../src/templates/dashboard.html`, `../src/static/app.js`. Autorité visuelle : maquette utilisateur, rappelée dans `design/dashboard.md`.

## Colors

### Primary
Le violet clair signale la sélection, le focus et l’onglet actif. Le violet saturé remplit les actions primaires ; son survol est défini par `primary-hover`.

### Secondary
Vert, gris, ambre et rouge identifient respectivement running, stopped, starting et error. Les badges associent un fond teinté, un libellé clair et un point.

### Neutral
Le fond général est le plus sombre ; les contrôles et les blocs internes sont légèrement relevés. `text` porte l’information principale, `muted` les aides, chemins et métadonnées ; `line` sépare les surfaces.

**The Explicit State Rule.** Accompagner chaque état d’un libellé ; la couleur seule ne porte pas le diagnostic.

## Typography

Le corps et les contrôles utilisent la pile système déclarée dans les tokens. Les données techniques utilisent la pile monospace. Les compteurs, PID et durées emploient des chiffres tabulaires.

La hiérarchie observée est portée par `page-title`, `detail-title`, `service-title`, `section-title`, `body` et `label`. Le titre de page passe à 28px et celui des services à 15px sur mobile. Le titre des dialogues est à 22px. Les titres héritent actuellement de la pile système : ce choix de fonte de titre n’est pas promu en règle de design.

## Layout

Sur grand écran, navigation fixe de 240px et contenu décalé de la même largeur. Le contenu possède un padding de 30px 26px 16px. La liste et le détail forment deux colonnes `minmax(320px,1.25fr) minmax(340px,1fr)` séparées de 16px. Les cartes ont 20px de padding et 12px entre elles.

À 1550px et au-delà, les marges, cartes et intercolonnes respirent davantage. À 1200px et moins, la navigation fait 210px, les cartes 18px de padding et perdent leur icône. À 950px et moins, la navigation fait 180px, les panneaux s’empilent et les icônes reviennent. À 600px et moins, la navigation devient une barre horizontale, le contenu n’a plus de décalage latéral et les cartes ont 16px de padding. Les filtres et actions peuvent revenir à la ligne.

## Elevation & Depth

Le dashboard utilise des dégradés sombres discrets dans la navigation et les cartes. Les panneaux, dialogues et notifications n’ont pas d’ombre portée. La sélection utilise `inset 0 0 0 1px var(--violet)` en complément de sa bordure. Le dialogue assombrit l’arrière-plan avec `#070911bb`.

**The Bordered Surface Rule.** Séparer les panneaux par leur fond et leur contour ; réserver le double contour violet au service sélectionné.

## Shapes

Les contrôles ont des coins courts, les panneaux des coins plus amples, et les dialogues le plus grand rayon. Les badges restent de petits rectangles arrondis ; seuls les points d’état sont circulaires. Les contours ont généralement une épaisseur de 1px.

## Components

### Buttons
Les boutons secondaires sont sombres et bordés ; les actions primaires ont un fond violet et un texte blanc. Le bouton destructif du dialogue utilise une teinte rouge sombre. Le survol change le fond sans animation. Les boutons désactivés passent à une opacité de 0.5 et affichent un curseur d’attente. Le focus visible est un contour violet de 2px décalé de 3px.

### Chips
Les badges d’état associent point et texte. Les filtres sont des boutons accompagnés de compteurs ; la sélection ajoute un fond et une bordure violets. Les compteurs sont des pastilles internes arrondies.

### Cards / Containers
Les cartes de services contiennent le nom, le badge, le chemin tronqué, les valeurs techniques et les actions. La sélection renforce uniquement le contour. Les détails utilisent des sous-blocs de faits et une rangée de trois valeurs ; les chaînes longues y reviennent à la ligne.

### Inputs / Fields
Les champs ont un fond sombre, une bordure et un caret violet. La recherche place une icône SVG dans son conteneur. Les formulaires gardent des labels visibles et des aides sous les champs. Les erreurs sont affichées dans un bloc rouge bordé. Les zones de texte techniques utilisent la pile monospace et sont redimensionnables verticalement.

### Navigation
La destination active a un fond violet sombre ; le service sélectionné a une teinte plus discrète. Les onglets du détail utilisent un texte violet et un trait inférieur de 2px pour la sélection. Les liens gardent un soulignement au survol et un focus visible au clavier. Sur mobile, la liste de navigation latérale disparaît ; les cartes conservent l’accès aux services.

### Logs
Les canaux sont nommés séparément. Chaque sortie utilise un bloc sombre bordé, défilant, avec retour à la ligne des longues valeurs. Sa hauteur va de 64px à 230px. La mise à jour conserve la position de lecture lorsque l’utilisateur ne suit plus le bas du flux.

## Do's and Don'ts

### Do:
- Do conserver les libellés des états avec leurs points colorés.
- Do conserver le focus visible et les valeurs techniques qui peuvent revenir à la ligne.
- Do empiler les panneaux lorsque la largeur ne permet plus leur lecture côte à côte.

### Don't:
- Don’t employer les couleurs d’état comme décoration sans signification.
- Don’t masquer une erreur ou un état vide derrière des valeurs de démonstration.
