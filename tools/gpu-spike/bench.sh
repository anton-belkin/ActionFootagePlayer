#!/bin/bash
# Each shader: 20 s timed (drops, shown fps, GPU time per pass from vo-passes), then 15 s untimed (the most it does)
. /gpu/common.sh
mkdir -p /out/gpu
run() { # label, mpv args...
  local name=$1; shift
  mpv $BASE --hwdec=vaapi --profile=fast --start=1 "$@" $C >/out/gpu/$name.log 2>&1 &
  local P=$!
  sleep 10; local p0=$(q time-pos) d0=$(q frame-drop-count) w0=$(date +%s.%N)
  sleep 20; local p1=$(q time-pos) d1=$(q frame-drop-count) w1=$(date +%s.%N)
  local passes=$(q vo-passes | jq -r '[.fresh[] | "\(.desc)=\((.avg/1000)|floor)us/\((.peak/1000)|floor)"] | join(" ")')
  kill $P; wait $P 2>/dev/null
  mpv $BASE --hwdec=vaapi --profile=fast --start=1 --untimed --video-sync=desync "$@" $C >/dev/null 2>&1 &
  P=$!
  sleep 6; local f0=$(q estimated-frame-number) u0=$(date +%s.%N)
  sleep 12; local f1=$(q estimated-frame-number) u1=$(date +%s.%N)
  kill $P; wait $P 2>/dev/null
  awk -v n=$name -v p0=$p0 -v p1=$p1 -v d0=$d0 -v d1=$d1 -v w0=$w0 -v w1=$w1 -v f0=$f0 -v f1=$f1 -v u0=$u0 -v u1=$u1 -v ps="$passes" 'BEGIN {
    w=w1-w0; v=p1-p0; printf "%-22s %.2fx, dropped %3d, shown %5.1f fps, untimed %5.1f fps | %s\n", n, v/w, d1-d0, (v*59.94-(d1-d0))/w, (f1-f0)/(u1-u0), ps }'
  grep -q "\[e\]" /out/gpu/$name.log && grep "\[e\]" /out/gpu/$name.log | head -3
}
for v in ${VARIANTS:-none bilinear bspline bicubic bicubic_shrink lanczos4 lanczos4_shrink ewa bicubic_rs bicubic_rows bicubic_planes bilinear_planes}; do
  if [ $v = none ]; then run none; else run $v --glsl-shaders=/gpu/w_$v.glsl; fi
done
