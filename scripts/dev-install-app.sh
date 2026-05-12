#!/usr/bin/env bash
# scripts/dev-install-app.sh
#
# Install (or reinstall) target/bundle/OpenLid.app into /Applications and
# force macOS to refresh its icon / Spotlight metadata caches. Without
# refreshing, copying a new .app over the same path often shows a stale
# icon (or no icon) until the next reboot.
#
# Run this any time you've rebuilt the .app and want to see the latest in
# the menu bar, Dock, Spotlight, or Finder.
set -euo pipefail

cd "$(dirname "$0")/.."

SRC="target/bundle/OpenLid.app"
DST="/Applications/OpenLid.app"

if [ ! -d "$SRC" ]; then
    echo "Build the bundle first:" >&2
    echo "  ./scripts/build-app-bundle.sh" >&2
    exit 1
fi

# Kill any currently-running instance so the binary swap is clean.
pkill -f "/Applications/OpenLid.app/Contents/MacOS/open-lid" 2>/dev/null || true

# Delete-first is essential. `cp -R` *over* an existing bundle reuses
# parts of the old directory tree, which leaves macOS's icon cache
# pointing at the old inode. Removing and re-copying ensures every file
# is fresh.
rm -rf "$DST"
cp -R "$SRC" "$DST"

# Touch the bundle root to bump its modification time so Spotlight sees
# it as "changed" and re-indexes everything inside.
touch "$DST"

# Force Spotlight to re-import the bundle's metadata (display name, icon).
mdimport "$DST" 2>/dev/null || true

# Force LaunchServices to re-read the bundle. This is the secret sauce
# that refreshes the icon shown in the Dock, Finder, and Spotlight.
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister \
    -f "$DST" 2>/dev/null || true

# Restart Dock + Finder so visible icons update immediately. (Optional —
# the cache eventually refreshes on its own, but waiting is annoying.)
killall Dock 2>/dev/null || true
killall Finder 2>/dev/null || true

# Now that the install is in /Applications, delete the build artifact so
# Spotlight doesn't show two OpenLid bundles (one installed, one in the
# project tree). The .metadata_never_index marker prevents Spotlight
# from indexing target/ in the future; removing the existing bundle
# clears any cached entry for it.
rm -rf "$SRC"

echo "Installed $DST. Cache refreshed."
echo "  open -a OpenLid    - launch the app"
