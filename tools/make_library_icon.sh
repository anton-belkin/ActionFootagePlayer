#!/bin/bash
# tools/make_library_icon.sh: library/icons/icon.svg → library/icons/icon.png, icon.icns and icon.ico, Action Footage
# Library's icon (the player's scene as a grid of clips: macos/icon/AppIcon.svg is its kin). As tools/make_icon.sh:
# rendered with Google Chrome (headless), scaled with sips, packed with iconutil and tools/png_to_ico.py; commit them.
set -euo pipefail
cd "$(dirname "$0")/.."
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
[ -x "$chrome" ] || { echo "tools/make_library_icon.sh renders the SVG with Google Chrome, which isn't in /Applications" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$chrome" --headless=new --disable-gpu --hide-scrollbars --default-background-color=00000000 --window-size=1024,1024 \
    --screenshot="$tmp/1024.png" "file://$PWD/library/icons/icon.svg" >/dev/null 2>&1
cp "$tmp/1024.png" library/icons/icon.png
set=$tmp/icon.iconset
mkdir "$set"
for size in 16 32 128 256 512; do
    sips -z $size $size "$tmp/1024.png" --out "$set/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$tmp/1024.png" --out "$set/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns -o library/icons/icon.icns "$set"
# Windows': the sizes Explorer and the taskbar show, at 100-200% scaling
for size in 16 20 24 32 40 48 64 96 128 256; do
    sips -z $size $size "$tmp/1024.png" --out "$tmp/ico-$size.png" >/dev/null
done
python3 -I tools/png_to_ico.py library/icons/icon.ico "$tmp"/ico-*.png
echo "library/icons: icon.png, icon.icns, icon.ico"
