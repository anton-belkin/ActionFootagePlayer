#!/bin/bash
# Whether an ffmpeg built by build_ffmpeg_libplacebo.sh keeps a full-range clip full range on gyroflow-export's GPU path
# (VideoToolbox decode -> libplacebo -> hevc_videotoolbox), as ffmpeg-patches/0002 makes it: without it the output is
# tagged limited range and a GoPro's full-range picture is read with the wrong contrast. About a second.
# Run: tools/export-gpu/check_ffmpeg_range.sh <ffmpeg> (macOS only; prints "ok" or why not, exit 1)
set -euo pipefail
ff="${1:?usage: $0 <ffmpeg>}"
[ "$(uname)" = Darwin ] || { echo "skipped: VideoToolbox is macOS's"; exit 0; }
export DYLD_FALLBACK_LIBRARY_PATH="${DYLD_FALLBACK_LIBRARY_PATH:-/opt/homebrew/lib}"
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
"$ff" -nostdin -v error -f lavfi -i testsrc2=s=320x240:r=30:d=0.3 -vf format=p010le -c:v hevc_videotoolbox -profile:v main10 \
    -color_range pc -colorspace bt709 -color_primaries bt709 -color_trc bt709 -tag:v hvc1 "$t/in.mov"
"$ff" -nostdin -v error -hwaccel videotoolbox -hwaccel_output_format videotoolbox_vld -i "$t/in.mov" \
    -vf "setparams=color_primaries=bt709:color_trc=bt709,libplacebo=w=320:h=240:format=p010le:colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=pc:dithering=none" \
    -c:v hevc_videotoolbox -profile:v main10 -color_range pc -colorspace bt709 -color_primaries bt709 -color_trc bt709 -tag:v hvc1 "$t/out.mov"
probe="$(dirname "$ff")/ffprobe"; [ -x "$probe" ] || probe=ffprobe
src=$("$probe" -v error -select_streams v:0 -show_entries stream=color_range -of csv=p=0 "$t/in.mov" | head -1)
got=$("$probe" -v error -select_streams v:0 -show_entries stream=color_range -of csv=p=0 "$t/out.mov" | head -1)
[ "$src" = pc ] || { echo "the test clip itself came out $src, not pc"; exit 1; }
[ "$got" = pc ] || { echo "$ff: a full-range clip comes out of the GPU path as '$got' (ffmpeg-patches/0002 missing?)"; exit 1; }
echo ok
