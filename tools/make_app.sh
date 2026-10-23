#!/bin/bash
# tools/make_app.sh <out.app> <binaries> <dlogm_hlg.glsl> [<version>]: builds Steady Player.app, with the three mpv
# plugins in it. install.sh --apps and tools/make_release.sh use it.
#   <binaries>       a folder with libvs_gyroflow.dylib, gyroflow-probe, gyroflow-embed, gyroflow-export
#   <dlogm_hlg.glsl> the HDR shader, baked by tools/cube_to_glsl.py
#
#   Contents/MacOS, Resources/Scripts   the applet (macos/player.applescript)
#   Contents/Resources/mpv.app           the launcher that starts mpv as an app of its own (macos/mpv-launcher.sh)
#   Contents/Resources/plugins/          the plugins: dlogm (without DJI's LUT, which isn't redistributed: the setup
#                                        downloads it), gyroflow, with the underwater preset as default.gyroflow,
#                                        and dji-telemetry
#   Contents/Resources/setup.sh          the setup, and the export script of the Stabilize + LUT Quick Action
#   Contents/Resources/tools/            what the setup runs
#
# Signed ad hoc; the app is read-only once signed (what's yours lives in ~/Library/Application Support/Steady Player).
set -euo pipefail
cd "$(dirname "$0")/.."

app=${1:?usage: tools/make_app.sh <out.app> <binaries> <dlogm_hlg.glsl> [<version>]}
bin=${2:?}
hlg=${3:?}
version=${4:-dev}
PLAYER_ID="io.github.anton-belkin.steady-player"

rm -rf "$app"
osacompile -o "$app" macos/player.applescript
res="$app/Contents/Resources"

launcher="$res/mpv.app"
mkdir -p "$launcher/Contents/MacOS"
install -m 755 macos/mpv-launcher.sh "$launcher/Contents/MacOS/mpv"
cat > "$launcher/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>mpv</string>
    <key>CFBundleIdentifier</key><string>$PLAYER_ID.mpv</string>
    <key>CFBundleName</key><string>mpv</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

dlogm="$res/plugins/dlogm"
gyroflow="$res/plugins/gyroflow"
telemetry="$res/plugins/dji-telemetry"
mkdir -p "$dlogm/shaders" "$dlogm/luts" "$gyroflow" "$telemetry"
install -m 644 mpv/scripts/dlogm/main.lua mpv/scripts/dlogm/dji_color.lua "$dlogm/"
install -m 644 "$hlg" "$dlogm/shaders/dlogm_hlg.glsl"
install -m 644 luts/dlogm_hlg.cube "$dlogm/luts/dlogm_hlg.cube"
install -m 644 mpv/scripts/gyroflow/main.lua mpv/scripts/gyroflow/export.lua mpv/scripts/gyroflow/platform.lua \
    mpv/scripts/gyroflow/gyroflow.vpy "$gyroflow/"
install -m 644 mpv/scripts/gyroflow/underwater.gyroflow "$gyroflow/default.gyroflow"
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-embed gyroflow-export; do
    install -m 755 "$bin/$f" "$gyroflow/$f"
done
install -m 644 mpv/scripts/dji-telemetry/main.lua mpv/scripts/dji-telemetry/telemetry.lua "$telemetry/"

install -m 755 macos/setup.sh macos/export-finder.sh "$res/"
mkdir -p "$res/tools"
install -m 644 tools/cube_to_glsl.py tools/make_workflow.py macos/default-app.py shaders/dlogm_lut3d.glsl.in "$res/tools/"

plist="$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $PLAYER_ID" "$plist" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string $PLAYER_ID" "$plist"
/usr/libexec/PlistBuddy -c "Delete :CFBundleShortVersionString" "$plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string $version" "$plist"
# Video files, at rank Alternate: listed in Open With, the default only when made so (setup.sh default)
/usr/libexec/PlistBuddy -c "Delete :CFBundleDocumentTypes" "$plist" 2>/dev/null || true
/usr/libexec/PlistBuddy \
    -c "Add :CFBundleDocumentTypes array" \
    -c "Add :CFBundleDocumentTypes:0 dict" \
    -c "Add :CFBundleDocumentTypes:0:CFBundleTypeName string Video file" \
    -c "Add :CFBundleDocumentTypes:0:CFBundleTypeRole string Viewer" \
    -c "Add :CFBundleDocumentTypes:0:LSHandlerRank string Alternate" \
    -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes array" \
    -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:0 string public.mpeg-4" \
    -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:1 string com.apple.quicktime-movie" \
    "$plist"

# Inside out: the binaries, the launcher, then the app, which seals the rest
for f in "$gyroflow/libvs_gyroflow.dylib" "$gyroflow/gyroflow-probe" "$gyroflow/gyroflow-embed" "$gyroflow/gyroflow-export"; do
    codesign --force -s - "$f" 2>/dev/null
done
codesign --force -s - "$launcher" 2>/dev/null
codesign --force -s - "$app" 2>/dev/null
