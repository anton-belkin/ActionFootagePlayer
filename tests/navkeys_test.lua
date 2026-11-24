-- The arrows with a modifier (mpv/scripts/side-panel/navkeys.lua): which keys do what on each platform, how a key is
-- written under the bar's buttons, and that a key bound in your input.conf is left alone. The keys in a running mpv:
-- tests/navkeys_ipc.py. From the repository's folder: luajit tests/navkeys_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path
local nk = require 'navkeys'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function cmd_of(list, key)
    for _, b in ipairs(list) do if b[1] == key then return b[2] end end
end

-- Option a frame, Cmd the playlist on a Mac; Ctrl elsewhere
local mac, win = nk.bindings(true), nk.bindings(false)
expect("mac: frame on", cmd_of(mac, "Alt+RIGHT"), "script-binding gyroflow/frame-on")
expect("mac: frame back", cmd_of(mac, "Alt+LEFT"), "script-binding gyroflow/frame-back")
expect("mac: next", cmd_of(mac, "Meta+RIGHT"), "playlist-next")
expect("mac: previous", cmd_of(mac, "Meta+LEFT"), "playlist-prev")
expect("mac: Ctrl+arrows left to the system", cmd_of(mac, "Ctrl+RIGHT"), nil)
expect("windows: next", cmd_of(win, "Ctrl+RIGHT"), "playlist-next")
expect("windows: frame", cmd_of(win, "Alt+LEFT"), "script-binding gyroflow/frame-back")
expect("windows: no Win key", cmd_of(win, "Meta+RIGHT"), nil)

-- Written as people read them
expect("mac cmd", nk.key_label("Meta+RIGHT", true), "⌘→")
expect("mac option", nk.key_label("Alt+LEFT", true), "⌥←")
expect("mac ctrl", nk.key_label("Ctrl+]", true), "⌃]")
expect("mac two", nk.key_label("Ctrl+Shift+LEFT", true), "⌃⇧←")
expect("windows", nk.key_label("Ctrl+RIGHT", false), "Ctrl+→")
expect("windows alt", nk.key_label("Alt+LEFT", false), "Alt+←")
expect("plain", nk.key_label(">", true), ">")
expect("a letter", nk.key_label("Shift+M", false), "Shift+M")
expect("the plus key", nk.key_label("Ctrl++", false), "Ctrl++")
expect("none", nk.key_label(nil, true), nil)

-- Installed: bound with mpv's keybind, but not over a key of your input.conf (mpv's own defaults are weak)
local ran = {}
local fake = {
    get_property_native = function() return {
        { key = "Alt+LEFT", cmd = "add video-pan-x 0.1", is_weak = true, section = "default" },
        { key = "Meta+RIGHT", cmd = "seek 30", is_weak = false, section = "default" },
        { key = "Meta+LEFT", cmd = "script-binding x/y", is_weak = false, section = "input_x" },
    } end,
    commandv = function(...) ran[#ran + 1] = table.concat({ ... }, " ", 1, 3) end,
}
nk.install(fake, true)
expect("bound: three", #ran, 3)
expect("over mpv's own", ran[1], "keybind Alt+LEFT script-binding gyroflow/frame-back")
local yours = false
for _, r in ipairs(ran) do if r:find("Meta+RIGHT", 1, true) then yours = true end end
expect("your input.conf's left as it is", yours, false)
expect("a plugin's own section doesn't stop it", ran[3], "keybind Meta+LEFT playlist-prev")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
