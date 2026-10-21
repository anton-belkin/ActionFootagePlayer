#!/bin/bash
# Installs the pipeline into mpv's config directory. Re-run it after pulling changes.
#
#   ./install.sh           shaders (with DJI's LUT, downloaded from DJI), mpv config and scripts, Gyroflow plugin
#   ./install.sh --apps    also ~/Applications/mpv.app and ~/Applications/DJI Router.app (see README)
#
# Files it replaces that differ from the new ones are kept as <file>.bak-<date>.
# MPV_CONFIG_DIR and APPS_DIR override where it installs (default ~/.config/mpv and ~/Applications).
set -euo pipefail
cd "$(dirname "$0")"

MPV_DIR="${MPV_CONFIG_DIR:-$HOME/.config/mpv}"
APPS_DIR="${APPS_DIR:-$HOME/Applications}"
BUILD=build

# DJI publishes it on the Osmo Action 6 downloads page (https://www.dji.com/osmo-action-6/downloads); it isn't
# redistributed here. The checksum is of version 11.17: if DJI updates the file, check it and update both.
DJI_LUT_URL="https://terra-1-g.djicdn.com/851d20f7b9f64838a34cd02351370894/AC206%20LUT/DJI%20OSMO%20Action%206%20D-LogM%20to%20Rec.709%20LUT-11.17.cube"
DJI_LUT_SHA256="1972eb9b05699ba19d4aaa67aaa5fb00b46935c60ba9219183289f79e044cbb2"

apps=0
for arg in "$@"; do
    case "$arg" in
        --apps) apps=1 ;;
        *) echo "usage: $0 [--apps]" >&2; exit 2 ;;
    esac
done

need() { command -v "$1" >/dev/null || { echo "Missing $1 ($2)" >&2; exit 1; }; }
need mpv "brew install mpv"
need python3 "Xcode command line tools"
need cargo "https://rustup.rs"
if ! grep -q vapoursynth <<< "$(mpv --vf=help 2>/dev/null)"; then
    echo "Warning: this mpv has no VapourSynth filter; videos will play, but not stabilized" >&2
fi

mkdir -p "$BUILD"

echo "== DJI D-Log M LUT"
lut="$BUILD/dji-dlogm-rec709.cube"
lut_ok() { echo "$DJI_LUT_SHA256  $lut" | shasum -a 256 -c --status 2>/dev/null; }
if ! lut_ok; then
    curl -fsSL -o "$lut" "$DJI_LUT_URL"
    lut_ok || { echo "The LUT DJI serves isn't the one this was made for (checksum differs)" >&2; exit 1; }
fi

echo "== Shaders"
python3 -I tools/cube_to_glsl.py shaders/dlogm_lut3d.glsl.in "$lut" "$BUILD/dlogm_lut3d.glsl"
python3 -I tools/cube_to_glsl.py shaders/dlogm_hlg.glsl.in luts/dlogm_hlg.cube "$BUILD/dlogm_hlg.glsl"

echo "== Gyroflow plugin"
(cd vs-gyroflow && cargo build --release)

echo "== Installing into $MPV_DIR"
stamp=$(date +%Y%m%d-%H%M%S)
# `install` writes a new file: copying over a loaded dylib keeps its inode, and macOS then kills whatever loads
# it (the kernel has the old code signature cached for that file)
put() {
    local src=$1 dst=$2 mode=$3
    mkdir -p "$(dirname "$dst")"
    if [ -e "$dst" ] && ! cmp -s "$src" "$dst"; then
        cp -p "$dst" "$dst.bak-$stamp"
        echo "   kept the previous $dst as $(basename "$dst").bak-$stamp"
    fi
    install -m "$mode" "$src" "$dst"
}
put mpv/mpv.conf                  "$MPV_DIR/mpv.conf" 644
put mpv/input.conf                "$MPV_DIR/input.conf" 644
put mpv/scripts/dlog_auto.lua     "$MPV_DIR/scripts/dlog_auto.lua" 644
put mpv/scripts/gyroflow_auto.lua "$MPV_DIR/scripts/gyroflow_auto.lua" 644
put mpv/gyroflow/gyroflow.vpy     "$MPV_DIR/gyroflow/gyroflow.vpy" 644
put "$BUILD/dlogm_lut3d.glsl"     "$MPV_DIR/shaders/dlogm_lut3d.glsl" 644
put "$BUILD/dlogm_hlg.glsl"       "$MPV_DIR/shaders/dlogm_hlg.glsl" 644
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-embed; do
    put "vs-gyroflow/target/release/$f" "$MPV_DIR/gyroflow/$f" 755
done

if [ $apps = 1 ]; then
    echo "== Apps in $APPS_DIR"
    mkdir -p "$APPS_DIR"

    # mpv as an app of its own, so the router can start it with `open -na`
    app="$APPS_DIR/mpv.app"
    mkdir -p "$app/Contents/MacOS"
    sed "s|@MPV@|$(command -v mpv)|" macos/mpv-launcher.sh > "$BUILD/mpv-launcher"
    put "$BUILD/mpv-launcher" "$app/Contents/MacOS/mpv" 755
    cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>mpv</string>
    <key>CFBundleIdentifier</key><string>io.github.mpv-launcher</string>
    <key>CFBundleName</key><string>mpv</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    codesign --force -s - "$app" >/dev/null 2>&1

    # The router: D-Log M clips to mpv, everything else to IINA
    router="$APPS_DIR/DJI Router.app"
    rm -rf "$router"
    osacompile -o "$router" macos/dji-router.applescript
    plist="$router/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier io.github.dji-router" "$plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string io.github.dji-router" "$plist"
    /usr/libexec/PlistBuddy -c "Delete :CFBundleDocumentTypes" "$plist" 2>/dev/null || true
    /usr/libexec/PlistBuddy \
        -c "Add :CFBundleDocumentTypes array" \
        -c "Add :CFBundleDocumentTypes:0 dict" \
        -c "Add :CFBundleDocumentTypes:0:CFBundleTypeName string Video file" \
        -c "Add :CFBundleDocumentTypes:0:CFBundleTypeRole string Viewer" \
        -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes array" \
        -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:0 string public.mpeg-4" \
        -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:1 string com.apple.quicktime-movie" \
        "$plist"
    codesign --force -s - "$router" >/dev/null 2>&1
    if [ "$APPS_DIR" = "$HOME/Applications" ]; then
        /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$router" "$app"
    fi
    echo "   To open MP4/MOV files with it: Finder > Get Info on a clip > Open with: DJI Router > Change All"
fi

echo "Done."
