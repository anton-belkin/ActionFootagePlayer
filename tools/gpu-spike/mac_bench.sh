#!/bin/bash
# The M1: the app's mpv (has PTS), a 1920x1080 window, VideoToolbox; each shader 20 s timed (drops, GPU time per pass),
# then 12 s untimed. Opens a player window: only with Anton's OK, screen awake (caffeinate -d)
cd "$(dirname "$0")"
MPV=~/Applications/ActionFootagePlayer.app/Contents/Helpers/mpv
C=${CLIP:?a local copy of clip 0126}
SOCK=/tmp/afp-gpu-bench.sock
BASE="--no-config --vo=gpu-next --ao=null --fs --input-ipc-server=$SOCK --hwdec=videotoolbox --profile=fast --start=1 --autocreate-playlist=no"
q() { echo "{\"command\":[\"get_property\",\"$1\"]}" | nc -U -w 1 $SOCK 2>/dev/null | jq -c 'select(has("error")) | .data' | head -1; }
run() {
  local name=$1; shift
  $MPV $BASE "$@" $C >/tmp/afp-gpu-$name.log 2>&1 &
  local P=$!
  sleep 8; local p0=$(q time-pos) d0=$(q frame-drop-count) w0=$(date +%s.%N)
  sleep 20; local p1=$(q time-pos) d1=$(q frame-drop-count) w1=$(date +%s.%N)
  local passes=$(q vo-passes | jq -r '[.fresh[] | "\(.desc)=\((.avg/1000)|floor)us/\((.peak/1000)|floor)"] | join(" ")')
  local osd=$(q osd-dimensions | jq -c '[.w,.h]')
  kill $P; wait $P 2>/dev/null
  $MPV $BASE --untimed --video-sync=desync "$@" $C >/dev/null 2>&1 &
  P=$!
  sleep 5; local f0=$(q estimated-frame-number) u0=$(date +%s.%N)
  sleep 12; local f1=$(q estimated-frame-number) u1=$(date +%s.%N)
  kill $P; wait $P 2>/dev/null
  awk -v n=$name -v p0=$p0 -v p1=$p1 -v d0=$d0 -v d1=$d1 -v w0=$w0 -v w1=$w1 -v f0=$f0 -v f1=$f1 -v u0=$u0 -v u1=$u1 -v ps="$passes" -v osd="$osd" 'BEGIN {
    w=w1-w0; v=p1-p0; printf "%-22s %.2fx, dropped %3d, shown %5.1f fps, untimed %5.1f fps %s | %s\n", n, v/w, d1-d0, (v*59.94-(d1-d0))/w, (f1-f0)/(u1-u0), osd, ps }'
}
for v in ${VARIANTS:-none bilinear bspline bicubic bicubic_shrink lanczos4 lanczos4_shrink ewa bicubic_rs bicubic_planes}; do
  if [ $v = none ]; then run none; else run $v --glsl-shaders=${SHADERS:-$PWD}/w_$v.glsl; fi
done
