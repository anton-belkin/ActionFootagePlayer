#!/bin/bash
# Frame identity and picture against the VapourSynth path: frames paused and shot (window: 1920x1080, as shown) both
# ways, then PSNR of the shader's frame N against VapourSynth's N-1, N, N+1
. /gpu/common.sh
mkdir -p /out/gpu/check; cd /out/gpu/check; rm -f *.png
SHADER=${SHADER:-/gpu/w_bicubic.glsl}
shot() { # name, start seconds, mpv args...
  local name=$1 t=$2; shift 2
  mpv $BASE --pause --start=$t "$@" $C >/out/gpu/check/$name.log 2>&1 &
  local P=$!
  sleep ${WAIT:-8}
  cmd "{\"command\":[\"screenshot-to-file\",\"/out/gpu/check/$name.png\",\"window\"]}"
  sleep 4
  echo "$name pts=$(q time-pos) frame=$(q estimated-frame-number)"
  kill $P; wait $P 2>/dev/null
}
for F in ${FRAMES:-600 1049 1181 2000}; do
  t=$(awk -v f=$F 'BEGIN{printf "%.6f", f*1001/60000}')
  tm=$(awk -v f=$F 'BEGIN{printf "%.6f", (f-1)*1001/60000}')
  tp=$(awk -v f=$F 'BEGIN{printf "%.6f", (f+1)*1001/60000}')
  shot gpu_$F $t --hwdec=vaapi --glsl-shaders=$SHADER
  WAIT=12 shot vs_$F $t --hwdec=vaapi-copy "--vf=$VS"
  WAIT=12 shot vsm_$F $tm --hwdec=vaapi-copy "--vf=$VS"
  WAIT=12 shot vsp_$F $tp --hwdec=vaapi-copy "--vf=$VS"
  for o in vsm vs vsp; do
    p=$(ffmpeg -hide_banner -i gpu_$F.png -i ${o}_$F.png -lavfi psnr -f null - 2>&1 | grep -o "average:[0-9.inf]*")
    echo "frame $F: shader vs $o: $p"
  done
done
grep -h "\[e\]\|error\|Error" /out/gpu/check/gpu_*.log | sort | uniq -c | head
