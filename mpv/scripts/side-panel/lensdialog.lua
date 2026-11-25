-- The lens dialog (Source › Lens for the mode › Choose…): the lenses of the clip's camera mode, each with what Find lens
-- and Estimate lens said of it on the clip, Find and Estimate run from here (Quick: Find stopped at the first lens that
-- fits), and the one chosen used for the whole mode, the clip measured again with it. The Library's lens dialog, in
-- the player: drawn over the video as the panel's lists are (one of its popovers: Esc or a click outside closes it).
-- One click selects a lens; Up and Down move along them, Enter uses the one chosen.
--
-- Its data is the gyroflow plugin's (gyroflow/measure.lua), in user-data/gyroflow: lenses (gyroflow-optical --lenses),
-- lens_job (a Find or Estimate running for the clip), lens_quick.

local mp = require 'mp'
local ui = require 'ui'

local M = {}
M.ID = "lens-dialog"

local C = ui.C
local chosen, scroll, rows_now, max_scroll = nil, 0, {}, 0

local VERDICT = { good = { "ok", "fits" }, single = { "ok", "fits, one part told" }, unchecked = { "", "not checked" },
                  loose = { "bad", "not reliable" }, poor = { "bad", "doesn't fit" }, untried = { "", "not tried" } }
local OK_BG, OK_FG, BAD_BG, BAD_FG = "273B1F", "8AD16F", "123D3D", "4AA6F0"

local function fits(r) return r.fits or r.verdict == "good" or r.verdict == "single" or r.verdict == "unchecked" end

-- In the dialog's order: what fits, what was tried and doesn't, what wasn't tried
local function ordered(list)
    local a, b, c = {}, {}, {}
    for _, r in ipairs(list or {}) do
        if r.verdict == "untried" then c[#c + 1] = r elseif fits(r) then a[#a + 1] = r else b[#b + 1] = r end
    end
    return a, b, c
end

local function cmd(...) mp.commandv("script-message-to", "gyroflow", ...) end

function M.open()
    chosen, scroll = nil, 0
    cmd("lens-dialog")
end

-- The clip measured from the picture, or without motion data of its own: measured again with the lens used
local function measurable(g)
    return g.kind ~= "project" and (type(g.optical) == "table" or not g.motion)
end

local function use()
    local r = chosen and rows_now[chosen]
    if not r or type(r.use) ~= "table" then return false end
    if r.use.keep ~= nil then cmd("lens-use", "keep", tostring(r.use.keep)) else cmd("lens-use", "assign", tostring(r.use.assign)) end
    return true
end

function M.move(dir)
    if #rows_now == 0 then return end
    chosen = math.max(1, math.min(#rows_now, (chosen or (dir > 0 and 0 or #rows_now + 1)) + dir))
end

-- Enter: the lens chosen used (true: the dialog closes)
function M.enter() return use() end

function M.wheel(dir)
    scroll = math.max(0, math.min(max_scroll, scroll + dir * 48))
end

-- A pill at the right of a row: its text and colors; returns its left
local function pill(u, right, cy, text, kind)
    local s = u.s
    local w = ui.text_width(text, 11, s) + 12 * s
    local bg, fg = C.field, C.label
    if kind == "ok" then bg, fg = OK_BG, OK_FG elseif kind == "bad" then bg, fg = BAD_BG, BAD_FG end
    u:box(right - w, cy - 9 * s, w, 18 * s, 5, bg)
    u:text(right - w / 2, cy, text, { align = 5, size = 11, color = fg })
    return right - w
end

-- Drawn over the video, w wide (the window less the panel), h high, below t0: where it is ({ x0, y0, x1, y1 }).
-- close(): closes it
function M.draw(u, g, w, h, t0, close)
    local s = u.s
    local d = type(g.lenses) == "table" and g.lenses or nil
    local job = type(g.lens_job) == "table" and g.lens_job or nil
    local running = job and job.state ~= "done" and job.state ~= "failed" and job.state ~= "stopped"
    local dw = math.min(760 * s, w - 40 * s)
    local x0 = (w - dw) / 2
    local y0 = t0 + 48 * s
    local bottom = h - 64 * s
    local first = #u.hits
    local pad = 20 * s
    local x, cw = x0 + pad, dw - 2 * pad
    -- Dim the video under it, then the dialog's card (its height known once drawn: drawn last, under, by inserting)
    local card_at = #u.events + 1
    local y = y0 + 18 * s
    local title = "Lens for " .. (d and d.mode or "this clip")
    for _, l in ipairs(ui.wrap(title, cw, 16, s)) do u:text(x, y + 9 * s, l, { size = 16, bold = true }); y = y + 22 * s end
    local now = d and type(d.now) == "table" and d.now or {}
    local sub = "Kept for every clip of this mode." .. ((now.name or "") ~= "" and (" Now: " .. tostring(now.name):gsub("_", " ")
        .. (now.readout_ms and string.format(", readout %.1f ms", now.readout_ms) or "") .. ".") or "")
    y = u:caption(x, cw, y + 2 * s, sub)
    -- Find, Estimate, Quick
    local bw = 112 * s
    u:button(x, bw, y + 6 * s, "lens-find", "Find lens", function() cmd("lens-find") end, { disabled = running or not d })
    u:button(x + bw + 8 * s, bw + 16 * s, y + 6 * s, "lens-estimate", "Estimate lens", function() cmd("lens-estimate") end, { disabled = running or not d })
    local qx = x + 2 * bw + 40 * s
    u:switch(qx, cw - (qx - x), y + 5 * s, "lens-quick", "Quick: stop at the first lens that fits", g.lens_quick ~= false,
             function() cmd("lens-quick") end)
    y = y + 44 * s
    if running then
        local f = tonumber(job.fraction) or 0
        u:box(x, y + 4 * s, cw, 6 * s, 3, C.line)
        u:box(x, y + 4 * s, math.max(6 * s, cw * f), 6 * s, 3, C.accent)
        y = u:caption(x, cw, y + 14 * s, string.format("%s · %d %%", job.kind == "estimate" and "Estimating the lens from the clip" or "Finding the lens", math.floor(f * 100)))
    elseif job and job.state == "failed" and job.message then
        y = u:caption(x, cw, y, tostring(job.message), { color = BAD_FG })
    else
        y = u:caption(x, cw, y, "Find tries the camera's known lenses on this clip and measures their readout; Estimate makes a lens from it.")
    end
    if d and d.error then y = u:caption(x, cw, y, tostring(d.error), { color = BAD_FG }) end
    -- The lenses, scrolled within the room left above the footer
    local foot_h = 56 * s
    local list_top, list_bottom = y + 4 * s, math.min(bottom - foot_h, y + 4 * s + 460 * s)
    rows_now = {}
    if not d then
        y = u:caption(x, cw, list_top, "Reading its lenses…")
        list_bottom = y
    else
        local a, b, c = ordered(d.lenses)
        local found = type(d.found) == "table" and d.found or {}
        local groups = { { "Fits this clip", a }, { "Tried, doesn't fit", b },
                         { (found.quick and (found.untried or 0) > 0) and "Not tried (Quick stopped at the first that fits)" or "Not tried", c } }
        local saved_clip = u.clip
        u.clip = { x0, list_top, x0 + dw, list_bottom }
        local ly = list_top - scroll * s
        local rh = 46 * s
        for _, grp in ipairs(groups) do
            if #grp[2] > 0 then
                ly = u:heading(x, cw, ly + 4 * s, grp[1])
                for _, r in ipairs(grp[2]) do
                    rows_now[#rows_now + 1] = r
                    local i = #rows_now
                    local id = "lens-row:" .. i
                    local on = chosen == i
                    if on then u:box(x - 8 * s, ly, cw + 16 * s, rh - 4 * s, 7, "4A2F1F", { border = 1 * s, border_color = C.accent })
                    elseif u:hovered(id) then u:box(x - 8 * s, ly, cw + 16 * s, rh - 4 * s, 7, C.hover) end
                    -- The round mark: filled for the one chosen
                    local cx, cy = x + 7 * s, ly + 14 * s
                    u:shape(ui.circle_path(cx, cy, 7 * s), on and C.accent or C.dim, { fill = on, border = 1.5 * s, border_color = on and C.accent or C.dim })
                    if on then u:shape(ui.circle_path(cx, cy, 3 * s), C.knob) end
                    local v = VERDICT[r.verdict] or { "", tostring(r.verdict) }
                    local right = pill(u, x + cw, ly + 13 * s, v[2], v[1])
                    if r.in_use then right = pill(u, right - 6 * s, ly + 13 * s, "In use") end
                    local name = tostring(r.name or ""):gsub("_", " ")
                    local room = right - (x + 24 * s) - 10 * s
                    while #name > 8 and ui.text_width(name, 13, s, true) > room do name = name:sub(1, -6):gsub("[\194-\244][\128-\191]*$", "") .. "…" end
                    u:text(x + 24 * s, ly + 13 * s, name, { size = 13, bold = true })
                    local about = { r.source or "" }
                    if r.readout_ms then about[#about + 1] = string.format("readout %.1f ms%s", r.readout_ms, r.readout_measured and ", measured on this clip" or "")
                    else about[#about + 1] = "no readout" end
                    if (r.limit or 0) > 0 then about[#about + 1] = string.format("moved %.1f of %.0f px", r.moved or 0, r.limit) end
                    if (r.refraction or 1) > 1 then about[#about + 1] = "under water" end
                    u:text(x + 24 * s, ly + 31 * s, table.concat(about, " · "), { size = 12, color = C.dim })
                    u:hit(x - 8 * s, ly, cw + 16 * s, rh - 4 * s, id, function() chosen = i end)
                    ly = ly + rh
                end
            end
        end
        if #rows_now == 0 then ly = u:caption(x, cw, ly, "No lens of this camera to choose from: Estimate lens makes one from the clip.") end
        u.clip = saved_clip
        local content = ly + scroll * s - list_top
        max_scroll = math.max(0, (content - (list_bottom - list_top)) / s)
        if scroll > max_scroll then scroll = max_scroll end
        list_bottom = math.min(list_bottom, list_top + content)
        if chosen == nil then
            for i, r in ipairs(rows_now) do if r.in_use then chosen = i end end
        end
    end
    -- The footer: Forget, Cancel, Use (and measure again)
    local fy = list_bottom + 12 * s
    u:box(x0, fy, dw, 1 * s, 0, C.divider)
    fy = fy + 12 * s
    local r = chosen and rows_now[chosen]
    if now.from == "own" then u:button(x, 250 * s, fy, "lens-forget", "Forget the lens chosen for this mode", function() cmd("lens-forget"); close() end) end
    local use_label = measurable(g) and "Use and measure again" or "Use for this mode"
    local uw = ui.text_width(use_label, 12, s, true) + 64 * s
    u:button(x + cw - uw, uw, fy, "lens-use", use_label, function() if use() then close() end end,
             { primary = true, key = "Enter", disabled = running or not (r and type(r.use) == "table") })
    u:button(x + cw - uw - 8 * s - 110 * s, 110 * s, fy, "lens-cancel", "Cancel", close, { key = "Esc" })
    local y1 = fy + 28 * s + 16 * s
    -- The card under it all, the video dimmed around it
    table.insert(u.events, card_at, string.format("{\\an7\\pos(0,0)\\shad0\\bord%.1f\\1c&H%s&\\1a&H00&\\3c&H%s&\\3a&H00&\\p1}%s{\\p0}",
        1 * s, C.card, C.line, ui.rrect_path(x0, y0, dw, y1 - y0, 12 * s)))
    table.insert(u.events, card_at, string.format("{\\an7\\pos(0,0)\\shad0\\bord0\\1c&H000000&\\1a&H90&\\p1}%s{\\p0}", ui.rrect_path(0, 0, w, h, 0)))
    -- Every place in it is the dialog's (a click on the card off a control does nothing; one outside closes it)
    for i = first + 1, #u.hits do u.hits[i].popover = true end
    table.insert(u.hits, first + 1, { x0 = x0, y0 = y0, x1 = x0 + dw, y1 = y1, id = M.ID, cmd = function() end, popover = true })
    return { x0, y0, x0 + dw, y1 }
end

return M
