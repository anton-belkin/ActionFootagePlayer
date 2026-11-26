#!/bin/bash
# tools/sample_clip.sh: the sample dive clip the apps carry, from this repository's `sample-clips` release, kept in
# build/sample/. Prints its path, or nothing when the release or the file isn't there (or there's no network): the
# app is then made without it. tools/make_app.sh (SAMPLE_CLIP), tools/make_release.sh, tools/dev_app.sh and
# tools/make_windows_app.sh use it.
#
# The copy kept is used again while its size matches the release's; a file replaced in the release is downloaded
# again. SAMPLE_CLIP_URL: another file (a test), SAMPLE_CLIP=none: none.
set -uo pipefail
cd "$(dirname "$0")/.."

NAME="Sample dive - Osmo Action 6.mp4"
URL=${SAMPLE_CLIP_URL:-"https://github.com/anton-belkin/ActionFootagePlayer/releases/download/sample-clips/Sample%20dive%20-%20Osmo%20Action%206.mp4"}
[ "${SAMPLE_CLIP:-}" != none ] || exit 0

dir=build/sample
out="$dir/$NAME"
# The release's size (after GitHub's redirect to its storage); none: no such file, or no network
size=
if headers=$(curl -fsIL --retry 2 --max-time 30 "$URL" 2>/dev/null); then
    size=$(tr -d '\r' <<< "$headers" | awk 'tolower($1) == "content-length:" { n = $2 } END { print n }')
fi
if [ -z "$size" ] || [ "$size" = 0 ]; then
    if [ -f "$out" ]; then
        echo "The sample clip: the release didn't answer, the copy kept is used" >&2
        echo "$PWD/$out"
    else
        echo "No sample clip ($URL isn't there): the app is made without it" >&2
    fi
    exit 0
fi
if [ ! -f "$out" ] || [ "$(wc -c < "$out" | tr -d ' ')" != "$size" ]; then
    mkdir -p "$dir"
    echo "== Downloading the sample clip ($((size / 1000000)) MB)" >&2
    if ! curl -fsSL --retry 3 -o "$out.part" "$URL" || [ "$(wc -c < "$out.part" | tr -d ' ')" != "$size" ]; then
        rm -f "$out.part"
        echo "The sample clip didn't download: the app is made without it" >&2
        exit 0
    fi
    mv "$out.part" "$out"
fi
echo "$PWD/$out"
