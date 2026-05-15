#!/usr/bin/env bash
# scripts/build-app-bundle.sh
#
# Build OpenLid.app. Two profiles:
#
#   ./scripts/build-app-bundle.sh           — DEV (default; ad-hoc signed,
#                                              permissive helper code-req,
#                                              for local development only).
#
#   PROFILE=release ./scripts/build-app-bundle.sh
#                                           — RELEASE (Developer ID signed,
#                                              strict Team-ID-pinned helper
#                                              code-req, ready for notarization).
#
# Output goes to `target/bundle/OpenLid.app`. The `target/` directory is
# Cargo-managed; Spotlight skips it; the .app is then installed to
# /Applications by scripts/dev-install-app.sh or by the user dragging it.
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="${PROFILE:-dev}"
case "$PROFILE" in
    dev|release) ;;
    *)
        echo "Unknown PROFILE=$PROFILE. Use PROFILE=dev or PROFILE=release." >&2
        exit 1
        ;;
esac

if [ "$PROFILE" = "release" ]; then
    CARGO_PROFILE_FLAG="--release"
    CARGO_TARGET_DIR_SUFFIX="release"
    OPEN_LID_HELPER_PROFILE="prod"
    SIGNING_IDENTITY="${SIGNING_IDENTITY:-Developer ID Application: Diyan Bogdanov (X5SZL4562S)}"
    SIGN_OPTS=(--force --sign "$SIGNING_IDENTITY" --options runtime --timestamp)
else
    CARGO_PROFILE_FLAG=""
    CARGO_TARGET_DIR_SUFFIX="debug"
    OPEN_LID_HELPER_PROFILE="dev"
    SIGNING_IDENTITY="-"
    SIGN_OPTS=(--force --sign - --options runtime)
fi

echo "Building profile=${PROFILE}, helper code-req=${OPEN_LID_HELPER_PROFILE}, signing=${SIGNING_IDENTITY}"

OPEN_LID_HELPER_PROFILE="$OPEN_LID_HELPER_PROFILE" \
    cargo build $CARGO_PROFILE_FLAG -p openlid -p openlid-helper

BUNDLE_DIR="target/bundle"
APP="${BUNDLE_DIR}/OpenLid.app"
mkdir -p "$BUNDLE_DIR"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"
mkdir -p "$APP/Contents/Library/LaunchDaemons"

# Belt-and-suspenders: tell Spotlight to never index target/ even if a user
# tweaks their indexer settings.
touch target/.metadata_never_index 2>/dev/null || true

cp "target/${CARGO_TARGET_DIR_SUFFIX}/openlid" "$APP/Contents/MacOS/openlid"
cp "target/${CARGO_TARGET_DIR_SUFFIX}/openlid-helper" "$APP/Contents/MacOS/openlid-helper"
cp resources/app/Info.plist "$APP/Contents/Info.plist"
cp resources/helper/io.openlid.helper.plist "$APP/Contents/Library/LaunchDaemons/io.openlid.helper.plist"

# App icon
if [ ! -f resources/app/AppIcon.icns ]; then
    echo "App icon missing — generating it…"
    ./scripts/generate-icon.sh
fi
cp resources/app/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

# ─────────────────────────────────────────────────────────────────────────────
# Sign. Order matters: nested binaries first, then the bundle. Neither binary
# needs special entitlements beyond hardened runtime: no JIT, no DYLD loading
# of unsigned libraries, no sandbox-grant requests. If we ever need to attach
# entitlements (e.g., for Mac App Store distribution), the *.entitlements
# files at resources/{app,helper}/ are the place to add them.
# ─────────────────────────────────────────────────────────────────────────────
codesign "${SIGN_OPTS[@]}" "$APP/Contents/MacOS/openlid-helper"
codesign "${SIGN_OPTS[@]}" "$APP/Contents/MacOS/openlid"
codesign "${SIGN_OPTS[@]}" --deep "$APP"

# Verify signing actually worked
codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | tail -3

echo
echo "Built $APP (profile=$PROFILE, signed by: $SIGNING_IDENTITY)"
if [ "$PROFILE" = "dev" ]; then
    echo
    echo "Next steps for local dev:"
    echo "  ./scripts/dev-install-app.sh"
elif [ "$PROFILE" = "release" ]; then
    echo
    echo "Next steps for release:"
    echo "  • Notarize: xcrun notarytool submit --wait …"
    echo "  • Staple:   xcrun stapler staple $APP"
    echo "  • DMG:      create-dmg … (release.yml handles this in CI)"
fi
