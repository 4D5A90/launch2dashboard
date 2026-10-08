#!/bin/sh
# Pulls, reinstalls the binary, and restarts the LaunchAgent if there is one.
set -eu
cd "$(dirname "$0")/.."
git pull --ff-only
cargo install --path . --locked --force
sh scripts/install-agent.sh --if-installed
