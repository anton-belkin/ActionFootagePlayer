-- The navigation bar at the bottom of the video (docs/navigation-bar.md), in place of mpv's own on-screen controller:
-- the track with the time, the camera's highlights and your marks, the moments and the A–B part on it; the buttons
-- below it. Drawn with the side panel's ui.lua; its reckoning is timeline.lua. main.lua shows it, docks it below the
-- video while the panel is open, and hands it the mouse.

local mp = require 'mp'
local ui = require 'ui'
local tl = require 'timeline'

local C = ui.C
local M = {}

-- Its height in points: the track's row and the buttons' row
M.HEIGHT = 66
local PAD = 14
local TRACK = 21
local ROW = 47
local BUTTON = 30
-- How near (points) a click must be to a mark to land on it
M.SNAP = 7

-- What it shows, read from the player and the plugins
function M.snapshot()
    local st = {
        pos = mp.get_property_number("time-pos"),
        duration = mp.get_property_number("duration"),
        pause = mp.get_property_native("pause"),
        speed = mp.get_property_number("speed", 1),
        mute = mp.get_property_native("mute"),
        fullscreen = mp.get_property_native("fullscreen"),
        loop = mp.get_property("loop-file") == "inf",
        ab_a = tonumber(mp.get_property("ab-loop-a")),
        ab_b = tonumber(mp.get_property("ab-loop-b")),
        n = mp.get_property_number("playlist-pos", 0),
        count = mp.get_property_number("playlist-count", 0),
    }
    st.items = tl.track_items(mp.get_property_native("user-data/highlights"), mp.get_property_native("user-data/workspace"),
                              mp.get_property_native("chapter-list"))
    local cache = mp.get_property_native("demuxer-cache-state")
    st.ranges = type(cache) == "table" and cache["seekable-ranges"] or {}
    return st
end

-- The icons, about 16 points across, centered at cx, cy
local function icon(u, name, cx, cy, color)
    local s = u.s
    local function at(x, y) return string.format("%.1f %.1f", cx + x * s, cy + y * s) end
    local function tri(x0, y0, x1, y1, x2, y2) u:shape("m " .. at(x0, y0) .. " l " .. at(x1, y1) .. " " .. at(x2, y2), color) end
    local function bar(x, y, w, h) u:box(cx + x * s, cy + y * s, w * s, h * s, 0.6, color) end
    if name == "play" then
        tri(-4.5, -7, 7, 0, -4.5, 7)
    elseif name == "pause" then
        bar(-5.5, -7, 4, 14); bar(1.5, -7, 4, 14)
    elseif name == "prev" then
        bar(-7, -6, 2.2, 12); tri(6, -6, -4, 0, 6, 6)
    elseif name == "next" then
        bar(4.8, -6, 2.2, 12); tri(-6, -6, 4, 0, -6, 6)
    elseif name == "fback" or name == "ffwd" then
        -- A chevron: a step
        local k = name == "fback" and 1 or -1
        u:shape("m " .. at(2 * k, -6) .. " l " .. at(3.4 * k, -4.6) .. " " .. at(-1.2 * k, 0) .. " " .. at(3.4 * k, 4.6) .. " "
                .. at(2 * k, 6) .. " " .. at(-4 * k, 0), color)
    elseif name == "mark" then
        -- A diamond, and a plus
        u:shape("m " .. at(-2, -6.5) .. " l " .. at(3, -1.5) .. " " .. at(-2, 3.5) .. " " .. at(-7, -1.5), color)
        u:box(cx + 2.5 * s, cy + 3.75 * s, 6 * s, 1.5 * s, 0, color)
        u:box(cx + 4.75 * s, cy + 1.5 * s, 1.5 * s, 6 * s, 0, color)
    elseif name == "mprev" or name == "mnext" then
        local dx = name == "mprev" and 2.5 or -2.5
        u:shape("m " .. at(dx, -5) .. " l " .. at(dx + 5, 0) .. " " .. at(dx, 5) .. " " .. at(dx - 5, 0), color)
        if name == "mprev" then
            tri(-4.5, -3, -8.5, 0, -4.5, 3)
        else
            tri(4.5, -3, 8.5, 0, 4.5, 3)
        end
    elseif name == "loop" then
        -- Two arrows round a rounded rectangle
        u:shape(ui.rrect_path(cx - 7 * s, cy - 4.5 * s, 14 * s, 9 * s, 3.5 * s), color, { fill = false, border = 1.4 * s })
        tri(1, -7.5, 4.5, -4.5, 1, -1.5)
        tri(-1, 1.5, -4.5, 4.5, -1, 7.5)
    elseif name == "sound" or name == "muted" then
        u:shape("m " .. at(-7, -2.5) .. " l " .. at(-3.5, -2.5) .. " " .. at(1, -6.5) .. " " .. at(1, 6.5) .. " " .. at(-3.5, 2.5)
                .. " " .. at(-7, 2.5), color)
        if name == "sound" then
            u:shape("m " .. at(3.5, -3.5) .. " b " .. at(5.5, -1.5) .. " " .. at(5.5, 1.5) .. " " .. at(3.5, 3.5), color,
                    { fill = false, border = 1.3 * s })
            u:shape("m " .. at(5.5, -6) .. " b " .. at(9, -2.5) .. " " .. at(9, 2.5) .. " " .. at(5.5, 6), color,
                    { fill = false, border = 1.3 * s })
        else
            local function line(x0, y0, x1, y1)
                local w = 0.8
                u:shape("m " .. at(x0 - w, y0 + w) .. " l " .. at(x0 + w, y0 - w) .. " " .. at(x1 + w, y1 - w) .. " " .. at(x1 - w, y1 + w), color)
            end
            line(3.5, 3, 8.5, -3); line(3.5, -3, 8.5, 3)
        end
    elseif name == "fullscreen" or name == "window" then
        local o = name == "fullscreen" and 0 or 2.5
        for _, c in ipairs({ { -1, -1 }, { 1, -1 }, { 1, 1 }, { -1, 1 } }) do
            local x, y = c[1] * (7 - o), c[2] * (6 - o)
            local hx, hy = -c[1] * 4.5, -c[2] * 4.5
            if name == "window" then hx, hy = -hx, -hy end
            u:box(cx + math.min(x, x + hx) * s, cy + (y - 0.8) * s, math.abs(hx) * s, 1.6 * s, 0, color)
            u:box(cx + (x - 0.8) * s, cy + math.min(y, y + hy) * s, 1.6 * s, math.abs(hy) * s, 0, color)
        end
    end
end

-- The buttons' row: each { id, w (points), keep, group ("left", "right"), and how it's drawn and what it does }
local function buttons(st, remaining)
    local t = st.pos or 0
    local d = st.duration
    local time = tl.clock(remaining and d and (t - d) or t) .. (d and (" / " .. tl.clock(d, 0)) or "")
    local ab = tl.ab_state(st.ab_a, st.ab_b)
    local inf = math.huge
    return {
        { id = "prev", icon = "prev", tip = "Previous clip", cmd = "playlist-prev", disabled = st.n <= 0, keep = 3, group = "left" },
        { id = "play", icon = st.pause and "play" or "pause", tip = st.pause and "Play" or "Pause", cmd = "cycle pause", keep = inf, group = "left" },
        { id = "next", icon = "next", tip = "Next clip", cmd = "playlist-next", disabled = st.n >= st.count - 1, keep = 3, group = "left", gap = 10 },
        { id = "fback", icon = "fback", tip = "A frame back", cmd = "frame-back-step", keep = 4, group = "left", small = true },
        { id = "time", text = time, tip = remaining and "Time left: click for the time" or "Click for the time left",
          cmd = "time", keep = inf, group = "left", wide = "-" .. tl.clock(d or 0) .. (d and (" / " .. tl.clock(d, 0)) or "") },
        { id = "ffwd", icon = "ffwd", tip = "A frame on", cmd = "frame-step", keep = 4, group = "left", small = true },
        { id = "mprev", icon = "mprev", tip = "Previous mark", cmd = "script-binding highlights/prev", keep = 5, group = "right" },
        { id = "mark", icon = "mark", tip = "Mark here (on a mark: remove it)", cmd = "script-binding highlights/mark", keep = 5, group = "right" },
        { id = "mnext", icon = "mnext", tip = "Next mark", cmd = "script-binding highlights/next", keep = 5, group = "right", gap = 10 },
        { id = "ab", text = ab == "ab" and "A–B" or ab == "a" and "A–" or "A–B", active = ab ~= "none",
          tip = ab == "none" and "Part: its start here" or ab == "a" and "Part: its end here" or "Clear the part",
          cmd = "ab-loop", keep = 6, group = "right", pill = true },
        { id = "speed", text = string.format("%g×", math.floor(st.speed * 100 + 0.5) / 100), tip = "Speed", active = math.abs(st.speed - 1) > 1e-3,
          cmd = "cycle-values speed 1.5 2 0.5 1", keep = 2, group = "right", pill = true },
        { id = "loop", icon = "loop", tip = "Loop the clip", active = st.loop, cmd = "cycle-values loop-file inf no", keep = 2, group = "right" },
        { id = "mute", icon = st.mute and "muted" or "sound", tip = st.mute and "Sound on" or "Mute", cmd = "cycle mute", keep = 2, group = "right" },
        { id = "fs", icon = st.fullscreen and "window" or "fullscreen", tip = st.fullscreen and "Leave fullscreen" or "Fullscreen",
          cmd = "cycle fullscreen", keep = 7, group = "right" },
    }
end

local function width_of(b, s)
    local w
    if b.text then
        w = ui.text_width(b.wide or b.text, b.pill and 12 or 13, s) / s + (b.pill and 18 or 12)
    else
        w = b.small and 24 or BUTTON + 2
    end
    return (w + (b.gap or 2)) * s
end

-- Draws the bar into u at the bottom of a window h high, from x0 to x1 (pixels). opts: docked (opaque, below the
-- video), hover (the id under the mouse), mouse ({ x, y }), dragging, drag_t (the time it's dragged to), remaining (the
-- time left shown), keys (the commands' keys, for the tips), toggle_time (a function: the time's click), seek (a
-- function(t, exact): the track clicked or dragged)
function M.draw(u, x0, x1, h, st, opts)
    local s = u.s
    local top = h - M.HEIGHT * s
    u:box(x0, top, x1 - x0, h - top, 0, C.panel, { alpha = opts.docked and "00" or "30" })
    if opts.docked then u:box(x0, top, x1 - x0, math.max(1, s), 0, C.divider) end

    -- The track
    local tx0, tx1, ty = x0 + PAD * s, x1 - PAD * s, top + TRACK * s
    local d = st.duration
    local function X(t) return tl.x_of(t, tx0, tx1, d) end
    local over = opts.hover == "bar:track" or opts.dragging
    local th = (over and 6 or 4) * s
    u:box(tx0, ty - th / 2, tx1 - tx0, th, 3, C.line)
    if d and d > 0 then
        for _, r in ipairs(st.ranges or {}) do
            if type(r.start) == "number" and type(r["end"]) == "number" then
                local a, b = X(r.start), X(r["end"])
                if b > a then u:box(a, ty - th / 2, b - a, th, 3, C.dim, { alpha = "A0" }) end
            end
        end
        -- The A–B part: a light band behind it, its ends' letters below
        if st.ab_a or st.ab_b then
            local a = st.ab_a and X(st.ab_a) or nil
            local b = st.ab_b and X(st.ab_b) or nil
            if a and b and b > a then u:box(a, ty - 6 * s, b - a, 12 * s, 2, C.knob, { alpha = "B8" }) end
            for _, e in ipairs({ { a, "A" }, { b, "B" } }) do
                if e[1] then
                    u:box(e[1] - 0.75 * s, ty - 7 * s, 1.5 * s, 14 * s, 0, C.knob)
                    u:text(e[1], ty + 13 * s, e[2], { align = 5, size = 9, bold = true, color = C.knob })
                end
            end
        end
        local px = X(opts.drag_t or st.pos or 0)
        u:box(tx0, ty - th / 2, px - tx0, th, 3, C.accent)
        -- The moments, under it
        for _, m in ipairs(st.items.moments) do
            local a, b = X(m.a), X(m.b)
            u:box(a, ty + 4.5 * s, math.max(2 * s, b - a), 3 * s, 1.5, C.label, { alpha = "50" })
        end
        -- The file's own chapters: ticks
        for _, c in ipairs(st.items.chapters) do
            u:box(X(c.t) - 0.75 * s, ty - 5 * s, 1.5 * s, 10 * s, 0, C.dim)
        end
        -- The marks: diamonds over it, the camera's amber, yours white
        for _, m in ipairs(st.items.marks) do
            local mx, my, r = X(m.t), ty - 10 * s, 4 * s
            local color = m.kind == "camera" and C.saved or C.knob
            u:shape(string.format("m %.1f %.1f l %.1f %.1f %.1f %.1f %.1f %.1f", mx, my - r, mx + r, my, mx, my + r, mx - r, my), color)
            u:box(mx - 0.6 * s, my + r, 1.2 * s, ty - my - r, 0, color, { alpha = "40" })
        end
        -- The knob
        u:shape(ui.circle_path(px, ty, (over and 7 or 5.5) * s), C.knob)
        -- Where the mouse is: the time there, or the mark it would land on
        if over and opts.mouse then
            local snap = tl.snap(opts.mouse.x, st.items, st.ab_a, st.ab_b, tx0, tx1, d, M.SNAP * s)
            local t = snap and snap.t or tl.t_of(opts.mouse.x, tx0, tx1, d)
            local label = tl.clock(t) .. (snap and ("  " .. snap.title) or "")
            local mx = X(t)
            u:box(mx - 0.75 * s, ty - 9 * s, 1.5 * s, 18 * s, 0, C.text, { alpha = "30" })
            local bw = ui.text_width(label, 12, s) + 16 * s
            local bx = math.min(math.max(mx - bw / 2, x0 + 4 * s), x1 - bw - 4 * s)
            local by = top - 30 * s
            u:box(bx, by, bw, 24 * s, 6, C.card, { border = 1 * s, border_color = C.line })
            u:text(bx + bw / 2, by + 12 * s, label, { align = 5, size = 12, color = C.text })
        end
        -- Held and dragged: keyframes as it moves (fast), the exact frame where it's let go; near a mark, on the mark
        if opts.seek then
            local function at(mx)
                local snap = tl.snap(mx, st.items, st.ab_a, st.ab_b, tx0, tx1, d, M.SNAP * s)
                return snap and snap.t or tl.t_of(mx, tx0, tx1, d)
            end
            u:hit(tx0 - 8 * s, ty - 14 * s, tx1 - tx0 + 16 * s, 26 * s, "bar:track", {
                drag = function(mx) opts.seek(at(mx), false) end,
                release = function(mx) opts.seek(at(mx), true) end,
            })
        end
    end

    -- The buttons
    local list = buttons(st, opts.remaining)
    for _, b in ipairs(list) do b.w = width_of(b, s) end
    local shown = tl.fit(list, x1 - x0 - 2 * (PAD - 4) * s)
    local lx, rx = x0 + (PAD - 4) * s, x1 - (PAD - 4) * s
    local cy = top + ROW * s
    local right = {}
    for _, b in ipairs(shown) do
        if b.group == "right" then right[#right + 1] = b end
    end
    local tip = nil
    local function place(b, bx)
        local w = b.w - (b.gap or 2) * s
        local id = "bar:" .. b.id
        local hovered = u:hovered(id) and not b.disabled
        local color = b.disabled and C.line or (b.active and C.text or (hovered and C.text or C.label))
        if b.pill then
            local ph = 22 * s
            u:box(bx, cy - ph / 2, w, ph, 6, b.active and C.accent or (hovered and C.hover or C.field),
                  { border = 1 * s, border_color = b.active and C.accent or C.line })
            u:text(bx + w / 2, cy, b.text, { align = 5, size = 12, bold = b.active, color = b.active and C.knob or C.label })
        elseif b.text then
            if hovered then u:box(bx, cy - BUTTON / 2 * s, w, BUTTON * s, 6, C.hover) end
            u:text(bx + 6 * s, cy, b.text, { size = 13, color = C.text })
        else
            if hovered then u:box(bx, cy - BUTTON / 2 * s, w, BUTTON * s, 6, C.hover) end
            if b.active then u:box(bx + 4 * s, cy + 11 * s, w - 8 * s, 2 * s, 1, C.accent) end
            icon(u, b.icon, bx + w / 2, cy, color)
        end
        if hovered then tip = { b = b, x = bx + w / 2 } end
        if not b.disabled then
            local cmd = b.cmd == "time" and opts.toggle_time or b.cmd
            u:hit(bx, cy - BUTTON / 2 * s, w, BUTTON * s, id, cmd)
        end
    end
    for _, b in ipairs(shown) do
        if b.group == "left" then place(b, lx); lx = lx + b.w end
    end
    for i = #right, 1, -1 do
        local b = right[i]
        rx = rx - b.w
        place(b, rx + (b.gap or 2) * s)
    end
    -- The hovered button's name, and its key, above the bar
    if tip and not over then
        local key = opts.keys and opts.keys[tip.b.cmd]
        local label = tip.b.tip .. (key and ("   " .. key) or "")
        local bw = ui.text_width(label, 12, s) + 16 * s
        local bx = math.min(math.max(tip.x - bw / 2, x0 + 4 * s), x1 - bw - 4 * s)
        local by = top - 30 * s
        u:box(bx, by, bw, 24 * s, 6, C.card, { border = 1 * s, border_color = C.line })
        u:text(bx + bw / 2, by + 12 * s, label, { align = 5, size = 12, color = C.text })
    end
    return u
end

return M
