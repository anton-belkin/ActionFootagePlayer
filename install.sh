#!/bin/bash
# Installs the mpv plugins into mpv's config directory. Re-run it after pulling changes.
#
#   ./install.sh           the plugins: scripts/dlogm (D-Log M and other log curves; its shaders are baked here, with
#                          DJI's LUT downloaded from DJI), scripts/gyroflow (stabilization, with the binaries built
#                          from vs-gyroflow/), scripts/dji-telemetry (the camera's telemetry over the video) and
#                          scripts/footage-menu (their right-click menu, mpv 0.41 and later); and mpv.conf when there's
#                          none yet
#   ./install.sh --apps    also ActionFootagePlayer.app in ~/Applications (the plugins in it, tools/make_app.sh), set up as
#                          the default app for MP4 videos, with the Finder Quick Actions (see README). On Linux,
#                          ActionFootagePlayer in the applications menu instead: the system's mpv with the plugins
#                          installed here, by its own name and icon, the default app for MP4 videos. Installed once,
#                          it's updated by every later run
#
# On Linux it installs the same plugins (the binaries built for Linux).
#
# Files it replaces that differ from the new ones are kept as <file>.bak-<date> (for scripts/, in scripts-backup/).
# The layout of earlier versions (loose scripts, profiles in mpv.conf, keys in input.conf, ~~/gyroflow) is moved
# over to the plugins, and Steady Router (0.2 and earlier) is removed: ActionFootagePlayer is the default player itself.
# MPV_CONFIG_DIR, APPS_DIR and SERVICES_DIR override where it installs (default ~/.config/mpv, ~/Applications and
# ~/Library/Services); on Linux, XDG_DATA_HOME where the menu entry goes (default ~/.local/share).
set -euo pipefail
cd "$(dirname "$0")"

MPV_DIR="${MPV_CONFIG_DIR:-$HOME/.config/mpv}"
APPS_DIR="${APPS_DIR:-$HOME/Applications}"
SERVICES_DIR="${SERVICES_DIR:-$HOME/Library/Services}"
# The working name; the bundle id is what macOS and the settings go by (tools/make_app.sh sets it)
PLAYER_NAME="ActionFootagePlayer"
PLAYER_ID="io.github.anton-belkin.action-footage-player"
# The app's name up to 0.3: it's replaced (what's yours moves over in setup.sh)
OLD_PLAYER_NAME="Steady Player"
OLD_PLAYER_ID="io.github.anton-belkin.steady-player"
# Steady Router, of 0.2 and earlier, and the ids it had before
ROUTER_NAME="Steady Router"
ROUTER_IDS="io.github.anton-belkin.steady-router com.antonbelkin.dji-router io.github.dji-router"
BUILD=build
# The log curves besides D-Log M with an HDR shader, from tools/make_hdr_lut.py (the dlogm plugin's CURVES)
HDR_CURVES="dlog gplog2 ilog"

macos=0
[ "$(uname -s)" = Darwin ] && macos=1
# The gyroflow plugin's binaries, as cargo names them here
if [ $macos = 1 ]; then LIBRARY=libvs_gyroflow.dylib; else LIBRARY=libvs_gyroflow.so; fi
BINARIES="$LIBRARY gyroflow-probe gyroflow-embed gyroflow-export"

apps=0
for arg in "$@"; do
    case "$arg" in
        --apps) apps=1 ;;
        *) echo "usage: $0 [--apps]" >&2; exit 2 ;;
    esac
done
# Linux's ActionFootagePlayer: a menu entry, named by the bundle id as Wayland wants it (mpv's window has it as its
# app id, which is how the desktop finds the entry's name and icon for it)
XDG_DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_FILE="$XDG_DATA/applications/$PLAYER_ID.desktop"
if [ $macos = 1 ]; then
    [ -d "$APPS_DIR/$PLAYER_NAME.app" ] || [ -d "$APPS_DIR/$OLD_PLAYER_NAME.app" ] && apps=1
else
    [ -f "$DESKTOP_FILE" ] && apps=1
fi

need() { command -v "$1" >/dev/null || { echo "Missing $1 ($2)" >&2; exit 1; }; }
if [ $macos = 1 ]; then
    need mpv "brew install mpv"
    need python3 "Xcode command line tools"
else
    need mpv "your distribution's mpv, built with VapourSynth for the stabilization"
    need python3 "your distribution's python3"
    need curl "your distribution's curl, to download DJI's LUT"
fi
# A release download has the binaries prebuilt in bin/; a git checkout builds them
if [ -d bin ]; then
    BIN=bin
    # Downloaded with a browser, they carry the quarantine flag, and macOS would refuse to run them
    xattr -dr com.apple.quarantine bin 2>/dev/null || true
else
    BIN=vs-gyroflow/target/release
    need cargo "https://rustup.rs, or download a release, which has the binaries prebuilt"
fi
command -v ffmpeg >/dev/null || echo "Warning: no ffmpeg ($([ $macos = 1 ] && echo "brew install ffmpeg" || echo "install your distribution's")); exporting needs it" >&2
if ! grep -q vapoursynth <<< "$(mpv --vf=help 2>/dev/null)"; then
    echo "Warning: this mpv has no VapourSynth filter; videos will play, but not stabilized" >&2
fi

mkdir -p "$BUILD"

echo "== DJI D-Log M LUT"
# Downloaded from DJI and checked (the URL and checksum are in setup.sh, which the app uses for the same)
lut="$BUILD/dji-dlogm-rec709.cube"
sh macos/setup.sh fetch-lut "$lut"

echo "== Shaders"
python3 -I tools/cube_to_glsl.py shaders/dlogm_lut3d.glsl.in "$lut" "$BUILD/dlogm_lut3d.glsl"
python3 -I tools/cube_to_glsl.py shaders/dlogm_hlg.glsl.in luts/dlogm_hlg.cube "$BUILD/dlogm_hlg.glsl"
# The other log curves' HDR LUTs, made from their makers' published formulas
for curve in $HDR_CURVES; do
    python3 -I tools/make_hdr_lut.py "$curve" "$BUILD/${curve}_hlg.cube"
    python3 -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in "$BUILD/${curve}_hlg.cube" "$BUILD/${curve}_hlg.glsl"
done
# GP-Log's, from Zeb Gardner's LUT (luts/gplog_hlg.cube)
python3 -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in luts/gplog_hlg.cube "$BUILD/gplog_hlg.glsl"

if [ "$BIN" != bin ]; then
    echo "== Gyroflow plugin"
    (cd vs-gyroflow && cargo build --release)
fi

echo "== Installing into $MPV_DIR"
DLOGM="$MPV_DIR/scripts/dlogm"
GYROFLOW="$MPV_DIR/scripts/gyroflow"
TELEMETRY="$MPV_DIR/scripts/dji-telemetry"
MENU="$MPV_DIR/scripts/footage-menu"
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
remove_ours "$MPV_DIR/gyroflow" gyroflow.vpy $BINARIES \
    gyroflow-router export-finder.sh
remove_ours "$MPV_DIR/shaders" dlogm_lut3d.glsl dlogm_hlg.glsl
remove_ours "$MPV_DIR/luts" dlogm_hlg.cube dji-dlogm-rec709.cube
# The keys the earlier input.conf had (the plugins bind them now): out of the user's input.conf, the rest kept
if grep -q 'apply-profile dji_sdr' "$MPV_DIR/input.conf" 2>/dev/null; then
    bak=$(backup_path "$MPV_DIR/input.conf")
    cp -p "$MPV_DIR/input.conf" "$bak"
    # Read here, not in a heredoc inside <(...): macOS's bash 3.2 misparses that (the quote in "aren't"), grep
    # then gets no patterns file, and all of input.conf went
    IFS= read -r -d '' old_keys <<'OLD' || true
# Auto-detection (scripts/dlog_auto.lua) handles 99% of cases on file-load.
# These manual overrides are escape hatches:
Ctrl+s   apply-profile dji_sdr
Ctrl+h   apply-profile dji_hdr
# Shift+L: turn off D-Log handling entirely -- for files named *_D.MP4 that
# aren't actually D-Log (e.g. DJI's HDR Live Photo bursts). Clears the shader,
# resets target-* to auto so mpv just plays the file natively.
L        no-osd set glsl-shaders ""; set image-lut ""; set target-trc auto; set target-prim auto; set target-peak auto; set target-colorspace-hint auto; show-text "D-Log shader OFF — playing file natively" 2000
OLD
    # grep exits 1 when no line is left (input.conf had only these), 2 on an error: then input.conf stays as it was
    rc=0
    grep -vxF -f <(printf '%s' "$old_keys") "$bak" > "$MPV_DIR/input.conf" || rc=$?
    if [ $rc -gt 1 ]; then
        cp -p "$bak" "$MPV_DIR/input.conf"
        echo "Couldn't take the D-Log M keys out of $MPV_DIR/input.conf; it's as it was" >&2
        exit 1
    fi
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
put mpv/scripts/dlogm/gopro_color.lua "$DLOGM/gopro_color.lua" 644
put mpv/scripts/dlogm/video_color.lua "$DLOGM/video_color.lua" 644
put "$BUILD/dlogm_hlg.glsl"         "$DLOGM/shaders/dlogm_hlg.glsl" 644
for curve in $HDR_CURVES gplog; do
    put "$BUILD/${curve}_hlg.glsl"  "$DLOGM/shaders/${curve}_hlg.glsl" 644
done
put "$BUILD/dlogm_lut3d.glsl"       "$DLOGM/shaders/dlogm_lut3d.glsl" 644
put shaders/denoise.glsl            "$DLOGM/shaders/denoise.glsl" 644
# The LUTs themselves, for gyroflow-export
put luts/dlogm_hlg.cube             "$DLOGM/luts/dlogm_hlg.cube" 644
put luts/gplog_hlg.cube             "$DLOGM/luts/gplog_hlg.cube" 644
for curve in $HDR_CURVES; do
    put "$BUILD/${curve}_hlg.cube"  "$DLOGM/luts/${curve}_hlg.cube" 644
done
put "$lut"                          "$DLOGM/luts/dji-dlogm-rec709.cube" 644

# The gyroflow plugin
put mpv/scripts/gyroflow/main.lua    "$GYROFLOW/main.lua" 644
put mpv/scripts/gyroflow/export.lua  "$GYROFLOW/export.lua" 644
put mpv/scripts/gyroflow/platform.lua "$GYROFLOW/platform.lua" 644
put mpv/scripts/gyroflow/gyroflow.vpy "$GYROFLOW/gyroflow.vpy" 644
# The default preset for clips without a project: underwater. Only if there's none yet, it's yours to change
if [ ! -e "$GYROFLOW/default.gyroflow" ]; then
    install -m 644 mpv/scripts/gyroflow/underwater.gyroflow "$GYROFLOW/default.gyroflow"
fi
for f in $BINARIES; do
    [ -f "$BIN/$f" ] || { echo "$BIN/$f is missing: a release for another system? (tools/make_release.sh, tools/make_plugin_zip.sh)" >&2; exit 1; }
    put "$BIN/$f" "$GYROFLOW/$f" 755
done

# The telemetry plugin
put mpv/scripts/dji-telemetry/main.lua      "$TELEMETRY/main.lua" 644
put mpv/scripts/dji-telemetry/telemetry.lua "$TELEMETRY/telemetry.lua" 644

# The right-click menu
put mpv/scripts/footage-menu/main.lua "$MENU/main.lua" 644

# 0.2 had Steady Router's script and the Quick Action's in the gyroflow plugin; ActionFootagePlayer carries them now
for f in gyroflow-router export-finder.sh; do
    rm -f "$GYROFLOW/$f"
done

if [ $apps = 1 ] && [ $macos = 0 ]; then
    echo "== $PLAYER_NAME in the applications menu"
    # mpv's own: the plugins are in its config. Older mpv (before 0.35) has no --wayland-app-id, and would refuse to
    # start with it
    window="--x11-name=$PLAYER_ID"
    mpv --list-options 2>/dev/null | grep -q -- '--wayland-app-id' && window="--wayland-app-id=$PLAYER_ID $window"
    icon="$XDG_DATA/icons/hicolor/scalable/apps/$PLAYER_ID.svg"
    mkdir -p "$(dirname "$icon")" "$(dirname "$DESKTOP_FILE")"
    install -m 644 macos/icon/AppIcon.svg "$icon"
    cat > "$DESKTOP_FILE.part" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$PLAYER_NAME
GenericName=Video Player
Comment=Action camera footage in full color, stabilized, with what the camera measured
Icon=$PLAYER_ID
Exec=mpv --player-operation-mode=pseudo-gui $window -- %F
Terminal=false
Categories=AudioVideo;Video;Player;
MimeType=video/mp4;video/quicktime;
StartupWMClass=$PLAYER_ID
DESKTOP
    mv "$DESKTOP_FILE.part" "$DESKTOP_FILE"
    command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$DESKTOP_FILE")" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$XDG_DATA/icons/hicolor" 2>/dev/null || true
    echo "   $DESKTOP_FILE"
    # Only for the real one: an install elsewhere (a test) leaves the system's defaults alone
    if [ -z "${XDG_DATA_HOME:-}" ] && command -v xdg-mime >/dev/null; then
        xdg-mime default "$PLAYER_ID.desktop" video/mp4 && echo "   $PLAYER_NAME is the default app for MP4 videos"
    fi
fi

if [ $apps = 1 ] && [ $macos = 1 ]; then
    echo "== $PLAYER_NAME in $APPS_DIR"
    mkdir -p "$APPS_DIR"
    player="$APPS_DIR/$PLAYER_NAME.app"
    # A release has its version in VERSION
    version=$(cat VERSION 2>/dev/null || git describe --tags --always 2>/dev/null || echo local)
    tools/make_app.sh "$player" "$BIN" "$BUILD/dlogm_hlg.glsl" "$version"
    old_player="$APPS_DIR/$OLD_PLAYER_NAME.app"
    if [ -d "$old_player" ]; then
        /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$old_player" 2>/dev/null || true
        rm -rf "$old_player"
        echo "   removed $OLD_PLAYER_NAME (it's $PLAYER_NAME now)"
    fi
    setup="$player/Contents/Resources/setup.sh"
    "$setup" lut "$lut" | sed 's/^/   /'
    SERVICES_DIR="$SERVICES_DIR" "$setup" quick-actions | sed 's/^/   /'
    # Only for the real Applications folder: an install elsewhere (a test) leaves the system's defaults alone
    if [ "$APPS_DIR" = "$HOME/Applications" ] || [ "$APPS_DIR" = /Applications ]; then
        "$setup" default | sed 's/^/   /'
        # Steady Router: gone, and MOV back to the player it was taking videos from (MP4 is ActionFootagePlayer's now)
        router="$APPS_DIR/$ROUTER_NAME.app"
        if [ -d "$router" ]; then
            /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$router" 2>/dev/null || true
            rm -rf "$router"
            echo "   removed $ROUTER_NAME (ActionFootagePlayer is the default player itself now)"
        fi
        mov=com.apple.quicktime-movie
        case " $ROUTER_IDS " in
            *" $(python3 -I macos/default-app.py $mov) "*)
                previous=$(defaults read "$PLAYER_ID" "previous-$mov" 2>/dev/null || defaults read "$OLD_PLAYER_ID" "previous-$mov" 2>/dev/null || true)
                case " $ROUTER_IDS $PLAYER_ID $OLD_PLAYER_ID " in *" ${previous:-none} "*) previous= ;; esac
                previous=${previous:-com.apple.QuickTimePlayerX}
                # macOS asks to confirm it; 3: not yet
                if python3 -I macos/default-app.py $mov "$previous" 2>/dev/null; then
                    echo "   MOV videos open with $previous again"
                else
                    echo "   MOV videos open with $previous again once you confirm it in macOS's dialog"
                fi ;;
        esac
        for key in fallback fallback-mp4 fallback-mov previous-public.mpeg-4 "previous-$mov"; do
            defaults delete "$PLAYER_ID" "$key" 2>/dev/null || true
        done
        defaults delete "$OLD_PLAYER_ID" 2>/dev/null || true
    fi
    echo "   A downloaded $PLAYER_NAME offers the same setup the first time it opens"
fi

echo "Done."
