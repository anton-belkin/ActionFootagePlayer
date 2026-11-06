#!/bin/bash
# tools/spike360/matrix.sh <clips> <results> [null|screen|sync]: the throughput spike's runs (run.sh) over the clips
# make_clips.sh made, then summarize.py's table.
#
#   null    without the screen, as fast as the chain goes: one track alone, both joined, joined + VapourSynth
#           handing the frame back, + a copy at the input's size, + a copy into 8192×4096
#   screen  gpu-next in full screen on the built-in display at the clip's rate: joined alone, + the copy at the
#           input's size, + into 8192×4096 (takes the screen: ask first)
#   sync    the seeks of measure.lua with copy.vpy reading both lenses' frame numbers, without the screen and with it
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
clips=${1:?usage: $0 <clips> <results> [null|screen|sync]} out=$2 what=${3:-null}
mkdir -p "$out"

# REPEAT runs of each (default 1), labelled -r1, -r2...: the Mac's other work makes single runs noisy
for clip in 5k7-30 8k-30 8k-60; do
  for r in $(seq "${REPEAT:-1}"); do
    f="$clips/$clip.mp4"
    [ -f "$f" ] || continue
    case "$what" in
        null)
            "$here/run.sh" "$out" "$f" "null-$clip-1track-r$r" none off null
            "$here/run.sh" "$out" "$f" "null-$clip-hstack-r$r" hstack off null
            "$here/run.sh" "$out" "$f" "null-$clip-vs-pass-r$r" hstack none null
            "$here/run.sh" "$out" "$f" "null-$clip-vs-same-r$r" hstack same null
            "$here/run.sh" "$out" "$f" "null-$clip-vs-8k-r$r" hstack 8k null
            ;;
        screen)
            "$here/run.sh" "$out" "$f" "screen-$clip-hstack-r$r" hstack off screen
            "$here/run.sh" "$out" "$f" "screen-$clip-vs-same-r$r" hstack same screen
            "$here/run.sh" "$out" "$f" "screen-$clip-vs-8k-r$r" hstack 8k screen
            ;;
        sync)
            for where in null screen; do
                log="$out/sync-$where-$clip.txt"
                rm -f "$log"
                vo=(--vo=null)
                [ $where = screen ] && vo=(--vo=gpu-next --fs "--screen-name=Built-in Retina Display"
                                           "--fs-screen-name=Built-in Retina Display")
                "$here/mpv.sh" "${vo[@]}" --ao=null --hwdec=videotoolbox-copy --autocreate-playlist=no \
                    "--script=$here/measure.lua" \
                    "--script-opts=spike360-vpy=$here/copy.vpy,spike360-vs=8k,spike360-sync=$log" \
                    "--log-file=$out/sync-$where-$clip.log" --really-quiet "$f" || true
                bad=$(awk '$2 != $3' "$log" | wc -l | tr -d ' ')
                echo "sync $where $clip: $(wc -l < "$log" | tr -d ' ') frames read, $bad with the lenses apart"
            done
            ;;
    esac
  done
done
[ "$what" = sync ] || python3 "$here/summarize.py" "$out"
