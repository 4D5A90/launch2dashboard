# launch2dashboard (L2D)

Local macOS dashboard for the `launchd` services in the `launch2dashboard.*` namespace. A single Rust binary, with the UI embedded.

Requires macOS, [Rust stable](https://rust-lang.org/tools/install/) and [just](https://just.systems), the task runner. Run as your normal user, not with `sudo`.

```sh
cargo install just
just   # lists the tasks
```

Each task is a thin wrapper around `cargo` or a script in [scripts/](scripts/), which you can also run directly.

## Quick start

```sh
just run   # cargo run --locked
```

Or build once and run the binary:

```sh
cargo build --release --locked
./target/release/launch2dashboard
```

Open <http://127.0.0.1:9090>. The server listens on loopback only. Ctrl-C stops L2D; the services keep running under launchd.

### Access through an SSH tunnel

Without a graphical session, L2D uses the `user/<uid>` launchd domain. Start it on the remote host, then tunnel from your machine:

```sh
ssh -N -L 9090:127.0.0.1:9090 user@remote
```

## Install

```sh
just install     # cargo install, then run it as a LaunchAgent (com.<user>.launch2dashboard)
just update      # pull, reinstall, restart the agent if installed
just uninstall   # stop the agent and remove its plist
```

For the binary alone, without the agent: `cargo install --path . --locked`.

## Develop

With [bacon](https://dystroy.org/bacon/) (`cargo install bacon`), the server rebuilds and restarts on every change:

```sh
just dev   # bacon run
```

Open pages reload by themselves once the server is back (debug builds only). In bacon, `c` runs clippy, `t` the tests.

Before committing:

```sh
just check   # fmt, check, test, clippy
```

More in [doc/](doc/README.md): [usage and API](doc/usage.md), [design](doc/DESIGN.md), [validation](doc/validation.md).
