#!/bin/zsh
# The "Stabilize + LUT" Quick Action: gyroflow-export on the selected videos, one after another, in the
# background (the Quick Action returns at once), with a notification as each one is done. The log of the last
# run is ~/Library/Logs/gyroflow-export.log.

export_bin="$HOME/.config/mpv/gyroflow/gyroflow-export"
log="$HOME/Library/Logs/gyroflow-export.log"

notify() { osascript -e "display notification \"$2\" with title \"$1\"" >/dev/null 2>&1; }

(
    : > "$log"
    notify "Stabilize + LUT" "Exporting ${#@} video(s)"
    for f in "$@"; do
        name=${f:t}
        out=$("$export_bin" "$f" 2>&1 | tee -a "$log" | tr '\r' '\n' | grep -v '%' )
        if saved=$(print -r -- "$out" | grep -o -- '-> /.*' | head -1) && [ -n "$saved" ]; then
            notify "Exported $name" "${saved#-> }"
        else
            notify "Not exported: $name" "$(print -r -- "$out" | tail -1)"
        fi
    done
) >/dev/null 2>&1 &!
