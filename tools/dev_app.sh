#!/bin/bash
# tools/dev_app.sh [--no-install]: ActionFootagePlayer.app built from this folder the way the release builds it
# (tools/make_release.sh with BUNDLE_RUNTIME: with its own mpv, ffmpeg, Python and VapourSynth, Homebrew's), and put
# in ~/Applications in place of the one there, which is kept as build/ActionFootagePlayer-previous.app. For trying a
# branch or a fix without waiting for GitHub: a few minutes, most of it the first cargo build. Uncommitted changes
# are built too. mpv's own config (~/.config/mpv) isn't touched: the app doesn't read it.
#
# The version is the tag's when HEAD is one (0.6.2), else <the last tag>-dev.<commits since>+<commit>, with .dirty
# for uncommitted changes. The mpv, ffmpeg and VapourSynth in it are this Mac's Homebrew ones: `brew upgrade mpv
# ffmpeg` first to have what the release would (its build gets the latest).
#
#   --no-install   only build/ActionFootagePlayer.app
# APPS_DIR and SERVICES_DIR override where it installs (default ~/Applications and ~/Library/Services); elsewhere
# than ~/Applications or /Applications, it doesn't make the app the default for MP4 videos.
set -euo pipefail
cd "$(dirname "$0")/.."

install=1
for arg in "$@"; do
    case "$arg" in
        --no-install) install=0 ;;
        *) echo "usage: $0 [--no-install]" >&2; exit 2 ;;
    esac
done

APPS_DIR="${APPS_DIR:-$HOME/Applications}"
SERVICES_DIR="${SERVICES_DIR:-$HOME/Library/Services}"
BUILD=build
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# Apple's: another strip or install_name_tool earlier in PATH (Anaconda's cctools) leaves a broken signature
STRIP=$(xcrun -f strip 2>/dev/null || echo /usr/bin/strip)
INSTALL_NAME_TOOL=$(xcrun -f install_name_tool 2>/dev/null || echo /usr/bin/install_name_tool)

[ "$(uname -s)" = Darwin ] || { echo "tools/dev_app.sh builds the Mac app; elsewhere, see install.sh" >&2; exit 1; }
command -v brew >/dev/null || { echo "Missing Homebrew (https://brew.sh): the app carries its mpv and ffmpeg" >&2; exit 1; }
prefix=$(brew --prefix)
for formula in mpv ffmpeg; do
    [ -x "$prefix/bin/$formula" ] || { echo "Missing $formula: brew install mpv ffmpeg" >&2; exit 1; }
done
# Homebrew's rustup is keg-only: its cargo isn't in PATH by itself
command -v cargo >/dev/null || [ ! -x "$prefix/opt/rustup/bin/cargo" ] || PATH="$prefix/opt/rustup/bin:$PATH"
command -v cargo >/dev/null || { echo "Missing cargo (https://rustup.rs, or brew install rustup)" >&2; exit 1; }

tag=$(git describe --tags --abbrev=0 2>/dev/null || echo v0)
if [ "$(git rev-parse HEAD)" = "$(git rev-parse "$tag^{commit}" 2>/dev/null)" ]; then
    version=${tag#v}
else
    version="${tag#v}-dev.$(git rev-list --count "$tag..HEAD" 2>/dev/null || echo 0)+$(git rev-parse --short HEAD)"
fi
git diff --quiet HEAD 2>/dev/null || version="$version.dirty"
echo "== ActionFootagePlayer $version"

echo "== Gyroflow plugin"
(cd vs-gyroflow && cargo build --release)
# As tools/make_release.sh puts them in the release
bin="$BUILD/dev-bin"
rm -rf "$bin"
mkdir -p "$bin"
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-project gyroflow-export; do
    install -m 755 "vs-gyroflow/target/release/$f" "$bin/$f"
    "$STRIP" -x "$bin/$f"
done
"$INSTALL_NAME_TOOL" -id @rpath/libvs_gyroflow.dylib "$bin/libvs_gyroflow.dylib"
for f in "$bin/"*; do
    /usr/bin/codesign --force -s - "$f"
done

echo "== The app, with Homebrew's mpv and ffmpeg in it"
python3 -I tools/cube_to_glsl.py shaders/dlogm_hlg.glsl.in luts/dlogm_hlg.cube "$BUILD/dlogm_hlg.glsl"
app="$BUILD/ActionFootagePlayer.app"
rm -rf "$app"
BUNDLE_RUNTIME="$prefix" tools/make_app.sh "$app" "$bin" "$BUILD/dlogm_hlg.glsl" "$version"
codesign --verify --deep --strict "$app"
echo "   $app"
[ $install = 1 ] || exit 0

echo "== In $APPS_DIR"
mkdir -p "$APPS_DIR"
player="$APPS_DIR/ActionFootagePlayer.app"
previous="$BUILD/ActionFootagePlayer-previous.app"
if [ -d "$player" ]; then
    "$LSREGISTER" -u "$player" 2>/dev/null || true
    rm -rf "$previous"
    mv "$player" "$previous"
    echo "   the one that was there is $previous"
fi
ditto "$app" "$player"
"$LSREGISTER" -f "$player"
setup="$player/Contents/Resources/setup.sh"
"$setup" lut | sed 's/^/   /'
SERVICES_DIR="$SERVICES_DIR" "$setup" quick-actions | sed 's/^/   /'
# Only for the real Applications folder: an install elsewhere (a test) leaves the system's defaults alone
if [ "$APPS_DIR" = "$HOME/Applications" ] || [ "$APPS_DIR" = /Applications ]; then
    "$setup" default | sed 's/^/   /'
fi
echo "Done: $player ($version)"
