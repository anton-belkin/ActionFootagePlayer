#!/bin/sh
# tools/scrub/build.sh <out>: afp-scrub (the seek bar's pictures, tools/scrub/afp-scrub.c), built on this computer's
# libav libraries, the ones its mpv links: pkg-config's, else FFMPEG_PREFIX's (Homebrew's ffmpeg when brew is there).
# tools/make_app.sh and install.sh run it.
set -eu
out=${1:?usage: tools/scrub/build.sh <out>}
src="$(dirname "$0")/afp-scrub.c"
libs="libavformat libavcodec libswscale libavutil"
if [ -z "${FFMPEG_PREFIX:-}" ] && command -v pkg-config >/dev/null 2>&1 && pkg-config --exists $libs 2>/dev/null; then
    flags=$(pkg-config --cflags --libs $libs)
else
    prefix=${FFMPEG_PREFIX:-}
    [ -n "$prefix" ] || ! command -v brew >/dev/null 2>&1 || prefix=$(brew --prefix ffmpeg 2>/dev/null || true)
    [ -n "$prefix" ] && [ -f "$prefix/include/libavformat/avformat.h" ] \
        || { echo "afp-scrub: no libav development files (ffmpeg's: libavformat-dev etc., ffmpeg-devel, brew install ffmpeg)" >&2; exit 1; }
    flags="-I$prefix/include -L$prefix/lib -lavformat -lavcodec -lswscale -lavutil"
fi
# On macOS, room in its header for tools/bundle_runtime.py to point it at the app's own copies of the libraries
extra=""
[ "$(uname -s)" != Darwin ] || extra="-Wl,-headerpad_max_install_names"
${CC:-cc} -O2 -Wall -o "$out" "$src" $flags -lpthread $extra
