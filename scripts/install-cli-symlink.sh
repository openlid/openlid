#!/usr/bin/env bash
# scripts/install-cli-symlink.sh
# Adds /usr/local/bin/open-lid → /Applications/OpenLid.app/Contents/MacOS/open-lid
set -euo pipefail
TARGET="/Applications/OpenLid.app/Contents/MacOS/open-lid"
LINK="/usr/local/bin/open-lid"
if [ ! -e "$TARGET" ]; then
    echo "Build OpenLid.app first: ./scripts/build-app-bundle.sh && cp -R OpenLid.app /Applications/"
    exit 1
fi
sudo ln -sf "$TARGET" "$LINK"
echo "open-lid is on your PATH: $LINK"
