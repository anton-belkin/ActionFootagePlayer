-- The side panel's drawing: rectangles, text and the controls it's made of, as ASS events for an OSD overlay, with
-- the places they take for the mouse. Sizes are in points, multiplied by the display's HiDPI scale; text is
-- measured by an estimate (libass gives no metrics), so labels get a little room to spare.

local mp = require 'mp'

local M = {}

local platform = mp.get_property("platform") or ""
M.FONT = platform == "darwin" and "Helvetica Neue" or platform == "windows" and "Segoe UI" or "sans-serif"

-- The mockup's colors, as ASS's BGR
M.C = {
    panel = "242222", rail = "1C1A1A", card = "2E2A2A", field = "322E2E", line = "4A4545", divider = "363333",
    text = "F4F2F2", dim = "A8A1A1", label = "DAD6D6", key = "CEC8C8", accent = "E56F1F", hover = "3A3636",
    knob = "FFFFFF",
    -- What's saved, beside what's chosen (an amber dot)
    saved = "6EC5F0",
}

-- libass's font size is a little smaller than the same size in points on the web: the mockup's sizes, matched
local TEXT_SCALE = 1.14

local Ui = {}
Ui.__index = Ui

-- s: points to pixels; clip: {x0, y0, x1, y1} the scrolled area, for what's drawn inside it
function M.new(s)
    return setmetatable({ s = s, events = {}, hits = {}, clip = nil, hover = nil }, Ui)
end

local function num(v) return string.format("%.1f", v) end

local function escape(text)
    local ok, r = pcall(mp.command_native, { "escape-ass", text })
    if ok and r then return r end
    return (text:gsub("\\", "\\\226\129\160"):gsub("{", "\\{"))
end

function Ui:clip_tag()
    local c = self.clip
    if not c then return "" end
    return string.format("\\clip(%d,%d,%d,%d)", math.floor(c[1]), math.floor(c[2]), math.ceil(c[3]), math.ceil(c[4]))
end

-- A rounded rectangle's path, in pixels
local function rrect_path(x, y, w, h, r)
    r = math.min(r, w / 2, h / 2)
    if r <= 0.5 then
        return string.format("m %s %s l %s %s %s %s %s %s", num(x), num(y), num(x + w), num(y), num(x + w), num(y + h), num(x), num(y + h))
    end
    local c = r * 0.4477
    local x1, y1 = x + w, y + h
    return table.concat({
        "m", num(x + r), num(y), "l", num(x1 - r), num(y),
        "b", num(x1 - c), num(y), num(x1), num(y + c), num(x1), num(y + r),
        "l", num(x1), num(y1 - r),
        "b", num(x1), num(y1 - c), num(x1 - c), num(y1), num(x1 - r), num(y1),
        "l", num(x + r), num(y1),
        "b", num(x + c), num(y1), num(x), num(y1 - c), num(x), num(y1 - r),
        "l", num(x), num(y + r),
        "b", num(x), num(y + c), num(x + c), num(y), num(x + r), num(y),
    }, " ")
end
M.rrect_path = rrect_path

local function circle_path(cx, cy, r)
    return rrect_path(cx - r, cy - r, 2 * r, 2 * r, r)
end
M.circle_path = circle_path

-- A filled shape; opts: border (px) and border_color, alpha ("00" opaque .. "FF"), fill (false: the outline alone)
function Ui:shape(path, color, opts)
    opts = opts or {}
    local fill_alpha = opts.fill == false and "FF" or (opts.alpha or "00")
    local tags = string.format("{\\an7\\pos(0,0)\\shad0\\bord%s\\1c&H%s&\\1a&H%s&\\3c&H%s&\\3a&H%s&%s\\p1}",
                               num(opts.border or 0), color, fill_alpha, opts.border_color or color, opts.alpha or "00", self:clip_tag())
    table.insert(self.events, tags .. path .. "{\\p0}")
end

function Ui:box(x, y, w, h, r, color, opts)
    self:shape(rrect_path(x, y, w, h, r * self.s), color, opts)
end

-- align: 4 left, 5 center, 6 right, at the vertical middle of y
function Ui:text(x, y, text, opts)
    opts = opts or {}
    local s = self.s
    local tags = string.format("{\\an%d\\pos(%s,%s)\\fn%s\\fs%s\\b%d\\shad0\\bord0\\1c&H%s&\\1a&H%s&%s}",
                               opts.align or 4, num(x), num(y), M.FONT, num((opts.size or 13) * TEXT_SCALE * s), opts.bold and 1 or 0,
                               opts.color or M.C.text, opts.alpha or "00", self:clip_tag())
    table.insert(self.events, tags .. escape(text))
end

-- About how wide a text is, in pixels
function M.text_width(text, size, s, bold)
    local w = 0
    for ch in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do
        if ch:match("[%u%d%%]") then w = w + 0.64
        elseif ch:match("[mwMW]") then w = w + 0.82
        elseif ch:match("[ilj%.,:;|'!]") then w = w + 0.26
        elseif ch == " " then w = w + 0.28
        else w = w + 0.53 end
    end
    return w * size * TEXT_SCALE * s * 0.92 * (bold and 1.06 or 1)
end

-- Lines that fit in width (pixels)
function M.wrap(text, width, size, s)
    local lines, line = {}, ""
    for word in text:gmatch("%S+") do
        local try = line == "" and word or (line .. " " .. word)
        if line ~= "" and M.text_width(try, size, s) > width then
            table.insert(lines, line)
            line = word
        else
            line = try
        end
    end
    if line ~= "" then table.insert(lines, line) end
    return lines
end

-- A place the mouse can click: cmd is an mpv command, or a function
function Ui:hit(x, y, w, h, id, cmd)
    local c = self.clip
    if c then
        local x0, y0, x1, y1 = math.max(x, c[1]), math.max(y, c[2]), math.min(x + w, c[3]), math.min(y + h, c[4])
        if x1 <= x0 or y1 <= y0 then return end
        x, y, w, h = x0, y0, x1 - x0, y1 - y0
    end
    table.insert(self.hits, { x0 = x, y0 = y, x1 = x + w, y1 = y + h, id = id, cmd = cmd })
end

function Ui:hovered(id)
    return id ~= nil and self.hover == id
end

-- The controls. Each takes the left edge x, the width w and the top y (pixels), and returns the next y

local function dim_alpha(disabled) return disabled and "99" or "00" end

-- A key, in a small box, its right edge at x
function Ui:kbd(right, cy, key)
    local s = self.s
    local w = math.max(18 * s, M.text_width(key, 11, s) + 10 * s)
    self:box(right - w, cy - 9 * s, w, 18 * s, 5, M.C.field, { border = 1 * s, border_color = M.C.line })
    self:text(right - w / 2, cy, key, { align = 5, size = 11, color = M.C.key })
    return w
end

function Ui:heading(x, w, y, title, key)
    local s = self.s
    local cy = y + 9 * s
    self:text(x, cy, title:upper(), { size = 11, bold = true, color = M.C.dim })
    if key then self:kbd(x + w, cy, key) end
    return y + 24 * s
end

function Ui:caption(x, w, y, text, opts)
    local s = self.s
    for _, l in ipairs(M.wrap(text, w, 12, s)) do
        self:text(x, y + 8 * s, l, { size = 12, color = (opts and opts.color) or M.C.dim })
        y = y + 16 * s
    end
    return y + 2 * s
end

-- The dot that marks what's saved, at a segment's top right corner
function Ui:saved_dot(right, top)
    local s = self.s
    self:shape(circle_path(right - 5 * s, top + 5 * s, 2.6 * s), M.C.saved)
end

-- options: { { label, cmd, disabled } }, selected: the index chosen (or nil), saved: the index saved (a dot; nil: none)
function Ui:segmented(x, w, y, id, options, selected, disabled, saved)
    local s = self.s
    local h, pad = 30 * s, 2 * s
    self:box(x, y, w, h, 8, M.C.field, { border = 1 * s, border_color = M.C.line, alpha = disabled and "60" or "00" })
    local bw = (w - 2 * pad - (#options - 1) * pad) / #options
    for i, opt in ipairs(options) do
        local bx = x + pad + (i - 1) * (bw + pad)
        local off = disabled or opt[3]
        local key = id .. ":" .. i
        if i == selected then
            self:box(bx, y + pad, bw, h - 2 * pad, 6, M.C.accent, { alpha = disabled and "80" or "00" })
        elseif self:hovered(key) and not off then
            self:box(bx, y + pad, bw, h - 2 * pad, 6, M.C.hover)
        end
        self:text(bx + bw / 2, y + h / 2, opt[1], { align = 5, size = 12, color = i == selected and M.C.knob or M.C.label,
                                                    alpha = off and "99" or "00" })
        if i == saved then self:saved_dot(bx + bw, y + pad) end
        if not off and i ~= selected then self:hit(bx, y, bw, h, key, opt[2]) end
    end
    return y + h + 6 * s
end

-- Frames of an aspect, drawn: options { { label, cmd, w, h } } (w, h: the frame's shape, its long side about 1), the
-- picture above, the label small below; selected, saved as in segmented
function Ui:ratios(x, w, y, id, options, selected, disabled, saved)
    local s = self.s
    local h, pad = 50 * s, 3 * s
    local bw = (w - (#options - 1) * pad) / #options
    for i, opt in ipairs(options) do
        local bx = x + (i - 1) * (bw + pad)
        local key = id .. ":" .. i
        local on = i == selected
        local fill = on and M.C.accent or (self:hovered(key) and not disabled and M.C.hover or M.C.field)
        self:box(bx, y, bw, h, 7, fill, { border = 1 * s, border_color = on and M.C.accent or M.C.line, alpha = disabled and "60" or "00" })
        local color = on and M.C.knob or M.C.label
        local fw, fh = (opt[3] or 1) * 22 * s, (opt[4] or 1) * 22 * s
        local cx, cy = bx + bw / 2, y + 19 * s
        self:shape(rrect_path(cx - fw / 2, cy - fh / 2, fw, fh, 2 * s), color,
                   { fill = false, border = 1.5 * s, alpha = disabled and "99" or "00" })
        self:text(cx, y + h - 9 * s, opt[1], { align = 5, size = 10.5, color = color, alpha = dim_alpha(disabled) })
        if i == saved then self:saved_dot(bx + bw, y) end
        if not disabled and not on then self:hit(bx, y, bw, h, key, opt[2]) end
    end
    return y + h + 6 * s
end

-- A row with a switch at the right: label, on, cmd, key (shown before the switch), disabled, saved (true or false: the
-- state saved, a dot by the label when it isn't the one shown)
function Ui:switch(x, w, y, id, label, on, cmd, key, disabled, saved)
    local s = self.s
    local h = 30 * s
    local cy = y + h / 2
    if self:hovered(id) and not disabled then self:box(x - 6 * s, y, w + 12 * s, h, 6, M.C.card) end
    self:text(x, cy, label, { size = 13, alpha = dim_alpha(disabled) })
    if saved ~= nil and saved ~= on then
        self:shape(circle_path(x + M.text_width(label, 13, s) + 8 * s, cy, 2.6 * s), M.C.saved)
    end
    local tw, th = 32 * s, 18 * s
    local tx = x + w - tw
    self:box(tx, cy - th / 2, tw, th, 9, on and M.C.accent or M.C.line, { alpha = disabled and "80" or "00" })
    local kr = 7 * s
    self:shape(circle_path(on and (tx + tw - th / 2) or (tx + th / 2), cy, kr), M.C.knob, { alpha = disabled and "80" or "00" })
    if key then self:kbd(tx - 8 * s, cy, key) end
    if not disabled then self:hit(x - 6 * s, y, w + 12 * s, h, id, cmd) end
    return y + h + 2 * s
end

-- One of a set: a round mark, the label, and a key or a note at the right
function Ui:radio(x, w, y, id, label, on, cmd, opts)
    opts = opts or {}
    local s = self.s
    local h = 28 * s
    local cy = y + h / 2
    if self:hovered(id) and not opts.disabled and not on then self:box(x - 6 * s, y, w + 12 * s, h, 6, M.C.card) end
    local r = 7 * s
    if on then
        self:shape(circle_path(x + r, cy, r), M.C.accent)
        self:shape(circle_path(x + r, cy, 3 * s), M.C.knob)
    else
        self:shape(circle_path(x + r, cy, r - 0.75 * s), M.C.dim, { fill = false, border = 1.5 * s, alpha = opts.disabled and "99" or "00" })
    end
    self:text(x + 2 * r + 9 * s, cy, label, { size = 13, alpha = dim_alpha(opts.disabled) })
    if opts.key then self:kbd(x + w, cy, opts.key) end
    if opts.note then self:text(x + w, cy, opts.note, { align = 6, size = 12, color = M.C.dim }) end
    if opts.saved then self:shape(circle_path(x + 2 * r + 9 * s + M.text_width(label, 13, s) + 8 * s, cy, 2.6 * s), M.C.saved) end
    if not opts.disabled and not on then self:hit(x - 6 * s, y, w + 12 * s, h, id, cmd) end
    return y + h
end

-- A button: primary is blue; key shown at its right
function Ui:button(x, w, y, id, label, cmd, opts)
    opts = opts or {}
    local s = self.s
    local h = 28 * s
    local color = opts.primary and M.C.accent or (self:hovered(id) and not opts.disabled and M.C.hover or M.C.field)
    self:box(x, y, w, h, 7, color, { border = 1 * s, border_color = opts.primary and M.C.accent or M.C.line,
                                     alpha = opts.disabled and "80" or "00" })
    local cx = x + w / 2
    if opts.key then cx = cx - (M.text_width(opts.key, 11, s) + 10 * s) / 2 end
    self:text(cx, y + h / 2, label, { align = 5, size = 12, bold = opts.primary, color = opts.primary and M.C.knob or M.C.label,
                                      alpha = dim_alpha(opts.disabled) })
    if opts.key then self:kbd(x + w - 6 * s, y + h / 2, opts.key) end
    if not opts.disabled then self:hit(x, y, w, h, id, cmd) end
    return y + h + 6 * s
end

-- A slider, value 0..1, with words at its ends: set(v) is called with where it's clicked or dragged to
function Ui:slider(x, w, y, id, value, set, left, right, disabled)
    local s = self.s
    local h = 30 * s
    local cy = y + h / 2
    local kr = 8 * s
    local tx, tw = x + kr, w - 2 * kr
    local a = disabled and "80" or "00"
    self:box(tx, cy - 2 * s, tw, 4 * s, 2, M.C.line, { alpha = a })
    if not disabled then self:box(tx, cy - 2 * s, tw * value, 4 * s, 2, M.C.accent) end
    self:shape(circle_path(tx + tw * value, cy, kr), M.C.knob, { alpha = a })
    if not disabled then
        self:hit(x, y, w, h, id, { drag = function(mx) set(math.min(1, math.max(0, (mx - tx) / tw))) end })
    end
    y = y + h
    if (left or "") == "" and (right or "") == "" then return y + 2 * s end
    self:text(x, y + 6 * s, left, { size = 11, color = M.C.dim, alpha = dim_alpha(disabled) })
    self:text(x + w, y + 6 * s, right, { size = 11, color = M.C.dim, align = 6, alpha = dim_alpha(disabled) })
    return y + 18 * s
end

-- A straight stroke from one point to another, t thick, as a filled shape's path
local function stroke_path(x1, y1, x2, y2, t)
    local dx, dy = x2 - x1, y2 - y1
    local len = math.sqrt(dx * dx + dy * dy)
    if len == 0 then return "" end
    local nx, ny = -dy / len * t / 2, dx / len * t / 2
    return string.format("m %s %s l %s %s %s %s %s %s", num(x1 + nx), num(y1 + ny), num(x2 + nx), num(y2 + ny),
                         num(x2 - nx), num(y2 - ny), num(x1 - nx), num(y1 - ny))
end

-- Drawn within a rectangle besides the scrolled area (a part of a rounded shape)
function Ui:within(x0, y0, x1, y1, fn)
    local c = self.clip
    self.clip = c and { math.max(x0, c[1]), math.max(y0, c[2]), math.min(x1, c[3]), math.min(y1, c[4]) } or { x0, y0, x1, y1 }
    fn()
    self.clip = c
end

M.SPLIT_ARROW = 30

-- A button in two: the main part (primary) runs cmd; the ▾ part at its right (id .. ":more") runs more, which opens a
-- list (dropdown). opts: open (the list is open: the ▾ part lit), disabled (the main part; the ▾ part still opens the
-- list, so another kind can be picked)
function Ui:split_button(x, w, y, id, label, cmd, more, opts)
    opts = opts or {}
    local s = self.s
    local h, aw = 28 * s, M.SPLIT_ARROW * s
    local mw = w - aw
    self:box(x, y, w, h, 7, M.C.accent, { border = 1 * s, border_color = M.C.accent, alpha = opts.disabled and "80" or "00" })
    local more_id = id .. ":more"
    if opts.open or self:hovered(more_id) then
        self:within(x + mw, y, x + w, y + h, function()
            self:box(x + mw - 8 * s, y, aw + 8 * s, h, 7, "000000", { alpha = opts.open and "A0" or "C8" })
        end)
    end
    self:box(x + mw, y + 5 * s, math.max(1, s), h - 10 * s, 0, M.C.knob, { alpha = "90" })
    self:text(x + mw / 2, y + h / 2, label, { align = 5, size = 12, bold = true, color = M.C.knob, alpha = dim_alpha(opts.disabled) })
    local cx, cy = x + mw + aw / 2, y + h / 2
    self:shape(string.format("m %s %s l %s %s %s %s", num(cx - 4 * s), num(cy - 2 * s), num(cx + 4 * s), num(cy - 2 * s),
                             num(cx), num(cy + 2.5 * s)), M.C.knob)
    if not opts.disabled then self:hit(x, y, mw, h, id, cmd) end
    self:hit(x + mw, y, aw, h, more_id, more)
    return y + h + 6 * s
end

M.ROW = 28

-- A list to pick from, over the panel's content: under the place [left, right] × [top, bottom] (a button), its right
-- edge at the button's, or above it when there's no room below. rows: { kind, label, disabled, cmd }; chosen: the kind
-- checked. Only noted here: popover() draws it, after everything else, so it's above it all and the mouse finds its
-- rows first
function Ui:dropdown(left, right, top, bottom, id, rows, chosen)
    self.pending = { left = left, right = right, top = top, bottom = bottom, id = id, rows = rows, chosen = chosen }
end

-- The list noted by dropdown, drawn within [top, bottom] of the window. Its places are marked popover = true: a click
-- elsewhere closes it. seen: { y0, y1 }, what shows of the scrolled area: a list whose button is out of it isn't drawn.
-- Returns where it is ({ x0, y0, x1, y1 }), or nil when there's none
function Ui:popover(top, bottom, seen)
    local p = self.pending
    if not p then return nil end
    self.pending = nil
    if seen and (p.bottom <= seen[1] or p.top >= seen[2]) then return nil end
    local s = self.s
    local rh, pad = M.ROW * s, 4 * s
    local lw = 0
    for _, r in ipairs(p.rows) do lw = math.max(lw, M.text_width(r.label, 12, s)) end
    local w = math.min(math.max(180 * s, lw + 46 * s), p.right - p.left)
    local h = #p.rows * rh + 2 * pad
    local x = p.right - w
    local y = p.bottom + 4 * s
    if y + h > bottom and p.top - 4 * s - h >= top then y = p.top - 4 * s - h end
    local saved_clip = self.clip
    self.clip = nil
    self:box(x, y + 3 * s, w, h, 8, "000000", { alpha = "A0" })
    self:box(x, y, w, h, 8, M.C.card, { border = 1 * s, border_color = M.C.line })
    -- The list's own area: a click on it off a row (a greyed one, the edge) does nothing, and the list stays
    table.insert(self.hits, { x0 = x, y0 = y, x1 = x + w, y1 = y + h, id = p.id, cmd = function() end, popover = true })
    for i, r in ipairs(p.rows) do
        local ry = y + pad + (i - 1) * rh
        local key = p.id .. ":" .. r.kind
        if self:hovered(key) and not r.disabled then self:box(x + pad, ry, w - 2 * pad, rh, 5, M.C.hover) end
        local cx, cy = x + 16 * s, ry + rh / 2
        if r.kind == p.chosen then
            self:shape(stroke_path(cx - 4.5 * s, cy + 0.2 * s, cx - 1.5 * s, cy + 3.2 * s, 1.8 * s), M.C.accent)
            self:shape(stroke_path(cx - 2.1 * s, cy + 3.2 * s, cx + 4.5 * s, cy - 3.6 * s, 1.8 * s), M.C.accent)
        end
        self:text(x + 30 * s, cy, r.label, { size = 12, color = r.disabled and M.C.dim or M.C.text, alpha = r.disabled and "50" or "00" })
        if not r.disabled then
            table.insert(self.hits, { x0 = x + pad, y0 = ry, x1 = x + w - pad, y1 = ry + rh, id = key, cmd = r.cmd, popover = true })
        end
    end
    self.clip = saved_clip
    return { x, y, x + w, y + h }
end

function Ui:divider(x, w, y)
    self:box(x, y, w, math.max(1, self.s), 0, M.C.divider)
    return y + math.max(1, self.s)
end

function Ui:render()
    return table.concat(self.events, "\n")
end

return M
