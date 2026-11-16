#!/bin/bash
# The GPU chain offscreen at the render sizes of docs/gpu-render.md's table, with fsr.glsl and with mpv's own scaler
# (Lanczos), with and without the denoise (README.md here).
#   run.sh <clip> <start s> <frames> <warp.glsl> <warp with the denoise.glsl> <look.glsl> <out dir> [shown WxH]
set -e
clip=$1 t=$2 n=$3 warp=$4 warp_dn=$5 look=$6 out=$7 shown=${8:-3024x1701}
here=$(cd "$(dirname "$0")" && pwd)
fsr="$here/../../mpv/scripts/gyroflow/fsr.glsl"
bin=${AFPCHAIN:-$here/afpchain}
W=${shown%x*} H=${shown#*x}
read -r w h fps < <(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,avg_frame_rate -of csv=p=0 "$clip" |
    awk -F, '{ split($3, r, "/"); printf "%s %s %.6f\n", $1, $2, r[1] / r[2] }')
mkdir -p "$out"
# the render's short side: the screen's, 0.8 of it, 1080, 720
for short in $H $(awk "BEGIN { print int($H * 0.8 + 0.5) }") 1080 720; do
    s=$(awk "BEGIN { printf \"%.4f\", $short / $H }")
    for dn in 0 1; do
        for f in 1 0; do
            [ "$short" = "$H" ] && [ $f = 0 ] && continue
            name="short${short}_dn${dn}_fsr${f}"
            shader=$warp; [ $dn = 1 ] && shader=$warp_dn
            echo "== $name (scale $s)"
            ffmpeg -v error -hwaccel videotoolbox -ss "$t" -i "$clip" -frames:v "$n" -pix_fmt yuv420p10le -f rawvideo - |
                "$bin" "$w" "$h" "$fps" "$t" "$W" "$H" "$out/$name." 30 gyroflow_render_scale=$s afp_fsr=$f \
                    gyroflow_dn_radius=3 gyroflow_dn_after=3 gyroflow_dn_keep=0.25 -- "$shader" "$look" "$fsr"
        done
    done
done
