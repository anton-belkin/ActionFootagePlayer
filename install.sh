#!/bin/bash
# Installs the two mpv plugins into mpv's config directory. Re-run it after pulling changes.
#
#   ./install.sh           the plugins: scripts/dlogm (D-Log M; its shaders are baked here, with DJI's LUT downloaded
#                          from DJI) and scripts/gyroflow (stabilization, with the binaries built from vs-gyroflow/);
#                          and mpv.conf when there's none yet
#   ./install.sh --apps    also Steady Player.app and Steady Router.app in ~/Applications, and Finder Quick Actions
#                          (see README). Installed once, they're updated by every later run
#
# Files it replaces that differ from the new ones are kept as <file>.bak-<date> (for scripts/, in scripts-backup/).
# The layout of earlier versions (loose scripts, profiles in mpv.conf, keys in input.conf, ~~/gyroflow) is moved
# over to the plugins.
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
[ -d "$APPS_DIR/$PLAYER_NAME.app" ] && apps=1

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
DLOGM="$MPV_DIR/scripts/dlogm"
GYROFLOW="$MPV_DIR/scripts/gyroflow"
stamp=$(date +%Y%m%d-%H%M%S)
# Not next to scripts: mpv would try to load scripts/*.bak-* as scripts
backup_path() {
    case "$1" in
        "$MPV_DIR/scripts/"*) mkdir -p "$MPV_DIR/scripts-backup"; local rel=${1#"$MPV_DIR/scripts/"}; echo "$MPV_DIR/scripts-backup/${rel//\//-}.bak-$stamp" ;;
        *) echo "$1.bak-$stamp" ;;
    esac
}
# `install` writes a new file: copying over a loaded dylib keeps its inode, and macOS then kills whatever loads
# it (the kernel has the old code signature cached for that file)
put() {
    local src=$1 dst=$2 mode=$3
    mkdir -p "$(dirname "$dst")"
    if [ -e "$dst" ] && ! cmp -s "$src" "$dst"; then
        local bak
        bak=$(backup_path "$dst")
        cp -p "$dst" "$bak"
        echo "   kept the previous $dst as $bak"
    fi
    install -m "$mode" "$src" "$dst"
}

# Earlier versions: loose scripts, the D-Log M profiles in mpv.conf and their keys in input.conf, and the rest in
# ~~/gyroflow, ~~/shaders and ~~/luts. The plugins replace all of it
for f in dlog_auto.lua gyroflow_auto.lua gyroflow_export.lua; do
    if [ -e "$MPV_DIR/scripts/$f" ]; then
        bak=$(backup_path "$MPV_DIR/scripts/$f")
        mv "$MPV_DIR/scripts/$f" "$bak"
        echo "   moved the earlier $f to $bak (the plugins replace it)"
    fi
done
if [ -f "$MPV_DIR/gyroflow/default.gyroflow" ] && [ ! -e "$GYROFLOW/default.gyroflow" ]; then
    mkdir -p "$GYROFLOW"
    mv "$MPV_DIR/gyroflow/default.gyroflow" "$GYROFLOW/default.gyroflow"
    echo "   moved your default preset to $GYROFLOW/default.gyroflow"
fi
remove_ours() {
    local dir=$1; shift
    [ -d "$dir" ] || return 0
    for f in "$@"; do rm -f "$dir/$f" "$dir/$f".bak-*; done
    rmdir "$dir" 2>/dev/null || echo "   left $dir: there's more in it than this setup put there"
}
remove_ours "$MPV_DIR/gyroflow" gyroflow.vpy libvs_gyroflow.dylib gyroflow-probe gyroflow-embed gyroflow-export \
    gyroflow-router export-finder.sh
remove_ours "$MPV_DIR/shaders" dlogm_lut3d.glsl dlogm_hlg.glsl
remove_ours "$MPV_DIR/luts" dlogm_hlg.cube dji-dlogm-rec709.cube
# The keys the earlier input.conf had (the plugins bind them now): out of the user's input.conf, the rest kept
if grep -q 'apply-profile dji_sdr' "$MPV_DIR/input.conf" 2>/dev/null; then
    bak=$(backup_path "$MPV_DIR/input.conf")
    cp -p "$MPV_DIR/input.conf" "$bak"
    grep -vxF -f <(cat <<'OLD'
# Auto-detection (scripts/dlog_auto.lua) handles 99% of cases on file-load.
# These manual overrides are escape hatches:
Ctrl+s   apply-profile dji_sdr
Ctrl+h   apply-profile dji_hdr
# Shift+L: turn off D-Log handling entirely -- for files named *_D.MP4 that
# aren't actually D-Log (e.g. DJI's HDR Live Photo bursts). Clears the shader,
# resets target-* to auto so mpv just plays the file natively.
L        no-osd set glsl-shaders ""; set image-lut ""; set target-trc auto; set target-prim auto; set target-peak auto; set target-colorspace-hint auto; show-text "D-Log shader OFF — playing file natively" 2000
OLD
    ) "$bak" > "$MPV_DIR/input.conf" || true
    grep -q '[^[:space:]]' "$MPV_DIR/input.conf" || rm "$MPV_DIR/input.conf"
    echo "   took the D-Log M keys out of input.conf (the plugin binds them); the previous one is $bak"
fi
# mpv.conf: ours when there's none, or in place of the one an earlier version installed (with the D-Log M profiles)
if [ ! -e "$MPV_DIR/mpv.conf" ] || grep -q '^\[dji_hdr\]' "$MPV_DIR/mpv.conf"; then
    put mpv/mpv.conf "$MPV_DIR/mpv.conf" 644
fi

# The dlogm plugin
put mpv/scripts/dlogm/main.lua      "$DLOGM/main.lua" 644
put mpv/scripts/dlogm/dji_color.lua "$DLOGM/dji_color.lua" 644
put "$BUILD/dlogm_hlg.glsl"         "$DLOGM/shaders/dlogm_hlg.glsl" 644
put "$BUILD/dlogm_lut3d.glsl"       "$DLOGM/shaders/dlogm_lut3d.glsl" 644
# The LUTs themselves, for gyroflow-export
put luts/dlogm_hlg.cube             "$DLOGM/luts/dlogm_hlg.cube" 644
put "$lut"                          "$DLOGM/luts/dji-dlogm-rec709.cube" 644

# The gyroflow plugin
put mpv/scripts/gyroflow/main.lua    "$GYROFLOW/main.lua" 644
put mpv/scripts/gyroflow/export.lua  "$GYROFLOW/export.lua" 644
put mpv/scripts/gyroflow/gyroflow.vpy "$GYROFLOW/gyroflow.vpy" 644
# The default preset for clips without a project: underwater. Only if there's none yet, it's yours to change
if [ ! -e "$GYROFLOW/default.gyroflow" ]; then
    install -m 644 mpv/scripts/gyroflow/underwater.gyroflow "$GYROFLOW/default.gyroflow"
fi
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-embed gyroflow-export; do
    put "$BIN/$f" "$GYROFLOW/$f" 755
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
    put "$BUILD/gyroflow-router" "$GYROFLOW/gyroflow-router" 755
    put macos/export-finder.sh "$GYROFLOW/export-finder.sh" 755

    # Finder Quick Actions (right click on videos)
    python3 -I tools/make_workflow.py "$SERVICES_DIR/Play Stabilized.workflow" "Play Stabilized" NSTouchBarPlayTemplate \
        "open -b $PLAYER_ID \"\$@\""
    python3 -I tools/make_workflow.py "$SERVICES_DIR/Stabilize + LUT.workflow" "Stabilize + LUT" NSTouchBarExitFullScreenTemplate \
        "\"\$HOME/.config/mpv/scripts/gyroflow/export-finder.sh\" \"\$@\""

    if [ "$APPS_DIR" = "$HOME/Applications" ]; then
        "$lsregister" -f "$player" "$router"
        /System/Library/CoreServices/pbs -flush >/dev/null 2>&1 || true
    fi
    echo "   $PLAYER_NAME is in Finder's Open With for MP4/MOV; Quick Actions: Play Stabilized, Stabilize + LUT"
    echo "   Router mode (the default player, sending other videos to your usual one): $GYROFLOW/gyroflow-router on"
fi

echo "Done."
