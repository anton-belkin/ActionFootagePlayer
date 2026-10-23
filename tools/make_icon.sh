#!/bin/bash
# tools/make_icon.sh: macos/icon/AppIcon.svg → macos/icon/AppIcon.icns, the icon tools/make_app.sh puts in the app.
# Run it after changing the SVG, and commit both: the .icns is kept in the repo, so building the app needs no SVG
# renderer. Renders with Google Chrome (headless), scales with sips, packs with iconutil.
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
