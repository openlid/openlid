#!/usr/bin/env bash
# scripts/install-cli-symlink.sh
# Adds /usr/local/bin/openlid → /Applications/OpenLid.app/Contents/MacOS/openlid
set -euo pipefail
TARGET="/Applications/OpenLid.app/Contents/MacOS/openlid"
LINK="/usr/local/bin/openlid"
if [ ! -e "$TARGET" ]; then
    echo "Build OpenLid.app first: ./scripts/build-app-bundle.sh && cp -R OpenLid.app /Applications/"
    exit 1
fi
sudo ln -sf "$TARGET" "$LINK"
echo "openlid is on your PATH: $LINK"
