#!/bin/bash
# Builds ffmpeg n9.0.2 with libplacebo v7.387.0 (the player's pinned one, static) and Vulkan (MoltenVK at run time, from
# Homebrew) for docs/export-gpu-path.md. FFPL_DIR: a work folder whose path has NO spaces (ffmpeg's configure breaks on them).
# Run: FFPL_DIR=/some/dir tools/export-gpu/build_ffmpeg_libplacebo.sh ; then DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib
# $FFPL_DIR/prefix/bin/ffmpeg (it dlopens libvulkan)
set -euo pipefail
cd "${FFPL_DIR:?}"
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
[ -d ffmpeg ] || git -c advice.detachedHead=false clone -q --depth 1 --branch n9.0.2 https://git.ffmpeg.org/ffmpeg.git ffmpeg
cd ffmpeg
if [ ! -f ffbuild/config.mak ]; then ./configure --prefix=$P --enable-gpl --enable-version3 --disable-doc --disable-ffplay \
  --enable-videotoolbox --enable-audiotoolbox --enable-vulkan --enable-libplacebo --enable-neon \
  --pkg-config-flags=--static --extra-ldflags="-L$P/lib" --extra-libs=-lc++; fi
make -j10
make install
$P/bin/ffmpeg -hide_banner -buildconf | head -20
