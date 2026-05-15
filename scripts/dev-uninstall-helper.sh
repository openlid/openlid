#!/usr/bin/env bash
# scripts/dev-uninstall-helper.sh
set -euo pipefail
sudo launchctl bootout system/io.openlid.helper 2>/dev/null || true
sudo rm -f /Library/LaunchDaemons/io.openlid.helper.plist
sudo rm -rf "/Library/Application Support/openlid"
echo "Helper uninstalled."
