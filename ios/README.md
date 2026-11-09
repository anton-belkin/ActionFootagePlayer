# Action Footage on the iPhone: the prototype

A first try of [the player on a phone](../docs/mobile-app.md), to install and play with. It plays clips stabilized and
with the log curve made HDR **on the phone's GPU**, from any folder Files can reach, and shows the NAS's library.

- **Files**: folders picked in Files, kept between launches: an SD card in a reader, a camera seen as a disk, the NAS's
  share (Files → Browse → ⋯ → Connect to Server, `smb://…`), and *On this iPhone* (the app's own folder, seen in Files;
  a clip's menu copies it there with its own files). A folder lists its subfolders and videos with their first frames.
- **NAS**: afp-stream's catalog (`/catalog.json`): moments and clip previews by folder, with covers. Each one plays as
  the NAS's stream (rendered there, HEVC HLG), or **on this iPhone** from its original when one of the folders has it
  (the NAS's share added in Files): a moment plays its part, looped.
- **The player**: AVFoundation decodes the video's planes as they are (10 bits); the motion library (afp-motion)
  builds the clip's table and the warp, the same code as the Mac's GPU path (vs-gyroflow's gpu_table and
  shaders/gyroflow/warp.glsl's exact pass, translated from GLSL to Metal by naga on the phone); one Metal pass draws
  each frame: the planes read at the warped coordinate, made R'G'B', through the log curve's LUT (D-Log M, GP-Log), into
  an HLG layer with EDR. Toggles: stabilized, log → HDR.

Not yet: the edge fill (uncovered edges are black), the color fringes under water, the denoise, the clip's saved
view (`.afp.json`: level, horizon, lens), marks and moments, crop mode. A clip with its own Gyroflow project
beside it (`<clip>.gyroflow`) plays with it.

## Building

    ios/build_motion.sh            # Rust: afp-motion for the iPhone and the simulator -> ios/build/AFPMotion.xcframework
    ios/build_app.sh sim           # the simulator's app (unsigned)
    ios/build_app.sh device TEAM   # a signed build for a connected iPhone (TEAM: the Apple developer team id)
    ios/build_app.sh archive TEAM  # an archive for TestFlight (a paid Apple Developer Program account)

On a phone without TestFlight: sign in to Xcode with an Apple ID (Settings → Accounts; a free one makes a "Personal
Team", its builds run 7 days), plug the iPhone in, turn on Developer Mode on it (Settings → Privacy & Security), then
`ios/build_app.sh device <team id>` and `xcrun devicectl device install app --device <iPhone> "ios/build/device/Action
Footage.app"`. TestFlight needs the paid program: an app record in App Store Connect with the bundle id
`io.github.anton-belkin.ActionFootage`, then `ios/build_app.sh archive <team id>` and an upload.

Checked in the simulator (iPhone 17 Pro, iOS 26.5): a D-Log M clip from On this iPhone plays stabilized and in color
(motion read in ~2 s for a 15 s 4K clip); the NAS's catalog with covers, a moment's stream, the same moment played here
from its original with its `.gyroflow` (its 4 s part, looped); landscape. The simulator shows AVKit's HLG streams too
bright; its Metal takes textures up to 8192 wide (the table is packed to that).

Needs Xcode, XcodeGen (`brew install xcodegen`) and Rust with the targets `aarch64-apple-ios` and
`aarch64-apple-ios-sim`. `afp-motion-test <clip>` (`cargo run --release --bin afp-motion-test` in ios/afp-motion)
checks the motion side on a Mac: it writes the Metal source to /tmp/afp-warp.metal.
