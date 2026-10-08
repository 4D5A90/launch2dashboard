# launch2dashboard (L2D)

Local macOS dashboard for the `launchd` services in the `launch2dashboard.*` namespace. A single Rust binary, with the UI embedded.

## Install

Requires macOS and [Rust stable](https://rust-lang.org/tools/install/). Run as your normal user, not with `sudo`.

```sh
cargo install --path . --locked
```

## Run

```sh
launch2dashboard
```

Open <http://127.0.0.1:9090>. The server listens on loopback only. Ctrl-C stops L2D; the services keep running under launchd.

### Remote Mac over SSH

Without a graphical session, L2D uses the `user/<uid>` launchd domain. Start it on the remote Mac, then tunnel from your machine:

```sh
ssh -N -L 9090:127.0.0.1:9090 user@remote-mac
```

## Develop

```sh
cargo run --locked
sh scripts/check.sh   # fmt, check, test, clippy
```

More in [doc/](doc/README.md): [usage and API](doc/usage.md), [design](doc/DESIGN.md), [validation](doc/validation.md).
