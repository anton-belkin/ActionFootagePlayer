#!/bin/bash
# Builds ffmpeg n9.0.2 with libplacebo v7.387.0 (the player's pinned one, static) and Vulkan (MoltenVK at run time, from
# Homebrew) for docs/design/export-gpu-path.md. FFPL_DIR: a work folder whose path has NO spaces (ffmpeg's configure breaks on them).
# Run: tools/export-gpu/build_ffmpeg_libplacebo.sh (FFPL_DIR, default ~/Library/Caches/afp-ffmpeg-build); tools/dev_app.sh
# then bundles its ffmpeg (FFMPEG_BIN). Run by hand: DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib $FFPL_DIR/prefix/bin/ffmpeg
# The build is keyed on the ffmpeg tag and the patches (their hash, the stamp): when they change, the checkout is reset,
# every patch applied again, and ffmpeg rebuilt and installed; the installed stamp is $FFPL_DIR/prefix/afp-ffmpeg.stamp.
# --stamp prints what it should be (tools/dev_app.sh refuses an ffmpeg whose stamp differs).
set -euo pipefail
PATCHES="$(cd "$(dirname "$0")" && pwd)/ffmpeg-patches"
FFMPEG_TAG=n9.0.2
stamp() { { echo "$FFMPEG_TAG"; [ -d "$PATCHES" ] && cat "$PATCHES"/*.patch; } | shasum -a 256 | cut -c1-16; }
STAMP=$(stamp)
if [ "${1:-}" = --stamp ]; then echo "$STAMP"; exit 0; fi
FFPL_DIR="${FFPL_DIR:-$HOME/Library/Caches/afp-ffmpeg-build}"
case "$FFPL_DIR" in *" "*) echo "FFPL_DIR has a space: ffmpeg's configure can't" >&2; exit 1;; esac
mkdir -p "$FFPL_DIR"
cd "$FFPL_DIR"
P="$PWD/prefix"
export PKG_CONFIG_PATH=$P/lib/pkgconfig:/opt/homebrew/lib/pkgconfig
[ -d libplacebo ] || git -c advice.detachedHead=false clone -q --recursive --shallow-submodules --depth 1 --branch v7.387.0 https://code.videolan.org/videolan/libplacebo.git libplacebo
if [ ! -f $P/lib/pkgconfig/libplacebo.pc ]; then
  meson setup pl-build libplacebo --prefix=$P --buildtype=release --default-library=static \
    -Dvulkan=enabled -Dshaderc=enabled -Dlcms=enabled -Dopengl=disabled -Dd3d11=disabled -Ddovi=enabled -Dlibdovi=disabled \
    -Dxxhash=disabled -Dunwind=disabled -Ddemos=false -Dtests=false \
    -Dvulkan-registry=/opt/homebrew/share/vulkan/registry/vk.xml
  ninja -C pl-build install
fi
# Another tag: a fresh clone (and build)
if [ -d ffmpeg ] && [ "$(git -C ffmpeg describe --tags --exact-match 2>/dev/null)" != "$FFMPEG_TAG" ]; then rm -rf ffmpeg; rm -f "$P/afp-ffmpeg.stamp"; fi
[ -d ffmpeg ] || git -c advice.detachedHead=false clone -q --depth 1 --branch $FFMPEG_TAG https://git.ffmpeg.org/ffmpeg.git ffmpeg
cd ffmpeg
# Our patches (tools/export-gpu/ffmpeg-patches): applied again from a clean checkout whenever the set changed (.afp-patched
# holds the stamp they were applied for; the build's objects stay, make rebuilds what the patches touch)
if [ "$(cat .afp-patched 2>/dev/null)" != "$STAMP" ]; then
  [ -f .afp-patched ] && echo "== ffmpeg patches changed: resetting the checkout and applying them again"
  git reset -q --hard
  git clean -fdq   # what a patch added (not the build's objects: git ignores them)
  rm -f "$P/afp-ffmpeg.stamp"
  if [ -d "$PATCHES" ]; then for p in "$PATCHES"/*.patch; do git apply "$p"; done; fi
  echo "$STAMP" > .afp-patched
fi
if [ ! -f ffbuild/config.mak ]; then ./configure --prefix=$P --enable-gpl --enable-version3 --disable-doc --disable-ffplay \
  --enable-videotoolbox --enable-audiotoolbox --enable-vulkan --enable-libplacebo --enable-neon \
  --pkg-config-flags=--static --extra-ldflags="-L$P/lib" --extra-libs="-lc++ -framework Metal -framework IOSurface -framework CoreVideo"; fi
make -j10
make install
# The range fix (ffmpeg-patches/0002) works before the build is stamped as done
"$PATCHES/../check_ffmpeg_range.sh" "$P/bin/ffmpeg"
echo "$STAMP" > "$P/afp-ffmpeg.stamp"
$P/bin/ffmpeg -hide_banner -buildconf | head -20
