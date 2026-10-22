#!/bin/sh
# Starts mpv as an app of its own, for Steady Player.app (`open -na .../Contents/Resources/mpv.app --args <file>...`).
# mpv started from the applet's `do shell script ... &` stayed attached to the applet, which quits right
# away, and macOS ran it as a background process: hardware decoding and the GPU throttled, a 4-5 s
# stall at start and stabilized playback at a fraction of realtime.
#
# mpv gets the plugins in Steady Player.app (Contents/Resources/plugins/), except one that mpv's config has a copy
# of (install.sh puts them there for mpv in a terminal): mpv would load both. What the plugins can't keep in the
# app, DJI's LUT and your default preset, is in ~/Library/Application Support/Steady Player/ (DLOGM_DATA,
# GYROFLOW_DATA). mpv is Homebrew's.

res=$(cd "$(dirname "$0")/../../.." && pwd)
data="$HOME/Library/Application Support/Steady Player"
export DLOGM_DATA="$data/dlogm" GYROFLOW_DATA="$data/gyroflow"
config=${MPV_HOME:-$HOME/.config/mpv}
for plugin in gyroflow dlogm; do
    [ -e "$config/scripts/$plugin" ] || set -- "--script=$res/plugins/$plugin" "$@"
done

# DJI's LUT (Ctrl+s, exports with --sdr), fetched in the background when the setup hasn't got it yet
[ -f "$DLOGM_DATA/shaders/dlogm_lut3d.glsl" ] || "$res/setup.sh" lut >/dev/null 2>&1 &

for mpv in /opt/homebrew/bin/mpv /usr/local/bin/mpv /opt/local/bin/mpv; do
    [ -x "$mpv" ] && exec "$mpv" "$@"
done
osascript -e 'display alert "Steady Player needs mpv" message "Install it with Homebrew: brew install mpv"' >/dev/null
exit 1
