-- The arrow keys with a modifier, for moving through what plays (Anton, 2026-11-19):
--   ← / →                 5 s back / on (mpv's own, kept)          Shift+← / →   1 s (mpv's own, kept)
--   Option (Alt)+← / →    a frame back / on, paused                 (mpv's: the picture panned)
--   Cmd+← / →             the previous / next item of the playlist  (Windows, Linux: Ctrl+← / →; mpv's: subtitles)
-- Bound as input.conf's are (mpv's keybind), so the bar's and the menu's shortcuts name them; a key you've bound
-- yourself in input.conf is left as you have it. key_label() writes a key as the bar shows it under its button.
--
-- install() is mpv's part; the rest is plain Lua (tests/navkeys_test.lua).

local M = {}

-- { key, command } for the platform: Cmd on a Mac, Ctrl elsewhere (on a Mac, Ctrl+arrows switch the desktops)
function M.bindings(mac)
    local clip = mac and "Meta+" or "Ctrl+"
    return {
        { "Alt+LEFT", "frame-back-step" },
        { "Alt+RIGHT", "frame-step" },
        { clip .. "LEFT", "playlist-prev" },
        { clip .. "RIGHT", "playlist-next" },
    }
end

local ARROWS = { LEFT = "←", RIGHT = "→", UP = "↑", DOWN = "↓" }
local MAC_MODS = { Ctrl = "⌃", Alt = "⌥", Shift = "⇧", Meta = "⌘" }

-- A key as people read it: "Meta+RIGHT" → "⌘→" on a Mac, "Ctrl+→" elsewhere; nil stays nil
function M.key_label(key, mac)
    if type(key) ~= "string" or key == "" then return nil end
    -- The key is after the last +, the modifiers before it ("Ctrl++": the plus key itself)
    local mods, last = key:match("^(.*)%+(.+)$")
    if key:sub(-2) == "++" then mods, last = key:sub(1, -3), "+" end
    if not last then mods, last = "", key end
    local parts = {}
    for p in mods:gmatch("[^+]+") do parts[#parts + 1] = p end
    last = ARROWS[last] or last
    if mac then
        local out = ""
        for _, p in ipairs(parts) do out = out .. (MAC_MODS[p] or (p .. "+")) end
        return out .. last
    end
    parts[#parts + 1] = last
    return table.concat(parts, "+")
end

-- Whether the key is taken by something other than mpv's own defaults (your input.conf, a plugin)
local function taken(bindings, key)
    for _, b in ipairs(bindings or {}) do
        if b.key == key and b.is_weak == false and (b.section == "default" or not b.section) then return true end
    end
    return false
end

function M.install(mp, mac)
    local bound = mp.get_property_native("input-bindings")
    for _, kb in ipairs(M.bindings(mac)) do
        if not taken(bound, kb[1]) then
            mp.commandv("keybind", kb[1], kb[2], "the arrows with a modifier: frames and the playlist (side-panel navkeys.lua)")
        end
    end
end

return M
