# launch2dashboard

Local macOS dashboard for `launchd` services in the `launch2dashboard.*` namespace. Rust + Axum + Askama; CSS and vanilla JavaScript are embedded in the executable. No frontend build, database or mandatory configuration file.

## Run

Requires macOS and Rust stable to build. Run as your normal user, not with `sudo`. Both a graphical session and an SSH session without a graphical login are supported.

```sh
cargo run --locked
```

Open <http://127.0.0.1:9090>. launch2dashboard binds **only** to `127.0.0.1:9090`. It does not install itself as a login item or create example services. Close it with Ctrl-C; your services remain managed by launchd.

At startup, L2D selects `gui/<uid>` when available, otherwise `user/<uid>`. Background services use `LimitLoadToSessionType=Background`; the same selected domain is used for actions and status reads. L2D does not switch domains after a failed action. Apple documents Aqua as the default session type when that plist key is omitted, so selecting the user domain alone is insufficient. See [Apple TN2083](https://developer.apple.com/library/archive/technotes/tn2083/_index.html).

```sh
cargo build --release --locked
./target/release/launch2dashboard
```

The release binary includes the HTML, CSS and JavaScript and can run from another directory.

## Services

Managed plists live in `~/Library/LaunchAgents/launch2dashboard.<id>.plist`. Create services with an absolute executable path, one argument per line, an optional absolute working directory, and one `KEY=value` environment entry per line. Arguments are passed directly to launchd, without shell expansion. Use the actual path instead of `~`, `$HOME` or shell redirection.

- **Start** loads an unloaded service and requests a launch.
- **Stop** unloads the job, including jobs configured to restart. It does not change its next-login autostart setting.
- **Restart** requests a fresh process, loading the job if necessary.
- **Restart on failure** uses `KeepAlive.SuccessfulExit=false`. macOS requires an initial launch for this policy, so automatic start must also be enabled.
- **Delete** unloads the job and removes its plist after confirmation. Existing logs are retained.

Saving an edit preserves whether the service is running. A stopped service remains unloaded until Start or the next login. If applying an edit fails, launch2dashboard attempts to restore the previous plist and running state and reports whether rollback succeeded. Existing advanced `KeepAlive` policies and separate `Program` keys cannot be edited through the simplified form; they are rejected instead of being silently replaced. Unknown plist keys are otherwise retained.

Logs are tailed from `~/Library/Logs/launch2dashboard/<id>.log` and `<id>.err.log`. stdout and stderr remain separate because their files do not provide a shared timestamp order. The browser receives bounded snapshots through SSE; log rotation and truncation are picked up on the next snapshot. launch2dashboard does not rotate logs itself.

The dashboard polls states every three seconds. PID and process uptime are shown when available. An unavailable restart count is shown as `—`; launch2dashboard does not maintain a fabricated counter. CPU/RAM monitoring and multiple hosts are outside V1.

Status comes from `launchctl print <domain>/<label>` in the selected domain. L2D reads only direct state, PID and exit-code fields; an unreadable response is an error rather than a fabricated status. A PID is considered running, including the short xpcproxy launch phase. The output format is not a stable macOS API, so its parsing is covered by fixtures. Invalid plists appear as errors without hiding healthy services. Files with unsupported IDs or structures must be repaired outside the form. Logs from externally authored plists are readable only when they use launch2dashboard's expected log paths.

## Local HTTP API

Read routes: `GET /api/services`, `GET /api/services/{id}`, `/status`, `/logs`, `/logs/stream`.

Mutations require `X-L2D-Request: 1`: `POST /api/services`, `PUT /api/services/{id}`, `DELETE /api/services/{id}`, and `POST /api/services/{id}/start`, `/stop`, `/restart`. Create/update accept JSON configuration:

```json
{
  "id": "my-service",
  "executable": "/absolute/path/to/program",
  "arguments": ["--port", "8099"],
  "working_directory": null,
  "environment": {},
  "autostart": true,
  "restart_on_failure": false
}
```

Only `localhost:9090` and `127.0.0.1:9090` Host headers and local origins are accepted. These browser protections are not authentication against another process or user on the same machine. Do not expose launch2dashboard through a network proxy.

## Verify

```sh
sh scripts/check.sh
```

Tests use temporary directories and a fake command executor, plus HTTP service doubles. They do not create, start, stop or delete real LaunchAgents. Browser checks may use clearly synthetic service data to exercise populated layouts without touching real services.

The original scope and design reference are `launch2dashboard-plan.md` and `design/dashboard-mockup.png`; implementation criteria are in `stories/v1.md`.
