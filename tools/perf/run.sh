#!/bin/bash
# tools/perf/run.sh [options] <clip>: the app plays the clip through tools/perf/scenario.lua (start, steady playback,
# fullscreen, the stabilization's views, the looks, seeks, the next clip) and what each action cost is written to a
# folder of its own, ~/ActionFootagePlayer-perf/<date>-<label>/ (issue #43):
#
#   summary.csv      one row per step (scenario.lua says what's in it)
#   samples.csv      the counters every 100 ms
#   meta.txt         what played and how
#   perf.log         the Gyroflow plugin's timings over the run (its `perf` lines): Engine::load by stage, and once a
#                    second the frames' rate, wait and GPU time, the sphere's writes
#   vs-gyroflow.log  the plugin's whole log over the run
#   mpv.log          mpv's log (verbose)
#   run.txt          the Mac, the app's version, the commit, the displays, the power and thermal state
#
# macOS, with the app as tools/dev_app.sh puts it in ~/Applications (built from this checkout: the plugin's `perf`
# lines need it). The screen must stay unlocked: mpv can't show frames on a locked screen. Don't touch the window
# while it runs, unless --monitor asks you to move it.
#
#   --label <name>    in the folder's name and the CSVs (default: the clip's name)
#   --cold            unmount and mount the clip's volume first, so it's read cold (an SD card, a USB drive; not the
#                     Mac's own disk). Fails while something else holds the volume
#   --monitor         also the window moved to the other monitor and back, by hand, when the OSD says
#   --view <view>     the stabilization's view the clip starts in, normal (default) or wide, whatever gyroflow.conf
#                     says; log clips start in HDR, whatever dlogm.conf says. So runs compare
#   --steps <list>    the steps, comma separated (scenario.lua's names)
#   --observe <s>     how long each action is watched (default 5)
#   --app <path>      the app (default ~/Applications/ActionFootagePlayer.app, else /Applications's)
#   --keep            leave the app open at the end
# Anything after -- goes to mpv as it is: -- --screen=1 --fs-screen=1 opens the window on another display.
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
label="" cold=0 monitor=0 view=normal steps="" observe="" app="" keep=0 clip=""
extra=()
while [ $# -gt 0 ]; do
    case "$1" in
        --label) label=$2; shift 2 ;;
        --cold) cold=1; shift ;;
        --monitor) monitor=1; shift ;;
        --view) view=$2; shift 2 ;;
        --steps) steps=$2; shift 2 ;;
        --observe) observe=$2; shift 2 ;;
        --app) app=$2; shift 2 ;;
        --keep) keep=1; shift ;;
        --) shift; extra=("$@"); break ;;
        -*) echo "unknown option $1 (see the top of $0)" >&2; exit 2 ;;
        *) clip=$1; shift ;;
    esac
done
[ -n "$clip" ] || { echo "usage: $0 [--label <name>] [--cold] [--monitor] [--steps <list>] [--observe <s>] <clip>" >&2; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo "tools/perf/run.sh measures the Mac app" >&2; exit 1; }
[ -f "$clip" ] || { echo "no such clip: $clip" >&2; exit 1; }
clip=$(cd "$(dirname "$clip")" && pwd)/$(basename "$clip")

if [ -z "$app" ]; then
    for candidate in "$HOME/Applications/ActionFootagePlayer.app" /Applications/ActionFootagePlayer.app; do
        [ -d "$candidate" ] && { app=$candidate; break; }
    done
fi
[ -x "$app/Contents/MacOS/ActionFootagePlayer" ] || { echo "no ActionFootagePlayer.app (build it with tools/dev_app.sh, or --app)" >&2; exit 1; }

[ -n "$label" ] || label=$(basename "${clip%.*}")
label=$(printf '%s' "$label" | tr -c 'A-Za-z0-9._-' '_')
out="$HOME/ActionFootagePlayer-perf/$(date +%Y%m%d-%H%M%S)-$label"
mkdir -p "$out"

if [ $cold = 1 ]; then
    device=$(stat -f %Sd "$clip")
    echo "== Reading cold: unmounting and mounting /dev/$device"
    diskutil unmount "/dev/$device" >/dev/null
    diskutil mount "/dev/$device" >/dev/null
    for _ in $(seq 50); do [ -f "$clip" ] && break; sleep 0.1; done
    [ -f "$clip" ] || { echo "the clip isn't back after mounting /dev/$device" >&2; exit 1; }
fi

{
    echo "label=$label"
    echo "clip=$clip"
    echo "cold=$cold"
    echo "view=$view"
    echo "commit=$(git -C "$repo" describe --always --dirty 2>/dev/null || echo unknown) ($(git -C "$repo" rev-parse --abbrev-ref HEAD 2>/dev/null || echo ?))"
    echo "app=$app $(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist" 2>/dev/null || echo ?)"
    echo "macos=$(sw_vers -productVersion)"
    echo "cpu=$(sysctl -n machdep.cpu.brand_string) ($(sysctl -n hw.memsize | awk '{printf "%.0f GB", $1 / 1073741824}'))"
    echo "power=$(pmset -g batt | head -1)"
    echo "lowpower=$(pmset -g | awk '/lowpowermode/ {print $2}')"
    echo "thermal:"; pmset -g therm 2>/dev/null | sed 's/^/  /'
    echo "displays:"; system_profiler SPDisplaysDataType 2>/dev/null | grep -E 'Chipset|Resolution|UI Looks like|Main Display|Refresh|Display Type|Connection' | sed 's/^ */  /'
} > "$out/run.txt"

log="$HOME/Library/Logs/vs-gyroflow.log"
offset=$(stat -f %z "$log" 2>/dev/null || echo 0)

case "$view" in normal|wide) ;; *) echo "--view: normal or wide" >&2; exit 2 ;; esac
opts=(--script-opts-append="perf-out=$out" --script-opts-append="perf-label=$label"
      --script-opts-append="gyroflow-view=$view" --script-opts-append="dlogm-mode=hdr")
[ -n "$steps" ] && opts+=(--script-opts-append="perf-steps=$steps")
[ -n "$observe" ] && opts+=(--script-opts-append="perf-observe=$observe")
[ $monitor = 1 ] && opts+=(--script-opts-append="perf-monitor=yes")
[ $keep = 1 ] && opts+=(--script-opts-append="perf-quit=no")

echo "== Playing $clip ($label); results in $out"
launched=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
# The clip loops, so a short one doesn't run into the next before the scenario asks for it. The window comes up first,
# as when Finder opens the app (pseudo-gui): the clip loads knowing the display it's on
VS_GYROFLOW_PERF=1 "$app/Contents/MacOS/ActionFootagePlayer" \
    "--script=$repo/tools/perf/scenario.lua" --script-opts-append="perf-launched=$launched" "${opts[@]}" \
    "--log-file=$out/mpv.log" --loop-file=inf --force-window=immediate ${extra[@]+"${extra[@]}"} "$clip" || true

tail -c +"$((offset + 1))" "$log" > "$out/vs-gyroflow.log" 2>/dev/null || true
grep ' perf ' "$out/vs-gyroflow.log" > "$out/perf.log" || true

echo
if [ -s "$out/summary.csv" ]; then
    column -s, -t < "$out/summary.csv"
else
    echo "no summary: see $out/mpv.log"
fi
echo
echo "Results: $out"
