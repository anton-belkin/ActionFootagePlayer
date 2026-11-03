#!/bin/bash
# tools/build_mpv.sh [--no-deps | --deps-only | --print-deps]: the mpv the macOS app carries, built from tools/mpv.lock's sources,
# into build/mpv-afp/out/mpv. tools/dev_app.sh uses it when it's there, and tools/bundle_runtime.py copies it into the
# app (MPV_BIN) in place of Homebrew's, with the libraries it links from Homebrew.
#
#   build/mpv-afp/src/mpv/                       mpv at MPV_COMMIT (a blobless clone: `git describe` names the version)
#   build/mpv-afp/src/mpv/subprojects/libplacebo libplacebo at LIBPLACEBO_TAG, with its submodules (glad, jinja...),
#                                                built by meson with mpv and linked in statically: mpv master needs a
#                                                newer libplacebo than Homebrew's
#   build/mpv-afp/build/                         meson's build folder
#   build/mpv-afp/out/                           what tools/bundle_runtime.py takes (MPV_BIN=build/mpv-afp/out/mpv):
#     mpv                                        the binary
#     mpv.lock                                   the lock it's built from, with MPV_DESCRIBE: its `git describe`
#     licenses/mpv/, licenses/libplacebo/        their license files, for the app's ThirdParty/
#
# Everything else it links (ffmpeg, libass, LuaJIT, VapourSynth, MoltenVK's Vulkan loader...) is Homebrew's, installed
# first with `brew install --only-dependencies mpv` and the build's tools. Built for macOS 15 and later
# (MACOSX_DEPLOYMENT_TARGET); the Homebrew libraries it links set the floor the app really needs (their bottles').
#
#   --no-deps      doesn't run brew install (it's done already)
#   --deps-only    only runs brew install
#   --print-deps   only prints the versions of what the build uses from Homebrew and Xcode, for a cache key
# (.github/workflows/release.yml: --deps-only, then --no-deps when its cache, keyed on --print-deps, has no build)
# MPV_SRC_DIR overrides where the sources are cloned (default build/mpv-afp/src).
set -euo pipefail
cd "$(dirname "$0")/.."

deps=1
build=1
print=0
for arg in "$@"; do
    case "$arg" in
        --no-deps) deps=0 ;;
        --deps-only) build=0 ;;
        --print-deps) print=1 ;;
        *) echo "usage: $0 [--no-deps | --deps-only | --print-deps]" >&2; exit 2 ;;
    esac
done

# shellcheck source=mpv.lock
. tools/mpv.lock
export MACOSX_DEPLOYMENT_TARGET=15.0
TOOLS=(meson ninja pkgconf shaderc vulkan-headers)
OUT=build/mpv-afp
SRC="${MPV_SRC_DIR:-$OUT/src}"

[ "$(uname -s)" = Darwin ] || { echo "tools/build_mpv.sh builds the Mac app's mpv" >&2; exit 1; }
command -v brew >/dev/null || { echo "Missing Homebrew (https://brew.sh): mpv's libraries are Homebrew's" >&2; exit 1; }
prefix=$(brew --prefix)

if [ $print = 1 ]; then
    # Everything mpv links or is built with: a new version of any is a new build (not Homebrew's libplacebo, unused)
    brew list --versions $(brew deps mpv | grep -vx libplacebo) "${TOOLS[@]}" | sort -u
    xcrun swiftc --version 2>&1 | head -1
    echo "MACOSX_DEPLOYMENT_TARGET=$MACOSX_DEPLOYMENT_TARGET"
    exit 0
fi

if [ $deps = 1 ]; then
    echo "== Homebrew: mpv's dependencies and the build's tools"
    HOMEBREW_NO_INSTALL_CLEANUP=1 HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1 \
        brew install --only-dependencies mpv
    HOMEBREW_NO_INSTALL_CLEANUP=1 brew install "${TOOLS[@]}"
fi
[ $build = 1 ] || exit 0

echo "== mpv $MPV_COMMIT"
mpv_src="$SRC/mpv"
if [ ! -d "$mpv_src/.git" ]; then
    mkdir -p "$SRC"
    git clone --filter=blob:none --no-checkout "$MPV_REPO" "$mpv_src"
fi
git -C "$mpv_src" remote set-url origin "$MPV_REPO"
git -C "$mpv_src" cat-file -e "$MPV_COMMIT^{commit}" 2>/dev/null || git -C "$mpv_src" fetch --tags origin "$MPV_COMMIT"
git -C "$mpv_src" checkout -q --detach "$MPV_COMMIT"
# Built as it's committed: a change left in the clone would be in the app, with the commit's name on it
if [ -n "$(git -C "$mpv_src" status --porcelain)" ]; then
    echo "$mpv_src has changes; the build wouldn't be $MPV_COMMIT" >&2
    exit 1
fi

echo "== libplacebo $LIBPLACEBO_TAG"
pl_src="$mpv_src/subprojects/libplacebo"
if [ ! -d "$pl_src/.git" ]; then
    git -c advice.detachedHead=false clone -q --recursive --shallow-submodules --depth 1 --branch "$LIBPLACEBO_TAG" "$LIBPLACEBO_REPO" "$pl_src"
elif [ "$(git -C "$pl_src" rev-parse HEAD)" != "$LIBPLACEBO_COMMIT" ]; then
    git -C "$pl_src" fetch -q --depth 1 origin tag "$LIBPLACEBO_TAG"
    git -C "$pl_src" checkout -q --detach "$LIBPLACEBO_TAG"
    git -C "$pl_src" submodule update -q --init --recursive --depth 1
fi
if [ "$(git -C "$pl_src" rev-parse HEAD)" != "$LIBPLACEBO_COMMIT" ]; then
    echo "libplacebo's $LIBPLACEBO_TAG is $(git -C "$pl_src" rev-parse HEAD), not $LIBPLACEBO_COMMIT (tools/mpv.lock)" >&2
    exit 1
fi

echo "== Build"
# libarchive is keg-only: its pkg-config file isn't in Homebrew's own folder
export PKG_CONFIG_PATH="$prefix/opt/libarchive/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
build="$OUT/build"
reconfigure=()
[ -f "$build/build.ninja" ] && reconfigure=(--reconfigure)
# swift-flags: mpv's Swift sources include libplacebo's headers, and with libplacebo a subproject, its config.h is in
# the build folder, which mpv doesn't tell swiftc (relative to it: swiftc runs there; split at spaces, so no full path).
# What the app uses enabled (a missing one fails the build rather than leaving it out); what it has no use for, and
# what would come in from whatever else this Mac has installed, disabled; the rest as mpv decides by itself (CoreAudio,
# VideoToolbox, Cocoa's other parts...), as Homebrew's build
meson setup ${reconfigure[@]+"${reconfigure[@]}"} "$build" "$mpv_src" \
    --buildtype=release \
    --force-fallback-for=libplacebo \
    -Db_ndebug=true \
    -Dbuild-date=false \
    -Dcplayer=true \
    -Dlibmpv=false \
    -Dgpl=true \
    -Dvulkan=enabled \
    -Dcocoa=enabled \
    -Dswift-build=enabled \
    -Dvideotoolbox-pl=enabled \
    -Dcoreaudio=enabled \
    -Dlua=luajit \
    -Dvapoursynth=enabled \
    -Dlibarchive=enabled \
    -Duchardet=enabled \
    -Dlcms2=enabled \
    -Dzimg=enabled \
    -Djavascript=disabled \
    -Dlibbluray=disabled \
    -Ddvdnav=disabled \
    -Dcdda=disabled \
    -Dsdl2-audio=disabled \
    -Dsdl2-video=disabled \
    -Dsdl2-gamepad=disabled \
    -Dopenal=disabled \
    -Djack=disabled \
    -Dpipewire=disabled \
    -Dpulse=disabled \
    -Dcaca=disabled \
    -Dsixel=disabled \
    -Dx11=disabled \
    -Dwayland=disabled \
    -Degl=disabled \
    -Dmanpage-build=disabled \
    -Dhtml-build=disabled \
    -Dpdf-build=disabled \
    -Dtests=false \
    -Dswift-flags="-Isubprojects/libplacebo/src/include" \
    -Dlibplacebo:vulkan=enabled \
    -Dlibplacebo:shaderc=enabled \
    -Dlibplacebo:lcms=enabled \
    -Dlibplacebo:vulkan-registry="$prefix/share/vulkan/registry/vk.xml" \
    -Dlibplacebo:opengl=enabled \
    -Dlibplacebo:dovi=enabled \
    -Dlibplacebo:libdovi=disabled \
    -Dlibplacebo:xxhash=disabled \
    -Dlibplacebo:unwind=disabled \
    -Dlibplacebo:d3d11=disabled \
    -Dlibplacebo:tests=false
meson compile -C "$build" mpv:executable

out="$OUT/out"
rm -rf "$out"
mkdir -p "$out/licenses/mpv" "$out/licenses/libplacebo"
install -m 755 "$build/mpv" "$out/mpv"
{ cat tools/mpv.lock; echo "MPV_DESCRIBE=$(git -C "$mpv_src" describe --tags --match "v[0-9]*" --always)"; } > "$out/mpv.lock"
cp "$mpv_src/LICENSE.GPL" "$mpv_src/LICENSE.LGPL" "$mpv_src/Copyright" "$out/licenses/mpv/"
cp "$pl_src/LICENSE" "$out/licenses/libplacebo/"
# What libplacebo builds in from its submodules: fast_float's licenses (glad's generated loader is the Khronos
# registry's, Apache 2.0, with the license in its header)
for f in "$pl_src"/3rdparty/fast_float/LICENSE*; do
    [ -f "$f" ] && cp "$f" "$out/licenses/libplacebo/fast_float-$(basename "$f")"
done
"$out/mpv" --no-config --version | head -1
echo "Done: $out/mpv"
