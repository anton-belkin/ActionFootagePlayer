#!/bin/bash
# tools/latency360/run.sh <log> <clip> [--pause] [--hz=N] [--seconds=S] [--screen=NAME] [--fs] [-- mpv args...]:
# one run of the 360 view's input-to-light latency in the app's mpv (build/mpv-afp/out/mpv, or $MPV), the view360
# script from this checkout on a stitched clip, then tools/latency360/summarize.py on its log.
#   --hz=N      made-up two-finger scrolls N times a second (--macos-latency-test); without it, real fingers
#   --pause     the clip paused (default: playing)
#   --screen    the NSScreen's name, e.g. "Built-in Retina Display" (the 120 Hz XDR) or "DELL P2217H"
#   --fs        full screen
# Takes the screen: a window for the run's seconds (+ ~3 s).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
log="$1"; clip="$2"; shift 2
hz=0; seconds=10; pause=no; screen=""; fs=()
extra=()
while [ $# -gt 0 ]; do
    case "$1" in
        --hz=*) hz="${1#*=}" ;;
        --seconds=*) seconds="${1#*=}" ;;
        --pause) pause=yes ;;
        --screen=*) screen="${1#*=}" ;;
        --fs) fs=(--fs) ;;
        --) shift; extra=("$@"); break ;;
        *) echo "unknown: $1" >&2; exit 2 ;;
    esac
    shift
done
mpv="${MPV:-$repo/build/mpv-afp/out/mpv}"
scr=()
[ -n "$screen" ] && scr=("--screen-name=$screen" "--fs-screen-name=$screen")
if ioreg -n Root -d1 | grep -q '"CGSSessionScreenIsLocked"=Yes'; then
    echo "The screen is locked: gpu-next would hang" >&2; exit 1
fi
"$mpv" --no-config --vo=gpu-next --hwdec=auto --geometry=1280x720 --loop-file=inf \
    --script="$repo/mpv/scripts/view360" --script="$here/drive.lua" \
    --script-opts="latency360-hz=$hz,latency360-seconds=$seconds,latency360-pause=$pause" \
    --macos-latency-log --msg-level=all=warn,latency=info,drive=info \
    ${scr[@]+"${scr[@]}"} ${fs[@]+"${fs[@]}"} ${extra[@]+"${extra[@]}"} "$clip" > "$log" 2>&1 || true
python3 "$here/summarize.py" "$log"
