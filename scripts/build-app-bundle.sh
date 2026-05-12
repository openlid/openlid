#!/usr/bin/env bash
# scripts/build-app-bundle.sh
# Build OpenLid.app for local dev. Ad-hoc signing only.
#
# Output goes to `target/bundle/OpenLid.app`. The `target/` directory is
# `cargo`-managed and Spotlight typically skips it; this avoids the
# previous footgun where a project-root `OpenLid.app` showed up as a
# *second* installable app in Spotlight alongside the real one in
# /Applications.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build -p open-lid -p open-lid-helper

BUNDLE_DIR="target/bundle"
APP="${BUNDLE_DIR}/OpenLid.app"
mkdir -p "$BUNDLE_DIR"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"
mkdir -p "$APP/Contents/Library/LaunchDaemons"

# Belt-and-suspenders: tell Spotlight to never index target/ even if a user
# tweaks their indexer settings. Touching this marker file is harmless if
# the directory is already excluded.
touch target/.metadata_never_index 2>/dev/null || true

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

# Bundle a self-contained helper installer for end-users (Homebrew cask
# users run this after `brew install --cask open-lid`). It points at the
# helper binary at .app's MacOS dir rather than `target/debug/`.
install -m 0755 resources/app/install-helper.sh "$APP/Contents/Resources/install-helper.sh"

codesign --force --sign - --options runtime "$APP/Contents/MacOS/open-lid-helper"
codesign --force --sign - --options runtime "$APP/Contents/MacOS/open-lid"
codesign --force --sign - --deep --options runtime "$APP"

echo "Built $APP."
echo
echo "Install (or reinstall) into /Applications:"
echo "  ./scripts/dev-install-app.sh"
