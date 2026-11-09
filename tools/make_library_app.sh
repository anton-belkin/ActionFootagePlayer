#!/bin/bash
# tools/make_library_app.sh <out.app> [<version>]: builds ActionFootageLibrary.app (library/, Tauri), the companion app
# that makes a trip's clip previews, moments, photos and exports in the background, with ActionFootagePlayer run
# without a window. tools/make_release.sh and tools/dev_app.sh use it; it goes beside ActionFootagePlayer.app, where
# it looks for the player first.
#
#   Contents/MacOS/ActionFootageLibrary   the app (cargo build --release in library/)
#   Contents/Resources/AppIcon.icns       its icon, and Project.afproject's
#   Contents/Info.plist                   it opens Project.afproject: the type is declared here (an exported UTI,
#                                         io.github.anton-belkin.action-footage-project), and this app is its owner
#
# LIBRARY_BIN: its binary built already (a release's bin/ActionFootageLibrary), else cargo builds it. Signed ad hoc, as
# the player.
set -euo pipefail
cd "$(dirname "$0")/.."

app=${1:?usage: tools/make_library_app.sh <out.app> [<version>]}
version=${2:-dev}
LIBRARY_ID="io.github.anton-belkin.action-footage-library"
PROJECT_UTI="io.github.anton-belkin.action-footage-project"

bin=${LIBRARY_BIN:-library/target/release/ActionFootageLibrary}
if [ -z "${LIBRARY_BIN:-}" ]; then
    command -v cargo >/dev/null || [ ! -x /opt/homebrew/opt/rustup/bin/cargo ] || PATH="/opt/homebrew/opt/rustup/bin:$PATH"
    (cd library && cargo build --release)
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 755 "$bin" "$app/Contents/MacOS/ActionFootageLibrary"
# Its Quick Look (the system's panel: qlmanage -p crashes on videos on macOS 27)
swiftc -O -target arm64-apple-macos12 macos/quicklook.swift -o "$app/Contents/MacOS/afp-quicklook"
install -m 644 library/icons/icon.icns "$app/Contents/Resources/AppIcon.icns"
# Tauri's own version is the crate's: the app shows the player's, which ships with it
plist_version=$(echo "$version" | sed -E 's/^([0-9]+(\.[0-9]+)*).*/\1/')
case "$plist_version" in [0-9]*) ;; *) plist_version=0.0.0 ;; esac
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>ActionFootageLibrary</string>
    <key>CFBundleIdentifier</key><string>$LIBRARY_ID</string>
    <key>CFBundleName</key><string>ActionFootageLibrary</string>
    <key>CFBundleDisplayName</key><string>Action Footage Library</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleShortVersionString</key><string>$plist_version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>UTExportedTypeDeclarations</key>
    <array>
        <dict>
            <key>UTTypeIdentifier</key><string>$PROJECT_UTI</string>
            <key>UTTypeDescription</key><string>Action Footage project</string>
            <key>UTTypeConformsTo</key><array><string>public.json</string><string>public.data</string></array>
            <key>UTTypeIconFile</key><string>AppIcon</string>
            <key>UTTypeTagSpecification</key>
            <dict><key>public.filename-extension</key><array><string>afproject</string></array></dict>
        </dict>
    </array>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>Action Footage project</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Owner</string>
            <key>CFBundleTypeIconFile</key><string>AppIcon</string>
            <key>LSItemContentTypes</key><array><string>$PROJECT_UTI</string></array>
        </dict>
    </array>
</dict>
</plist>
PLIST
codesign --force -s - "$app/Contents/MacOS/afp-quicklook"
codesign --force -s - "$app" 2>&1 | { grep -v "replacing existing signature" >&2 || true; }
