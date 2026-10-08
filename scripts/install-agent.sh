#!/bin/sh
# Installs launch2dashboard itself as a LaunchAgent of the current user.
#   sh scripts/install-agent.sh              install, or reinstall after an update
#   sh scripts/install-agent.sh --if-installed  reinstall only if already installed (update.sh)
#   sh scripts/install-agent.sh --print      show the plist without installing
#   sh scripts/install-agent.sh --uninstall  stop the agent and remove its plist
# The label stays outside launch2dashboard.* so L2D never lists itself.
set -eu

uid=$(id -u)
label="com.$(id -un).launch2dashboard"
plist="$HOME/Library/LaunchAgents/$label.plist"
log="$HOME/Library/Logs/launch2dashboard.server.log"
bin=${L2D_BIN:-$(command -v launch2dashboard || echo "$HOME/.cargo/bin/launch2dashboard")}

# Same rule as L2D: the GUI domain when available, else the Background user domain (SSH).
if launchctl print "gui/$uid" >/dev/null 2>&1; then
    domain="gui/$uid"
    session=Aqua
else
    domain="user/$uid"
    session=Background
fi

render() {
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>ProgramArguments</key>
  <array><string>$bin</string></array>
  <key>EnvironmentVariables</key>
  <dict><key>HOME</key><string>$HOME</string></dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key>
  <dict><key>SuccessfulExit</key><false/></dict>
  <key>LimitLoadToSessionType</key><string>$session</string>
  <key>StandardOutPath</key><string>$log</string>
  <key>StandardErrorPath</key><string>$log</string>
</dict>
</plist>
EOF
}

unload() {
    if launchctl print "$domain/$label" >/dev/null 2>&1; then
        launchctl bootout "$domain/$label"
        echo "Stopped $domain/$label"
    fi
}

case "${1:-}" in
    --print)
        render
        exit 0
        ;;
    --uninstall)
        unload
        if [ -f "$plist" ]; then
            rm "$plist"
            echo "Removed $plist"
        fi
        exit 0
        ;;
    --if-installed)
        if [ ! -f "$plist" ]; then
            echo "No LaunchAgent installed; restart launch2dashboard yourself."
            exit 0
        fi
        ;;
    "") ;;
    *)
        echo "usage: $0 [--print | --if-installed | --uninstall]" >&2
        exit 2
        ;;
esac

if [ ! -x "$bin" ]; then
    echo "launch2dashboard not found at $bin; run: cargo install --path . --locked" >&2
    exit 1
fi

unload
# A just-stopped agent may hold the port briefly; any other instance on it
# would make launchd restart this one in a loop.
for _ in 1 2 3 4 5; do
    lsof -nP -iTCP:9090 -sTCP:LISTEN >/dev/null 2>&1 || break
    sleep 1
done
if lsof -nP -iTCP:9090 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "Port 9090 is already in use; stop that process first:" >&2
    lsof -nP -iTCP:9090 -sTCP:LISTEN >&2
    exit 1
fi

mkdir -p "$(dirname "$plist")" "$(dirname "$log")"
render >"$plist"
plutil -lint -s "$plist"
launchctl bootstrap "$domain" "$plist"

for _ in 1 2 3 4 5 6 7 8 9 10; do
    if curl -sf -o /dev/null http://127.0.0.1:9090/api/services; then
        echo "Running as $domain/$label: http://127.0.0.1:9090 (log: $log)"
        exit 0
    fi
    sleep 1
done
echo "Agent loaded but not answering yet; check $log" >&2
exit 1
