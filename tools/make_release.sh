#!/bin/bash
# tools/make_release.sh <version>: into dist/
#   ActionFootagePlayer-<version>-macos-arm64.zip             ActionFootagePlayer.app with the plugins in it (tools/make_app.sh):
#                                                       unzipped into Applications, its setup downloads DJI's LUT.
#                                                       With BUNDLE_RUNTIME, with mpv and ffmpeg in it too
#   mpv-dlogm-<version>.zip                             the dlogm plugin alone, for any mpv on any system: unzipped
#                                                       into ~~/scripts/. HDR only: DJI's Rec.709 LUT isn't
#                                                       redistributed (install.sh downloads it from DJI). With the
#                                                       HLG LUT itself, for the gyroflow plugin's exports
#   mpv-dji-telemetry-<version>.zip                     the dji-telemetry plugin alone, the same way (plain Lua)
#   mpv-highlights-<version>.zip                        the highlights plugin alone, the same way (plain Lua)
#   mpv-footage-menu-<version>.zip                      the right-click menu for the plugins, the same way (plain
#                                                       Lua, mpv 0.41 and later)
#   mpv-gyroflow-<version>-macos-arm64.zip              the gyroflow plugin alone, with its binaries, for Homebrew's
#                                                       mpv: unzipped into ~~/scripts/ (Linux's and Windows' are
#                                                       tools/make_plugin_zip.sh's)
#   ActionFootagePlayer-<version>-macos-arm64.tar.gz  the committed tree plus the Rust binaries prebuilt in bin/, so
#                                                       install.sh needs no Rust (it uses bin/ when it's there)
#
# Runs on macOS (Apple Silicon); .github/workflows/release.yml runs it for a pushed v* tag.
# Built from HEAD's sources (refuses to run with uncommitted changes to them). The binaries are stripped and
# signed ad hoc, like any local build; macOS runs them once install.sh has cleared the download quarantine.
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: tools/make_release.sh <version>}
name="ActionFootagePlayer-$version-macos-arm64"
if ! git diff --quiet HEAD -- vs-gyroflow; then
    echo "vs-gyroflow has uncommitted changes; the binaries would not match the tree" >&2
    exit 1
fi

(cd vs-gyroflow && cargo build --release)

rm -rf "dist/$name" "dist/$name.tar.gz"
mkdir -p "dist/$name/bin"
git archive HEAD | tar -x -C "dist/$name"
echo "$version" > "dist/$name/VERSION"
for f in libvs_gyroflow.dylib gyroflow-probe gyroflow-embed gyroflow-export; do
    install -m 755 "vs-gyroflow/target/release/$f" "dist/$name/bin/$f"
    strip -x "dist/$name/bin/$f"
done
# The build machine's path otherwise stays in the library as its install name
install_name_tool -id @rpath/libvs_gyroflow.dylib "dist/$name/bin/libvs_gyroflow.dylib"
for f in "dist/$name/bin/"*; do
    codesign --force -s - "$f"
done

tar -C dist -czf "dist/$name.tar.gz" "$name"

# Built from the tarball's tree, which is HEAD's
python3 -I tools/cube_to_glsl.py shaders/dlogm_hlg.glsl.in luts/dlogm_hlg.cube "dist/dlogm_hlg.glsl"
# The zip has the tarball's name: the app is built in a folder of its own, not in dist/$name
app="ActionFootagePlayer-$version-macos-arm64"
stage="dist/app-$version"
rm -rf "$stage" "dist/$app.zip"
mkdir -p "$stage"
tools/make_app.sh "$stage/ActionFootagePlayer.app" "dist/$name/bin" dist/dlogm_hlg.glsl "$version"
ditto -c -k --keepParent "$stage/ActionFootagePlayer.app" "dist/$app.zip"
rm -rf "$stage"

gyroflow="mpv-gyroflow-$version-macos-arm64"
rm -rf "dist/$gyroflow" "dist/$gyroflow.zip"
mkdir -p "dist/$gyroflow/gyroflow"
git archive HEAD mpv/scripts/gyroflow | tar -x --strip-components 2 -C "dist/$gyroflow"
mv "dist/$gyroflow/gyroflow/underwater.gyroflow" "dist/$gyroflow/gyroflow/default.gyroflow"
cp -p "dist/$name/bin/"* "dist/$gyroflow/gyroflow/"
(cd "dist/$gyroflow" && zip -qry "../$gyroflow.zip" gyroflow)
rm -rf "dist/$gyroflow" "dist/$name"

dlogm="mpv-dlogm-$version"
rm -rf "dist/$dlogm" "dist/$dlogm.zip"
mkdir -p "dist/$dlogm/dlogm/shaders" "dist/$dlogm/dlogm/luts"
git archive HEAD mpv/scripts/dlogm | tar -x --strip-components 2 -C "dist/$dlogm"
cp dist/dlogm_hlg.glsl "dist/$dlogm/dlogm/shaders/dlogm_hlg.glsl"
git show HEAD:shaders/denoise.glsl > "dist/$dlogm/dlogm/shaders/denoise.glsl"
git show HEAD:luts/dlogm_hlg.cube > "dist/$dlogm/dlogm/luts/dlogm_hlg.cube"
# The other log curves' HDR shaders, from their makers' published formulas (the app's are made by make_app.sh)
for curve in dlog gplog2 ilog; do
    python3 -I tools/make_hdr_lut.py "$curve" "dist/${curve}_hlg.cube" >/dev/null
    python3 -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in "dist/${curve}_hlg.cube" \
        "dist/$dlogm/dlogm/shaders/${curve}_hlg.glsl" >/dev/null
    rm "dist/${curve}_hlg.cube"
done
git show HEAD:luts/gplog_hlg.cube > dist/gplog_hlg.cube
python3 -I tools/cube_to_glsl.py shaders/log_hlg.glsl.in dist/gplog_hlg.cube "dist/$dlogm/dlogm/shaders/gplog_hlg.glsl" >/dev/null
rm dist/gplog_hlg.cube
(cd "dist/$dlogm" && zip -qr "../$dlogm.zip" dlogm)
rm -rf "dist/$dlogm"

rm dist/dlogm_hlg.glsl

telemetry="mpv-dji-telemetry-$version"
rm -rf "dist/$telemetry" "dist/$telemetry.zip"
mkdir -p "dist/$telemetry"
git archive HEAD mpv/scripts/dji-telemetry | tar -x --strip-components 2 -C "dist/$telemetry"
(cd "dist/$telemetry" && zip -qr "../$telemetry.zip" dji-telemetry)
rm -rf "dist/$telemetry"

highlights="mpv-highlights-$version"
rm -rf "dist/$highlights" "dist/$highlights.zip"
mkdir -p "dist/$highlights"
git archive HEAD mpv/scripts/highlights | tar -x --strip-components 2 -C "dist/$highlights"
(cd "dist/$highlights" && zip -qr "../$highlights.zip" highlights)
rm -rf "dist/$highlights"

menu="mpv-footage-menu-$version"
rm -rf "dist/$menu" "dist/$menu.zip"
mkdir -p "dist/$menu"
git archive HEAD mpv/scripts/footage-menu | tar -x --strip-components 2 -C "dist/$menu"
(cd "dist/$menu" && zip -qr "../$menu.zip" footage-menu)
rm -rf "dist/$menu"

(cd dist && shasum -a 256 "$app.zip" "$dlogm.zip" "$telemetry.zip" "$highlights.zip" "$menu.zip" "$gyroflow.zip" "$name.tar.gz" > SHA256SUMS)
ls -l "dist/$app.zip" "dist/$dlogm.zip" "dist/$telemetry.zip" "dist/$highlights.zip" "dist/$menu.zip" "dist/$gyroflow.zip" "dist/$name.tar.gz"
cat dist/SHA256SUMS
