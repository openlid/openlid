#!/bin/sh
# Installer for openlid auto-update.
#
# This script is written to /tmp by `openlid update` and exec'd
# detached. It survives the parent's exit (setsid + nohup, stdio
# redirected to a log file) so it can replace /Applications/OpenLid.app
# even if the openlid binary doing the update lives inside that bundle.
#
# All placeholders are substituted by `render_installer_script` in
# the Rust side. No shell variable expansion -- those would expand
# the wrong way given that the placeholders may contain spaces.

set -eu

PARENT_PID="__PARENT_PID__"
DMG_PATH="__DMG_PATH__"
APP_PATH="__APP_PATH__"
APP_EXEC_RE="$APP_PATH/Contents/MacOS/openlid([[:space:]]|$)"

log() {
    echo "[$(date '+%H:%M:%S')] $*"
}

# (1) Wait for the parent `openlid update` process to exit. kill -0
# returns 0 if the process exists. Without this wait we'd race the
# `rm -rf` of the .app while the parent's file handle was still open.
log "waiting for parent pid $PARENT_PID to exit"
while kill -0 "$PARENT_PID" 2>/dev/null; do sleep 0.2; done

# (2) Kill any remaining menubar process. The parent already exited,
# but there may be a separately-launched menubar (the .app bundle
# entry) still running.
log "stopping any running menubar instance"
pkill -TERM -f "$APP_EXEC_RE" 2>/dev/null || true

TERM_DEADLINE=$(( $(date +%s) + 10 ))
while pgrep -f "$APP_EXEC_RE" >/dev/null 2>&1; do
    if [ "$(date +%s)" -ge "$TERM_DEADLINE" ]; then
        log "old menubar still running after TERM; forcing stop"
        pkill -KILL -f "$APP_EXEC_RE" 2>/dev/null || true
        break
    fi
    sleep 0.2
done

KILL_DEADLINE=$(( $(date +%s) + 5 ))
while pgrep -f "$APP_EXEC_RE" >/dev/null 2>&1; do
    if [ "$(date +%s)" -ge "$KILL_DEADLINE" ]; then
        log "old menubar still running after KILL; aborting"
        exit 1
    fi
    sleep 0.2
done

# (3) Mount the DMG read-only. -nobrowse prevents Finder from showing
# the volume; -plist gives us structured output we can parse.
log "mounting DMG: $DMG_PATH"
MOUNT_OUTPUT="$(hdiutil attach -nobrowse -readonly -plist "$DMG_PATH")"

# Extract the mount point. Prefer plutil for a robust XML walk; fall
# back to grep against the same XML if plutil fails (older macOS).
VOLUME_PATH="$(echo "$MOUNT_OUTPUT" \
    | plutil -extract 'system-entities.0.mount-point' raw -o - - 2>/dev/null \
    || true)"
if [ -z "${VOLUME_PATH:-}" ] || [ ! -d "$VOLUME_PATH" ]; then
    VOLUME_PATH="$(echo "$MOUNT_OUTPUT" | grep -Eo '/Volumes/[^<]+' | head -1)"
fi
if [ -z "${VOLUME_PATH:-}" ] || [ ! -d "$VOLUME_PATH" ]; then
    log "could not determine the DMG mount point; aborting"
    exit 1
fi
log "DMG mounted at: $VOLUME_PATH"

# (3b) Verify the new bundle's code signature BEFORE anything destructive.
# The DMG was fetched over HTTPS, but it was downloaded programmatically
# (via the updater's HTTP client), so it carries no `com.apple.quarantine`
# attribute — which means macOS will NOT run a Gatekeeper assessment when we
# later `open` it. We therefore verify the signature ourselves here, pinning
# the same Apple-anchored Developer ID + Team ID that the privileged helper
# requires of its XPC clients (crates/helper/src/main.rs PROD_REQUIREMENT).
# A bundle not signed by us is rejected and the install aborts with the
# user's existing app left untouched.
OPENLID_CODE_REQUIREMENT='anchor apple generic and identifier "io.openlid.app" and certificate leaf[subject.OU] = "X5SZL4562S"'
log "verifying code signature of the new bundle"
if ! codesign --verify --strict -R="$OPENLID_CODE_REQUIREMENT" "$VOLUME_PATH/OpenLid.app"; then
    log "code-signature verification FAILED; refusing to install. Your existing OpenLid is unchanged."
    hdiutil detach "$VOLUME_PATH" -quiet 2>/dev/null || true
    exit 1
fi
log "code signature OK (Developer ID, Team X5SZL4562S)"

# (4) Stage the new bundle next to the old, then atomically swap.
# The `mv` is atomic on a single filesystem; the small window with
# neither path present is irrelevant because step 2 already killed
# the user-visible process.
log "staging new bundle at ${APP_PATH}.new"
rm -rf "${APP_PATH}.new"
cp -R "$VOLUME_PATH/OpenLid.app" "${APP_PATH}.new"

log "swapping in the new bundle"
rm -rf "$APP_PATH"
mv "${APP_PATH}.new" "$APP_PATH"

# (5) Detach the DMG. Failure is non-fatal -- macOS auto-detaches
# after a while.
log "detaching DMG"
hdiutil detach "$VOLUME_PATH" -quiet 2>/dev/null || true

# (6) Refresh LaunchServices and Spotlight metadata so the new .app
# is recognised immediately rather than after the next reboot. Same
# steps as scripts/dev-install-app.sh.
log "refreshing LaunchServices / Spotlight"
touch "$APP_PATH"
mdimport "$APP_PATH" 2>/dev/null || true
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister -f "$APP_PATH" 2>/dev/null || true

# (7) Relaunch the menubar app via its bundle identifier. LaunchServices
# now points at the new bundle.
log "relaunching openlid"
open -b io.openlid.app

# (8) Clean up the downloaded DMG. The cache dir is reusable; the
# specific DMG file is not.
rm -f "$DMG_PATH"

log "update complete"
