#!/usr/bin/env bash
# install-helper.sh
# Installs the Open-Lid privileged helper into /Library/LaunchDaemons.
#
# Bundled into OpenLid.app at:
#   /Applications/OpenLid.app/Contents/Resources/install-helper.sh
#
# End users (Homebrew cask install) run this once after dragging
# OpenLid.app into /Applications:
#
#     /Applications/OpenLid.app/Contents/Resources/install-helper.sh
#
# Requires sudo (writes to /Library/LaunchDaemons and bootstraps the
# system-domain launchd job).
set -euo pipefail

# Resolve the bundle root from this script's location:
#   .app/Contents/Resources/install-helper.sh
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_BUNDLE="$(cd "$SCRIPT_DIR/../.." && pwd)"
HELPER_BIN="$APP_BUNDLE/Contents/MacOS/open-lid-helper"
PLIST_TEMPLATE="$APP_BUNDLE/Contents/Library/LaunchDaemons/io.openlid.helper.plist"

if [ ! -x "$HELPER_BIN" ]; then
    echo "error: helper binary not found at $HELPER_BIN" >&2
    echo "        is OpenLid.app correctly installed in /Applications?" >&2
    exit 1
fi
if [ ! -f "$PLIST_TEMPLATE" ]; then
    echo "error: launchd plist template not found at $PLIST_TEMPLATE" >&2
    exit 1
fi

TMP_PLIST="$(mktemp)"
sed "s|__OPEN_LID_HELPER_PATH__|$HELPER_BIN|" "$PLIST_TEMPLATE" > "$TMP_PLIST"

sudo cp "$TMP_PLIST" /Library/LaunchDaemons/io.openlid.helper.plist
sudo chown root:wheel /Library/LaunchDaemons/io.openlid.helper.plist
sudo chmod 644 /Library/LaunchDaemons/io.openlid.helper.plist
rm "$TMP_PLIST"

# Re-bootstrap if a previous instance is loaded; ignore failure on first install.
sudo launchctl bootout system/io.openlid.helper 2>/dev/null || true
sudo launchctl bootstrap system /Library/LaunchDaemons/io.openlid.helper.plist

echo "Open-Lid helper installed."
echo "Logs: /Library/Logs/open-lid/helper.log"
