#!/bin/bash
# Builds pipebench against libplacebo at tools/mpv.lock's tag (static, in <dir>/libplacebo), with the app's libplacebo
# patches (tools/libplacebo-patches/) or without them (--plain): the before and after of a patch, side by side.
#
#   tools/pipebench/build.sh <dir> [--plain]
#
# Needs Homebrew's shaderc, little-cms2, vulkan-loader and vulkan-headers (what tools/build_mpv.sh builds with).
set -euo pipefail
cd "$(dirname "$0")/../.."
source tools/mpv.lock
dir=${1:?usage: build.sh <dir> [--plain]}
plain=${2:-}
mkdir -p "$dir"
dir=$(cd "$dir" && pwd)
src="$dir/libplacebo-src"
if [ ! -d "$src" ]; then
    git -c advice.detachedHead=false clone -q --recursive --shallow-submodules --depth 1 --branch "$LIBPLACEBO_TAG" "$LIBPLACEBO_REPO" "$src"
fi
git -C "$src" checkout -q --detach "$LIBPLACEBO_TAG" && git -C "$src" checkout -q -- .
if [ "$plain" != "--plain" ]; then
    for f in tools/libplacebo-patches/*.patch; do [ -f "$f" ] && git -C "$src" apply "$PWD/$f"; done
fi
prefix=$(brew --prefix)
rm -rf "$dir/build" "$dir/libplacebo"
meson setup "$dir/build" "$src" --default-library=static --buildtype=release --prefix="$dir/libplacebo" \
    -Dvulkan=enabled -Dshaderc=enabled -Dlcms=enabled -Dvulkan-registry="$prefix/share/vulkan/registry/vk.xml" \
    -Dopengl=disabled -Ddovi=enabled -Dlibdovi=disabled -Dxxhash=disabled -Dunwind=disabled -Dd3d11=disabled \
    -Dtests=false -Ddemos=false >/dev/null
ninja -C "$dir/build" install >/dev/null
PKG_CONFIG_PATH="$dir/libplacebo/lib/pkgconfig" cc -O2 -o "$dir/pipebench" tools/pipebench/pipebench.c \
    $(PKG_CONFIG_PATH="$dir/libplacebo/lib/pkgconfig" pkg-config --cflags --libs --static libplacebo) -lc++
echo "$dir/pipebench"
