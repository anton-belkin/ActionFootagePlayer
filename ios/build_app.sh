#!/bin/bash
# Generates the Xcode project (XcodeGen) and builds the app.
#   ios/build_app.sh sim                       the simulator's build (unsigned), in ios/build/sim
#   ios/build_app.sh device <TEAM_ID>          a signed build for a connected iPhone, in ios/build/device
#   ios/build_app.sh archive <TEAM_ID>         an archive for TestFlight, in ios/build/ActionFootage.xcarchive
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"
[ -d build/AFPMotion.xcframework ] || ./build_motion.sh
xcodegen generate --quiet
case "${1:-sim}" in
  sim)
    xcodebuild -project ActionFootage.xcodeproj -scheme ActionFootage -configuration Debug \
      -destination 'generic/platform=iOS Simulator' -derivedDataPath build/dd ARCHS=arm64 CODE_SIGNING_ALLOWED=NO build -quiet
    mkdir -p build/sim && rm -rf "build/sim/Action Footage.app"
    cp -R "build/dd/Build/Products/Debug-iphonesimulator/Action Footage.app" build/sim/
    echo "$here/build/sim/Action Footage.app" ;;
  device)
    xcodebuild -project ActionFootage.xcodeproj -scheme ActionFootage -configuration Release \
      -destination 'generic/platform=iOS' -derivedDataPath build/dd DEVELOPMENT_TEAM="$2" -allowProvisioningUpdates build -quiet
    mkdir -p build/device && rm -rf "build/device/Action Footage.app"
    cp -R "build/dd/Build/Products/Release-iphoneos/Action Footage.app" build/device/
    echo "$here/build/device/Action Footage.app" ;;
  archive)
    xcodebuild -project ActionFootage.xcodeproj -scheme ActionFootage -configuration Release \
      -destination 'generic/platform=iOS' -archivePath build/ActionFootage.xcarchive DEVELOPMENT_TEAM="$2" -allowProvisioningUpdates archive -quiet
    echo "$here/build/ActionFootage.xcarchive" ;;
esac
