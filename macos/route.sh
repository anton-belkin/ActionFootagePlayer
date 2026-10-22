#!/bin/sh
# route.sh <video>...: opens the videos with Steady Player when the first one is ours, else with the fallback
# player. Ours: a Gyroflow project next to it, or (gyroflow-probe) a project embedded in it, motion data to
# stabilize it with, or D-Log M. Only the first file decides; Steady Player plays the rest of the folder anyway.
#
# @PLAYER_ID@ is filled in at install. Fallbacks are set by `gyroflow-router on`, per extension:
#   defaults read @PLAYER_ID@ fallback-mp4

first=$1
probe="$HOME/.config/mpv/scripts/gyroflow/gyroflow-probe"

ours=0
if [ -f "${first%.*}.gyroflow" ] || [ -f "$first.gyroflow" ]; then
    ours=1
elif [ -x "$probe" ]; then
    case "$("$probe" "$first" 2>/dev/null)" in
        *'"color":"dlogm"'* | *'"mode":"embedded"'* | *'"mode":"default"'*) ours=1 ;;
    esac
fi

if [ $ours = 1 ]; then
    exec /usr/bin/open -b @PLAYER_ID@ "$@"
fi
ext=$(printf %s "${first##*.}" | tr '[:upper:]' '[:lower:]')
fallback=$(defaults read @PLAYER_ID@ "fallback-$ext" 2>/dev/null || defaults read @PLAYER_ID@ fallback 2>/dev/null)
exec /usr/bin/open -b "${fallback:-com.apple.QuickTimePlayerX}" "$@"
