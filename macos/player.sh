#!/bin/sh
# ActionFootagePlayer's executable (Contents/MacOS/ActionFootagePlayer): mpv, with this project's plugins, as the app
# itself. With MPVBUNDLE mpv runs as the app of a bundle: in the Dock and the menu bar as ActionFootagePlayer, with
# its icon (mpv on its own would put up its own), taking the videos Finder opens with it (Apple Events, which reach
# mpv once it's running: this script execs it). A video opened while one plays takes its place.
#
# The app is whole by itself: mpv gets the plugins in ActionFootagePlayer.app (Contents/Resources/plugins/) and no
# others, and its settings (Contents/Resources/app.conf), with a config folder of its own,
# ~/Library/Application Support/ActionFootagePlayer/mpv/, where the plugins keep what you set (script-opts/). An mpv
# you may also have, its ~/.config/mpv and the plugins install.sh puts there, isn't read: the first time, only the
# plugins' settings are copied over from it. What the plugins can't keep in the app, DJI's LUT and your default
# preset, is in ~/Library/Application Support/ActionFootagePlayer/ too (DLOGM_DATA, GYROFLOW_DATA). mpv is the app's
# own when it has one (tools/bundle_runtime.py: in Contents/Helpers/, with Python, VapourSynth and MoltenVK beside
# it), else Homebrew's.
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

# mpv's config folder, the app's own. The first time, the plugins' settings come over from mpv's (up to 0.6.0 the app
# read that one)
config="$data/mpv"
if [ ! -d "$config" ]; then
    mkdir -p "$config/script-opts"
    for opts in dlogm gyroflow dji_telemetry highlights footage_menu; do
        from="${MPV_HOME:-$HOME/.config/mpv}/script-opts/$opts.conf"
        [ -f "$from" ] && cp "$from" "$config/script-opts/"
    done
fi
# Started by Finder, the videos come as Apple Events once mpv runs: until then it waits, its window not up yet. Up
# before the video, it doesn't always switch to HDR (finder-launch.lua, which puts it up when no video comes)
[ $# = 0 ] && set -- --player-operation-mode=pseudo-gui --force-window=no "--script=$res/finder-launch.lua"
for plugin in gyroflow dlogm dji-telemetry highlights footage-menu; do
    set -- "--script=$res/plugins/$plugin" "$@"
done
set -- "--config-dir=$config" --load-scripts=no "--include=$res/app.conf" "$@"
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
contents="$app/Contents"
if [ -x "$contents/Helpers/mpv" ]; then
    # VapourSynth's Python (and the `vapoursynth` command that tells VSScript where it is), the Vulkan driver
    # (mpv's HDR output), and the ffmpeg gyroflow-export runs
    export PATH="$res/bin:$contents/Helpers:$PATH"
    for home in "$contents"/Frameworks/Python.framework/Versions/[0-9]*; do export PYTHONHOME="$home"; done
    # VapourSynth 74 and later: VSScript is in the vapoursynth package, where mpv doesn't look for it
    for vs in "$PYTHONHOME"/lib/python*/site-packages/vapoursynth/libvsscript.dylib; do
        [ -e "$vs" ] && export VSSCRIPT_PATH="$vs"
    done
    # VSScript finds that Python in ~/.config/vapoursynth/vapoursynth.toml, by its python3 and that file's time. An app
    # put in place of another (an update, tools/dev_app.sh) has a newer one, and VSScript's own `vapoursynth config`
    # fails from inside mpv: the stabilization is then off. So it's written here first when it isn't this app's
    py="$PYTHONHOME/bin/python3"
    entry="\"$py\",\"$PYTHONHOME/Python\",\"$(stat -L -f %m "$py" 2>/dev/null)\""
    grep -qF "$entry" "${XDG_CONFIG_HOME:-$HOME/.config}/vapoursynth/vapoursynth.toml" 2>/dev/null \
        || "$res/bin/vapoursynth" config >/dev/null 2>&1 || true
    export VK_DRIVER_FILES="$res/vulkan/icd.d/MoltenVK_icd.json"
    export VK_ICD_FILENAMES="$VK_DRIVER_FILES" GYROFLOW_TOOLS="$contents/Helpers"
    exec "$contents/Helpers/mpv" "$@"
fi
for mpv in /opt/homebrew/bin/mpv /usr/local/bin/mpv /opt/local/bin/mpv; do
    [ -x "$mpv" ] || continue
    # The same with Homebrew's VapourSynth (74 and later): without it mpv can't make the gyroflow filter, and plays
    # the sound alone
    for vs in "${mpv%/bin/mpv}"/opt/vapoursynth/libexec/lib/python*/site-packages/vapoursynth/libvsscript.dylib; do
        [ -e "$vs" ] && export VSSCRIPT_PATH="$vs"
    done
    exec "$mpv" "$@"
done
osascript -e 'display alert "ActionFootagePlayer needs mpv" message "Install it with Homebrew: brew install mpv"' >/dev/null
exit 1
