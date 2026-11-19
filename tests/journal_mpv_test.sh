#!/bin/sh
# The responsiveness journal in a real mpv, no window (tests/journal_mpv_test.lua): mpv's own overlays, commands and
# subprocesses through the journal's hooks. The mpv: $MPV_BIN, else the app's (build/mpv-afp/out/mpv), else the one on
# PATH; none: skipped. From the repository's folder: sh tests/journal_mpv_test.sh
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
mpv=${MPV_BIN:-}
[ -z "$mpv" ] && [ -x "$root/build/mpv-afp/out/mpv" ] && mpv="$root/build/mpv-afp/out/mpv"
[ -z "$mpv" ] && mpv=$(command -v mpv || true)
if [ -z "$mpv" ]; then
    echo "journal_mpv_test: skipped (no mpv)"
    exit 0
fi
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
# A plugin killed by the journal leaves mpv idling (the bug this guards against did): stopped after 30 s, a failure
AFP_TEST_ROOT="$root" AFP_JOURNAL_DIR="$dir" "$mpv" --no-config --vo=null --ao=null --idle=yes --force-window=no \
    --msg-level=all=error,journal_mpv_test=info --script="$root/tests/journal_mpv_test.lua" &
pid=$!
( sleep 30; kill "$pid" 2>/dev/null && echo "journal_mpv_test: mpv stopped after 30 s (the test script died?)" ) &
watch=$!
wait "$pid"
code=$?
kill "$watch" 2>/dev/null; wait "$watch" 2>/dev/null
echo "journal_mpv_test: mpv exited $code ($mpv)"
exit $code
