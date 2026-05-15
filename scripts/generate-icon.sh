#!/usr/bin/env bash
# scripts/generate-icon.sh
#
# Generate `resources/app/AppIcon.icns` from a source SVG. Designed to use
# only built-in macOS tooling — no Homebrew dependencies. Re-run this whenever
# you change the icon design (the .icns is committed so other builds skip the
# generation step).
#
# Pipeline:
#   1. Write a 1024×1024 source SVG to a temp file.
#   2. Render with `qlmanage -t` (macOS QuickLook).
#   3. Downscale to each iconset size with `sips`.
#   4. Pack with `iconutil --convert icns`.
set -euo pipefail

cd "$(dirname "$0")/.."

OUT_DIR="resources/app"
OUT_ICNS="${OUT_DIR}/AppIcon.icns"
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
# 2. Render SVG → 1024×1024 PNG using QuickLook.
# ─────────────────────────────────────────────────────────────────────────────
qlmanage -t -s 1024 -o "$TMP_DIR" "$SVG_PATH" > /dev/null 2>&1
mv "${SVG_PATH}.png" "$PNG_1024"

# Sanity check: file should be a real PNG of the right size.
if [ ! -f "$PNG_1024" ]; then
    echo "qlmanage failed to produce a PNG" >&2
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
