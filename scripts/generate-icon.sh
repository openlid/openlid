#!/usr/bin/env bash
# scripts/generate-icon.sh
#
# Generate `resources/app/AppIcon.icns` (and the README + org-avatar PNGs)
# from a source SVG. Designed to use only built-in macOS tooling — no
# Homebrew dependencies. Re-run this whenever you change the icon design
# (outputs are committed so other builds skip the generation step).
#
# Pipeline:
#   1. Write a 1024×1024 source SVG to a temp file.
#   2. Render with a Swift one-liner (NSImage + NSBitmapImageRep).
#      We deliberately do NOT use `qlmanage -t` here — qlmanage always
#      fills its canvas with an opaque white background, regardless of
#      the SVG's transparency. NSImage + NSBitmapImageRep with
#      `hasAlpha: true` preserves the SVG's alpha channel, so the
#      corners outside the rx/ry squircle stay transparent. Swift ships
#      with Xcode CLT, which the project already requires.
#   3. Downscale to each iconset size with `sips`.
#   4. Pack with `iconutil --convert icns`.
#   5. Emit the README header PNG (256×256) and GitHub org avatar PNG
#      (1024×1024) from the same source render so all icon surfaces
#      stay in lockstep across the .icns, README, and org-avatar.
set -euo pipefail

cd "$(dirname "$0")/.."

OUT_DIR="resources/app"
OUT_ICNS="${OUT_DIR}/AppIcon.icns"
OUT_README_PNG="${OUT_DIR}/AppIcon-readme.png"     # 256×256, used in README header
OUT_AVATAR_PNG="resources/branding/openlid-org-avatar.png"  # 1024×1024, GitHub org avatar
TMP_DIR="$(mktemp -d)"
trap "rm -rf '$TMP_DIR'" EXIT

SVG_PATH="${TMP_DIR}/icon.svg"
PNG_1024="${TMP_DIR}/icon-1024.png"
ICONSET="${TMP_DIR}/AppIcon.iconset"

# ─────────────────────────────────────────────────────────────────────────────
# 1. Source SVG.
#
# A rounded-square (~21.5% corner radius — matches Apple's macOS "squircle"
# approximation) in flat teal, with the Tabler `device-laptop` glyph in white.
# The viewBox area outside the squircle is left transparent so the icon sits
# cleanly against any background. Tabler paths are MIT-licensed.
# ─────────────────────────────────────────────────────────────────────────────
cat > "$SVG_PATH" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <!-- Squircle background. Corners outside the rx/ry are transparent. -->
  <rect x="0" y="0" width="1024" height="1024" rx="220" ry="220" fill="#2688a8"/>

  <!-- Tabler device-laptop glyph, scaled from 24×24 viewBox onto a 600pt
       design grid centered at (512,512). 1 unit = 25pt; translate(212,212)
       then scale(25) puts the glyph's (12,12) at the canvas centre. -->
  <g transform="translate(212, 212) scale(25)"
     fill="none" stroke="#FFFFFF" stroke-width="2"
     stroke-linecap="round" stroke-linejoin="round">
    <path d="M3 19 l18 0"/>
    <path d="M5 7 a1 1 0 0 1 1-1 h12 a1 1 0 0 1 1 1 v8 a1 1 0 0 1-1 1 h-12 a1 1 0 0 1-1-1 l0-8"/>
  </g>
</svg>
EOF

# ─────────────────────────────────────────────────────────────────────────────
# 2. Render SVG → 1024×1024 PNG via Swift + NSImage + NSBitmapImageRep.
#    Preserves the SVG's alpha channel — corners outside the squircle's
#    rx/ry stay transparent. See file header for why this isn't qlmanage.
# ─────────────────────────────────────────────────────────────────────────────
SWIFT_RENDERER="${TMP_DIR}/render.swift"
cat > "$SWIFT_RENDERER" <<'SWIFT'
import Cocoa

guard CommandLine.arguments.count == 4,
      let size = Int(CommandLine.arguments[3]) else {
    FileHandle.standardError.write("usage: render <svg> <png> <size>\n".data(using: .utf8)!)
    exit(2)
}
let svgURL = URL(fileURLWithPath: CommandLine.arguments[1])
let pngURL = URL(fileURLWithPath: CommandLine.arguments[2])

guard let image = NSImage(contentsOf: svgURL) else {
    FileHandle.standardError.write("error: NSImage failed to load \(svgURL.path)\n".data(using: .utf8)!)
    exit(1)
}
image.size = NSSize(width: size, height: size)

guard let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: size, pixelsHigh: size,
    bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0
) else {
    FileHandle.standardError.write("error: NSBitmapImageRep alloc failed\n".data(using: .utf8)!)
    exit(1)
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
NSGraphicsContext.restoreGraphicsState()

guard let data = bitmap.representation(using: .png, properties: [:]) else {
    FileHandle.standardError.write("error: png encode failed\n".data(using: .utf8)!)
    exit(1)
}
try data.write(to: pngURL)
SWIFT

swift "$SWIFT_RENDERER" "$SVG_PATH" "$PNG_1024" 1024

# Sanity check: file should be a real PNG of the right size.
if [ ! -f "$PNG_1024" ]; then
    echo "swift renderer failed to produce a PNG" >&2
    exit 1
fi

# ─────────────────────────────────────────────────────────────────────────────
# 3. Build the iconset directory with all required sizes.
# Apple's naming convention is icon_<W>x<H>.png and icon_<W>x<H>@2x.png.
# ─────────────────────────────────────────────────────────────────────────────
mkdir -p "$ICONSET"

declare -a SIZES=("16:1x" "32:1x" "32:2x" "64:2x" "128:1x" "256:1x" "256:2x" "512:1x" "512:2x" "1024:1x")

# Map each entry to (logical_size, suffix) and pixel size we feed to sips.
for entry in "${SIZES[@]}"; do
    px="${entry%:*}"
    scale="${entry##*:}"
    if [ "$scale" = "2x" ]; then
        logical=$((px / 2))
        name="icon_${logical}x${logical}@2x.png"
    else
        logical=$px
        name="icon_${logical}x${logical}.png"
    fi
    sips -z "$px" "$px" "$PNG_1024" --out "${ICONSET}/${name}" > /dev/null
done

# ─────────────────────────────────────────────────────────────────────────────
# 4. Pack into .icns.
# ─────────────────────────────────────────────────────────────────────────────
mkdir -p "$OUT_DIR"
iconutil --convert icns "$ICONSET" --output "$OUT_ICNS"

echo "Wrote $OUT_ICNS ($(stat -f%z "$OUT_ICNS") bytes)"

# ─────────────────────────────────────────────────────────────────────────────
# 5. Marketing / docs PNGs from the same source SVG.
#
# We re-render these from `$PNG_1024` (not the iconset) because the iconset
# entries get rounded by sips's high-quality downscaler, but we want the
# README and org avatar to come straight off the 1024×1024 render to keep
# the squircle edges crisp.
# ─────────────────────────────────────────────────────────────────────────────
mkdir -p "$(dirname "$OUT_AVATAR_PNG")"
cp "$PNG_1024" "$OUT_AVATAR_PNG"
echo "Wrote $OUT_AVATAR_PNG ($(stat -f%z "$OUT_AVATAR_PNG") bytes)"

sips -z 256 256 "$PNG_1024" --out "$OUT_README_PNG" > /dev/null
echo "Wrote $OUT_README_PNG ($(stat -f%z "$OUT_README_PNG") bytes)"
