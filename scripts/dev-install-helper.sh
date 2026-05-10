#!/usr/bin/env bash
# scripts/dev-install-helper.sh
# Manually install the helper to /Library/LaunchDaemons pointing at the
# debug-built binary. Used during Plan 1 development before SMAppService
# is wired up (Plan 2).
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build -p open-lid-helper

ABS_HELPER_PATH="$PWD/target/debug/open-lid-helper"
codesign --force --sign - --options runtime "$ABS_HELPER_PATH"

TMP_PLIST="$(mktemp)"
sed "s|__OPEN_LID_HELPER_PATH__|$ABS_HELPER_PATH|" \
    resources/helper/io.openlid.helper.plist > "$TMP_PLIST"

sudo cp "$TMP_PLIST" /Library/LaunchDaemons/io.openlid.helper.plist
sudo chown root:wheel /Library/LaunchDaemons/io.openlid.helper.plist
sudo chmod 644 /Library/LaunchDaemons/io.openlid.helper.plist
rm "$TMP_PLIST"

sudo launchctl bootout system/io.openlid.helper 2>/dev/null || true
sudo launchctl bootstrap system /Library/LaunchDaemons/io.openlid.helper.plist
echo "Helper installed and bootstrapped. Log: /Library/Logs/open-lid/helper.log"
