# launch2dashboard — plan V1

## Objectif

Créer un **daemon Rust standalone pour macOS** qui pilote des services `launchd` via une interface web propre.

launch2dashboard ne remplace pas `launchd` : il sert de couche de gestion et d'interface au-dessus de `launchctl`.

```text
Browser
   │
   ▼
launch2dashboard :9090
   │
   ├── UI HTML/CSS/JS
   ├── API HTTP
   └── launchctl
        │
        ▼
      launchd
        │
        ├── llama-swap
        ├── hermes
        └── autres services
```

`launchd` reste la source de vérité.

---

## Structure du projet

```text
launch2dashboard/
├── Cargo.toml
├── askama.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── domain.rs
│   ├── launchd.rs
│   ├── web.rs
│   ├── templates/
│   │   └── dashboard.html
│   └── static/
│       ├── style.css
│       └── app.js
├── tests/
└── doc/
```

Principes :

- pas de DB ;
- pas de YAML ;
- pas de fichier de configuration launch2dashboard obligatoire ;
- pas de framework frontend ;
- pas de Node / npm ;
- pas de build frontend séparé ;
- les services sont représentés directement par leurs fichiers `launchd`.

```text
~/Library/LaunchAgents/
└── launch2dashboard.*.plist
```

---

## Backend Rust

### `main.rs`

Responsabilités :

- démarrage du serveur Axum ;
- déclaration des routes ;
- wiring global de l'application.

Routes prévues :

```text
GET    /
GET    /services/:id

POST   /api/services
DELETE /api/services/:id

POST   /api/services/:id/start
POST   /api/services/:id/stop
POST   /api/services/:id/restart

GET    /api/services/:id/status
GET    /api/services/:id/logs
GET    /api/services/:id/logs/stream
```

Serveur :

```text
127.0.0.1:9090
```

---

## Couche `launchd`

### `launchd.rs`

Toute la logique spécifique à macOS vit ici.

API interne envisagée :

```rust
list_services()
get_service(id)

create_service(service)
delete_service(id)

start_service(id)
stop_service(id)
restart_service(id)

get_status(id)
get_logs(id)
```

launch2dashboard s'appuie sur :

```text
launchctl
~/Library/LaunchAgents
```

et génère les fichiers `.plist`.

---

## Création d'un service

Le bouton `+ Add Service` ouvre une modal avec :

```text
Create Service

Name
llama-swap

Executable
/Users/hermes/homebrew/bin/llama-swap

Arguments
--config
/Users/hermes/tools/llama-swap-config.yaml
--listen
0.0.0.0:8099

Working directory
/Users/hermes/tools

Environment
KEY=value

[x] Start automatically
[x] Restart on failure

[Cancel]             [Create]
```

launch2dashboard génère alors :

```text
~/Library/LaunchAgents/launch2dashboard.llama-swap.plist
```

Exemple de contenu :

```xml
<key>Label</key>
<string>launch2dashboard.llama-swap</string>

<key>ProgramArguments</key>
...

<key>WorkingDirectory</key>
...

<key>RunAtLoad</key>
<true/>

<key>KeepAlive</key>
<true/>

<key>StandardOutPath</key>
...

<key>StandardErrorPath</key>
...
```

Puis le service est chargé via `launchctl`.

---

## Dashboard

La maquette de référence est le dashboard sombre déjà généré.

Organisation :

```text
┌────────────────────────────────────────────────────────────────────┐
│ L2D Services                                         + Add Service │
│ Mac mini · localhost                                               │
├──────────────┬───────────────────────────┬─────────────────────────┤
│              │                           │                         │
│ SERVICES     │ Services                  │ llama-swap              │
│              │                           │ ● Running               │
│ ● llama-swap │ ● llama-swap             │                         │
│ ● Hermes     │   PID 38291               │ CPU       14%           │
│ ○ Qwen       │   uptime 4h 32m           │ RAM       3.2 GB        │
│              │                           │ PID       38291         │
│              │ [Stop] [Restart] [Open]   │ Uptime    4h 32m       │
│              │                           │                         │
│              │ ● Hermes                  │ ─────────────────────   │
│              │   PID 38412               │ Logs                    │
│              │                           │                         │
│              │ [Stop] [Restart]          │ 19:32 loading config    │
│              │                           │ 19:32 server started    │
│              │ ○ Qwen                    │ 19:33 model loaded      │
│              │                           │ ...                     │
│              │ [Start]                   │                         │
└──────────────┴───────────────────────────┴─────────────────────────┘
```

Principes UI :

- sidebar services ;
- liste centrale ;
- service sélectionné à droite ;
- statut ;
- boutons Start / Stop / Restart ;
- métriques ;
- logs ;
- bouton `+ Add Service`.

---

## Templates Askama

### `dashboard.html`

Le template génère les cartes des services.

```html
{% for service in services %}
<article class="service-card"
         data-service="{{ service.id }}">

    <div class="service-status {{ service.status_class }}"></div>

    <div>
        <strong>{{ service.name }}</strong>
        <small>{{ service.command }}</small>
    </div>

    <div class="actions">
        {% if service.running %}
            <button data-action="stop">Stop</button>
            <button data-action="restart">Restart</button>
        {% else %}
            <button data-action="start">Start</button>
        {% endif %}
    </div>
</article>
{% endfor %}
```

Les templates doivent rester simples : la logique métier reste côté Rust.

---

## JavaScript

`app.js` reste volontairement petit.

Responsabilités :

- appels `fetch()` pour Start / Stop / Restart ;
- mise à jour des statuts sans rechargement complet ;
- ouverture / fermeture de la modal de création ;
- logs live ;
- éventuel polling léger.

Exemple :

```js
async function serviceAction(id, action) {
    const response = await fetch(
        `/api/services/${id}/${action}`,
        { method: "POST" }
    );

    if (!response.ok)
        throw new Error(await response.text());

    await refreshService(id);
}
```

### Logs live

```js
const logs = new EventSource(
    `/api/services/${id}/logs/stream`
);

logs.onmessage = ({ data }) => {
    terminal.textContent += `${data}\n`;
};
```

### Statuts

V1 simple :

```js
setInterval(refreshStatuses, 3000);
```

SSE global pourra être ajouté plus tard si nécessaire.

---

## Page service

Route :

```text
/services/llama-swap
```

Contenu envisagé :

```text
llama-swap                                   ● Running

/Users/hermes/homebrew/bin/llama-swap

PID                 38291
Uptime              4h 32m
Restart count       1

[Stop] [Restart] [Edit] [Delete]


Configuration
────────────────────────────────────────────

Executable
/Users/hermes/homebrew/bin/llama-swap

Arguments
--config ...
--listen ...

Working directory
/Users/hermes/tools


Logs
────────────────────────────────────────────

[stdout + stderr]

19:31:44 loading config
19:31:45 llama.cpp detected
19:31:48 server ready
...
```

---

## Logs

Chaque service géré par launch2dashboard reçoit ses propres fichiers :

```text
~/Library/Logs/launch2dashboard/
├── llama-swap.log
├── llama-swap.err.log
├── hermes.log
└── hermes.err.log
```

Le `.plist` contient :

```xml
<key>StandardOutPath</key>
<string>/Users/hermes/Library/Logs/launch2dashboard/llama-swap.log</string>

<key>StandardErrorPath</key>
<string>/Users/hermes/Library/Logs/launch2dashboard/llama-swap.err.log</string>
```

launch2dashboard lit ensuite simplement ces fichiers.

---

## Statuts

V1 volontairement limitée :

```text
● Running
○ Stopped
● Starting
● Error
```

Informations secondaires possibles :

```text
PID
Uptime
```

Pas besoin de reproduire tout le modèle d'état de `systemd`.

---

## Métriques

### V1

```text
PID
Uptime
Restart count
```

### V1.1

Ajout éventuel :

```text
CPU
RAM
```

avec `sysinfo` uniquement si nécessaire.

---

## Dépendances Rust initiales

```toml
[dependencies]
axum = "..."
tokio = { version = "...", features = [
    "rt-multi-thread",
    "macros",
    "process",
    "fs"
] }

askama = "..."

serde = { version = "...", features = ["derive"] }
serde_json = "..."
```

Objectif : rester au minimum utile, sans ajouter de crate tant qu'un vrai besoin n'apparaît pas.

---

## Roadmap

1. Lister les services `launch2dashboard.*` depuis `launchd`.
2. Start / Stop / Restart avec `launchctl`.
3. Afficher le dashboard basé sur la maquette.
4. Créer un service et générer son `.plist`.
5. Modifier / supprimer un service.
6. Afficher les logs.
7. Logs live via SSE.
8. Polish UI.
9. CPU / RAM éventuellement.
10. Multi-host seulement plus tard.

---

## Résumé de la V1

```text
launchd
   +
launchctl
   +
Rust
   +
Axum
   +
Askama
   +
petit JS vanilla
   +
CSS
```

Le résultat doit rester un **Cockpit minimaliste spécialisé pour `launchd`**, avec un binaire autonome et une UI moderne.
