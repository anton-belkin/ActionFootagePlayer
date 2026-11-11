-- The Project tab's culling and tags of what plays (the workspace plugin's user-data/workspace/tags, its tags.lua;
-- docs/tags-and-culling.md): Accept / Reject (one or the other), the stars, the custom tags as chips (a click takes
-- one off), the tags used last that it hasn't (a click puts one on), and "Tag…" (K). The layout is plain Lua
-- (tests/sidepanel_test.lua draws the tab with it); clicks run `script-message-to workspace tag-pick <which>`,
-- `tag-rate <n>`, `tag-toggle <name>` and `tag-ask`.

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

-- Draws it at x, y, w wide: the next y. `send(...)` runs a script message to the workspace plugin; keys: the keys of
-- accept, reject, tags (as the panel shows them)
function M.draw(u, ui, x, w, y, t, keys, send)
    local C, s = ui.C, u.s
    keys = keys or {}
    y = u:heading(x, w, y, t.kind == "moment" and "This moment: keep it?" or "The original: keep it?")
    -- Accept / Reject: the one on is lit; clicked again, neither
    local half = (w - 8 * s) / 2
    local function pick(px, id, label, key, on, color)
        local h = 28 * s
        u:box(px, y, half, h, 7, on and color or (u:hovered(id) and C.hover or C.field), { border = 1 * s, border_color = on and color or C.line })
        u:text(px + (key and 10 * s or half / 2), y + h / 2, label, { align = key and 4 or 5, size = 12, bold = on, color = on and C.knob or C.label })
        if key then u:kbd(px + half - 6 * s, y + h / 2, key) end
        u:hit(px, y, half, h, id, function() send("tag-pick", id:match("^pick:(.*)$")) end)
    end
    pick(x, "pick:accept", "✓ Accept", keys.accept, t.pick == "accept", "3A8F4E")
    pick(x + half + 8 * s, "pick:reject", "✕ Reject", keys.reject, t.pick == "reject", "3A3AC8")
    y = y + 34 * s
    -- The stars: the one clicked, or none when it's the one there (keys 1–5, 0)
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
    u:text(sx + 6 * s, cy, t.rating and "1–5, 0: none" or "no stars · 1–5", { size = 11, color = C.dim })
    y = y + 26 * s

    -- The custom tags, flowing over lines: { label, id, on (its own), cmd }
    local chips = {}
    for _, n in ipairs(t.list or {}) do
        chips[#chips + 1] = { n .. "  ×", "tag:" .. n, true, function() send("tag-toggle", n) end }
    end
    for _, n in ipairs(M.offered(t)) do
        chips[#chips + 1] = { "+ " .. n, "recent:" .. n, false, function() send("tag-toggle", n) end }
    end
    chips[#chips + 1] = { "Tag…" .. (keys.tags and ("  " .. keys.tags) or ""), "tag-ask", false, function() send("tag-ask") end }
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
    local where = t.kind == "moment" and "Kept in its link" or "Kept in the original's own file"
    local synced = {}
    for _, st in ipairs(t.stores or {}) do synced[#synced + 1] = st == "finder" and "Finder's tags" or st end
    y = u:caption(x, w, y, where .. (#synced > 0 and ("; synced with " .. table.concat(synced, ", ")) or ""))
    return y
end

return M
