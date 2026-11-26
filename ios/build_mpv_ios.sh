#!/bin/bash
# libmpv for iOS (spike 1, docs/mobile-app.md): our mpv (tools/mpv.lock + tools/mpv-patches + ios/mpv-patches-ios)
# with its libraries built from source for the iPhone and its simulator, as ios/build/MPV.xcframework (one static
# library a platform, every dependency merged in) with mpv's client headers.
#
#   ios/build_mpv_ios.sh [sim|device|all] [step...]     steps: ffmpeg freetype fribidi harfbuzz libass luajit
#                                                       glslang moltenvk mpv xcframework (default: all of them)
#
# Sources in ~/afp-ios-build/src (AFP_IOS_BUILD), each platform's prefix in ~/afp-ios-build/ios-<plat>/prefix; mpv's
# sources (with the patches applied) in src/mpv. MoltenVK comes from its own
# Xcode build (ios/build/deps-src/MoltenVK: ./fetchDependencies --ios --iossim; make ios iossim).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
# Built away from the repo: ffmpeg and LuaJIT don't build in a path with spaces
BR="${AFP_IOS_BUILD:-$HOME/afp-ios-build}"
SRC="$BR/src"
MVK="${AFP_MOLTENVK:-$here/build/deps-src/MoltenVK}"
MIN_IOS=17.0
JOBS=$(sysctl -n hw.ncpu)
mkdir -p "$SRC"

FFMPEG_TAG=n8.0
FREETYPE_TAG=VER-2-13-3
FRIBIDI_TAG=v1.0.16
HARFBUZZ_TAG=11.2.1
LIBASS_TAG=0.17.4
LUAJIT_BRANCH=v2.1
GLSLANG_TAG=16.0.0

plats=(sim device)
case "${1:-all}" in
  sim) plats=(sim) ;;
  device) plats=(device) ;;
  all) ;;
  *) echo "usage: $0 [sim|device|all] [step...]" >&2; exit 2 ;;
esac
shift || true
steps=("$@")
[ ${#steps[@]} -gt 0 ] || steps=(ffmpeg freetype fribidi harfbuzz libass luajit glslang moltenvk mpv xcframework)
want() { local s; for s in "${steps[@]}"; do [ "$s" = "$1" ] && return 0; done; return 1; }

fetch() { # name url tag
  [ -d "$SRC/$1/.git" ] || git -c advice.detachedHead=false clone -q --depth 1 --branch "$3" "$2" "$SRC/$1"
}

setup_plat() {
  plat=$1
  if [ "$plat" = sim ]; then sdk=iphonesimulator; target=arm64-apple-ios$MIN_IOS-simulator; mk=simulator
  else sdk=iphoneos; target=arm64-apple-ios$MIN_IOS; mk=iphoneos; fi
  SYSROOT=$(xcrun --sdk $sdk --show-sdk-path)
  CC="$(xcrun --sdk $sdk -f clang)"
  CXX="$(xcrun --sdk $sdk -f clang++)"
  ARCHFLAGS="-target $target -isysroot $SYSROOT"
  PREFIX="$BR/ios-$plat/prefix"
  BUILD="$BR/ios-$plat/build"
  mkdir -p "$PREFIX/lib/pkgconfig" "$PREFIX/include" "$BUILD"
  export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
  unset PKG_CONFIG_PATH
  CROSS="$BUILD/cross.txt"
  cat > "$CROSS" <<EOF
[binaries]
c = ['$CC']
cpp = ['$CXX']
objc = ['$CC']
objcpp = ['$CXX']
ar = '$(xcrun --sdk $sdk -f ar)'
strip = '$(xcrun --sdk $sdk -f strip)'
pkg-config = 'pkg-config'

[built-in options]
c_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-I$PREFIX/include']
cpp_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-I$PREFIX/include']
objc_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-I$PREFIX/include']
objcpp_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-I$PREFIX/include']
c_link_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-L$PREFIX/lib']
cpp_link_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-L$PREFIX/lib']
objc_link_args = ['-target', '$target', '-isysroot', '$SYSROOT', '-L$PREFIX/lib']
default_library = 'static'

[properties]
needs_exe_wrapper = true

[host_machine]
system = 'darwin'
subsystem = 'ios'
cpu_family = 'aarch64'
cpu = 'arm64'
endian = 'little'
EOF
}

meson_build() { # name src [options...]
  local name=$1 src=$2; shift 2
  rm -rf "$BUILD/$name"
  meson setup "$BUILD/$name" "$src" --cross-file "$CROSS" --prefix "$PREFIX" --libdir lib --buildtype release \
    -Ddefault_library=static "$@" >"$BUILD/$name.log" 2>&1 || { tail -40 "$BUILD/$name.log"; exit 1; }
  meson compile -C "$BUILD/$name" >>"$BUILD/$name.log" 2>&1 || { tail -40 "$BUILD/$name.log"; exit 1; }
  meson install -C "$BUILD/$name" >>"$BUILD/$name.log" 2>&1
  echo "  $name ok"
}

build_ffmpeg() {
  fetch ffmpeg https://github.com/FFmpeg/FFmpeg.git $FFMPEG_TAG
  rm -rf "$BUILD/ffmpeg"; mkdir -p "$BUILD/ffmpeg"
  (cd "$BUILD/ffmpeg" && "$SRC/ffmpeg/configure" --prefix="$PREFIX" --enable-cross-compile --target-os=darwin \
    --arch=aarch64 --cc="$CC" --cxx="$CXX" --sysroot="$SYSROOT" --extra-cflags="$ARCHFLAGS" --extra-ldflags="$ARCHFLAGS" \
    --enable-static --disable-shared --enable-pic --disable-programs --disable-doc --disable-debug \
    --disable-autodetect --enable-videotoolbox --enable-audiotoolbox --enable-zlib --enable-pthreads \
    --disable-avdevice --disable-indevs --disable-outdevs >"$BUILD/ffmpeg.log" 2>&1 \
    && make -j$JOBS >>"$BUILD/ffmpeg.log" 2>&1 && make install >>"$BUILD/ffmpeg.log" 2>&1) || { tail -40 "$BUILD/ffmpeg.log"; exit 1; }
  echo "  ffmpeg ok"
}

build_luajit() {
  fetch luajit https://github.com/LuaJIT/LuaJIT.git $LUAJIT_BRANCH
  local s="$BUILD/luajit-src"
  rm -rf "$s"; cp -R "$SRC/luajit" "$s"
  # The VM is built by a host tool (64-bit, as the target); the target has no JIT on iOS (LJ_TARGET_IOS)
  (cd "$s" && make -j$JOBS DEFAULT_CC=clang HOST_CC="clang" CROSS="" STATIC_CC="$CC" DYNAMIC_CC="$CC" \
     TARGET_LD="$CC" TARGET_AR="$(xcrun --sdk $sdk -f ar) rcus" TARGET_STRIP=true TARGET_FLAGS="$ARCHFLAGS" \
     TARGET_SYS=iOS BUILDMODE=static XCFLAGS="-DLUAJIT_DISABLE_JIT" >"$BUILD/luajit.log" 2>&1 \
   && make install PREFIX="$PREFIX" TARGET_SYS=iOS BUILDMODE=static >>"$BUILD/luajit.log" 2>&1) || { tail -40 "$BUILD/luajit.log"; exit 1; }
  rm -f "$PREFIX"/lib/libluajit*.dylib "$PREFIX"/lib/libluajit*.so*
  echo "  luajit ok"
}

build_glslang() {
  fetch glslang https://github.com/KhronosGroup/glslang.git $GLSLANG_TAG
  rm -rf "$BUILD/glslang"
  cmake -S "$SRC/glslang" -B "$BUILD/glslang" -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_SYSTEM_NAME=iOS \
    -DCMAKE_OSX_SYSROOT=$sdk -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=$MIN_IOS \
    -DCMAKE_INSTALL_PREFIX="$PREFIX" -DBUILD_SHARED_LIBS=OFF -DENABLE_OPT=OFF -DGLSLANG_TESTS=OFF \
    -DENABLE_GLSLANG_BINARIES=OFF -DENABLE_HLSL=OFF -DENABLE_SPVREMAPPER=OFF >"$BUILD/glslang.log" 2>&1 \
    && cmake --build "$BUILD/glslang" -j$JOBS >>"$BUILD/glslang.log" 2>&1 \
    && cmake --install "$BUILD/glslang" >>"$BUILD/glslang.log" 2>&1 || { tail -40 "$BUILD/glslang.log"; exit 1; }
  # glslang 16 has SPIRV inside libglslang; libplacebo still asks for libSPIRV: an empty one
  if [ ! -f "$PREFIX/lib/libSPIRV.a" ]; then
    echo 'static int afp_spirv_stub;' > "$BUILD/spirv_stub.c"
    "$CC" $ARCHFLAGS -c "$BUILD/spirv_stub.c" -o "$BUILD/spirv_stub.o"
    "$(xcrun --sdk $sdk -f ar)" rcs "$PREFIX/lib/libSPIRV.a" "$BUILD/spirv_stub.o"
  fi
  echo "  glslang ok"
}

build_moltenvk() {
  local mvk="$MVK/Package/Release/MoltenVK/static/MoltenVK.xcframework"
  [ -d "$mvk" ] || { echo "MoltenVK isn't built: $mvk" >&2; exit 1; }
  local slice=ios-arm64
  [ $plat = sim ] && slice=$(ls "$mvk" | grep -E '^ios-.*simulator$' | head -1)
  cp "$mvk/$slice/libMoltenVK.a" "$PREFIX/lib/libMoltenVK.a"
  local headers="$MVK/External/Vulkan-Headers/include"
  cp -R "$headers/vulkan" "$headers/vk_video" "$PREFIX/include/" 2>/dev/null || cp -R "$headers/vulkan" "$PREFIX/include/"
  local ver; ver=$(grep -E '#define VK_HEADER_VERSION ' "$headers/vulkan/vulkan_core.h" | awk '{print $3}')
  cat > "$PREFIX/lib/pkgconfig/vulkan.pc" <<EOF
prefix=$PREFIX
Name: vulkan
Description: MoltenVK as the Vulkan implementation (linked directly, no loader)
Version: 1.4.$ver
Cflags: -I\${prefix}/include
Libs: -L\${prefix}/lib -lMoltenVK -framework Metal -framework IOSurface -framework QuartzCore -framework UIKit -framework Foundation -framework CoreGraphics -lc++
EOF
  echo "  moltenvk ok (slice $slice, headers 1.4.$ver)"
}

build_mpv() {
  local m="$SRC/mpv"
  [ -d "$m/subprojects/libplacebo" ] || { echo "no mpv sources in $m (see the top of this file)" >&2; exit 1; }
  # libplacebo looks for glslang's static libraries in the compiler's own folders only, but SPIRV's in vulkan-sdk's
  sed -i '' "s/cxx.find_library('glslang', required: required, static: static)/cxx.find_library('glslang', required: required, static: static, dirs: vulkan_lib_dirs)/" \
    "$m/subprojects/libplacebo/src/glsl/meson.build"
  rm -rf "$BUILD/mpv"
  meson setup "$BUILD/mpv" "$m" --cross-file "$CROSS" --prefix "$PREFIX" --libdir lib --buildtype release \
    -Ddefault_library=static -Db_ndebug=true -Dbuild-date=false -Dcplayer=false -Dlibmpv=true -Dgpl=true \
    -Dvulkan=enabled -Dvideotoolbox-pl=enabled -Daudiounit=enabled -Dcoreaudio=disabled -Davfoundation=disabled \
    -Dcocoa=disabled -Dswift-build=disabled -Dgl=disabled -Dios-gl=disabled -Dlua=luajit -Djavascript=disabled \
    -Dvapoursynth=disabled -Dlibarchive=disabled -Duchardet=disabled -Dlcms2=disabled -Dzimg=disabled -Diconv=disabled \
    -Dlibbluray=disabled -Ddvdnav=disabled -Dcdda=disabled -Drubberband=disabled -Dmanpage-build=disabled \
    -Dhtml-build=disabled -Dpdf-build=disabled -Dtests=false -Dmacos-media-player=disabled -Dmacos-touchbar=disabled \
    -Dmacos-cocoa-cb=disabled \
    -Dlibplacebo:vulkan=enabled -Dlibplacebo:glslang=enabled -Dlibplacebo:shaderc=disabled -Dlibplacebo:opengl=disabled \
    -Dlibplacebo:lcms=disabled -Dlibplacebo:dovi=enabled -Dlibplacebo:libdovi=disabled -Dlibplacebo:xxhash=disabled \
    -Dlibplacebo:unwind=disabled -Dlibplacebo:d3d11=disabled -Dlibplacebo:demos=false -Dlibplacebo:tests=false \
    -Dlibplacebo:vulkan-sdk="$PREFIX" -Dlibplacebo:vulkan-registry="$MVK/External/Vulkan-Headers/registry/vk.xml" \
    >"$BUILD/mpv.log" 2>&1 || { tail -60 "$BUILD/mpv.log"; exit 1; }
  meson compile -C "$BUILD/mpv" >>"$BUILD/mpv.log" 2>&1 || { grep -E 'error' "$BUILD/mpv.log" | head -30; exit 1; }
  meson install -C "$BUILD/mpv" >>"$BUILD/mpv.log" 2>&1
  echo "  mpv ok"
}

merge_libs() {
  # Every static library mpv needs, as one
  local out="$BR/ios-$plat/libmpv-all.a"
  local libs=("$PREFIX"/lib/libmpv.a "$PREFIX"/lib/libplacebo.a "$PREFIX"/lib/libav*.a "$PREFIX"/lib/libsw*.a
              "$PREFIX"/lib/libpostproc.a "$PREFIX"/lib/libass.a "$PREFIX"/lib/libfreetype.a "$PREFIX"/lib/libharfbuzz.a
              "$PREFIX"/lib/libfribidi.a "$PREFIX"/lib/libluajit-5.1.a "$PREFIX"/lib/libglslang*.a "$PREFIX"/lib/libSPIRV*.a
              "$PREFIX"/lib/libMachineIndependent.a "$PREFIX"/lib/libGenericCodeGen.a "$PREFIX"/lib/libOSDependent.a
              "$PREFIX"/lib/libMoltenVK.a)
  local have=(); local l; for l in "${libs[@]}"; do [ -f "$l" ] && have+=("$l"); done
  libtool -static -no_warning_for_no_symbols -o "$out" "${have[@]}"
  echo "  merged ${#have[@]} libraries: $out"
}

for p in "${plats[@]}"; do
  setup_plat "$p"
  echo "== $p ($target)"
  want ffmpeg && build_ffmpeg
  want freetype && { fetch freetype https://gitlab.freedesktop.org/freetype/freetype.git $FREETYPE_TAG
    meson_build freetype "$SRC/freetype" -Dbrotli=disabled -Dbzip2=disabled -Dharfbuzz=disabled -Dpng=disabled -Dzlib=disabled -Dtests=disabled; }
  want fribidi && { fetch fribidi https://github.com/fribidi/fribidi.git $FRIBIDI_TAG
    meson_build fribidi "$SRC/fribidi" -Ddocs=false -Dbin=false -Dtests=false; }
  want harfbuzz && { fetch harfbuzz https://github.com/harfbuzz/harfbuzz.git $HARFBUZZ_TAG
    meson_build harfbuzz "$SRC/harfbuzz" -Dfreetype=enabled -Dglib=disabled -Dgobject=disabled -Dcairo=disabled -Dicu=disabled \
      -Dtests=disabled -Ddocs=disabled -Dutilities=disabled -Dintrospection=disabled -Dcoretext=disabled; }
  want libass && { fetch libass https://github.com/libass/libass.git $LIBASS_TAG
    meson_build libass "$SRC/libass" -Dfontconfig=disabled -Ddirectwrite=disabled -Dcoretext=enabled -Drequire-system-font-provider=false -Dasm=disabled -Dtest=disabled -Dcompare=disabled -Dprofile=disabled -Dfuzz=disabled -Dcheckasm=disabled; }
  want luajit && build_luajit
  want glslang && build_glslang
  want moltenvk && build_moltenvk
  want mpv && build_mpv
  want xcframework && merge_libs
done

if want xcframework; then
  args=()
  for p in sim device; do
    [ -f "$BR/ios-$p/libmpv-all.a" ] || continue
    mkdir -p "$BR/ios-$p/headers/mpv"
    cp "$BR/ios-$p/prefix/include/mpv/"*.h "$BR/ios-$p/headers/mpv/"
    args+=(-library "$BR/ios-$p/libmpv-all.a" -headers "$BR/ios-$p/headers")
  done
  rm -rf "$here/build/MPV.xcframework"
  xcodebuild -create-xcframework "${args[@]}" -output "$here/build/MPV.xcframework" >/dev/null
  echo "== $here/build/MPV.xcframework"
fi
