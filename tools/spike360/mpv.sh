#!/bin/bash
# tools/spike360/mpv.sh [mpv options...]: the app's own mpv (ActionFootagePlayer.app's Contents/Helpers/mpv, with its
# Python, VapourSynth and MoltenVK, set up as Contents/MacOS/ActionFootagePlayer does), but with no config and none
# of the app's plugins: the throughput spike measures the bare chain. APP names another app (default
# ~/Applications/ActionFootagePlayer.app, as tools/dev_app.sh installs it).
set -euo pipefail
app=${APP:-$HOME/Applications/ActionFootagePlayer.app}
contents="$app/Contents"
res="$contents/Resources"
[ -x "$contents/Helpers/mpv" ] || { echo "no mpv of the app's own in $app (tools/dev_app.sh builds one)" >&2; exit 1; }
export PATH="$res/bin:$contents/Helpers:$PATH"
for home in "$contents"/Frameworks/Python.framework/Versions/[0-9]*; do export PYTHONHOME="$home"; done
for vs in "$PYTHONHOME"/lib/python*/site-packages/vapoursynth/libvsscript.dylib; do
    [ -e "$vs" ] && export VSSCRIPT_PATH="$vs"
done
export VK_DRIVER_FILES="$res/vulkan/icd.d/MoltenVK_icd.json"
export VK_ICD_FILENAMES="$VK_DRIVER_FILES"
exec "$contents/Helpers/mpv" --no-config "$@"
