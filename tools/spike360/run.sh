#!/bin/bash
# tools/spike360/run.sh <results> <clip> <label> <join> <vs> [screen|null] [mpv options...]: one run of the
# throughput spike. The app's mpv (mpv.sh) plays <clip> through measure.lua with the chain <join> (hstack, none) and
# copy.vpy's <vs> (same, 8k, none, off), and writes to <results>/<label>.*:
#
#   .csv    mpv's counters every 0.5 s (measure.lua)
#   .top    mpv's CPU (% of one core) and memory, the system's CPU and the GPU's utilization, every second
#   .log    mpv's log
#
# null (the default): --vo=null --untimed, the chain as fast as it goes, without the screen: what decoding,
# joining and VapourSynth can do. screen: gpu-next on the built-in display, in full screen, at the clip's rate
# (takes the screen: ask first). Anything after goes to mpv.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$1 clip=$2 label=$3 join=$4 vs=$5 where=${6:-null}
# copy.vpy's VapourSynth frame cache in MB (CACHE; empty: VapourSynth's default, 8192 at R81)
opts=""
[ -n "${CACHE:-}" ] && opts=",spike360-cache=$CACHE"
shift 5; [ $# -gt 0 ] && shift
mkdir -p "$out"
base="$out/$label"

case "$where" in
    null) vo=(--vo=null --untimed) ;;
    screen) vo=(--vo=gpu-next --fs "--screen-name=Built-in Retina Display" "--fs-screen-name=Built-in Retina Display"
               --force-window=immediate) ;;
    *) echo "null or screen, not $where" >&2; exit 2 ;;
esac

# Nothing else heavy meanwhile (another session's build or tests, rsync to the NAS, an encode, another mpv): wait
# until the Mac is at least QUIET% idle (default 85) for a few seconds, up to 15 minutes, then go on regardless
quiet=${QUIET:-85}
for i in $(seq 300); do
    idle=$(top -l 3 -s 1 -n 0 | grep 'CPU usage' | tail -2 | sed -E 's/.* ([0-9.]+)% idle.*/\1/' | sort -n | head -1)
    [ "${idle%.*}" -ge "$quiet" ] && break
    [ $i = 1 ] && echo "waiting for the Mac to be ${quiet}% idle (now ${idle}%): $(ps -Ao %cpu=,comm= -r | head -2 | tr '\n' ' ')" >&2
done
[ "${idle%.*}" -ge "$quiet" ] || echo "warning: only ${idle}% idle: $(ps -Ao %cpu=,comm= -r | head -3 | tr '\n' ' ')" >&2

"$here/mpv.sh" "${vo[@]}" --ao=null --hwdec=videotoolbox-copy --autocreate-playlist=no --keep-open=no \
    "--script=$here/measure.lua" \
    "--script-opts=spike360-vpy=$here/copy.vpy,spike360-join=$join,spike360-vs=$vs,spike360-csv=$base.csv$opts" \
    "--log-file=$base.log" --really-quiet "$@" "$clip" &
pid=$!

# Every second: the system's CPU line, mpv's own row, the GPU's utilization, the memory compressed
{
    # (head and awk close their pipes early: no pipefail here)
    set +e +o pipefail
    while kill -0 $pid 2>/dev/null; do
        t=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
        top=$(top -l 2 -s 1 -n 1 -pid $pid -stats pid,cpu,mem 2>/dev/null | tail -n 30)
        sys=$(printf '%s\n' "$top" | grep 'CPU usage' | tail -1 | sed -E 's/.*: ([0-9.]+)% user, ([0-9.]+)% sys.*/\1 \2/')
        me=$(printf '%s\n' "$top" | awk -v p=$pid '$1 == p {print $2, $3}' | tail -1)
        gpu=$(ioreg -r -d 1 -c IOAccelerator 2>/dev/null | grep -o '"Device Utilization %"=[0-9]*' | head -1 | cut -d= -f2)
        # and what else is running hard (the busiest other process, % of one core)
        other=$(ps -Ao pid=,%cpu=,comm= -r | awk -v p=$pid '$1 != p {print $2, $3; exit}')
        # memory the system compressed (MB): mpv's frames crowding the 16 GB
        comp=$(printf '%s\n' "$top" | grep PhysMem | tail -1 | sed -E 's/.*, ([0-9]+)([MG]) compressor.*/\1 \2/' | awk '{print ($2 == "G") ? $1 * 1024 : $1}')
        echo "$t $sys $me ${gpu:--} ${comp:--} $other"
    done
} > "$base.top" &
wait $pid || true
wait
