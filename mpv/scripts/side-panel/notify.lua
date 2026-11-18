-- What the player is doing, shown in one place and one look (docs/notifications.md): cards at the top of the video,
-- in the middle (a card for each thing going on: working with a bar, done, an error, a short note), and the dialog a
-- name is typed in, in the middle of the video. Drawn in ASS in the panel's colors, on an overlay of its own above the
-- panel and the bar. The plugins talk to it with script messages (notify_client.lua sends them):
--
--   notify <json>   a card: { id, kind, title, detail, share, right, seconds, quiet }
--                     kind: working (stays until the same id comes again as done, error or hide; a bar: share 0..1,
--                           or none: one that moves, with the time so far), done (a tick, a few seconds), error
--                           (stays until it's clicked or Esc), info (a short note, `seconds`), hide (the card goes)
--                     quiet: a note the panel shows anyway (a key's toggle): none while the panel is open
--   ask <json>      the dialog: { id, reply, title, subtitle, value, suggestion, names, ok, ok_typed, placeholder }
--                     answered with `script-message-to <reply> notify-answer <json>`, { id, text, cancelled }:
--                     text is what was typed, trimmed ("" when nothing was: the asker takes its suggestion)
--
-- The mouse is the cards' while it's over one (a click closes a done or an error card) and the dialog's while it's
-- open; the keys are the dialog's while it's open (typing, Enter, Esc, ↑↓ the earlier names, Tab the suggestion).

local mp = require 'mp'
local utils = require 'mp.utils'
local ui = require 'ui'

local M = {}
local C = ui.C
-- Done in green, an error in red, as ASS's BGR
M.GREEN, M.RED = "4E9A3A", "5A5AE8"
local ERROR_FILL = "201E30"
local W_CARD, GAP, MAX_CARDS = 380, 8, 4
local DONE_SECONDS, INFO_SECONDS = 3, 2
local TITLE_BAR = 28

local overlay = mp.create_osd_overlay("ass-events")
overlay.z = 20
local platform = mp.get_property("platform") or ""

local cards = {}          -- in the order they came: { id, kind, title, detail, share, right, started, till }
local dialog = nil        -- the dialog open: its request, and value, cursor, pick (the earlier name chosen with ↑↓)
local hits = {}
local hover = nil
local panel_open = false
local timer = nil
local captured = false

local function now() return mp.get_time() end

local function scale() return mp.get_property_number("display-hidpi-scale", 1) or 1 end

-- The video's place in the window: beside the panel, above the bar when it's docked; and the title bar over its top
-- (macOS, a window with its border)
function M.area(w, h)
    local right = mp.get_property_number("video-margin-ratio-right", 0) or 0
    local bottom = mp.get_property_number("video-margin-ratio-bottom", 0) or 0
    local s = scale()
    local t0 = (platform == "darwin" and not mp.get_property_native("fullscreen") and mp.get_property_native("border") ~= false)
               and TITLE_BAR * s or 0
    return { x0 = 0, y0 = t0, x1 = w * (1 - right), y1 = h * (1 - bottom), s = s }
end

-- m:ss
local function clock(t)
    t = math.max(0, math.floor(t))
    return string.format("%d:%02d", math.floor(t / 60), t % 60)
end
M.clock = clock

-- The text cut to fit width (pixels), with … at its end
local function fit(text, width, size, s, bold)
    if ui.text_width(text, size, s, bold) <= width then return text end
    local chars = {}
    for ch in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do chars[#chars + 1] = ch end
    local n = #chars
    while n > 1 and ui.text_width(table.concat(chars, "", 1, n) .. "…", size, s, bold) > width do n = n - 1 end
    return table.concat(chars, "", 1, n) .. "…"
end

-- A bar: share 0..1, or nil: a stretch that moves along it (at phase 0..1)
local function bar(u, x, y, w, h, share, color, phase)
    u:box(x, y, w, h, h / 2, C.line)
    if share then
        if share > 0 then u:box(x, y, math.max(h, w * math.min(1, share)), h, h / 2, color or C.accent) end
        return
    end
    local seg = w * 0.3
    local sx = x - seg + (w + seg) * (phase or 0)
    local a, b = math.max(x, sx), math.min(x + w, sx + seg)
    if b - a > 1 then u:box(a, y, b - a, h, h / 2, color or C.accent) end
end

-- The round mark at a card's left: a tick, !, or a ring with an arc (working)
local function mark(u, cx, cy, kind, phase)
    local s = u.s
    if kind == "done" then
        u:shape(ui.circle_path(cx, cy, 9 * s), M.GREEN)
        u:text(cx, cy + 0.5 * s, "✓", { align = 5, size = 11, bold = true, color = "FFFFFF" })
    elseif kind == "error" then
        u:shape(ui.circle_path(cx, cy, 9 * s), M.RED)
        u:text(cx, cy + 0.5 * s, "!", { align = 5, size = 12, bold = true, color = "FFFFFF" })
    elseif kind == "working" then
        local r = 7.5 * s
        u:shape(ui.circle_path(cx, cy, r), C.line, { fill = false, border = 2 * s })
        -- A quarter of the ring, turning
        local a0 = (phase or 0) * 2 * math.pi
        local pts = {}
        for i = 0, 6 do
            local a = a0 + i / 6 * math.pi / 2
            pts[#pts + 1] = string.format("%.1f %.1f", cx + r * math.cos(a), cy + r * math.sin(a))
        end
        u:shape("m " .. pts[1] .. " l " .. table.concat(pts, " ", 2), C.accent, { fill = false, border = 2 * s })
    end
end

-- A card's height and its detail's lines, at width w
local function measure(c, w, s)
    local lines = {}
    local inner = w - 54 * s
    for para in ((c.detail or "") .. "\n"):gmatch("([^\n]*)\n") do
        if para ~= "" then
            for _, l in ipairs(ui.wrap(para, inner, 12, s)) do lines[#lines + 1] = l end
        end
    end
    local max = c.kind == "error" and 5 or 2
    while #lines > max do table.remove(lines) end
    local h = 40 + 16 * #lines
    if c.kind == "working" or c.kind == "done" then h = h + 12 end
    if c.kind == "error" then h = h + 18 end
    if #lines == 0 and c.kind == "info" then h = 38 end
    return h * s, lines
end

local function draw_card(u, c, x, y, w, phase)
    local s = u.s
    local h, lines = measure(c, w, s)
    u:box(x + 1 * s, y + 3 * s, w, h, 12, "000000", { alpha = "A0" })
    u:box(x, y, w, h, 12, c.kind == "error" and ERROR_FILL or C.panel,
          { border = 1 * s, border_color = c.kind == "error" and M.RED or C.line, alpha = "08" })
    local tx = c.kind == "info" and x + 16 * s or x + 40 * s
    local ty = y + (c.kind == "info" and #lines == 0 and h / 2 or 20 * s)
    if c.kind ~= "info" then mark(u, x + 22 * s, y + 20 * s, c.kind, phase) end
    local right = c.right
    if not right and c.kind == "working" then
        right = c.share and string.format("%d %%", math.floor(c.share * 100 + 0.5)) or clock(now() - c.started)
    end
    local closable = c.kind == "error" or c.kind == "done"
    local rw = (right and (ui.text_width(right, 12, s) + 10 * s) or 0) + (closable and 22 * s or 0)
    u:text(tx, ty, fit(c.title or "", x + w - 14 * s - rw - tx, 13, s, true), { size = 13, bold = true })
    if right then u:text(x + w - 14 * s - (closable and 22 * s or 0), ty, right, { align = 6, size = 12, color = C.dim }) end
    if closable then
        local hov = hover == c
        u:text(x + w - 18 * s, ty, "×", { align = 5, size = 16, color = hov and C.text or C.dim })
    end
    local yy = y + 38 * s
    for _, l in ipairs(lines) do
        u:text(tx, yy, l, { size = 12, color = c.kind == "error" and C.label or C.dim })
        yy = yy + 16 * s
    end
    if c.kind == "working" then bar(u, tx, yy - 4 * s, x + w - 14 * s - tx, 4 * s, c.share, nil, phase) end
    if c.kind == "done" then bar(u, tx, yy - 4 * s, x + w - 14 * s - tx, 4 * s, 1, M.GREEN) end
    if c.kind == "error" then u:text(tx, yy + 3 * s, "Click or Esc closes it", { size = 11, color = C.dim }) end
    -- A click closes a done or an error card; on the others it does nothing (not the player's either)
    u:hit(x, y, w, h, c, closable and c or function() end)
    return h
end

-- UTF-8: where the character before (after) byte i starts
local function prev_char(t, i)
    if i <= 1 then return 1 end
    i = i - 1
    while i > 1 and t:byte(i) and t:byte(i) >= 0x80 and t:byte(i) < 0xC0 do i = i - 1 end
    return i
end
local function next_char(t, i)
    if i > #t then return #t + 1 end
    i = i + 1
    while i <= #t and t:byte(i) >= 0x80 and t:byte(i) < 0xC0 do i = i + 1 end
    return i
end
M.prev_char, M.next_char = prev_char, next_char

local function escape(text)
    return (text:gsub("\\", "\\\226\129\160"):gsub("{", "\\{"):gsub("\n", " "))
end

local answer -- forward

local function draw_dialog(u, a)
    local d = dialog
    local s = u.s
    local vw, vh = a.x1 - a.x0, a.y1 - a.y0
    u:box(a.x0, a.y0, vw, vh, 0, "000000", { alpha = "70" })
    u:hit(a.x0, a.y0, vw, vh, "backdrop", function() end)
    local w = math.min(460 * s, vw - 32 * s)
    local p = 20 * s
    local names = d.names or {}
    local chip_rows = {}
    -- The earlier names as chips, in rows that fit (two at most)
    do
        local row, rx = {}, 0
        for i = #names, 1, -1 do
            local n = fit(names[i], w - 2 * p - 22 * s, 12, s)
            local cw = ui.text_width(n, 12, s) + 22 * s
            if rx > 0 and rx + cw > w - 2 * p then
                chip_rows[#chip_rows + 1] = row
                if #chip_rows == 2 then row = nil; break end
                row, rx = {}, 0
            end
            row[#row + 1] = { label = n, name = names[i], w = cw }
            rx = rx + cw + 6 * s
        end
        if row and #row > 0 then chip_rows[#chip_rows + 1] = row end
    end
    local h = (138 + (#chip_rows > 0 and (24 + 30 * #chip_rows) or 0) + (d.subtitle and 18 or 0)) * s
    local x, y = a.x0 + (vw - w) / 2, a.y0 + (vh - h) / 2
    u:box(x + 2 * s, y + 4 * s, w, h, 14, "000000", { alpha = "80" })
    u:box(x, y, w, h, 14, C.panel, { border = 1 * s, border_color = C.line })
    u:hit(x, y, w, h, "dialog", function() end)
    local yy = y + 28 * s
    u:text(x + p, yy, fit(d.title or "", w - 2 * p, 15, s, true), { size = 15, bold = true })
    if d.subtitle then
        yy = yy + 22 * s
        u:text(x + p, yy, fit(d.subtitle, w - 2 * p, 12, s), { size = 12, color = C.dim })
    end
    -- The field: what's typed, or the suggestion dimmed (Enter takes it, Tab puts it in to change it)
    local fy = yy + 20 * s
    local fw, fh = w - 2 * p, 34 * s
    u:box(x + p, fy, fw, fh, 8, C.field, { border = 1.5 * s, border_color = C.accent })
    local tx, cy = x + p + 11 * s, fy + fh / 2
    local value = d.value
    local fs = string.format("%.1f", 14 * 1.14 * s)
    local clip = string.format("\\clip(%d,%d,%d,%d)", math.floor(x + p + 4 * s), math.floor(fy), math.ceil(x + p + fw - 4 * s), math.ceil(fy + fh))
    local head = string.format("{\\an4\\pos(%.1f,%.1f)\\fn%s\\fs%s\\shad0\\bord0%s", tx, cy, ui.FONT, fs, clip)
    if value == "" then
        local hint = d.suggestion and d.suggestion ~= "" and d.suggestion or d.placeholder or ""
        table.insert(u.events, head .. "\\1c&H" .. C.dim .. "&}" .. escape(hint))
    else
        table.insert(u.events, head .. "\\1c&H" .. C.text .. "&}" .. escape(value))
    end
    -- The caret: after the text before it, laid out invisibly, so it lands where libass puts the letters
    table.insert(u.events, head .. "\\1c&H" .. C.text .. "&\\1a&HFF&}" .. escape(value:sub(1, d.cursor - 1))
                 .. "{\\1a&H00&\\1c&H" .. C.accent .. "&}|")
    yy = fy + fh
    if #chip_rows > 0 then
        yy = yy + 20 * s
        u:text(x + p, yy, "EARLIER NAMES", { size = 10.5, bold = true, color = C.dim })
        u:text(x + w - p, yy, "↑↓ or a click puts one in", { align = 6, size = 11, color = C.dim })
        yy = yy + 8 * s
        for _, row in ipairs(chip_rows) do
            local cx = x + p
            for _, chip in ipairs(row) do
                local on = d.pick and names[d.pick] == chip.name
                local hov = hover == "chip:" .. chip.name
                u:box(cx, yy, chip.w, 24 * s, 12, on and C.accent or (hov and C.hover or C.field), { border = 1 * s, border_color = on and C.accent or C.line })
                u:text(cx + chip.w / 2, yy + 12 * s, chip.label, { align = 5, size = 12, color = on and C.knob or C.label })
                local name = chip.name
                u:hit(cx, yy, chip.w, 24 * s, "chip:" .. name, function()
                    d.value, d.cursor, d.pick = name, #name + 1, nil
                end)
                cx = cx + chip.w + 6 * s
            end
            yy = yy + 30 * s
        end
    end
    -- Keys at the left, the buttons at the right
    local by = y + h - 46 * s
    u:box(x, by - 8 * s, w, math.max(1, s), 0, C.divider)
    local kx = x + p
    for _, k in ipairs({ { "Enter", d.value == "" and (d.ok or "OK") or (d.ok_typed or d.ok or "OK") }, { "Esc", "Cancel" } }) do
        local kw = ui.text_width(k[1], 11, s) + 10 * s
        u:kbd(kx + kw, by + 15 * s, k[1])
        u:text(kx + kw + 6 * s, by + 15 * s, k[2], { size = 11.5, color = C.dim })
        kx = kx + kw + ui.text_width(k[2], 11.5, s) + 20 * s
    end
    local ok = d.value == "" and (d.ok or "OK") or (d.ok_typed or d.ok or "OK")
    local bw = math.max(92 * s, ui.text_width(ok, 12, s, true) + 28 * s)
    u.hover = hover
    u:button(x + w - p - bw, bw, by + 1 * s, "dlg-ok", ok, function() answer(false) end, { primary = true })
    u:button(x + w - p - bw - 8 * s - 92 * s, 92 * s, by + 1 * s, "dlg-cancel", "Cancel", function() answer(true) end)
end

local function animated()
    for _, c in ipairs(cards) do
        if c.kind == "working" or c.till then return true end
    end
    return false
end

local render

-- area: the video's place (M.area's), given when it's drawn at another size than the window's (the picture rig)
local function draw(w, h, area)
    local a = area or M.area(w, h)
    local s = a.s
    local u = ui.new(s)
    u.hover = hover
    local phase = (now() % 1.4) / 1.4
    -- The cards: the newest error and the ones working first, at most MAX_CARDS (the oldest notes go)
    local cw = math.min(W_CARD * s, (a.x1 - a.x0) - 32 * s)
    local x = a.x0 + ((a.x1 - a.x0) - cw) / 2
    local y = a.y0 + 12 * s
    local shown = 0
    for _, c in ipairs(cards) do
        if shown >= MAX_CARDS then break end
        if not (c.quiet and panel_open) then
            y = y + draw_card(u, c, x, y, cw, phase) + GAP * s
            shown = shown + 1
        end
    end
    if dialog then draw_dialog(u, a) end
    return u
end

local function capture(on)
    if on == captured then return end
    captured = on
    if on then
        mp.add_forced_key_binding("MBTN_LEFT", "notify-click", function(info)
            if info.event ~= "down" and info.event ~= "press" then return end
            local pos = mp.get_property_native("mouse-pos")
            if not pos then return end
            for i = #hits, 1, -1 do
                local t = hits[i]
                if pos.x >= t.x0 and pos.x < t.x1 and pos.y >= t.y0 and pos.y < t.y1 then
                    if type(t.cmd) == "function" then t.cmd()
                    elseif type(t.cmd) == "table" then M.hide(t.cmd.id) end
                    return render()
                end
            end
        end, { complex = true })
        mp.add_forced_key_binding("MBTN_LEFT_DBL", "notify-click-dbl", function() end)
    else
        mp.remove_key_binding("notify-click")
        mp.remove_key_binding("notify-click-dbl")
    end
end

local function over_card(pos)
    if not pos or not pos.hover then return nil end
    for i = #hits, 1, -1 do
        local t = hits[i]
        if pos.x >= t.x0 and pos.x < t.x1 and pos.y >= t.y0 and pos.y < t.y1 then return t end
    end
end

-- Esc closes the newest error while one shows (not while the dialog is open: it's the dialog's then)
local esc_bound = false
local function bind_esc()
    local want = false
    if not dialog then
        for _, c in ipairs(cards) do if c.kind == "error" then want = true end end
    end
    if want == esc_bound then return end
    esc_bound = want
    if want then
        mp.add_forced_key_binding("ESC", "notify-esc", function()
            for i = #cards, 1, -1 do
                if cards[i].kind == "error" then table.remove(cards, i); break end
            end
            render()
        end)
    else
        mp.remove_key_binding("notify-esc")
    end
end

render = function()
    local d = mp.get_property_native("osd-dimensions")
    -- Expired cards go
    local t = now()
    for i = #cards, 1, -1 do
        if cards[i].till and cards[i].till <= t then table.remove(cards, i) end
    end
    bind_esc()
    if not d or d.w <= 0 then return end
    if #cards == 0 and not dialog then
        overlay:remove()
        hits = {}
        capture(false)
        if timer then timer:kill() end
        return
    end
    local u = draw(d.w, d.h)
    hits = u.hits
    overlay.res_x, overlay.res_y = d.w, d.h
    overlay.data = u:render()
    overlay:update()
    capture(dialog ~= nil or over_card(mp.get_property_native("mouse-pos")) ~= nil)
    if animated() then
        if not timer then timer = mp.add_periodic_timer(1 / 15, function() render() end)
        elseif not timer:is_enabled() then timer:resume() end
    elseif timer then
        timer:kill()
    end
end
M.render = render

-- A card shown, changed or ended (see the top)
function M.notify(t)
    if type(t) ~= "table" then return end
    local id = t.id ~= nil and tostring(t.id) or nil
    local at
    for i, c in ipairs(cards) do if id and c.id == id then at = i end end
    if t.kind == "hide" then
        if at then table.remove(cards, at) end
        return render()
    end
    local c = at and cards[at] or { id = id or ("card" .. tostring(now())), started = now() }
    if c.kind ~= "working" and t.kind == "working" then c.started = now() end
    local kind = t.kind
    if kind ~= "working" and kind ~= "done" and kind ~= "error" then kind = "info" end
    c.kind = kind
    c.title, c.detail, c.right, c.quiet = t.title, t.detail, t.right, t.quiet == true
    c.share = tonumber(t.share)
    if c.kind == "done" then c.till = now() + (tonumber(t.seconds) or DONE_SECONDS)
    elseif c.kind == "info" then c.till = now() + math.max(1.5, tonumber(t.seconds) or INFO_SECONDS)
    else c.till = nil end
    if not at then
        -- Notes go first when there are too many
        if #cards >= MAX_CARDS * 2 then
            for i, x in ipairs(cards) do if x.kind == "info" then table.remove(cards, i); break end end
        end
        cards[#cards + 1] = c
    end
    render()
end

function M.hide(id)
    M.notify({ id = id, kind = "hide" })
end

-- The panel open: the quiet notes aren't shown (the panel shows what they'd say)
function M.set_panel_open(v)
    if v == panel_open then return end
    panel_open = v
    if #cards > 0 then render() end
end

-- The dialog -----------------------------------------------------------------------------------------------------

local DIALOG_KEYS = {}

local function close_dialog()
    dialog = nil
    for _, k in ipairs(DIALOG_KEYS) do mp.remove_key_binding("notify-dlg-" .. k[1]) end
    mp.remove_key_binding("notify-dlg-text")
    render()
end

-- The answer, to the plugin that asked
answer = function(cancelled)
    local d = dialog
    if not d then return end
    local text = (d.value:gsub("^%s+", ""):gsub("%s+$", ""))
    close_dialog()
    if d.reply then
        mp.commandv("script-message-to", d.reply, "notify-answer", utils.format_json({ id = d.id, text = cancelled and "" or text, cancelled = cancelled }))
    end
end

local function edit(fn)
    return function()
        if not dialog then return end
        fn(dialog)
        render()
    end
end

local function type_text(d, text)
    d.value = d.value:sub(1, d.cursor - 1) .. text .. d.value:sub(d.cursor)
    d.cursor = d.cursor + #text
    d.pick = nil
end
M.type_text = type_text

-- ↑ goes back through the earlier names (the newest first), ↓ forward and back to what was typed
local function pick(d, dir)
    local names = d.names or {}
    if #names == 0 then return end
    if d.pick == nil then
        if dir > 0 then return end
        d.typed = d.value
        d.pick = #names
    else
        d.pick = d.pick + (dir < 0 and -1 or 1)
        if d.pick < 1 then d.pick = 1 end
        if d.pick > #names then
            d.pick = nil
            d.value = d.typed or ""
            d.cursor = #d.value + 1
            return
        end
    end
    d.value = names[d.pick]
    d.cursor = #d.value + 1
end
M.pick = pick

local function paste(d)
    local ok, text = pcall(mp.get_property, "clipboard/text")
    if ok and type(text) == "string" and text ~= "" then type_text(d, (text:gsub("[\r\n]+", " "))) end
end

DIALOG_KEYS = {
    { "ENTER", function() answer(false) end }, { "KP_ENTER", function() answer(false) end },
    { "ESC", function() answer(true) end },
    { "BS", edit(function(d) local p = prev_char(d.value, d.cursor); d.value = d.value:sub(1, p - 1) .. d.value:sub(d.cursor); d.cursor = p; d.pick = nil end), true },
    { "DEL", edit(function(d) local n = next_char(d.value, d.cursor); d.value = d.value:sub(1, d.cursor - 1) .. d.value:sub(n); d.pick = nil end), true },
    { "LEFT", edit(function(d) d.cursor = prev_char(d.value, d.cursor) end), true },
    { "RIGHT", edit(function(d) d.cursor = next_char(d.value, d.cursor) end), true },
    { "HOME", edit(function(d) d.cursor = 1 end) }, { "END", edit(function(d) d.cursor = #d.value + 1 end) },
    { "Meta+LEFT", edit(function(d) d.cursor = 1 end) }, { "Meta+RIGHT", edit(function(d) d.cursor = #d.value + 1 end) },
    { "UP", edit(function(d) pick(d, -1) end), true }, { "DOWN", edit(function(d) pick(d, 1) end), true },
    { "TAB", edit(function(d) if d.value == "" and d.suggestion then d.value = d.suggestion; d.cursor = #d.value + 1 end end) },
    { "Meta+BS", edit(function(d) d.value = d.value:sub(d.cursor); d.cursor = 1 end) },
    { "Ctrl+u", edit(function(d) d.value = d.value:sub(d.cursor); d.cursor = 1 end) },
    { "Meta+v", edit(paste) }, { "Ctrl+v", edit(paste) },
}

-- The dialog opened for a plugin (see the top); another one open is cancelled first
function M.ask(req)
    if type(req) ~= "table" then return end
    if dialog then answer(true) end
    local value = type(req.value) == "string" and req.value or ""
    dialog = {
        id = req.id, reply = req.reply, title = req.title, subtitle = req.subtitle, suggestion = req.suggestion,
        placeholder = req.placeholder, ok = req.ok, ok_typed = req.ok_typed,
        names = type(req.names) == "table" and req.names or {}, value = value, cursor = #value + 1,
    }
    for _, k in ipairs(DIALOG_KEYS) do
        mp.add_forced_key_binding(k[1], "notify-dlg-" .. k[1], k[2], { repeatable = k[3] == true })
    end
    mp.add_forced_key_binding("any_unicode", "notify-dlg-text", function(info)
        if not dialog or info.event == "up" then return end
        local text = info.key_text
        if type(text) ~= "string" or text == "" or text:byte(1) < 32 then return end
        type_text(dialog, text)
        render()
    end, { complex = true, repeatable = true })
    render()
end

function M.dialog_open() return dialog ~= nil end
-- For the tests and the picture rig: the state as it is
function M.state() return { cards = cards, dialog = dialog } end
M.draw = draw

mp.register_script_message("notify", function(json)
    local t = utils.parse_json(json or "")
    if type(t) == "table" then M.notify(t) end
end)
mp.register_script_message("ask", function(json)
    local t = utils.parse_json(json or "")
    if type(t) == "table" then M.ask(t) end
end)

-- Drawn again as the window or the video's place changes; the mouse over a card is the cards'
for _, p in ipairs({ "osd-dimensions", "video-margin-ratio-right", "video-margin-ratio-bottom", "fullscreen", "display-hidpi-scale" }) do
    mp.observe_property(p, "native", function() if #cards > 0 or dialog then render() end end)
end
mp.observe_property("mouse-pos", "native", function(_, pos)
    if #cards == 0 and not dialog then return end
    local t = over_card(pos)
    local h = t and t.id or nil
    if t and type(t.cmd) == "table" then h = t.cmd end
    if h ~= hover then hover = h; render() else capture(dialog ~= nil or t ~= nil) end
end)

return M
