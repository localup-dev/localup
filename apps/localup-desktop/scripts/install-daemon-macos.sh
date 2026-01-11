#!/bin/bash
# Install LocalUp daemon as a macOS LaunchAgent
# This allows tunnels to run even when the app is closed

set -e

PLIST_NAME="com.localup.daemon"
PLIST_PATH="$HOME/Library/LaunchAgents/${PLIST_NAME}.plist"
DAEMON_PATH="${1:-$(which localup-daemon 2>/dev/null || echo "$HOME/.local/bin/localup-daemon")}"
LOG_DIR="$HOME/.localup/logs"

# Check if daemon exists
if [ ! -f "$DAEMON_PATH" ]; then
    echo "Error: localup-daemon not found at $DAEMON_PATH"
    echo "Usage: $0 /path/to/localup-daemon"
    exit 1
fi

# Create log directory
mkdir -p "$LOG_DIR"
mkdir -p "$HOME/.localup"

# Create LaunchAgent plist
cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${PLIST_NAME}</string>

    <key>ProgramArguments</key>
    <array>
        <string>${DAEMON_PATH}</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>${LOG_DIR}/daemon.stdout.log</string>

    <key>StandardErrorPath</key>
    <string>${LOG_DIR}/daemon.stderr.log</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>

    <key>ProcessType</key>
    <string>Background</string>

    <key>LowPriorityIO</key>
    <true/>
</dict>
</plist>
EOF

echo "Created LaunchAgent plist at: $PLIST_PATH"

# Load the LaunchAgent
launchctl unload "$PLIST_PATH" 2>/dev/null || true
launchctl load -w "$PLIST_PATH"

echo "LocalUp daemon installed and started!"
echo ""
echo "To check status: launchctl list | grep localup"
echo "To view logs: tail -f $LOG_DIR/daemon.stderr.log"
echo "To stop: launchctl unload $PLIST_PATH"
echo "To uninstall: rm $PLIST_PATH && launchctl remove $PLIST_NAME"
