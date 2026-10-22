#!/bin/bash
# tools/make_release.sh <version>: dist/dlogm-gyroflow-player-<version>-macos-arm64.tar.gz, the committed tree
# plus the Rust binaries prebuilt in bin/, so installing needs no Rust. install.sh uses bin/ when it's there.
#
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
rm -rf "dist/$name"
(cd dist && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
ls -l "dist/$name.tar.gz"
cat "dist/$name.tar.gz.sha256"
