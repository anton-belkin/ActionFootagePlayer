#!/bin/bash
# tools/make_windows_app.sh <version> [<rust target>]: into dist/
#   ActionFootagePlayer-<version>-windows-x86_64-setup.exe   the Windows app's installer: ActionFootagePlayer with
#                                                            everything it needs in its folder (no mpv, Python or
#                                                            VapourSynth to install), for the user alone (no admin)
#
# The app (windows/launcher/src/main.rs has its layout): the launcher, mpv, Python with VapourSynth, the VC++ runtime
# VapourSynth needs, a Vulkan loader, ffmpeg for exports, the plugins (and their right-click menu) and the setup.
# What it carries from elsewhere is downloaded once into build/windows-deps/, at the versions pinned here, and
# checked; build/windows-app/ is the app's folder as installed. The installer is windows/installer.iss, made with
# Inno Setup 6 or 7 (ISCC.exe, in PATH or where its installer puts it; without it, the folder is made and the
# installer isn't).
#
# Runs on Windows in Git Bash (.github/workflows/release.yml runs it after tools/make_plugin_zip.sh, whose binaries
# it uses). Built from HEAD's sources, like the plugin zip.
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: tools/make_windows_app.sh <version> [<rust target>]}
target=${2:-}

# What the app carries. The sources' own checksums; updating one means downloading it and updating both
# mpv: shinchiro's build (with VapourSynth support), as SourceForge mirrors it; x86-64 without AVX2 (v3), for any CPU
MPV_URL="https://downloads.sourceforge.net/project/mpv-player-windows/64bit/mpv-x86_64-20261018-git-01e2533f31.7z"
MPV_SHA256="d7157567a2fd88e46f739d2e8b8e6c02d2ad812db04547b45c59a2800821991f"
# Python: python.org's build, as NuGet has it (a plain folder, which VapourSynth's vsscript.dll finds it in)
PYTHON_URL="https://api.nuget.org/v3-flatcontainer/python/3.13.17/python.3.13.17.nupkg"
PYTHON_SHA256="dce409744b088f94c6e37ba41f8100c384c27e029e843237364c602d30d475e5"
PYTHON_DLL="python313.dll"
# VapourSynth R81, the wheel (for any Python from 3.12)
VAPOURSYNTH_URL="https://files.pythonhosted.org/packages/6c/c1/d1448dd3a34ac2d3f439496167b3ffe14d6c0b7f9647d468c2292a06e4c2/vapoursynth-81-cp312-abi3-win_amd64.whl"
VAPOURSYNTH_SHA256="8de011b9d372ca1593ccbe6ea164c5d093ef44905a69597fa3d8099d38ce381c"
# The VC++ runtime VapourSynth is built for (14.50 or later), its DLLs as a wheel: in the app's folder, so there's
# nothing for the system to install
MSVC_RUNTIME_URL="https://files.pythonhosted.org/packages/c2/6e/7ba5696e9d8c1bcb01352f06e441497c308e84f75aba598400646ab31ca7/msvc_runtime-14.51.36231-cp313-cp313-win_amd64.whl"
MSVC_RUNTIME_SHA256="f3c3ac4f733a774f6ae209567e533bfde6f72dc1c10977733bfe09ce01e5b624"
# The Vulkan loader (Khronos's, as Silk.NET builds it), which mpv.exe needs to start at all: the GPU's driver installs
# one, and the system's comes first (the launcher puts this one last in PATH); without one Windows refuses to start
# mpv, with a dialog. And its license, from Khronos at the same version
VULKAN_LOADER_URL="https://api.nuget.org/v3-flatcontainer/silk.net.vulkan.loader.native/2025.9.12/silk.net.vulkan.loader.native.2025.9.12.nupkg"
VULKAN_LOADER_SHA256="33811c05ab0bcba632ad38abba459b0d72d6e58859342e0782d011863dde07d1"
VULKAN_LICENSE_URL="https://raw.githubusercontent.com/KhronosGroup/Vulkan-Loader/v1.3.283/LICENSE.txt"
VULKAN_LICENSE_SHA256="43c0a37e6a0fa7ff3c843b3ec5a4fac84b712558ddac103fbd4c1649662a9ece"
# ffmpeg for gyroflow-export: Gyan's essentials build (x265, NVENC, Quick Sync, AMF)
FFMPEG_URL="https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.7z"
FFMPEG_SHA256="4705843ccaaf54257c16ad90f3e952ece33c17df964ecf7bfdbb0f49c7171077"
# The HEVC encoders gyroflow-export tries, which the ffmpeg has to have
FFMPEG_ENCODERS="hevc_nvenc hevc_qsv hevc_amf libx265 prores_ks"

if ! git diff --quiet HEAD -- vs-gyroflow mpv windows; then
    echo "vs-gyroflow, the plugins or windows/ have uncommitted changes; the app would not match the tree" >&2
    exit 1
fi
case "${target:-$(uname -s)}" in
    *windows*|MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "tools/make_windows_app.sh is for Windows (Git Bash), or a windows Rust target" >&2; exit 1 ;;
esac
py=$(command -v python3 || command -v python)
sevenzip=$(command -v 7z || echo "/c/Program Files/7-Zip/7z.exe")

deps=build/windows-deps
fetch() {
    local url=$1 sha=$2 out="$deps/$(basename "$1")"
    mkdir -p "$deps"
    if [ ! -f "$out" ] || ! echo "$sha  $out" | sha256sum -c --status; then
        echo "== Downloading $(basename "$url")" >&2
        curl -fsSL --retry 3 -o "$out.part" "$url"
        if ! echo "$sha  $out.part" | sha256sum -c --status; then
            echo "$url: the checksum differs from the one pinned here ($(sha256sum "$out.part" | cut -d' ' -f1))" >&2
            exit 1
        fi
        mv "$out.part" "$out"
    fi
    echo "$out"
}
unzip_to() { "$py" -I -m zipfile -e "$1" "$2"; }

(cd vs-gyroflow && cargo build --release ${target:+--target "$target"})
(cd windows/launcher && cargo build --release ${target:+--target "$target"})
gyroflow_bin="vs-gyroflow/target/${target:+$target/}release"
launcher_bin="windows/launcher/target/${target:+$target/}release"
# A cross build's set-icon runs in Wine (SET_ICON: how to run it)
set_icon=${SET_ICON:-$launcher_bin/set-icon.exe}

app=build/windows-app
tmp=build/windows-tmp
rm -rf "$app" "$tmp"
mkdir -p "$app" "$tmp"

echo "== mpv"
archive=$(fetch "$MPV_URL" "$MPV_SHA256")
"$sevenzip" x -y -o"$tmp/mpv" "$archive" >/dev/null
mkdir -p "$app/mpv"
cp "$tmp/mpv/mpv.exe" "$tmp/mpv/d3dcompiler_43.dll" "$app/mpv/"
# Its window and the taskbar show the app's icon, not mpv's
$set_icon "$app/mpv/mpv.exe" windows/AppIcon.ico

echo "== The VC++ runtime"
archive=$(fetch "$MSVC_RUNTIME_URL" "$MSVC_RUNTIME_SHA256")
unzip_to "$archive" "$tmp/msvc"
cp "$tmp"/msvc/msvc_runtime-*.data/data/Scripts/*.dll "$app/mpv/"

echo "== The Vulkan loader"
archive=$(fetch "$VULKAN_LOADER_URL" "$VULKAN_LOADER_SHA256")
unzip_to "$archive" "$tmp/vulkan"
mkdir -p "$app/vulkan"
cp "$tmp/vulkan/runtimes/win-x64/native/vulkan-1.dll" "$app/vulkan/"
vulkan_license=$(fetch "$VULKAN_LICENSE_URL" "$VULKAN_LICENSE_SHA256")

echo "== Python and VapourSynth"
archive=$(fetch "$PYTHON_URL" "$PYTHON_SHA256")
unzip_to "$archive" "$tmp/python"
python="$app/python"
mkdir -p "$python"
cp "$tmp/python/tools/"{python.exe,pythonw.exe,python3.dll,"$PYTHON_DLL",vcruntime140.dll,vcruntime140_1.dll,LICENSE.txt} "$python/"
cp -r "$tmp/python/tools/DLLs" "$tmp/python/tools/Lib" "$python/"
# What the app doesn't run: tests, the GUI toolkit, pip
rm -rf "$python/Lib/"{test,idlelib,tkinter,turtledemo,ensurepip,turtle.py} "$python/Lib/site-packages/"*
find "$python" -name __pycache__ -type d -prune -exec rm -rf {} +
site="$python/Lib/site-packages"
archive=$(fetch "$VAPOURSYNTH_URL" "$VAPOURSYNTH_SHA256")
unzip_to "$archive" "$tmp/vapoursynth"
cp -r "$tmp/vapoursynth/vapoursynth" "$tmp/vapoursynth/"vapoursynth-*.dist-info "$site/"
rm -rf "$site/vapoursynth/"{include,pkgconfig} "$site/vapoursynth/"*.pdb

echo "== The plugins"
plugins="$app/plugins"
dlogm="$plugins/dlogm"
gyroflow="$plugins/gyroflow"
telemetry="$plugins/dji-telemetry"
highlights="$plugins/highlights"
menu="$plugins/footage-menu"
mkdir -p "$dlogm/shaders" "$dlogm/luts" "$gyroflow" "$telemetry" "$highlights" "$menu"
cp mpv/scripts/dlogm/main.lua mpv/scripts/dlogm/dji_color.lua mpv/scripts/dlogm/gopro_color.lua mpv/scripts/dlogm/video_color.lua "$dlogm/"
"$py" -I tools/cube_to_glsl.py shaders/dlogm_hlg.glsl.in luts/dlogm_hlg.cube "$dlogm/shaders/dlogm_hlg.glsl" >/dev/null
cp shaders/denoise.glsl "$dlogm/shaders/"
cp luts/dlogm_hlg.cube luts/gplog_hlg.cube "$dlogm/luts/"
# The other log curves' HDR shaders, from their makers' published formulas, and their LUTs for gyroflow-export (as
# tools/make_app.sh)
for curve in dlog gplog2 ilog; do
    "$py" -I tools/make_hdr_lut.py "$curve" "$dlogm/luts/${curve}_hlg.cube" >/dev/null
    "$py" -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in "$dlogm/luts/${curve}_hlg.cube" "$dlogm/shaders/${curve}_hlg.glsl" >/dev/null
done
"$py" -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in luts/gplog_hlg.cube "$dlogm/shaders/gplog_hlg.glsl" >/dev/null
cp mpv/scripts/gyroflow/main.lua mpv/scripts/gyroflow/export.lua mpv/scripts/gyroflow/platform.lua \
    mpv/scripts/gyroflow/gyroflow.vpy "$gyroflow/"
cp mpv/scripts/gyroflow/underwater.gyroflow "$gyroflow/default.gyroflow"
for f in vs_gyroflow.dll gyroflow-probe.exe gyroflow-embed.exe gyroflow-export.exe; do
    cp "$gyroflow_bin/$f" "$gyroflow/"
done
cp mpv/scripts/dji-telemetry/main.lua mpv/scripts/dji-telemetry/telemetry.lua "$telemetry/"
cp mpv/scripts/highlights/main.lua mpv/scripts/highlights/markers.lua "$highlights/"
cp mpv/scripts/footage-menu/main.lua "$menu/"

echo "== ffmpeg"
archive=$(fetch "$FFMPEG_URL" "$FFMPEG_SHA256")
"$sevenzip" x -y -o"$tmp/ffmpeg" "$archive" >/dev/null
ffmpeg_dir=$(dirname "$(find "$tmp/ffmpeg" -name ffmpeg.exe | head -1)")
cp "$ffmpeg_dir/ffmpeg.exe" "$ffmpeg_dir/ffprobe.exe" "$gyroflow/"
if [ -z "$target" ]; then
    encoders=$("$gyroflow/ffmpeg.exe" -hide_banner -encoders)
    for e in $FFMPEG_ENCODERS; do
        grep -q " $e " <<< "$encoders" || { echo "This ffmpeg has no $e" >&2; exit 1; }
    done
fi

echo "== The launcher and the setup"
cp "$launcher_bin/ActionFootagePlayer.exe" "$app/"
$set_icon "$app/ActionFootagePlayer.exe" windows/AppIcon.ico
mkdir -p "$app/tools"
cp windows/setup.py tools/cube_to_glsl.py shaders/dlogm_lut3d.glsl.in "$app/tools/"
cp mpv/app.conf "$app/"
# DJI's LUT as macos/setup.sh has it
lut_url=$(sed -n 's/^DJI_LUT_URL="\(.*\)"$/\1/p' macos/setup.sh)
lut_sha=$(sed -n 's/^DJI_LUT_SHA256="\(.*\)"$/\1/p' macos/setup.sh)
[ -n "$lut_url" ] && [ -n "$lut_sha" ] || { echo "DJI's LUT isn't in macos/setup.sh" >&2; exit 1; }
printf '{"url": "%s", "sha256": "%s"}\n' "$lut_url" "$lut_sha" > "$app/tools/dji_lut.json"

echo "== Licenses"
licenses="$app/licenses"
mkdir -p "$licenses"
cp LICENSE "$licenses/ActionFootagePlayer.txt"
cp windows/THIRD-PARTY.txt "$licenses/"
cp "$python/LICENSE.txt" "$licenses/Python.txt"
cp "$site"/vapoursynth-*.dist-info/licenses/COPYING.LESSER "$licenses/VapourSynth.txt"
cp "$tmp"/msvc/msvc_runtime-*.dist-info/licenses/LICENSE "$licenses/VC++ runtime.txt"
cp "$vulkan_license" "$licenses/Vulkan loader.txt"
find "$tmp/ffmpeg" -maxdepth 2 -name LICENSE -exec cp {} "$licenses/ffmpeg.txt" \;
echo "$version" > "$app/VERSION"
rm -rf "$tmp"

iscc=$(command -v ISCC || true)
for candidate in "/c/Program Files (x86)/Inno Setup "*/ISCC.exe "/c/Program Files/Inno Setup "*/ISCC.exe; do
    if [ -z "$iscc" ] && [ -x "$candidate" ]; then iscc=$candidate; fi
done
if [ -z "$iscc" ]; then
    echo "No Inno Setup (ISCC.exe): the app is in $app, without an installer" >&2
    exit 0
fi
echo "== The installer"
mkdir -p dist
win() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
# Git Bash would take /D... for paths and rewrite them
MSYS2_ARG_CONV_EXCL='*' "$iscc" /Q "/DAppVersion=$version" "/DSourceDir=$(win "$PWD/$app")" \
    "/O$(win "$PWD/dist")" "$(win "$PWD/windows/installer.iss")"
ls -l "dist/ActionFootagePlayer-$version-windows-x86_64-setup.exe"
