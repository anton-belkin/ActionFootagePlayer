#!/bin/bash
# The grid and plane modes' picture against the exact pass and the VapourSynth path (check.sh's shots), paused at one
# frame, and the passes' order (vo-passes lists them as they run)
. /gpu/common.sh
cd /out/gpu/check
F=${FRAME:-1049}; t=$(awk -v f=$F 'BEGIN{printf "%.6f", f*1001/60000}')
for v in ${VARIANTS:-exact_bicubic grid_bicubic grid32_bicubic planes_bicubic}; do
  mpv $BASE --pause --start=$t --hwdec=vaapi --glsl-shaders=/gpu/w_$v.glsl $C >/out/gpu/check/$v.log 2>&1 &
  P=$!
  sleep 8
  cmd "{\"command\":[\"screenshot-to-file\",\"/out/gpu/check/${v}_$F.png\",\"window\"]}"
  sleep 4
  order=$(q vo-passes | jq -r '[.fresh[].desc] | join(" > ")')
  kill $P; wait $P 2>/dev/null
  a=$(ffmpeg -hide_banner -i ${v}_$F.png -i exact_bicubic_$F.png -lavfi psnr -f null - 2>&1 | grep -o "average:[0-9.inf]*")
  b=$(ffmpeg -hide_banner -i ${v}_$F.png -i vs_$F.png -lavfi psnr -f null - 2>&1 | grep -o "average:[0-9.inf]*")
  echo "$v frame $F: vs exact $a, vs VapourSynth $b, errors $(grep -c '\[e\]' /out/gpu/check/$v.log) | $order"
done
