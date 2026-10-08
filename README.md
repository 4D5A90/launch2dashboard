# launch2dashboard (L2D)

Local macOS dashboard for the `launchd` services in the `launch2dashboard.*` namespace. A single Rust binary, with the UI embedded.

Requires macOS and [Rust stable](https://rust-lang.org/tools/install/). Run as your normal user, not with `sudo`.

## Quick start

```sh
cargo run --locked
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
cargo install --path . --locked
launch2dashboard
```

## Checks

```sh
sh scripts/check.sh   # fmt, check, test, clippy
```

More in [doc/](doc/README.md): [usage and API](doc/usage.md), [design](doc/DESIGN.md), [validation](doc/validation.md).
