#!/bin/bash
# Builds the motion library (ios/afp-motion, Rust) for the iPhone and its simulator and wraps both as
# ios/build/AFPMotion.xcframework, which the Xcode project links.
#   ios/build_motion.sh [--sim-only | --device-only]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
export IPHONEOS_DEPLOYMENT_TARGET=17.0
targets=(aarch64-apple-ios aarch64-apple-ios-sim)
case "${1:-}" in
  --sim-only) targets=(aarch64-apple-ios-sim) ;;
  --device-only) targets=(aarch64-apple-ios) ;;
esac
cd "$here/afp-motion"
args=()
for t in "${targets[@]}"; do
  cargo build --release --lib --target "$t"
  args+=(-library "$here/afp-motion/target/$t/release/libafp_motion.a" -headers "$here/afp-motion/include")
done
rm -rf "$here/build/AFPMotion.xcframework"
mkdir -p "$here/build"
xcodebuild -create-xcframework "${args[@]}" -output "$here/build/AFPMotion.xcframework"
echo "$here/build/AFPMotion.xcframework"
