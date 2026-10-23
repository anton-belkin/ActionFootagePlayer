#!/bin/sh
# ActionFootagePlayer's executable (Contents/MacOS/ActionFootagePlayer): mpv, with this project's plugins, as the app
# itself. With MPVBUNDLE mpv runs as the app of a bundle: in the Dock and the menu bar as ActionFootagePlayer, with
# its icon (mpv on its own would put up its own), taking the videos Finder opens with it (Apple Events, which reach
# mpv once it's running: this script execs it). A video opened while one plays takes its place.
#
# mpv gets the plugins in ActionFootagePlayer.app (Contents/Resources/plugins/), except one that mpv's config has a copy
# of (install.sh puts them there for mpv in a terminal): mpv would load both. What the plugins can't keep in the
# app, DJI's LUT and your default preset, is in ~/Library/Application Support/ActionFootagePlayer/ (DLOGM_DATA,
# GYROFLOW_DATA). mpv is Homebrew's.
#
# The first time it starts, it offers the setup (Contents/Resources/setup.sh, through setup-offer.lua): the default
# app for MP4 videos, the Finder Quick Actions and DJI's LUT. Once asked, or once set up (setup.sh, install.sh
# --apps), it doesn't again.

app=$(cd "$(dirname "$0")/../.." && pwd)
res="$app/Contents/Resources"
data="$HOME/Library/Application Support/ActionFootagePlayer"
# Steady Player's, as the app was called up to 0.3
old="$HOME/Library/Application Support/Steady Player"
[ -d "$old" ] && [ ! -e "$data" ] && mv "$old" "$data"
export DLOGM_DATA="$data/dlogm" GYROFLOW_DATA="$data/gyroflow"

# Downloaded, the whole app carries the quarantine flag, and macOS would refuse the plugin's binaries
xattr -p com.apple.quarantine "$app" >/dev/null 2>&1 && xattr -dr com.apple.quarantine "$app" 2>/dev/null

config=${MPV_HOME:-$HOME/.config/mpv}
# Started by Finder, the videos come as Apple Events once mpv runs: until then it waits, with its window up
[ $# = 0 ] && set -- --player-operation-mode=pseudo-gui
for plugin in gyroflow dlogm dji-telemetry footage-menu; do
    [ -e "$config/scripts/$plugin" ] || set -- "--script=$res/plugins/$plugin" "$@"
done
# The menu offers the setup again
set -- "--script-opts-append=footage_menu-setup=$res/setup.sh" "$@"

# The first time, the setup's offered (from mpv: see setup-offer.lua). After that, DJI's LUT (Ctrl+s, exports
# with --sdr) is fetched in the background when the setup hasn't got it
if [ ! -e "$data/setup-offered" ]; then
    mkdir -p "$data" && : > "$data/setup-offered"
    set -- "--script=$res/setup-offer.lua" "$@"
elif [ ! -f "$DLOGM_DATA/shaders/dlogm_lut3d.glsl" ]; then
    "$res/setup.sh" lut >/dev/null 2>&1 &
fi

export MPVBUNDLE=true
for mpv in /opt/homebrew/bin/mpv /usr/local/bin/mpv /opt/local/bin/mpv; do
    [ -x "$mpv" ] && exec "$mpv" "$@"
done
osascript -e 'display alert "ActionFootagePlayer needs mpv" message "Install it with Homebrew: brew install mpv"' >/dev/null
exit 1
