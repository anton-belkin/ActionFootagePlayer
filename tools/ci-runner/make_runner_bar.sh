#!/bin/bash
# tools/ci-runner/make_runner_bar.sh [<out.app>]: builds AFP Runner (RunnerBar.swift), the menu bar app for the GitHub
# runner on Anton's Mac, and installs it (default ~/Applications/AFP Runner.app). Seconds with swiftc; no Xcode project.
set -euo pipefail
cd "$(dirname "$0")"
app=${1:-"$HOME/Applications/AFP Runner.app"}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/AFP Runner.app/Contents/MacOS"
swiftc -O -parse-as-library -target arm64-apple-macos14 RunnerBar.swift -o "$tmp/AFP Runner.app/Contents/MacOS/AFPRunner"
cat > "$tmp/AFP Runner.app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>io.github.anton-belkin.afp-runner</string>
    <key>CFBundleName</key><string>AFP Runner</string>
    <key>CFBundleExecutable</key><string>AFPRunner</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>
EOF
codesign --force -s - "$tmp/AFP Runner.app"
pkill -x AFPRunner 2>/dev/null || true
rm -rf "$app"
mkdir -p "$(dirname "$app")"
ditto "$tmp/AFP Runner.app" "$app"
echo "$app"
