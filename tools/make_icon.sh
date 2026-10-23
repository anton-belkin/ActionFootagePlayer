#!/bin/bash
# tools/make_icon.sh: macos/icon/AppIcon.svg → macos/icon/AppIcon.icns, the icon tools/make_app.sh puts in the app,
# and windows/AppIcon.ico, the Windows app's (tools/make_windows_app.sh). Run it after changing the SVG, and commit
# them all: they're kept in the repo, so building the apps needs no SVG renderer. Renders with Google Chrome
# (headless), scales with sips, packs with iconutil and tools/png_to_ico.py. (Linux's is the SVG itself.)
set -euo pipefail
cd "$(dirname "$0")/.."

chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
[ -x "$chrome" ] || { echo "tools/make_icon.sh renders the SVG with Google Chrome, which isn't in /Applications" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

"$chrome" --headless=new --disable-gpu --hide-scrollbars --default-background-color=00000000 --window-size=1024,1024 \
    --screenshot="$tmp/1024.png" "file://$PWD/macos/icon/AppIcon.svg" >/dev/null 2>&1
set=$tmp/AppIcon.iconset
mkdir "$set"
for size in 16 32 128 256 512; do
    sips -z $size $size "$tmp/1024.png" --out "$set/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$tmp/1024.png" --out "$set/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns -o macos/icon/AppIcon.icns "$set"
# Windows': the sizes Explorer and the taskbar show, at 100-200% scaling
for size in 16 20 24 32 40 48 64 96 128 256; do
    sips -z $size $size "$tmp/1024.png" --out "$tmp/ico-$size.png" >/dev/null
done
python3 -I tools/png_to_ico.py windows/AppIcon.ico "$tmp"/ico-*.png
