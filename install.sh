#!/bin/bash
# Installs the pipeline into mpv's config directory. Re-run it after pulling changes.
#
#   ./install.sh           shaders (with DJI's LUT, downloaded from DJI), mpv config and scripts, Gyroflow plugin
#   ./install.sh --apps    also Steady Player.app and Steady Router.app in ~/Applications, and Finder Quick Actions
#                          (see README)
#
# Files it replaces that differ from the new ones are kept as <file>.bak-<date>.
# MPV_CONFIG_DIR, APPS_DIR and SERVICES_DIR override where it installs (default ~/.config/mpv, ~/Applications and
# ~/Library/Services).
set -euo pipefail
cd "$(dirname "$0")"

MPV_DIR="${MPV_CONFIG_DIR:-$HOME/.config/mpv}"
APPS_DIR="${APPS_DIR:-$HOME/Applications}"
SERVICES_DIR="${SERVICES_DIR:-$HOME/Library/Services}"
# Working names; the bundle ids are what macOS and the settings go by
PLAYER_NAME="Steady Player"
ROUTER_NAME="Steady Router"
PLAYER_ID="io.github.anton-belkin.steady-player"
ROUTER_ID="io.github.anton-belkin.steady-router"
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
# A release download has the binaries prebuilt in bin/; a git checkout builds them
if [ -d bin ]; then
    BIN=bin
    # Downloaded with a browser, they carry the quarantine flag, and macOS would refuse to run them
    xattr -dr com.apple.quarantine bin 2>/dev/null || true
else
    BIN=vs-gyroflow/target/release
    need cargo "https://rustup.rs, or download a release, which has the binaries prebuilt"
fi
command -v ffmpeg >/dev/null || echo "Warning: no ffmpeg (brew install ffmpeg); exporting needs it" >&2
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

if [ "$BIN" != bin ]; then
    echo "== Gyroflow plugin"
    (cd vs-gyroflow && cargo build --release)
fi

echo "== Installing into $MPV_DIR"
stamp=$(date +%Y%m%d-%H%M%S)
# `install` writes a new file: copying over a loaded dylib keeps its inode, and macOS then kills whatever loads
# it (the kernel has the old code signature cached for that file)
put() {
    local src=$1 dst=$2 mode=$3
    mkdir -p "$(dirname "$dst")"
    if [ -e "$dst" ] && ! cmp -s "$src" "$dst"; then
        # Not next to scripts: mpv would try to load scripts/*.bak-* as scripts
        local bak="$dst.bak-$stamp"
        case "$dst" in "$MPV_DIR/scripts/"*) mkdir -p "$MPV_DIR/scripts-backup"; bak="$MPV_DIR/scripts-backup/$(basename "$dst").bak-$stamp" ;; esac
        cp -p "$dst" "$bak"
        echo "   kept the previous $dst as $bak"
    fi
    install -m "$mode" "$src" "$dst"
}
put mpv/mpv.conf                  "$MPV_DIR/mpv.conf" 644
put mpv/input.conf                "$MPV_DIR/input.conf" 644
put mpv/scripts/dlog_auto.lua     "$MPV_DIR/scripts/dlog_auto.lua" 644
put mpv/scripts/gyroflow_auto.lua "$MPV_DIR/scripts/gyroflow_auto.lua" 644
put mpv/scripts/gyroflow_export.lua "$MPV_DIR/scripts/gyroflow_export.lua" 644
put mpv/gyroflow/gyroflow.vpy     "$MPV_DIR/gyroflow/gyroflow.vpy" 644
# The default preset for clips without a project: underwater. Only if there's none yet, it's yours to change
if [ ! -e "$MPV_DIR/gyroflow/default.gyroflow" ]; then
    install -m 644 mpv/gyroflow/underwater.gyroflow "$MPV_DIR/gyroflow/default.gyroflow"
fi
put "$BUILD/dlogm_lut3d.glsl"     "$MPV_DIR/shaders/dlogm_lut3d.glsl" 644
put "$BUILD/dlogm_hlg.glsl"       "$MPV_DIR/shaders/dlogm_hlg.glsl" 644
# The LUTs themselves, for gyroflow-export
put luts/dlogm_hlg.cube           "$MPV_DIR/luts/dlogm_hlg.cube" 644
put "$lut"                        "$MPV_DIR/luts/dji-dlogm-rec709.cube" 644
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-embed gyroflow-export; do
    put "$BIN/$f" "$MPV_DIR/gyroflow/$f" 755
done

if [ $apps = 1 ]; then
    echo "== Apps in $APPS_DIR, Quick Actions in $SERVICES_DIR"
    mkdir -p "$APPS_DIR" "$SERVICES_DIR"
    lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

    # Adds "Video file" document types to an app, at a rank: Alternate lists it in Open With without taking
    # videos over
    doc_types() {
        local plist=$1 rank=$2
        /usr/libexec/PlistBuddy -c "Delete :CFBundleDocumentTypes" "$plist" 2>/dev/null || true
        /usr/libexec/PlistBuddy \
            -c "Add :CFBundleDocumentTypes array" \
            -c "Add :CFBundleDocumentTypes:0 dict" \
            -c "Add :CFBundleDocumentTypes:0:CFBundleTypeName string Video file" \
            -c "Add :CFBundleDocumentTypes:0:CFBundleTypeRole string Viewer" \
            -c "Add :CFBundleDocumentTypes:0:LSHandlerRank string $rank" \
            -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes array" \
            -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:0 string public.mpeg-4" \
            -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:1 string com.apple.quicktime-movie" \
            "$plist"
    }
    set_id() {
        /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $2" "$1" 2>/dev/null \
            || /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string $2" "$1"
    }

    # The player: an applet that starts mpv through a launcher app of its own (see macos/player.applescript)
    player="$APPS_DIR/$PLAYER_NAME.app"
    rm -rf "$player"
    osacompile -o "$player" macos/player.applescript
    launcher="$player/Contents/Resources/mpv.app"
    mkdir -p "$launcher/Contents/MacOS"
    sed "s|@MPV@|$(command -v mpv)|" macos/mpv-launcher.sh > "$launcher/Contents/MacOS/mpv"
    chmod 755 "$launcher/Contents/MacOS/mpv"
    cat > "$launcher/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>mpv</string>
    <key>CFBundleIdentifier</key><string>$PLAYER_ID.mpv</string>
    <key>CFBundleName</key><string>mpv</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    set_id "$player/Contents/Info.plist" "$PLAYER_ID"
    doc_types "$player/Contents/Info.plist" Alternate
    codesign --force -s - "$launcher" >/dev/null 2>&1
    codesign --force -s - "$player" >/dev/null 2>&1

    # The router: the default player in router mode only (gyroflow-router on)
    router="$APPS_DIR/$ROUTER_NAME.app"
    rm -rf "$router"
    osacompile -o "$router" macos/router.applescript
    sed "s|@PLAYER_ID@|$PLAYER_ID|g" macos/route.sh > "$router/Contents/Resources/route.sh"
    chmod 755 "$router/Contents/Resources/route.sh"
    set_id "$router/Contents/Info.plist" "$ROUTER_ID"
    doc_types "$router/Contents/Info.plist" Alternate
    codesign --force -s - "$router" >/dev/null 2>&1

    sed -e "s|@PLAYER_ID@|$PLAYER_ID|g" -e "s|@ROUTER_ID@|$ROUTER_ID|g" macos/gyroflow-router.py > "$BUILD/gyroflow-router"
    put "$BUILD/gyroflow-router" "$MPV_DIR/gyroflow/gyroflow-router" 755
    put macos/export-finder.sh "$MPV_DIR/gyroflow/export-finder.sh" 755

    # Finder Quick Actions (right click on videos)
    python3 -I tools/make_workflow.py "$SERVICES_DIR/Play Stabilized.workflow" "Play Stabilized" NSTouchBarPlayTemplate \
        "open -b $PLAYER_ID \"\$@\""
    python3 -I tools/make_workflow.py "$SERVICES_DIR/Stabilize + LUT.workflow" "Stabilize + LUT" NSTouchBarExitFullScreenTemplate \
        "\"\$HOME/.config/mpv/gyroflow/export-finder.sh\" \"\$@\""

    if [ "$APPS_DIR" = "$HOME/Applications" ]; then
        "$lsregister" -f "$player" "$router"
        /System/Library/CoreServices/pbs -flush >/dev/null 2>&1 || true
    fi
    echo "   $PLAYER_NAME is in Finder's Open With for MP4/MOV; Quick Actions: Play Stabilized, Stabilize + LUT"
    echo "   Router mode (the default player, sending other videos to your usual one): $MPV_DIR/gyroflow/gyroflow-router on"
fi

echo "Done."
