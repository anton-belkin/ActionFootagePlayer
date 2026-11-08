#!/bin/bash
# The table shader's load time: from mpv's start to the first frame shown, with no shader, the clip's table (3 MB) and
# a table as large as an hour of 4K60 with knots (the every-row table of this clip, 165 MB); 3 runs each
. /gpu/common.sh
for v in none bicubic bicubic_rows; do
  for i in 1 2 3; do
    S=""; [ $v != none ] && S="--glsl-shaders=/gpu/w_$v.glsl"
    t0=$(date +%s.%N)
    mpv $BASE --hwdec=vaapi --profile=fast --frames=1 --log-file=/out/gpu/load_$v.log $S $C >/dev/null 2>&1
    t1=$(date +%s.%N)
    echo "$v run $i: $(awk -v a=$t0 -v b=$t1 'BEGIN{printf "%.2f", b-a}') s to the first frame and out, rss n/a"
  done
done
