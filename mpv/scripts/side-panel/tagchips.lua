-- The Project tab's tags of what plays (the workspace plugin's user-data/workspace/tags, its tags.lua): the stars, its
-- tags as chips (a click takes one off), the tags used last that it hasn't (a click puts one on), and "Tag…" (K),
-- which asks for a new one. The layout is plain Lua (tests/sidepanel_test.lua draws the tab with it); clicks run
-- `script-message-to workspace tag-toggle <name>`, `tag-rate <n>` and `tag-ask`.

local M = {}

-- How many of the recent tags are offered
M.OFFER = 6

-- The recent tags not on it, at most OFFER
function M.offered(t)
    local out = {}
    local on = {}
    for _, n in ipairs(t.list or {}) do on[n:lower()] = true end
    for _, r in ipairs(t.recent or {}) do
        if #out >= M.OFFER then break end
        if not on[r:lower()] then out[#out + 1] = r end
    end
    return out
end

-- Draws it at x, y, w wide: the next y. `send(...)` runs a script message to the workspace plugin
function M.draw(u, ui, x, w, y, t, key, send)
    local C, s = ui.C, u.s
    local what = ({ clip = "this clip", moment = "this moment", export = "this export" })[t.kind] or "it"
    y = u:heading(x, w, y, "Tags of " .. what, key)
    -- The stars: the one clicked, or none when it's the one there
    local sx = x
    local cy = y + 10 * s
    for n = 1, 5 do
        local id = "star:" .. n
        local on = t.rating and n <= t.rating
        if u:hovered(id) then u:box(sx - 2 * s, y, 22 * s, 20 * s, 5, C.hover) end
        u:text(sx + 9 * s, cy, on and "★" or "☆", { align = 5, size = 15, color = on and C.accent or C.dim })
        u:hit(sx - 2 * s, y, 22 * s, 20 * s, id, function() send("tag-rate", tostring(n)) end)
        sx = sx + 22 * s
    end
    if not t.rating then u:text(sx + 6 * s, cy, "no stars", { size = 11, color = C.dim }) end
    y = y + 26 * s

    -- The chips, flowing over lines: { label, id, on (its own), cmd }
    local chips = {}
    for _, n in ipairs(t.list or {}) do
        chips[#chips + 1] = { n .. "  ×", "tag:" .. n, true, function() send("tag-toggle", n) end }
    end
    for _, n in ipairs(M.offered(t)) do
        chips[#chips + 1] = { "+ " .. n, "recent:" .. n, false, function() send("tag-toggle", n) end }
    end
    chips[#chips + 1] = { "Tag…", "tag-ask", false, function() send("tag-ask") end }
    local h, gap = 22 * s, 4 * s
    local cx = x
    for _, c in ipairs(chips) do
        local label = c[1]
        local tw = math.min(ui.text_width(label, 12, s) + 16 * s, w)
        if cx > x and cx + tw > x + w then cx, y = x, y + h + gap end
        local color = c[3] and C.card or (u:hovered(c[2]) and C.hover or C.field)
        u:box(cx, y, tw, h, 11, color, { border = 1 * s, border_color = c[3] and C.accent or C.line })
        u:text(cx + tw / 2, y + h / 2, label, { align = 5, size = 12, color = c[3] and C.text or C.label })
        u:hit(cx, y, tw, h, c[2], c[4])
        cx = cx + tw + gap
    end
    y = y + h + 6 * s
    if t.finder and t.finder ~= "off" then
        y = u:caption(x, w, y, t.kind == "clip" and (t.finder == "all" and "Kept in the clip's own file; Finder shows them on the clip and its preview"
            or "Kept in the clip's own file; Finder shows them on its clip preview") or "Kept in its link; Finder shows them on its file")
    end
    return y
end

return M
