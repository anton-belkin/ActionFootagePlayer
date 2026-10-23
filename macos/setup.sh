#!/bin/sh
# ActionFootagePlayer's setup, in Contents/Resources/ of the app. Opening the app on its own offers `setup.sh all`;
# install.sh --apps runs the parts.
#
#   setup.sh all [<cube>]       the three below
#   setup.sh lut [<cube>]       DJI's D-Log M -> Rec.709 LUT, and the sdr shader made from it, into
#                               ~/Library/Application Support/ActionFootagePlayer/dlogm/ (the dlogm plugin's DLOGM_DATA).
#                               DJI's LUT isn't redistributed: it's downloaded from DJI and checked, or <cube> is used
#   setup.sh default            ActionFootagePlayer becomes the default app for MP4 videos
#   setup.sh quick-actions      the Finder Quick Actions Play Stabilized and Stabilize + LUT, in ~/Library/Services
#                               (SERVICES_DIR overrides it)
#   setup.sh fetch-lut <out>    just the download, checked
#
# It also clears the download quarantine from the app: macOS would refuse the plugin's binaries otherwise.
set -eu

res=$(cd "$(dirname "$0")" && pwd)
app=${res%/Contents/Resources}
data="$HOME/Library/Application Support/ActionFootagePlayer"
# Steady Player's, as the app was called up to 0.3
old="$HOME/Library/Application Support/Steady Player"
[ -d "$old" ] && [ ! -e "$data" ] && mv "$old" "$data"
services=${SERVICES_DIR:-$HOME/Library/Services}

# DJI publishes it on the Osmo Action 6 downloads page (https://www.dji.com/osmo-action-6/downloads). The checksum
# is of version 11.17: if DJI updates the file, check it and update both.
DJI_LUT_URL="https://terra-1-g.djicdn.com/851d20f7b9f64838a34cd02351370894/AC206%20LUT/DJI%20OSMO%20Action%206%20D-LogM%20to%20Rec.709%20LUT-11.17.cube"
DJI_LUT_SHA256="1972eb9b05699ba19d4aaa67aaa5fb00b46935c60ba9219183289f79e044cbb2"

lut_ok() { [ -f "$1" ] && echo "$DJI_LUT_SHA256  $1" | shasum -a 256 -c --status 2>/dev/null; }

fetch_lut() {
    lut_ok "$1" && return 0
    curl -fsSL -o "$1.part" "$DJI_LUT_URL" || { rm -f "$1.part"; echo "Couldn't download DJI's LUT" >&2; return 1; }
    if ! lut_ok "$1.part"; then
        rm -f "$1.part"
        echo "The LUT DJI serves isn't the one this was made for (checksum differs)" >&2
        return 1
    fi
    mv "$1.part" "$1"
}

lut() {
    dlogm="$data/dlogm"
    cube="$dlogm/luts/dji-dlogm-rec709.cube"
    shader="$dlogm/shaders/dlogm_lut3d.glsl"
    # Explicit returns: `lut || ...` runs it with set -e off
    mkdir -p "$dlogm/luts" "$dlogm/shaders" || return 1
    if [ -n "${1:-}" ] && ! lut_ok "$cube"; then
        lut_ok "$1" || { echo "$1 isn't DJI's LUT 11.17 (checksum differs)" >&2; return 1; }
        cp "$1" "$cube" || return 1
    fi
    fetch_lut "$cube" || return 1
    if [ ! -f "$shader" ] || [ "$cube" -nt "$shader" ]; then
        python3 -I "$res/tools/cube_to_glsl.py" "$res/tools/dlogm_lut3d.glsl.in" "$cube" "$shader.part" >/dev/null \
            && mv "$shader.part" "$shader" || { rm -f "$shader.part"; return 1; }
    fi
    echo "DJI's Rec.709 LUT: $dlogm"
}

default() {
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
    # macOS asks to confirm it in a dialog; default-app.py waits for that, and exits with 3 when it hasn't come
    rc=0
    python3 -I "$res/tools/default-app.py" public.mpeg-4 "$player_id" 2>/dev/null || rc=$?
    case $rc in
        0) echo "ActionFootagePlayer is the default app for MP4 videos" ;;
        3) echo "ActionFootagePlayer becomes the default app for MP4 videos once you confirm it in macOS's dialog" ;;
        *) echo "Couldn't make ActionFootagePlayer the default app for MP4 videos" >&2; return $rc ;;
    esac
}

quick_actions() {
    mkdir -p "$services"
    python3 -I "$res/tools/make_workflow.py" "$services/Play Stabilized.workflow" "Play Stabilized" \
        NSTouchBarPlayTemplate "open -b $player_id \"\$@\""
    python3 -I "$res/tools/make_workflow.py" "$services/Stabilize + LUT.workflow" "Stabilize + LUT" \
        NSTouchBarExitFullScreenTemplate "\"$res/export-finder.sh\" \"\$@\""
    /System/Library/CoreServices/pbs -flush >/dev/null 2>&1 || true
    echo "Finder Quick Actions: Play Stabilized, Stabilize + LUT"
}

if [ "${1:-}" = fetch-lut ]; then
    [ -n "${2:-}" ] || { echo "usage: setup.sh fetch-lut <out.cube>" >&2; exit 2; }
    fetch_lut "$2"
    exit
fi

# The rest is the app's
case "$app" in
    */AppTranslocation/*)
        echo "Move ActionFootagePlayer to Applications first (macOS runs it from a read-only copy where it is now)" >&2
        exit 1 ;;
esac
player_id=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$app/Contents/Info.plist")
xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

case "${1:-}" in
    all) lut "${2:-}" || echo "(Without DJI's LUT, Ctrl+s and Rec.709 exports aren't available; the rest is)"
         default
         quick_actions ;;
    lut) lut "${2:-}" ;;
    default) default ;;
    quick-actions) quick_actions ;;
    *) echo "usage: setup.sh all [<cube>] | lut [<cube>] | default | quick-actions | fetch-lut <out.cube>" >&2; exit 2 ;;
esac
