# Entry point for the project's tasks; the logic stays in scripts/.

[private]
default:
    @just --list --unsorted

# Run the dashboard at http://127.0.0.1:9090
run:
    cargo run --locked

# Rebuild and restart on every change (needs bacon)
dev:
    bacon run

# fmt, check, test, clippy — run before committing
check:
    sh scripts/check.sh

# Install the binary and run it as a LaunchAgent
install:
    cargo install --path . --locked
    sh scripts/install-agent.sh

# Stop the LaunchAgent and remove its plist
uninstall:
    sh scripts/install-agent.sh --uninstall

# Pull, reinstall, restart the agent if installed
update:
    sh scripts/update.sh
