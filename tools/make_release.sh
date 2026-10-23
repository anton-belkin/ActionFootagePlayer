#!/bin/bash
# tools/make_release.sh <version>: into dist/
#   Steady-Player-<version>-macos-arm64.zip             Steady Player.app with the plugins in it (tools/make_app.sh):
#                                                       unzipped into Applications, its setup downloads DJI's LUT
#   mpv-dlogm-<version>.zip                             the dlogm plugin alone, for any mpv on any system: unzipped
#                                                       into ~~/scripts/. HDR only: DJI's Rec.709 LUT isn't
#                                                       redistributed (install.sh downloads it from DJI)
#   mpv-dji-telemetry-<version>.zip                     the dji-telemetry plugin alone, the same way (plain Lua)
#   mpv-gyroflow-<version>-macos-arm64.zip              the gyroflow plugin alone, with its binaries, for Homebrew's
#                                                       mpv: unzipped into ~~/scripts/
#   dlogm-gyroflow-player-<version>-macos-arm64.tar.gz  the committed tree plus the Rust binaries prebuilt in bin/, so
#                                                       install.sh needs no Rust (it uses bin/ when it's there)
#
# Runs on macOS (Apple Silicon); .github/workflows/release.yml runs it for a pushed v* tag.
# Built from HEAD's sources (refuses to run with uncommitted changes to them). The binaries are stripped and
# signed ad hoc, like any local build; macOS runs them once install.sh has cleared the download quarantine.
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: tools/make_release.sh <version>}
name="dlogm-gyroflow-player-$version-macos-arm64"
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
app="Steady-Player-$version-macos-arm64"
rm -rf "dist/$app" "dist/$app.zip"
mkdir -p "dist/$app"
tools/make_app.sh "dist/$app/Steady Player.app" "dist/$name/bin" dist/dlogm_hlg.glsl "$version"
ditto -c -k --keepParent "dist/$app/Steady Player.app" "dist/$app.zip"
rm -rf "dist/$app"

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
mkdir -p "dist/$dlogm/dlogm/shaders"
git archive HEAD mpv/scripts/dlogm | tar -x --strip-components 2 -C "dist/$dlogm"
cp dist/dlogm_hlg.glsl "dist/$dlogm/dlogm/shaders/dlogm_hlg.glsl"
(cd "dist/$dlogm" && zip -qr "../$dlogm.zip" dlogm)
rm -rf "dist/$dlogm"

rm dist/dlogm_hlg.glsl

telemetry="mpv-dji-telemetry-$version"
rm -rf "dist/$telemetry" "dist/$telemetry.zip"
mkdir -p "dist/$telemetry"
git archive HEAD mpv/scripts/dji-telemetry | tar -x --strip-components 2 -C "dist/$telemetry"
(cd "dist/$telemetry" && zip -qr "../$telemetry.zip" dji-telemetry)
rm -rf "dist/$telemetry"

(cd dist && shasum -a 256 "$app.zip" "$dlogm.zip" "$telemetry.zip" "$gyroflow.zip" "$name.tar.gz" > SHA256SUMS)
ls -l "dist/$app.zip" "dist/$dlogm.zip" "dist/$telemetry.zip" "dist/$gyroflow.zip" "dist/$name.tar.gz"
cat dist/SHA256SUMS
