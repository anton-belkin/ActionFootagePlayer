#!/bin/bash
# The player's shader for a clip, offscreen through libplacebo (afpchain): per-pass GPU time per render, saved frames.
#   chain.sh <shader> <clip> <start s> <frames> <W> <H> <out prefix|-> <frames to save|-> [input WxH]
# The shader: gyroflow-export's for the clip (AFP_GPU_KEEP=1 keeps it: .<output>.partial.glsl, PTS from libplacebo's
# pass counter); its gyroflow_time_offset is set to <start>. [input WxH]: the decoded frames scaled first (8K clips).
set -e
sh=$1 clip=$2 t=$3 n=$4 W=$5 H=$6 out=$7 save=$8 insize=$9
here=$(cd "$(dirname "$0")" && pwd)
read -r w h fps < <(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,avg_frame_rate -of csv=p=0 "$clip" | head -1 | awk -F, '{ split($3, r, "/"); printf "%s %s %.6f\n", $1, $2, r[1] / r[2] }')
vf=""
if [ -n "$insize" ]; then w=${insize%x*}; h=${insize#*x}; vf="-vf scale=$w:$h"; fi
ffmpeg -v error -hwaccel videotoolbox -ss "$t" -i "$clip" -frames:v "$n" $vf -pix_fmt yuv420p10le -f rawvideo - |
    "$here/afpchain" "$w" "$h" "$fps" "$t" "$W" "$H" "$out" "$save" gyroflow_time_offset=$t -- "$sh"
