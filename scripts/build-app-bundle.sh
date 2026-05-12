#!/usr/bin/env bash
# scripts/build-app-bundle.sh
# Build OpenLid.app for local dev. Ad-hoc signing only.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build -p open-lid -p open-lid-helper

APP="OpenLid.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"
mkdir -p "$APP/Contents/Library/LaunchDaemons"

cp target/debug/open-lid "$APP/Contents/MacOS/open-lid"
cp target/debug/open-lid-helper "$APP/Contents/MacOS/open-lid-helper"
cp resources/app/Info.plist "$APP/Contents/Info.plist"
cp resources/helper/io.openlid.helper.plist "$APP/Contents/Library/LaunchDaemons/io.openlid.helper.plist"

# App icon. Generated on demand by scripts/generate-icon.sh if not present.
if [ ! -f resources/app/AppIcon.icns ]; then
    echo "App icon missing — generating it…"
    ./scripts/generate-icon.sh
fi
cp resources/app/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

codesign --force --sign - --options runtime "$APP/Contents/MacOS/open-lid-helper"
codesign --force --sign - --options runtime "$APP/Contents/MacOS/open-lid"
codesign --force --sign - --deep --options runtime "$APP"

echo "Built $APP. Move to /Applications:"
echo "  cp -R $APP /Applications/"
