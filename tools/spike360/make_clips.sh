#!/bin/bash
# tools/spike360/make_clips.sh <folder> [seconds] [names...]: synthetic 360 clips for the throughput spike (step 2 of
# docs/360-implementation-plan.md), each an MP4 with two HEVC Main10 video tracks, as a dual-lens camera records them,
# encoded with VideoToolbox:
#
#   5k7-30.mp4   two 2880² tracks at 30 fps, 120 Mbit/s in all   (5.7K: X3, X4, Osmo 360 at 5.7K)
#   8k-30.mp4    two 3840² tracks at 30 fps, 160 Mbit/s in all   (8K: X5, Osmo 360)
#   8k-60.mp4    two 3840² tracks at 60 fps, 180 Mbit/s in all   (8K60: the next ones, X6, Osmo 360 II)
#
# The picture is testsrc2 under temporal noise (so the encoder spends the bits a real clip gets), the second lens a
# different hue. Each frame carries its number as a barcode of 32 blocks of 64×64 at (256, 256) of each lens, black
# or white, bit 0 on the left: sync.vpy reads both lenses' and so tells whether the two tracks stay in step. Delete
# the files when done: they're ~500 MB each.
set -euo pipefail
out=${1:?usage: $0 <folder> [seconds] [names...]}
secs=${2:-25}
shift 2 2>/dev/null || shift $#
names=("$@")
[ ${#names[@]} -gt 0 ] || names=(5k7-30 8k-30 8k-60)
mkdir -p "$out"

lens() { # <size> <fps> <kbit/s> <hue> <file>
    local s=$1 fps=$2 rate=$3 hue=$4 file=$5
    # The barcode: block i (64 px wide) is white when bit i of the frame number is set
    local bar="color=c=black:s=2048x64:r=$fps:d=$secs,format=gray,geq=lum='255*mod(floor(N/pow(2\,floor(X/64)))\,2)'"
    ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=s=${s}x${s}:r=$fps:d=$secs,hue=h=$hue,noise=alls=12:allf=t+u" \
        -f lavfi -i "$bar" \
        -filter_complex "[0:v][1:v]overlay=x=256:y=256,format=p010le" \
        -c:v hevc_videotoolbox -profile:v main10 -allow_sw 0 -b:v "${rate}k" -maxrate "$((rate * 3 / 2))k" \
        -g "$fps" -tag:v hvc1 -color_range tv -colorspace bt709 -color_primaries bt709 -color_trc bt709 \
        "$file"
}

for name in "${names[@]}"; do
    case "$name" in
        5k7-30) s=2880 fps=30 rate=120000 ;;
        8k-30)  s=3840 fps=30 rate=160000 ;;
        8k-60)  s=3840 fps=60 rate=180000 ;;
        *) echo "unknown clip $name (5k7-30, 8k-30, 8k-60)" >&2; exit 2 ;;
    esac
    echo "== $name: two ${s}² tracks at $fps fps, $((rate / 1000)) Mbit/s, $secs s"
    start=$(date +%s)
    # One track per lens, in parallel (each is limited by the noise filter, single threaded), then muxed as two
    # video tracks without re-encoding
    lens "$s" "$fps" "$((rate / 2))" 0 "$out/$name.lens0.mp4" &
    lens "$s" "$fps" "$((rate / 2))" 120 "$out/$name.lens1.mp4" &
    wait %1 && wait %2
    ffmpeg -hide_banner -loglevel error -y -i "$out/$name.lens0.mp4" -i "$out/$name.lens1.mp4" \
        -map 0:v -map 1:v -c copy -movflags +faststart "$out/$name.mp4"
    rm -f "$out/$name.lens0.mp4" "$out/$name.lens1.mp4"
    echo "   $(( $(date +%s) - start )) s, $(du -h "$out/$name.mp4" | cut -f1)"
done
