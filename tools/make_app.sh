#!/bin/bash
# tools/make_app.sh <out.app> <binaries> <dlogm_hlg.glsl> [<version>]: builds ActionFootagePlayer.app, with the three mpv
# plugins in it. install.sh --apps and tools/make_release.sh use it.
#   <binaries>       a folder with libvs_gyroflow.dylib, gyroflow-probe, gyroflow-embed, gyroflow-export
#   <dlogm_hlg.glsl> the HDR shader, baked by tools/cube_to_glsl.py
#
#   Contents/MacOS/ActionFootagePlayer   the app: mpv with the plugins (macos/player.sh), offering the setup at
#                                        first launch
#   Contents/Resources/AppIcon.icns      the icon (macos/icon/, made by tools/make_icon.sh)
#   Contents/Resources/plugins/          the plugins: dlogm (without DJI's LUT, which isn't redistributed: the setup
#                                        downloads it), gyroflow, with the underwater preset as default.gyroflow,
#                                        dji-telemetry, and footage-menu (the right-click menu)
#   Contents/Resources/setup.sh          the setup (offered at first launch by setup-offer.lua), and the export script
#                                        of the Stabilize + LUT Quick Action
#   Contents/Resources/tools/            what the setup runs
#   Contents/Helpers/, Frameworks/,      with BUNDLE_RUNTIME: mpv, ffmpeg and what they load, and their licenses and
#     Resources/ThirdParty/              sources (tools/bundle_runtime.py)
#
# With BUNDLE_RUNTIME set to Homebrew's prefix (`brew --prefix`), the app also gets Homebrew's mpv and ffmpeg and
# what they load, Python and VapourSynth among it, with their licenses (tools/bundle_runtime.py): it then needs no
# Homebrew, and the macOS the bottles were built for. Without it, the app runs Homebrew's mpv.
#
# Signed ad hoc; the app is read-only once signed (what's yours lives in ~/Library/Application Support/ActionFootagePlayer).
set -euo pipefail
cd "$(dirname "$0")/.."

app=${1:?usage: tools/make_app.sh <out.app> <binaries> <dlogm_hlg.glsl> [<version>]}
bin=${2:?}
hlg=${3:?}
version=${4:-dev}
PLAYER_ID="io.github.anton-belkin.action-footage-player"

rm -rf "$app"
res="$app/Contents/Resources"
mkdir -p "$app/Contents/MacOS" "$res"
install -m 755 macos/player.sh "$app/Contents/MacOS/ActionFootagePlayer"
install -m 644 macos/icon/AppIcon.icns "$res/AppIcon.icns"
# Video files, at rank Alternate: listed in Open With, the default only when made so (setup.sh default)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>ActionFootagePlayer</string>
    <key>CFBundleIdentifier</key><string>$PLAYER_ID</string>
    <key>CFBundleName</key><string>ActionFootagePlayer</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>Video file</string>
            <key>CFBundleTypeRole</key><string>Viewer</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>LSItemContentTypes</key>
            <array><string>public.mpeg-4</string><string>com.apple.quicktime-movie</string></array>
        </dict>
    </array>
</dict>
</plist>
PLIST

dlogm="$res/plugins/dlogm"
gyroflow="$res/plugins/gyroflow"
telemetry="$res/plugins/dji-telemetry"
menu="$res/plugins/footage-menu"
mkdir -p "$dlogm/shaders" "$dlogm/luts" "$gyroflow" "$telemetry" "$menu"
install -m 644 mpv/scripts/dlogm/main.lua mpv/scripts/dlogm/dji_color.lua mpv/scripts/dlogm/gopro_color.lua "$dlogm/"
install -m 644 "$hlg" "$dlogm/shaders/dlogm_hlg.glsl"
install -m 644 luts/dlogm_hlg.cube "$dlogm/luts/dlogm_hlg.cube"
# The other log curves' HDR shaders, from their makers' published formulas (tools/make_hdr_lut.py)
cubes=$(mktemp -d)
for curve in dlog gplog2 ilog; do
    python3 -I tools/make_hdr_lut.py "$curve" "$cubes/$curve.cube" >/dev/null
    python3 -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in "$cubes/$curve.cube" "$dlogm/shaders/${curve}_hlg.glsl" >/dev/null
done
rm -rf "$cubes"
python3 -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in luts/gplog_hlg.cube "$dlogm/shaders/gplog_hlg.glsl" >/dev/null
install -m 644 mpv/scripts/gyroflow/main.lua mpv/scripts/gyroflow/export.lua mpv/scripts/gyroflow/platform.lua \
    mpv/scripts/gyroflow/gyroflow.vpy "$gyroflow/"
install -m 644 mpv/scripts/gyroflow/underwater.gyroflow "$gyroflow/default.gyroflow"
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-embed gyroflow-export; do
    install -m 755 "$bin/$f" "$gyroflow/$f"
done
install -m 644 mpv/scripts/dji-telemetry/main.lua mpv/scripts/dji-telemetry/telemetry.lua "$telemetry/"
install -m 644 mpv/scripts/footage-menu/main.lua "$menu/"

install -m 755 macos/setup.sh macos/export-finder.sh "$res/"
install -m 644 macos/setup-offer.lua "$res/"
mkdir -p "$res/tools"
install -m 644 tools/cube_to_glsl.py tools/make_workflow.py macos/default-app.py shaders/dlogm_lut3d.glsl.in "$res/tools/"

if [ -n "${BUNDLE_RUNTIME:-}" ]; then
    minos=$(python3 -I tools/bundle_runtime.py "$app" "$BUNDLE_RUNTIME")
    /usr/libexec/PlistBuddy -c "Add :LSMinimumSystemVersion string $minos" "$app/Contents/Info.plist"
fi

# Inside out: the binaries, then the app, which seals the rest. codesign's errors shown, not its "replacing
# existing signature"
sign() { codesign --force -s - "$1" 2>&1 | { grep -v "replacing existing signature" >&2 || true; }; return "${PIPESTATUS[0]}"; }
for f in "$gyroflow/libvs_gyroflow.dylib" "$gyroflow/gyroflow-probe" "$gyroflow/gyroflow-embed" "$gyroflow/gyroflow-export"; do
    sign "$f"
done
sign "$app"
