#!/bin/bash
# Every frame of chain.sh's render as an H.264 clip (the coded values taken as Rec.709: the export's --sdr look)
#   clip.sh <shader> <clip> <start s> <frames> <W> <H> <out.mp4> [input WxH]
set -e
sh=$1 clip=$2 t=$3 n=$4 W=$5 H=$6 out=$7 insize=$8
here=$(cd "$(dirname "$0")" && pwd)
fifo=$(mktemp -u /tmp/afpraw.XXXX); mkfifo "$fifo"
fps=$(ffprobe -v error -select_streams v:0 -show_entries stream=avg_frame_rate -of csv=p=0 "$clip" | head -1)
ffmpeg -v error -y -f rawvideo -pix_fmt rgba64le -s "${W}x${H}" -r "$fps" -i "$fifo" -vf format=yuv420p -c:v libx264 -crf 16 -preset fast \
    -color_primaries bt709 -color_trc bt709 -colorspace bt709 "$out" &
AFP_RAW_OUT=$fifo "$here/chain.sh" "$sh" "$clip" "$t" "$n" "$W" "$H" - - $insize > /dev/null
wait; rm -f "$fifo"
