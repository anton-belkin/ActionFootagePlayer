#!/bin/bash
# tools/make_plugin_zip.sh <version> [<rust target>]: into dist/
#   mpv-gyroflow-<version>-linux-x86_64.zip     the gyroflow plugin alone, with its binaries built for this system:
#   mpv-gyroflow-<version>-windows-x86_64.zip   unzipped into mpv's scripts/ folder (~/.config/mpv/scripts/,
#                                               %APPDATA%\mpv\scripts\)
#
# The Linux and Windows counterpart of the macOS plugin zip in tools/make_release.sh (which makes the rest of the
# release). Runs on Linux, and on Windows in Git Bash (.github/workflows/release.yml runs it on both). A Rust target
# cross-compiles, e.g. x86_64-pc-windows-gnu on Linux with mingw-w64.
# Built from HEAD's sources (refuses to run with uncommitted changes to them).
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: tools/make_plugin_zip.sh <version> [<rust target>]}
target=${2:-}
if ! git diff --quiet HEAD -- vs-gyroflow mpv/scripts/gyroflow; then
    echo "vs-gyroflow or the plugin has uncommitted changes; the zip would not match the tree" >&2
    exit 1
fi

case "${target:-$(uname -s)}" in
    *windows*|MINGW*|MSYS*|CYGWIN*) os=windows ;;
    *linux*|Linux) os=linux ;;
    *) echo "tools/make_plugin_zip.sh is for Linux and Windows; macOS's zip is tools/make_release.sh's" >&2; exit 1 ;;
esac
arch=$(case "$target" in "") uname -m ;; *) echo "${target%%-*}" ;; esac)
[ "$arch" = amd64 ] && arch=x86_64
if [ $os = windows ]; then
    files="vs_gyroflow.dll gyroflow-probe.exe gyroflow-embed.exe gyroflow-export.exe"
else
    files="libvs_gyroflow.so gyroflow-probe gyroflow-embed gyroflow-export"
fi

(cd vs-gyroflow && cargo build --release ${target:+--target "$target"})
bin="vs-gyroflow/target/${target:+$target/}release"

name="mpv-gyroflow-$version-$os-$arch"
rm -rf "dist/$name" "dist/$name.zip"
mkdir -p "dist/$name"
git archive HEAD mpv/scripts/gyroflow | tar -x --strip-components 2 -C "dist/$name"
mv "dist/$name/gyroflow/underwater.gyroflow" "dist/$name/gyroflow/default.gyroflow"
# Without the debug info the release profile keeps (the MSVC build has it in .pdb files, which aren't copied)
case "$os-$target" in
    linux-*) strip=strip ;;
    windows-*-gnu) strip=x86_64-w64-mingw32-strip ;;
    *) strip= ;;
esac
for f in $files; do
    cp "$bin/$f" "dist/$name/gyroflow/$f"
    chmod 755 "dist/$name/gyroflow/$f"
    [ -z "$strip" ] || "$strip" --strip-debug "dist/$name/gyroflow/$f"
done

# zip isn't in Git Bash; Python's keeps the executable bits too
py=$(command -v python3 || command -v python)
(cd "dist/$name" && "$py" -I -m zipfile -c "../$name.zip" gyroflow)
rm -rf "dist/$name"
ls -l "dist/$name.zip"
