#!/bin/sh
# Every test in tests/, found by its name, so a new one runs without being listed anywhere:
#   tests/*_test.lua  with LuaJIT (one driven by a tests/<same name>.sh is run through that instead)
#   tests/*_test.py   with python3
#   tests/*_test.sh   with sh
# Rigs that need a live player, a screen or the NAS are named otherwise (*_ipc.py, *_e2e.py, photo_link_frame.py)
# and are not run here. Each runs from the repository's folder; all run even when one fails, and the exit is 1 if any
# did. CI runs this (.github/workflows/release.yml); so can you: sh tools/run_tests.sh [name-filter]
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1
only=${1:-}
failed=""
ran=0
for f in tests/*_test.lua tests/*_test.py tests/*_test.sh; do
    [ -e "$f" ] || continue
    case "$f" in *"$only"*) ;; *) continue ;; esac
    case "$f" in
        *.lua) [ -e "${f%.lua}.sh" ] && continue; cmd="luajit $f" ;;
        *.py) cmd="python3 $f" ;;
        *.sh) cmd="sh $f" ;;
    esac
    echo "== $f"
    ran=$((ran + 1))
    if ! $cmd; then failed="$failed $f"; fi
done
echo
if [ -n "$failed" ]; then
    echo "$ran test files, failed:$failed"
    exit 1
fi
echo "$ran test files, all passed"
