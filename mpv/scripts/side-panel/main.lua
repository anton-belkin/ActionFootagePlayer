-- side-panel: the plugins' settings in a panel at the right of the window, beside the video (it's moved over for it,
-- not covered): a rail of icons at the edge, one for each part (the project: the trip's marks, moments and clip
-- previews, the workspace plugin's; the clip, its look, the stabilization, performance and quality, the telemetry,
-- the Dolby Vision trims; support at the bottom), and the panel of the one chosen, scrolled
-- with the wheel. The plugins' settings as controls: it reads the state they keep in user-data/<plugin>/ and runs
-- their commands. While it's loaded, footage-menu leaves the right-click menu to mpv's own (it publishes
-- user-data/side_panel/loaded). An mpv plugin: this folder goes into ~~/scripts/.
--
-- Keys (rebind with `<key> script-binding side_panel/<name>` in input.conf):
--   TAB    toggle   the panel and its rail shown or hidden
-- Clicking the chosen icon again, or the arrow in the panel's header, leaves only the rail.
--
-- While it shows, mpv's on-screen controller is the box one at half its size, at the bottom of the video (the bottom bar
-- would go under the panel).
--
-- Options (~~/script-opts/side_panel.conf):
--   shown=yes        the rail shows when the player starts
--   open=yes         and the panel beside it
--   tab=project      the part it opens on: project, clip, look, stabilization, quality, telemetry, dv, support
--
-- Script messages (`script-message-to side_panel <message>`):
--   tab <name>                      open the panel on that part
--   dump <path> <w> <h> [<scale>]   write what it draws at that window size as an .ass file (for checking its look
--                                   without a window)

local mp = require 'mp'
local options = require 'mp.options'
local ui = require 'ui'

local o = { shown = true, open = true, tab = "project" }
options.read_options(o, "side_panel")

local C = ui.C
local RAIL, PANEL, HEADER, FOOTER, PAD = 48, 292, 44, 56, 16
local platform = mp.get_property("platform") or ""
local WEBSITE = "https://actionfootageplayer.ch"
local REPO = "https://github.com/anton-belkin/ActionFootagePlayer"
local REVEAL = platform == "darwin" and "Show in Finder" or platform == "windows" and "Show in Explorer" or "Open folder"
local CAMERAS = { ["dvtm_ac206.proto"] = "DJI Osmo Action 6", ["dvtm_oq101.proto"] = "DJI Osmo 360" }
local CURVES = { { "dlogm", "D-Log M" }, { "dlog", "D-Log" }, { "gplog", "GP-Log" }, { "gplog2", "GP-Log2" }, { "ilog", "I-Log (10-bit)" } }
local CURVE_NAMES = {}
for _, c in ipairs(CURVES) do CURVE_NAMES[c[1]] = c[2] end
local CORNERS = { { "top-left", "Top left" }, { "top-right", "Top right" }, { "bottom-left", "Bottom left" }, { "bottom-right", "Bottom right" } }
local FIELDS = { { "depth", "Depth" }, { "water", "In or out of water" }, { "pressure", "Pressure" }, { "altitude", "Altitude" },
                 { "gps", "GPS" }, { "speed", "Speed" }, { "temperature", "Sensor temperature" }, { "heartrate", "Heart rate" },
                 { "exposure", "Exposure" } }

local shown, open, tab = o.shown, o.open, o.tab
local scroll = {}
local hover, hits = nil, {}
local overlay = mp.create_osd_overlay("ass-events")
overlay.z = 10

local function state(plugin)
    return mp.get_property_native("user-data/" .. plugin)
end

local function local_file(path)
    return path and not path:find("://", 1, true)
end

local function clock(s)
    s = math.floor(s)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

-- The key each command is bound to (as footage-menu finds them): "Ctrl+g"; a capital letter as Shift+ it
local keys = {}
local function read_keys()
    local priority = {}
    keys = {}
    for _, b in ipairs(mp.get_property_native("input-bindings") or {}) do
        if b.cmd then b.cmd = b.cmd:gsub("^nonscalable ", "") end
        if b.cmd and not b.key:find("^MBTN") and not b.key:find("^MOUSE") and not b.key:find("^WHEEL")
           and (not priority[b.cmd] or b.priority > priority[b.cmd] or (b.priority == priority[b.cmd] and b.key == "SPACE")) then
            keys[b.cmd] = b.key:match("^%u$") and ("Shift+" .. b.key) or b.key
            priority[b.cmd] = b.priority
        end
    end
end

-- The icons, 20 points across, centered at cx, cy

local function icon(u, name, cx, cy, color)
    local s = u.s
    local stroke = { fill = false, border = 1.5 * s }
    local function at(x, y) return string.format("%.1f %.1f", cx + x * s, cy + y * s) end
    if name == "project" then
        -- A folder: its tab, and its body
        u:shape("m " .. at(-9, -6) .. " l " .. at(-3.5, -6) .. " " .. at(-1.5, -4) .. " " .. at(9, -4) .. " " .. at(9, 6.5) .. " " .. at(-9, 6.5)
                .. " " .. at(-9, -6), color, stroke)
        u:box(cx - 9 * s, cy - 1.5 * s, 18 * s, 1.5 * s, 0, color)
    elseif name == "clip" then
        u:shape(ui.rrect_path(cx - 8.5 * s, cy - 6.5 * s, 17 * s, 13 * s, 2.5 * s), color, stroke)
        u:shape("m " .. at(-2, -3.5) .. " l " .. at(3.5, 0) .. " " .. at(-2, 3.5), color)
    elseif name == "look" then
        u:shape(ui.circle_path(cx, cy, 7.5 * s), color, stroke)
        u:shape("m " .. at(0, -7.5) .. " b " .. at(4.2, -7.5) .. " " .. at(7.5, -4.2) .. " " .. at(7.5, 0)
                .. " b " .. at(7.5, 4.2) .. " " .. at(4.2, 7.5) .. " " .. at(0, 7.5), color)
    elseif name == "stabilization" then
        u:shape(ui.circle_path(cx, cy, 6.5 * s), color, stroke)
        u:box(cx - 10 * s, cy - 0.75 * s, 5 * s, 1.5 * s, 0, color)
        u:box(cx + 5 * s, cy - 0.75 * s, 5 * s, 1.5 * s, 0, color)
        u:shape(ui.circle_path(cx, cy, 1.8 * s), color)
    elseif name == "telemetry" then
        -- A gauge: an arc, and its needle
        local R, r = 8, 6.4
        local k1, k2 = R * 0.5523, r * 0.5523
        u:shape("m " .. at(-R, 3) .. " l " .. at(-R, 0) .. " b " .. at(-R, -k1) .. " " .. at(-k1, -R) .. " " .. at(0, -R)
                .. " b " .. at(k1, -R) .. " " .. at(R, -k1) .. " " .. at(R, 0) .. " l " .. at(R, 3) .. " " .. at(r, 3) .. " " .. at(r, 0)
                .. " b " .. at(r, -k2) .. " " .. at(k2, -r) .. " " .. at(0, -r) .. " b " .. at(-k2, -r) .. " " .. at(-r, -k2) .. " " .. at(-r, 0)
                .. " l " .. at(-r, 3), color)
        u:shape("m " .. at(-0.9, 1.6) .. " l " .. at(3.6, -4.6) .. " " .. at(4.6, -3.8) .. " " .. at(0.9, 2.6), color)
        u:shape(ui.circle_path(cx, cy + 2.1 * s, 2 * s), color)
    elseif name == "quality" then
        -- A runner: the head, and the body, arms and legs as thick strokes with round joints
        local function limb(points, wd)
            u:shape(ui.circle_path(cx + points[1][1] * s, cy + points[1][2] * s, wd / 2 * s), color)
            for i = 1, #points - 1 do
                local p0, p1 = points[i], points[i + 1]
                local dx, dy = p1[1] - p0[1], p1[2] - p0[2]
                local len = math.sqrt(dx * dx + dy * dy)
                local nx, ny = -dy / len * wd / 2, dx / len * wd / 2
                u:shape("m " .. at(p0[1] + nx, p0[2] + ny) .. " l " .. at(p1[1] + nx, p1[2] + ny) .. " " .. at(p1[1] - nx, p1[2] - ny)
                        .. " " .. at(p0[1] - nx, p0[2] - ny), color)
                u:shape(ui.circle_path(cx + p1[1] * s, cy + p1[2] * s, wd / 2 * s), color)
            end
        end
        u:shape(ui.circle_path(cx + 3 * s, cy - 7 * s, 2.2 * s), color)
        limb({ { 1.2, -3.6 }, { -1.2, 2 } }, 2.6)
        limb({ { 1.2, -3.2 }, { 3.6, -0.4 }, { 6.6, -1.6 } }, 1.7)
        limb({ { 0.6, -3.2 }, { -2.6, -2 }, { -4.6, 0.6 } }, 1.7)
        limb({ { -1.2, 2 }, { 2.4, 4.4 }, { 1.6, 8.6 } }, 2)
        limb({ { -1.2, 2 }, { -3.6, 5 }, { -7.6, 5.4 } }, 2)
    elseif name == "player" then
        u:box(cx - 8.5 * s, cy - 4.75 * s, 17 * s, 1.5 * s, 0, color)
        u:box(cx - 8.5 * s, cy + 3.25 * s, 17 * s, 1.5 * s, 0, color)
        u:shape(ui.circle_path(cx - 3 * s, cy - 4 * s, 2.8 * s), color)
        u:shape(ui.circle_path(cx + 3.5 * s, cy + 4 * s, 2.8 * s), color)
    elseif name == "dv" then
        -- "DV" over "SDR", and a grading wheel: a circle half filled
        u:text(cx - 2 * s, cy - 3.6 * s, "DV", { align = 5, size = 9, bold = true, color = color })
        u:text(cx - 2 * s, cy + 5 * s, "SDR", { align = 5, size = 6.5, bold = true, color = color })
        local gx, gy, gr = 7.2, -6.6, 2.6
        u:shape(ui.circle_path(cx + gx * s, cy + gy * s, gr * s), color, { fill = false, border = 1 * s })
        u:shape("m " .. at(gx, gy - gr) .. " b " .. at(gx + gr * 0.55, gy - gr) .. " " .. at(gx + gr, gy - gr * 0.55) .. " " .. at(gx + gr, gy)
                .. " b " .. at(gx + gr, gy + gr * 0.55) .. " " .. at(gx + gr * 0.55, gy + gr) .. " " .. at(gx, gy + gr), color)
    elseif name == "keys" then
        u:shape(ui.rrect_path(cx - 9 * s, cy - 6 * s, 18 * s, 12 * s, 2.5 * s), color, stroke)
        for _, x in ipairs({ -5.5, -1, 3.5 }) do u:box(cx + x * s, cy - 3 * s, 2 * s, 2 * s, 0, color) end
        u:box(cx - 4.5 * s, cy + 1.5 * s, 9 * s, 1.6 * s, 0, color)
    elseif name == "support" then
        u:shape(ui.circle_path(cx, cy, 8 * s), color, stroke)
        u:text(cx, cy + 0.5 * s, "?", { align = 5, size = 12, bold = true, color = color })
    end
end

-- The parts: { name, title, hidden (not on the rail for now) }. Support is at the rail's bottom
local TABS = {
    { "project", "Project" }, { "clip", "Clip" }, { "look", "Look" }, { "stabilization", "Stabilization" }, { "quality", "Performance & Quality" },
    { "telemetry", "Telemetry" }, { "player", "Player", true }, { "dv", "Dolby Vision" },
}
local TAB_TITLES = { support = "Support" }
for _, t in ipairs(TABS) do TAB_TITLES[t[1]] = t[2] end

-- The parts the rail has: Dolby Vision only where the dv look can be (the app's mpv on macOS)
local function shown_tabs()
    local list, d = {}, state("dlogm")
    for _, t in ipairs(TABS) do
        if not t[3] and (t[1] ~= "dv" or (d and d.dv)) then table.insert(list, t) end
    end
    return list
end

local function choose(name)
    if tab == name and open then open = false else tab, open = name, true end
end

-- The parts. Each draws into u from y (pixels) in the column x..x+w and returns where it ended

local function passport(d, g, t, h)
    local parts = {}
    if t and t.model and t.model ~= "" then
        table.insert(parts, CAMERAS[t.model] or (t.model:find("^dvtm_") and "DJI" or t.model))
    end
    if d then
        if d.log then table.insert(parts, CURVE_NAMES[d.curve] or d.curve)
        elseif d.hdr and d.hdr ~= "" then table.insert(parts, "HDR (" .. d.hdr:upper() .. ")")
        else table.insert(parts, "not log") end
    end
    if g then table.insert(parts, g.available and (g.kind == "project" and "Gyroflow project" or "gyro") or "no motion data") end
    if t and t.available then table.insert(parts, "telemetry") end
    if h and h.times and #h.times > 0 then table.insert(parts, #h.times == 1 and "1 mark" or (#h.times .. " marks")) end
    return table.concat(parts, " · ")
end

local function clip_part(u, x, w, y)
    local s = u.s
    local path = mp.get_property("path")
    if not path then return u:caption(x, w, y, "Nothing plays") end
    local d, g, t, h = state("dlogm"), state("gyroflow"), state("dji_telemetry"), state("highlights")
    for i, l in ipairs(ui.wrap(mp.get_property("filename", ""), w, 14, s)) do
        if i > 2 then break end
        u:text(x, y + 9 * s, l, { size = 14, bold = true })
        y = y + 19 * s
    end
    y = u:caption(x, w, y + 2 * s, passport(d, g, t, h))
    local half = (w - 8 * s) / 2
    y = y + 6 * s
    u:button(x, half, y, "reveal", REVEAL, "script-message-to footage_menu reveal", { disabled = not local_file(path) })
    y = u:button(x + half + 8 * s, half, y, "copy", "Copy path", "script-message-to footage_menu copy-path")

    y = u:heading(x, w, y + 14 * s, "Part", keys["ab-loop"])
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    u:button(x, half, y, "mark-a", a and ("Start " .. clock(a)) or "Mark start", "set ab-loop-a ${=time-pos}")
    y = u:button(x + half + 8 * s, half, y, "mark-b", b and ("End " .. clock(b)) or "Mark end", "set ab-loop-b ${=time-pos}", { disabled = not a })
    if a or b then y = u:button(x, w, y, "clear", "Clear the part", "set ab-loop-a no; set ab-loop-b no") end

    if g and local_file(path) then
        y = u:heading(x, w, y + 10 * s, "Export")
        if g.exporting then
            y = u:button(x, w, y, "export", "Cancel export", "script-message-to gyroflow cancel-export")
        else
            local label = (a and b and b > a) and ("Export the part " .. clock(a) .. "–" .. clock(b)) or "Export the clip"
            y = u:button(x, w, y, "export", label, "script-binding gyroflow/export", { primary = true, key = keys["script-binding gyroflow/export"] })
        end
        if d and d.log and (d.sdr_look or "") ~= "" then
            y = u:button(x, w, y, "export-sdr", "Export in SDR", "script-message-to gyroflow export sdr", { disabled = g.exporting })
        end
        y = u:button(x, w, y, "last", "Show the last export", "script-message-to gyroflow reveal-last-export", { disabled = (g.last_export or "") == "" })
    end

    if h and h.times and #h.times > 0 then
        y = u:heading(x, w, y + 10 * s, "Marks", keys["script-binding highlights/next"])
        local pos = mp.get_property_number("time-pos", 0)
        for i, time in ipairs(h.times) do
            local m = h.marks and h.marks[i] or {}
            local label = (m.name and m.name ~= "") and m.name or (m.camera and ("Highlight " .. i) or ("Mark " .. i))
            y = u:radio(x, w, y, "hl:" .. i, label, math.abs(pos - time) < 0.5, "script-message-to highlights go " .. i, { note = clock(time) })
        end
    end

    y = u:heading(x, w, y + 10 * s, "In the folder")
    local n, count = mp.get_property_number("playlist-pos", 0), mp.get_property_number("playlist-count", 0)
    u:button(x, half, y, "prev", "Previous clip", "playlist-prev", { disabled = n <= 0 })
    y = u:button(x + half + 8 * s, half, y, "next", "Next clip", "playlist-next", { disabled = n >= count - 1 })
    y = u:button(x, w, y, "frame", "Save the frame as an image", "screenshot", { key = keys["screenshot"] })
    return y
end

-- The project tab's own state: the folder chosen to start a project in (and the list open), which "⋯" is open
local chosen_root, choosing, card_more, moment_more = nil, false, false, nil

-- A box drawn behind what was drawn since event `at`
local function behind(u, at, x, y, w, h, color, opts)
    u:box(x, y, w, h, 8, color, opts)
    table.insert(u.events, at, table.remove(u.events))
end

-- The text cut to fit width, with an ellipsis
local function fit(text, width, size, s, bold)
    -- Room to spare: the estimate is short for some names (capitals, wide letters)
    width = width * 0.9
    if ui.text_width(text, size, s, bold) <= width then return text end
    local chars = {}
    for ch in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do chars[#chars + 1] = ch end
    while #chars > 1 and ui.text_width(table.concat(chars) .. "…", size, s, bold) > width do table.remove(chars) end
    return table.concat(chars) .. "…"
end

-- The end of a text that fits width, after an ellipsis: a path's last folders
local function fit_end(text, width, size, s)
    width = width * 0.9
    if ui.text_width(text, size, s) <= width then return text end
    local chars = {}
    for ch in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do chars[#chars + 1] = ch end
    while #chars > 1 and ui.text_width("…" .. table.concat(chars), size, s) > width do table.remove(chars, 1) end
    return "…" .. table.concat(chars)
end

-- A folder's path as a line: its parts from the volume's name ("NAS › Trips › Egypt"), the home folder as "Home"
local function folder_label(path)
    local home = os.getenv(platform == "windows" and "USERPROFILE" or "HOME")
    if home and path:sub(1, #home + 1) == home .. "/" then path = "Home/" .. path:sub(#home + 2) end
    path = path:gsub("^/Volumes/", ""):gsub("^/media/[^/]+/", ""):gsub("^/run/media/[^/]+/", ""):gsub("^/mnt/", "")
    local parts = {}
    for p in path:gmatch("[^/\\]+") do parts[#parts + 1] = p end
    return #parts > 0 and table.concat(parts, " › ") or path
end

-- A row of the project tab: a label, a note at the right, highlighted when on ("play": a moment playing, with its
-- mark); a "⋯" at the right when more is given
local function list_row(u, x, w, y, id, label, note, on, cmd, more)
    local s = u.s
    local h = 26 * s
    local cy = y + h / 2
    if on then u:box(x - 6 * s, y, w + 12 * s, h, 6, C.card)
    elseif u:hovered(id) then u:box(x - 6 * s, y, w + 12 * s, h, 6, C.hover) end
    local right = x + w
    if more then
        local mid = id .. ":more"
        if u:hovered(mid) then u:box(right - 20 * s, y + 3 * s, 22 * s, h - 6 * s, 5, C.field) end
        u:text(right - 9 * s, cy - 1 * s, "⋯", { align = 5, size = 15, color = C.dim })
        u:hit(right - 20 * s, y, 22 * s, h, mid, more)
        right = right - 26 * s
    end
    local nw = note and (ui.text_width(note, 12, s) + 8 * s) or 0
    if note then u:text(right, cy, note, { align = 6, size = 12, color = C.dim }) end
    local lx = x
    if on == "play" then
        u:shape(string.format("m %.1f %.1f l %.1f %.1f %.1f %.1f", x, cy - 4.5 * s, x + 7 * s, cy, x, cy + 4.5 * s), C.text)
        lx = x + 13 * s
    end
    u:text(lx, cy, fit(label, right - nw - lx, 13, s), { size = 13 })
    u:hit(x - 6 * s, y, (right - x) + 6 * s, h, id, cmd)
    return y + h + 2 * s
end

-- Small text buttons in a row, what a "⋯" opens: { label, cmd }
local function actions(u, x, w, y, id, list)
    local s = u.s
    local at = #u.events + 1
    local top = y
    local cx = x + 8 * s
    local h = 24 * s
    for i, a in ipairs(list) do
        local tw = ui.text_width(a[1], 12, s) + 14 * s
        if cx + tw > x + w then cx, y = x + 8 * s, y + h end
        local aid = id .. ":" .. i
        if u:hovered(aid) then u:box(cx - 4 * s, y + 3 * s, tw, h - 6 * s, 5, C.hover) end
        u:text(cx + 3 * s, y + h / 2, a[1], { size = 12, color = C.label })
        u:hit(cx - 4 * s, y + 2 * s, tw, h - 2 * s, aid, a[2])
        cx = cx + tw + 4 * s
    end
    y = y + h
    behind(u, at, x, top, w, y - top, C.field)
    return y + 4 * s
end

local function project_part(u, x, w, y)
    local s = u.s
    local ws = state("workspace")
    if not ws or not ws.clip then return u:caption(x, w, y, "Nothing plays") end
    if ws.clip:find("://", 1, true) then return u:caption(x, w, y, "A project is for footage on this computer or the NAS") end
    local h = state("highlights") or {}
    local pad = 10 * s
    local ix, iw = x + pad, w - 2 * pad

    -- The project, or how to start one
    local at = #u.events + 1
    local top = y
    y = y + pad
    if not ws.project then
        u:text(ix, y + 8 * s, "No project yet", { size = 14, bold = true })
        y = y + 22 * s
        y = u:caption(ix, iw, y, "A project is a trip's folder. In it the player makes the folders _Moments, _Clips and _Exports, "
            .. "and _ActionFootagePlayer for its own notes. A clip's own files, its marks and Gyroflow project, "
            .. "stay beside the clip. Every folder inside the project is part of it. Marks need no project: they're "
            .. "kept beside the clip anyway.")
        local choices = ws.project_choices or {}
        local known = false
        for _, c in ipairs(choices) do if c == chosen_root then known = true end end
        if not known then chosen_root = ws.project_new or choices[#choices] end
        y = y + 4 * s
        if chosen_root then
            -- The folder chosen by its name, the path to it below; the list, a folder a line, indented by depth
            local fh = 42 * s
            u:box(ix, y, iw, fh, 7, C.panel, { border = 1 * s, border_color = choosing and C.accent or C.line })
            local name = chosen_root:match("([^/\\]+)[/\\]?$") or chosen_root
            local parent = chosen_root:match("^(.*)[/\\][^/\\]+[/\\]?$")
            u:text(ix + 10 * s, y + 14 * s, fit(name, iw - 34 * s, 13, s, true), { size = 13, bold = true })
            u:text(ix + 10 * s, y + 30 * s, parent and parent ~= "" and fit_end("in " .. folder_label(parent), iw - 34 * s, 11, s) or "the top of the disk",
                   { size = 11, color = C.dim })
            u:text(ix + iw - 13 * s, y + fh / 2, choosing and "▴" or "▾", { align = 5, size = 12, color = C.dim })
            u:hit(ix, y, iw, fh, "choose", function() choosing = not choosing end)
            y = y + fh + 2 * s
            if choosing then
                for i, c in ipairs(choices) do
                    local id = "root:" .. i
                    local rh = 24 * s
                    if c == chosen_root then u:box(ix, y, iw, rh, 5, C.accent)
                    elseif u:hovered(id) then u:box(ix, y, iw, rh, 5, C.hover) end
                    local indent = math.min(i - 1, 6) * 10 * s
                    local cname = c:match("([^/\\]+)[/\\]?$") or c
                    u:text(ix + 8 * s + indent, y + rh / 2, fit(cname, iw - 16 * s - indent, 12, s), { size = 12, color = c == chosen_root and C.knob or C.label })
                    u:hit(ix, y, iw, rh, id, function() chosen_root, choosing = c, false end)
                    y = y + rh
                end
                y = y + 4 * s
            end
            local root = chosen_root
            y = u:button(ix, iw, y + 4 * s, "init", "Initialize the project here", function()
                mp.commandv("script-message-to", "workspace", "init-project", root)
            end, { primary = true })
        end
    else
        local name = ws.project:match("([^/\\]+)[/\\]?$") or ws.project
        u:text(ix, y + 9 * s, fit(name, iw - 30 * s, 14, s, true), { size = 14, bold = true })
        if u:hovered("card:more") then u:box(ix + iw - 22 * s, y - 2 * s, 24 * s, 22 * s, 5, C.field) end
        u:text(ix + iw - 10 * s, y + 8 * s, "⋯", { align = 5, size = 15, color = C.dim })
        u:hit(ix + iw - 22 * s, y - 2 * s, 24 * s, 22 * s, "card:more", function() card_more = not card_more end)
        y = y + 22 * s
        local st = ws.clips_stats
        local total = ws.moments_total or 0
        local parts = { total == 1 and "1 moment" or (total .. " moments") }
        if st and st.total > 0 then parts[#parts + 1] = st.previews .. " of " .. st.total .. " clip previews" end
        y = u:caption(ix, iw, y, table.concat(parts, " · "))
        if card_more then
            y = actions(u, ix, iw, y + 2 * s, "card", {
                { platform == "darwin" and "Project folder in Finder" or "Project folder", "script-message-to workspace project-folder" },
                { "Make again for removed clips", "script-message-to workspace clips-again" },
                { (ws.moments_alone and "✓ " or "") .. "New moments stabilized on their own", "script-message-to workspace moments-alone" },
            })
        end
        local where = platform == "darwin" and "Finder" or "the folder"
        if ws.clips then
            y = u:caption(ix, iw, y + 2 * s, "Making clip previews: " .. ws.clips.done .. " of " .. ws.clips.total, { color = C.text })
            y = u:button(ix, iw, y, "clips-stop", "Stop", "script-message-to workspace clips-stop")
        elseif st and st.new > 0 then
            y = u:button(ix, iw, y + 2 * s, "clips-make", "Make clip previews · " .. st.new .. " new", "script-message-to workspace clips-make")
            -- All in _Clips/ itself: for a camera's folders by date, say, that mean nothing to the trip
            y = u:switch(ix, iw, y - 2 * s, "clips-flat", "Ignore subfolders", ws.clips_flat == true, "script-message-to workspace clips-flat")
        end
        if ws.clips_failed and not ws.clips then
            y = u:button(ix, iw, y, "clips-failed", ws.clips_failed .. (ws.clips_failed == 1 and " preview" or " previews") .. " couldn't be made: why",
                         "script-message-to workspace clips-failed")
        end
        if st and st.previews > 0 then
            y = u:button(ix, iw, y, "clips-show", "Show all clips in " .. where, "script-message-to workspace clips-folder")
        end
        if total > 0 then
            y = u:button(ix, iw, y, "moments-show", "Show all moments in " .. where, "script-message-to workspace moments-folder")
        end
    end
    behind(u, at, x, top, w, y - top + pad - 6 * s, C.card)
    y = y + pad

    -- This clip
    y = y + 8 * s
    u:divider(x, w, y)
    local file = ws.clip:match("([^/\\]+)$") or ws.clip
    -- A moment playing is a video of its own (its part of the clip): it's what the panel shows
    local mo = ws.moment and not ws.moment.missing and ws.offset and ws.moment or nil
    u:text(x, y + 14 * s, fit((mo and ("THIS MOMENT · " .. mo.name) or ("THIS CLIP · " .. file)), w, 11, s, true), { size = 11, bold = true, color = C.dim })
    y = y + 28 * s
    if mo then
        y = u:caption(x, w, y - 6 * s, clock(mo["in"]) .. "–" .. clock(mo.out) .. " of " .. file)
        y = u:button(x, w, y + 2 * s, "whole", "Open the whole clip at " .. clock(mo["in"]), "script-message-to workspace open-original")
        -- How it's stabilized: as it was made
        if ws.snap then
            y = u:switch(x, w, y, "alone", "Stabilize on its own", ws.alone == true, "script-message-to workspace moment-alone")
            local g = state("gyroflow") or {}
            y = u:caption(x, w, y - 2 * s, ws.own_project and "Its own Gyroflow project: changes to the clip's don't reach it"
                or ("The default preset, " .. (g.view == "off" and "stabilization off" or ((g.level or "medium") .. (g.view == "wide" and ", wider view" or "")))
                    .. ": the clip's project, if it gets one, doesn't reach it"))
        else
            y = u:caption(x, w, y, "Made before moments kept their stabilization: it plays with the clip's. Trim or update it to keep its own")
        end
        local idx
        for i, m in ipairs(ws.moments or {}) do if m.file == mo.file then idx = i end end
        if idx then
            y = actions(u, x, w, y, "this-moment", {
                { "Rename…", "script-message-to workspace moment-rename " .. idx },
                { "Remove", "script-message-to workspace moment-remove " .. idx },
                { REVEAL, "script-message-to workspace moment-reveal " .. idx },
            })
        end
    elseif ws.playing_preview then
        -- A clip preview playing: its own stabilization, a variant of the clip's
        y = u:switch(x, w, y, "clip-own", "Its own stabilization", ws.own_project ~= nil, "script-message-to workspace clip-own")
        y = u:caption(x, w, y - 2 * s, ws.own_project and "A project of this preview's own: a variant of the clip's (Ctrl+e opens it)"
            or "The clip's, as it plays. Copy the preview in Finder for another variant")
    elseif ws.editing then
        y = u:caption(x, w, y, "Editing the moment " .. ws.editing.name .. ": move its start or end below, then "
            .. (keys["script-binding workspace/moment"] or "k"), { color = C.text })
    end

    local pos = mp.get_property_number("time-pos", 0)
    y = u:heading(x, w, y + (mo and 6 * s or 0), mo and "Marks in this moment" or "Marks of this clip", keys["script-binding highlights/mark"])
    local near = nil
    local times = h.times or {}
    for i, t in ipairs(times) do
        local m = h.marks and h.marks[i] or {}
        local label = (m.name and m.name ~= "") and m.name
            or (m.camera == "dji" and "DJI quick switch" or m.camera == "gopro" and "GoPro HiLight" or ("Mark " .. i))
        -- Here: within half a second, or the one just jumped to (a jump lands h.before seconds ahead of it)
        local here = math.abs(pos - t) <= 0.5 or (h.jumped == i and pos >= t - (h.before or 2) - 0.25 and pos <= t + 0.5)
        if here and not near then near = i end
        y = list_row(u, x, w, y, "mark:" .. i, label, clock(t), here, "script-message-to highlights go " .. i)
    end
    if #times == 0 then y = u:caption(x, w, y, "None yet") end
    y = u:button(x, w, y + 4 * s, "mark", near and "Remove the mark here" or "Add a mark here", "script-binding highlights/mark",
                 { key = keys["script-binding highlights/mark"] })

    local list = mo and {} or (ws.moments or {})
    if not mo then y = u:heading(x, w, y + 8 * s, "Moments of this clip") end
    for i, m in ipairs(list) do
        local playing = (ws.moment and ws.moment.file == m.file) or (ws.editing and ws.editing.file == m.file)
        y = list_row(u, x, w, y, "moment:" .. i, m.name, clock(m["in"]) .. "–" .. clock(m.out), playing and "play" or false,
                     "script-message-to workspace moment-play " .. i, function() moment_more = moment_more ~= i and i or nil end)
        if moment_more == i then
            y = actions(u, x, w, y, "moment:" .. i, {
                { "Play", "script-message-to workspace moment-play " .. i },
                { "In its clip", "script-message-to workspace moment-edit " .. i },
                { "Rename…", "script-message-to workspace moment-rename " .. i },
                { "Remove", "script-message-to workspace moment-remove " .. i },
                { REVEAL, "script-message-to workspace moment-reveal " .. i },
            })
        end
    end
    if mo then
        -- nothing: the moment is what plays
    elseif not ws.project then
        y = u:caption(x, w, y, "Moments need the project: initialize it above")
    elseif #list == 0 then
        y = u:caption(x, w, y, "None yet: mark a part below, then " .. (keys["script-binding workspace/moment"] or "k"))
    end

    if ws.project and not mo then
        local label = ws.clip_preview == "made" and "Mark this clip as not interesting"
            or ws.clip_preview == "removed" and "Not interesting · undo" or "Make this clip's preview"
        y = u:button(x, w, y + 6 * s, "not-interesting", label, "script-binding workspace/not-interesting",
                     { key = keys["script-binding workspace/not-interesting"], disabled = ws.clips ~= nil })
    end

    -- The part marked now: it goes with the clip
    y = y + 10 * s
    local pat = #u.events + 1
    local ptop = y
    y = y + pad
    y = u:heading(ix, iw, y, "Selected part", keys["ab-loop"])
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local ihalf = (iw - 8 * s) / 2
    u:button(ix, ihalf, y, "part-a", a and ("Start " .. clock(a)) or "Mark start", "set ab-loop-a ${=time-pos}")
    y = u:button(ix + ihalf + 8 * s, ihalf, y, "part-b", b and ("End " .. clock(b)) or "Mark end", "set ab-loop-b ${=time-pos}", { disabled = not a })
    local k = keys["script-binding workspace/moment"]
    local m = ws.moment or ws.editing
    -- The part in the clip's time (a moment playing starts at its in)
    local off = ws.offset or 0
    if ws.saving then
        y = u:caption(ix, iw, y, "Saving " .. ws.saving .. "…", { color = C.text })
    elseif a and b and b > a and not ws.project then
        y = u:caption(ix, iw, y, "To keep it as a moment, initialize the project above")
        y = u:button(ix, iw, y, "part-clear", "Clear the part", "set ab-loop-a no; set ab-loop-b no")
    elseif a and b and b > a then
        if m and not m.missing and math.abs(a + off - m["in"]) < 0.001 and math.abs(b + off - m.out) < 0.001 then
            y = u:caption(ix, iw, y, m.name .. " as it is: move its start or end to change it")
        elseif m and not m.missing then
            y = u:button(ix, iw, y, "save", (mo and "Trim " or "Update ") .. fit(m.name, iw * 0.4, 12, s, true) .. " to " .. clock(a) .. "–" .. clock(b),
                         "script-binding workspace/moment", { primary = true, key = k })
        else
            y = u:button(ix, iw, y, "save", "Save as moment " .. clock(a) .. "–" .. clock(b), "script-binding workspace/moment", { primary = true, key = k })
        end
        y = u:button(ix, iw, y, "part-clear", "Clear the part", "set ab-loop-a no; set ab-loop-b no")
    else
        local l = keys["ab-loop"] or "l"
        y = u:caption(ix, iw, y, a and ("Now its end: " .. l) or mo and ("To trim the moment, mark the part of it to keep: " .. l .. ", then "
            .. l .. " again. To make it longer: Open the whole clip") or ("Its start: " .. l .. ", then its end: " .. l .. " again"))
    end
    behind(u, pat, x, ptop, w, y - ptop + pad - 6 * s, C.panel, { border = 1 * s, border_color = C.line })
    return y + pad
end

local function look_title(d, look)
    if look == "hdr" then return (not d.log and d.hdr == "pq") and "HDR, PQ" or "HDR, HLG" end
    if look == "dv" then return "HDR, HLG+DV" end
    if look == "dvsdr" then return "SDR, HLG+DV" end
    if look == "tonemap" then return "SDR tone mapped from HDR by mpv" end
    if look == "nbcu" then return "SDR, NBCU" end
    if look == "maker" then return "SDR, camera maker's LUT" end
    return look
end

-- What the look chosen is, in a few words
local function look_note(d, look)
    if look == "hdr" then
        if platform == "windows" then return "The HDR picture as it is: an HDR display shows it, for an SDR one mpv tones it down" end
        return (d.hdr ~= "" and not d.log and ("The file's HDR (" .. d.hdr:upper() .. ") as it is. ") or "The HDR picture as it is. ")
            .. "An SDR screen gets it toned down by macOS"
    end
    if look == "dv" then
        if d.dv_own then return "HDR with the file's own Dolby Vision metadata: macOS tones it down by them, as Apple's devices do" end
        return "HLG with Dolby Vision metadata, shown by macOS: in HDR on an HDR screen, toned down by the metadata's trims "
            .. "on an SDR one, as QuickTime and Apple's devices show the export. " .. (d.dv_custom and "These are your own trims (Dolby Vision, in the rail)."
            or "They're picked by Action Footage Player's author, fitted to NBCU's HLG to SDR conversion, and can be changed in Dolby Vision.")
    end
    if look == "dvsdr" then
        if d.dv_own then return "The file's own Dolby Vision, toned down to SDR by macOS on any screen, as an SDR screen shows it" end
        return "The same HLG + Dolby Vision, always toned down to SDR by macOS with the trims, on an HDR screen too: "
            .. "what an SDR screen shows. " .. (d.dv_custom and "These are your own trims (Dolby Vision, in the rail)."
            or "The trims are the author's, fitted to NBCU's conversion, and can be changed in Dolby Vision.")
    end
    if look == "tonemap" then return "mpv tones the HDR down to SDR itself, its reference white at SDR white (BT.2408): the same on every system and screen" end
    if look == "nbcu" then return "NBCUniversal's HLG to SDR conversion (LUT v1.2): neutral, the same for every camera" end
    if look == "maker" then return (d.maker_lut or "") ~= "" and d.maker_lut or ((d.maker ~= "" and d.maker or "The maker") .. "'s own LUT to Rec.709") end
    if look == "off" then return "The log signal as the camera recorded it, without a look" end
    return nil
end

-- The dv look's trims (12-bit codes, 2048 neutral), as DV Lab has them; `scene`: the scene levels' offsets from L1,
-- not offered while L1 is each frame's own
local DV_TRIMS = {
    { "slope", "Slope", "Gain after the tone curve: lighter, the highlights clip sooner" },
    { "offset", "Offset", "Lifts everything, the shadows most" },
    { "power", "Power", "Mid-tones darker (right) or lighter (left)" },
    { "midContrast", "Mid contrast", "Contrast around the mid-tones" },
    { "clipTrim", "Highlight roll-off", "Right: highlights roll off to white; left: they clip" },
    { "saturation", "Saturation", nil },
    { "chromaWeight", "Chroma weight", "How much saturated colors are kept as brightness is compressed" },
    { "l3Avg", "Scene average", "The scene's average brightness Dolby Vision exposes by", scene = true },
    { "l3Max", "Scene highlights", "Right: highlights taken as brighter, given more room", scene = true },
    { "l3Min", "Scene blacks", nil, scene = true },
}

-- The dv look's trims (not for a file with its own Dolby Vision): as sliders, kept by dlogm as they change
local function dv_part(u, x, w, y)
    local s = u.s
    local d = state("dlogm")
    if not mp.get_property("path") then return u:caption(x, w, y, "Nothing plays") end
    if not d or not d.dv_trims then return u:caption(x, w, y, "Needs the dlogm plugin and the app's mpv") end
    if d.dv_own then return u:caption(x, w, y, "This file has its own Dolby Vision: the trims here are for log clips") end
    y = u:caption(x, w, y, "The metadata macOS tones HLG+DV down to SDR by. The author's are picked to taste, "
                  .. "fitted to NBCU's conversion; exports take what's set here", { color = C.label })
    if d.dv_live then
        y = u:caption(x, w, y + 4 * s, "The scene's levels (Dolby Vision's L1) are measured from every frame as it plays, "
                      .. "so macOS maps each scene for what's in it; the trims here go on top")
        y = u:caption(x, w, y + 4 * s, "Extreme values of some change the picture on an HDR screen too")
    else
        y = u:caption(x, w, y + 4 * s, "Extreme values of some, the scene levels most, change the picture on an HDR screen too")
    end
    if d.look ~= "dv" and d.look ~= "dvsdr" then
        local has = {}
        for _, l in ipairs(d.looks or {}) do has[l] = true end
        local m = has.dvsdr and "dvsdr" or "dv"
        y = u:button(x, w, y + 6 * s, "show-dv", "Show " .. (m == "dvsdr" and "SDR, HLG+DV" or "HDR, HLG+DV") .. ", to see them",
                     "script-message-to dlogm look " .. m, { primary = true })
    end
    y = y + 6 * s
    for _, t in ipairs(DV_TRIMS) do
        if not (t.scene and d.dv_live) then
            local v = d.dv_trims[t[1]] or 2048
            u:text(x, y + 8 * s, t[2], { size = 12, color = C.label })
            y = u:slider(x, w, y + 14 * s, "dv:" .. t[1], v / 4095, function(p)
                mp.commandv("script-message-to", "dlogm", "dv-trim", t[1], tostring(math.floor(p * 4095 + 0.5)))
            end, "", "")
            if t[3] then y = u:caption(x, w, y, t[3]) end
            y = y + 8 * s
        end
    end
    y = u:button(x, w, y + 4 * s, "dv-reset", d.dv_custom and "Back to the author's trims" or "These are the author's trims",
                 "script-message-to dlogm dv-reset", { disabled = not d.dv_custom })
    return u:button(x, w, y + 4 * s, "dv-neutral", d.dv_neutral and "These are neutral trims" or "Neutral trims: none added",
                    "script-message-to dlogm dv-neutral", { disabled = d.dv_neutral })
end

local function look_part(u, x, w, y)
    local s = u.s
    local d = state("dlogm")
    if not mp.get_property("path") then return u:caption(x, w, y, "Nothing plays") end
    if not d then return u:caption(x, w, y, "Needs the dlogm plugin") end
    y = u:heading(x, w, y, "Video output", keys["script-binding dlogm/sdr"])
    if not d.log and (d.hdr or "") == "" then
        y = u:caption(x, w, y, "Not log, as the camera says")
    else
        if d.screen == "hdr" then y = u:caption(x, w, y, "HDR screen: SDR shows as an SDR screen would") end
        for _, look in ipairs(d.looks or {}) do
            y = u:radio(x, w, y, "look:" .. look, look_title(d, look), d.look == look, "script-message-to dlogm look " .. look,
                        { key = look == "hdr" and keys["script-binding dlogm/hdr"] or nil })
        end
        if d.log then
            y = u:radio(x, w, y, "look:off", "As recorded, log", d.look == "off", "script-binding dlogm/off", { key = keys["script-binding dlogm/off"] })
        end
        if d.log and d.maker == "" then
            y = u:caption(x, w, y + 2 * s, d.curve == "dlogm" and "DJI's LUT isn't downloaded" or ("No maker's LUT for " .. (CURVE_NAMES[d.curve] or d.curve) .. " yet"))
        end
        local note = look_note(d, d.look)
        if note then y = u:caption(x, w, y + 6 * s, note, { color = C.label }) end
    end

    if (d.look == "dv" or d.look == "dvsdr") and not d.dv_own and d.dv_trims then
        y = u:button(x, w, y + 4 * s, "to-dv", "Dolby Vision trims…", function() tab, open = "dv", true end)
    end

    y = u:heading(x, w, y + 10 * s, "Log curve", keys["script-binding dlogm/curve"])
    y = u:radio(x, w, y, "curve:auto", d.recorded ~= "" and ("From the clip: " .. (CURVE_NAMES[d.recorded] or d.recorded)) or "None, not log",
                d.curve == d.recorded, "script-message-to dlogm curve auto")
    for _, c in ipairs(CURVES) do
        if c[1] ~= d.recorded then
            y = u:radio(x, w, y, "curve:" .. c[1], c[2], d.curve == c[1], "script-message-to dlogm curve " .. c[1])
        end
    end

    y = u:heading(x, w, y + 10 * s, "Picture")
    y = u:switch(x, w, y, "denoise", "Noise reduction", d.denoise, "script-binding dlogm/denoise", keys["script-binding dlogm/denoise"])
    return y
end

local LEVEL_NOTES = { weak = "10 % cropped, smoothness 50 %", medium = "20 % cropped, smoothness 75 %", strong = "30 % cropped, smoothness 100 %" }

local function stabilization_part(u, x, w, y)
    local s = u.s
    local g = state("gyroflow")
    if not local_file(mp.get_property("path")) then return u:caption(x, w, y, "Nothing to stabilize") end
    if not g then return u:caption(x, w, y, "Needs the gyroflow plugin") end
    local edit = keys["script-binding gyroflow/edit"]

    -- What it's stabilized with
    local title, note
    if not g.available then
        title, note = "No motion data in this clip", "Gyroflow can find the motion in the picture (optical flow)"
    elseif g.kind == "project" then
        title, note = "Gyroflow project", "Played as Gyroflow renders it: its smoothing, zoom and lens"
    else
        title, note = "The camera's gyro data", "Gyroflow's lens profile, smoothed as the level says"
    end
    local lines = ui.wrap(note, w - 20 * s, 12, s)
    local ch = (10 + 18 + 16 * #lines + 8 + 28 + 10) * s
    u:box(x, y, w, ch, 8, C.card)
    u:text(x + 10 * s, y + 19 * s, title, { size = 13, bold = true })
    local ty = y + 30 * s
    for _, l in ipairs(lines) do
        u:text(x + 10 * s, ty + 6 * s, l, { size = 12, color = C.dim })
        ty = ty + 16 * s
    end
    if g.waiting then
        u:text(x + 10 * s, ty + 22 * s, "Waiting for the project from Gyroflow…", { size = 12, color = C.dim })
    else
        u:button(x + 10 * s, w - 20 * s, ty + 8 * s, "edit", g.available and "Edit in Gyroflow…" or "Stabilize in Gyroflow…",
                 "script-binding gyroflow/edit", { key = edit, primary = not g.available })
    end
    y = y + ch + 14 * s
    if not g.available then return y end

    y = u:heading(x, w, y, "View", keys["script-binding gyroflow/toggle"])
    local views = { normal = 1, wide = 2, off = 3 }
    y = u:segmented(x, w, y, "view", {
        { "On", "script-message-to gyroflow view normal" },
        { "Wider", "script-message-to gyroflow view wide" },
        { "Off", "script-message-to gyroflow view off" },
    }, views[g.view])
    y = u:caption(x, w, y, g.view == "wide" and "No zoom: the edges show at times" or g.view == "off" and "As recorded, for this clip" or "Zoomed in so no edge shows")

    local project = g.kind == "project"
    y = u:heading(x, w, y + 8 * s, "Level", keys["script-binding gyroflow/level"])
    local levels = { weak = 1, medium = 2, strong = 3 }
    y = u:segmented(x, w, y, "level", {
        { "Weak", "script-message-to gyroflow level weak" },
        { "Medium", "script-message-to gyroflow level medium" },
        { "Strong", "script-message-to gyroflow level strong" },
    }, levels[g.level], project)
    y = u:caption(x, w, y, project and "This clip has its project's smoothing; the level is for clips without one" or LEVEL_NOTES[g.level] or "")

    y = u:heading(x, w, y + 8 * s, "Horizon")
    local horizons = { project = 1, yes = 2, no = 3 }
    y = u:segmented(x, w, y, "horizon", {
        { project and "As project" or "As preset", "script-message-to gyroflow horizon-lock project" },
        { "Level", "script-message-to gyroflow horizon-lock yes" },
        { "Free", "script-message-to gyroflow horizon-lock no" },
    }, horizons[g.horizon_lock])

    y = u:heading(x, w, y + 8 * s, "Lens", keys["script-binding gyroflow/lens"])
    -- A clip with a project starts with the project's lens; one without, with what sticks
    local lens_items = {
        { "Auto", "script-message-to gyroflow underwater auto", not g.water },
        { "In water", "script-message-to gyroflow underwater yes" },
        { "Above", "script-message-to gyroflow underwater no" },
    }
    local lenses = { auto = 1, yes = 2, no = 3 }
    if project then
        table.insert(lens_items, 1, { "As project", "script-message-to gyroflow underwater project" })
        lenses = { project = 1, auto = 2, yes = 3, no = 4 }
    end
    y = u:segmented(x, w, y, "lens", lens_items, lenses[g.underwater])
    y = u:caption(x, w, y, g.underwater == "project" and "As the project says"
                           or g.water and "Auto: as the camera says it was in the water"
                           or ("Auto: this camera doesn't say, so as the " .. (project and "project" or "preset")))
    y = u:switch(x, w, y, "fringes", "Reduce color aberration", g.chromatic == true, "script-message-to gyroflow chromatic toggle",
                 keys["script-binding gyroflow/fringes"], not g.chromatic_available or g.underwater == "no")

    return y
end

-- What the right-click menu's Quality has, for a GPU that doesn't keep up (kept in gyroflow.conf as it's picked, a
-- setting of this computer's), and what fills the edges
local function quality_part(u, x, w, y)
    local s = u.s
    local g = state("gyroflow")
    if not g or not g.available or not local_file(mp.get_property("path")) then
        return u:caption(x, w, y, "Only for a clip that's stabilized")
    end
    y = u:caption(x, w, y, "Lighter settings for a GPU that doesn't keep up. Frame rate and size are kept for this computer as you pick them")
    y = u:heading(x, w, y + 6 * s, "Frame rate")
    local rates = { ["0"] = 1, ["30"] = 2, ["15"] = 3 }
    y = u:segmented(x, w, y, "fps", {
        { "Every frame", "script-message-to gyroflow quality fps_limit 0" },
        { "30 fps", "script-message-to gyroflow quality fps_limit 30" },
        { "15 fps", "script-message-to gyroflow quality fps_limit 15" },
    }, rates[tostring(g.fps_limit or 0)])
    y = u:caption(x, w, y, "How often the stabilized frame is rendered")
    y = u:heading(x, w, y + 8 * s, "Size")
    local sizes = { video = 1, display = 2, ["1080"] = 3, ["720"] = 4 }
    y = u:segmented(x, w, y, "size", {
        { "Video", "script-message-to gyroflow quality resolution video" },
        { "Display", "script-message-to gyroflow quality resolution display" },
        { "1080p", "script-message-to gyroflow quality resolution 1080" },
        { "720p", "script-message-to gyroflow quality resolution 720" },
    }, sizes[tostring(g.resolution)])
    y = u:caption(x, w, y, "The size it's rendered at: the video's own, fitting the display, or at most 1080p or 720p")
    y = u:heading(x, w, y + 8 * s, "Paused frame", keys["script-binding gyroflow/full"])
    local paused = { off = 1, auto = 2 }
    y = u:segmented(x, w, y, "paused", {
        { "As it played", "script-message-to gyroflow quality paused_full_size off" },
        { "Full size", "script-message-to gyroflow quality paused_full_size auto" },
    }, paused[tostring(g.paused_full_size or "off")])
    y = u:caption(x, w, y, "Full size: rendered again at the video's own size a moment after pausing (Ctrl+f does it once)")
    y = u:switch(x, w, y, "still", "Temporal denoise", g.still, "script-binding gyroflow/still", keys["script-binding gyroflow/still"])
    y = u:caption(x, w, y, "The paused frame rebuilt from the 5 frames on each side of it: its noise and the particles in the water go, the detail stays")
    y = u:heading(x, w, y + 8 * s, "Edges", keys["script-binding gyroflow/edges"])
    for _, e in ipairs({ { "sphere", "Trail by direction" }, { "smooth", "Frame's edge spread out" }, { "off", "Background color" } }) do
        y = u:radio(x, w, y, "edges:" .. e[1], e[2], g.edges == e[1], "script-message-to gyroflow edges " .. e[1])
    end
    -- Only the sphere trail is written apart from the frames
    local sphere = g.edges == "sphere"
    y = u:caption(x, w, y + 6 * s, "Edge fill rate")
    local fills = { auto = 1, ["50"] = 2, ["25"] = 3 }
    y = u:segmented(x, w, y, "fill", {
        { "Auto", "script-message-to gyroflow quality edge_fill_rate auto" },
        { "50 %", "script-message-to gyroflow quality edge_fill_rate 50" },
        { "25 %", "script-message-to gyroflow quality edge_fill_rate 25" },
    }, fills[tostring(g.edge_fill_rate or "auto")], not sphere)
    y = u:caption(x, w, y, sphere and "How many frames the edges' trail is written from; Auto: as the GPU has room"
                               or "Only for edges filled by the trail")
    y = u:heading(x, w, y + 8 * s, "More")
    y = u:switch(x, w, y, "fringes", "Reduce color aberration", g.chromatic == true, "script-message-to gyroflow chromatic toggle",
                 keys["script-binding gyroflow/fringes"], not g.chromatic_available or g.underwater == "no")
    y = u:switch(x, w, y, "ask", "Pause and ask when slow", g.ask_when_slow ~= false,
                 "script-message-to gyroflow quality ask_when_slow " .. (g.ask_when_slow == false and "yes" or "no"))
    return y
end

local function telemetry_part(u, x, w, y)
    local s = u.s
    local t = state("dji_telemetry")
    if not mp.get_property("path") then return u:caption(x, w, y, "Nothing plays") end
    if not t then return u:caption(x, w, y, "Needs the dji-telemetry plugin") end
    if not t.available then return u:caption(x, w, y, "No telemetry in this clip") end
    y = u:switch(x, w, y, "show", "Show over the video", t.visible, "script-binding dji_telemetry/toggle", keys["script-binding dji_telemetry/toggle"])
    y = u:heading(x, w, y + 8 * s, "Corner")
    for _, c in ipairs(CORNERS) do
        y = u:radio(x, w, y, "corner:" .. c[1], c[2], t.position == c[1], "script-message-to dji_telemetry position " .. c[1])
    end
    y = u:heading(x, w, y + 8 * s, "Units")
    y = u:segmented(x, w, y, "units", {
        { "Metric", "script-message-to dji_telemetry units metric" },
        { "Imperial", "script-message-to dji_telemetry units imperial" },
    }, t.units == "imperial" and 2 or 1)
    y = u:heading(x, w, y + 8 * s, "Fields")
    for _, f in ipairs(FIELDS) do
        local on = t.shown and t.shown[f[1]]
        y = u:switch(x, w, y, "field:" .. f[1], f[2], on, "script-message-to dji_telemetry field " .. f[1])
    end
    return y
end

local SPEEDS = { 0.5, 1, 1.5, 2 }

local function player_part(u, x, w, y)
    local s = u.s
    y = u:heading(x, w, y, "Speed")
    local speed, chosen = mp.get_property_number("speed", 1), nil
    local opts = {}
    for i, v in ipairs(SPEEDS) do
        if math.abs(speed - v) < 1e-3 then chosen = i end
        table.insert(opts, { string.format("%g×", v), "set speed " .. v })
    end
    y = u:segmented(x, w, y, "speed", opts, chosen)
    y = u:heading(x, w, y + 8 * s, "Window")
    y = u:switch(x, w, y, "loop", "Loop the clip", mp.get_property("loop-file") == "inf", "cycle-values loop-file inf no")
    y = u:switch(x, w, y, "fs", "Fullscreen", mp.get_property_native("fullscreen"), "cycle fullscreen", keys["cycle fullscreen"])
    y = u:switch(x, w, y, "ontop", "Always on top", mp.get_property_native("ontop"), "cycle ontop", keys["cycle ontop"])
    local audio = {}
    for _, tr in ipairs(mp.get_property_native("track-list") or {}) do
        if tr.type == "audio" then table.insert(audio, tr) end
    end
    if #audio > 1 then
        y = u:heading(x, w, y + 8 * s, "Sound")
        for _, tr in ipairs(audio) do
            y = u:radio(x, w, y, "aid:" .. tr.id, tr.title or tr.lang or ("Track " .. tr.id), tr.selected, "set aid " .. tr.id)
        end
    end
    return y
end

local function open_url(url)
    local args = platform == "darwin" and { "open", url } or platform == "windows" and { "rundll32", "url.dll,FileProtocolHandler", url }
                 or { "xdg-open", url }
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false }, function() end)
end

-- What the right-click menu had about the app: its version, help and links (footage-menu runs the commands; the app
-- gives it its version and setup script)
local function support_part(u, x, w, y)
    local s = u.s
    local opts = mp.get_property_native("script-opts") or {}
    local version, setup = opts["footage_menu-version"] or "", opts["footage_menu-setup"] or ""
    local path = mp.get_property("path")
    local menu = state("footage_menu")
    u:text(x, y + 9 * s, "ActionFootagePlayer", { size = 14, bold = true })
    y = y + 22 * s
    y = u:caption(x, w, y, version ~= "" and ("Version " .. version) or "The plugins, in your own mpv")
    y = u:caption(x, w, y, mp.get_property("mpv-version") or "mpv")
    y = u:heading(x, w, y + 10 * s, "Help")
    y = u:button(x, w, y, "keys", "Keys…", "script-message-to footage_menu keys", { disabled = not menu })
    y = u:button(x, w, y, "note", "Note for the agent…", "script-binding agent/report",
                 { disabled = not path or not state("agent"), key = keys["script-binding agent/report"] })
    y = u:button(x, w, y, "report", "Report this camera…", "script-message-to footage_menu report",
                 { disabled = not menu or not local_file(path) })
    y = u:caption(x, w, y, "Opens an issue on GitHub with what the plugins found in this clip")
    if setup ~= "" then
        y = u:button(x, w, y + 4 * s, "setup", "Set up again…", "script-message-to footage_menu setup", { disabled = not menu })
        y = u:caption(x, w, y, "Runs the app's first setup again")
    end
    y = u:heading(x, w, y + 10 * s, "Links")
    local half = (w - 8 * s) / 2
    u:button(x, half, y, "website", "Website", function() open_url(WEBSITE) end)
    return u:button(x + half + 8 * s, half, y, "github", "GitHub", function() open_url(REPO) end)
end

local PARTS = { project = project_part, clip = clip_part, look = look_part, dv = dv_part, stabilization = stabilization_part, quality = quality_part,
                telemetry = telemetry_part, player = player_part, support = support_part }
-- Each part's "these as the defaults"
local REMEMBER = { look = "script-message-to dlogm remember", stabilization = "script-message-to gyroflow remember",
                   quality = "script-message-to gyroflow remember",
                   telemetry = "script-message-to dji_telemetry remember" }

-- How wide it is, in pixels, at scale s
local function width(s)
    if not shown then return 0 end
    return (open and RAIL + PANEL or RAIL) * s
end

-- macOS shows the window's title bar over the video's top when the mouse moves there: the panel starts below it
local TITLE_BAR = 28

local function draw(w, h, s, hovered)
    local u = ui.new(s)
    u.hover = hovered
    if not shown then return u end
    local rx = w - RAIL * s
    local t0 = (platform == "darwin" and not mp.get_property_native("fullscreen") and mp.get_property_native("border") ~= false)
               and TITLE_BAR * s or 0

    if open then
        local px = w - (RAIL + PANEL) * s
        u:box(px, 0, PANEL * s, h, 0, C.panel)
        local x, cw = px + PAD * s, (PANEL - 2 * PAD) * s
        -- The header: the part's name, and the arrow that leaves the rail
        u:text(x, t0 + HEADER / 2 * s, TAB_TITLES[tab] or tab, { size = 15, bold = true })
        local ax = px + (PANEL - 22) * s
        if u:hovered("collapse") then u:box(ax - 12 * s, t0 + 10 * s, 24 * s, 24 * s, 6, C.field) end
        local cy = t0 + HEADER / 2 * s
        local function at(x, y) return string.format("%.1f %.1f", ax + x * s, cy + y * s) end
        u:shape("m " .. at(-3, -4.6) .. " l " .. at(-1.9, -5.7) .. " " .. at(3.4, 0) .. " " .. at(-1.9, 5.7) .. " " .. at(-3, 4.6)
                .. " " .. at(1.2, 0), u:hovered("collapse") and C.text or C.dim)
        u:hit(ax - 12 * s, t0 + 10 * s, 24 * s, 24 * s, "collapse", function() open = false end)
        u:divider(px, PANEL * s, t0 + HEADER * s)

        local footer = REMEMBER[tab] and mp.get_property("path") and FOOTER * s or 0
        local top, bottom = t0 + HEADER * s + 1, h - footer
        local before = #u.events
        u.clip = { px, top, px + PANEL * s, bottom }
        local y0 = top + 14 * s
        local off = scroll[tab] or 0
        local fn = PARTS[tab]
        local y_end = fn and fn(u, x, cw, y0 - off) or y0
        u.clip = nil
        -- Scrolled within what there is
        local content = y_end + off - y0 + 24 * s
        local room = bottom - top
        local max = math.max(0, content - room)
        if off > max then
            scroll[tab] = max
            for i = #u.events, before + 1, -1 do table.remove(u.events, i) end
            u.hits = {}
            return nil
        end
        if max > 0 then
            -- The scroll bar
            local bh = math.max(24 * s, room * room / content)
            local by = top + (room - bh) * off / max
            u:box(px + PANEL * s - 6 * s, by + 2 * s, 3 * s, bh - 4 * s, 2, C.line)
        end
        if footer > 0 then
            u:divider(px, PANEL * s, bottom)
            u:button(x, cw, bottom + 14 * s, "remember", "Make these the defaults", REMEMBER[tab])
        end
    end

    -- The rail
    u:box(rx, 0, RAIL * s, h, 0, C.rail)
    u:box(rx, 0, math.max(1, s), h, 0, C.divider)
    local function rail_button(i, name, label, cmd, active, y)
        local bx, by, bs = rx + 6 * s, y, 36 * s
        if active then u:box(bx, by, bs, bs, 8, C.accent)
        elseif u:hovered("rail:" .. name) then u:box(bx, by, bs, bs, 8, C.field) end
        icon(u, name, bx + bs / 2, by + bs / 2, active and C.knob or (u:hovered("rail:" .. name) and C.text or C.dim))
        u:hit(bx, by, bs, bs, "rail:" .. name, cmd)
    end
    for i, t in ipairs(shown_tabs()) do
        rail_button(i, t[1], t[2], function() choose(t[1]) end, open and tab == t[1], t0 + (8 + (i - 1) * 44) * s)
    end
    rail_button(0, "support", "Support", function() choose("support") end, open and tab == "support", h - (8 + 36) * s)
    return u
end

-- On screen

local dims = { w = 0, h = 0 }
local function scale()
    return mp.get_property_number("display-hidpi-scale", 1) or 1
end

local place_osc

local function render()
    local d = mp.get_property_native("osd-dimensions")
    if not d or d.w <= 0 then return end
    dims = d
    local s = scale()
    local u = draw(d.w, d.h, s, hover)
    if not u then u = draw(d.w, d.h, s, hover) end
    hits = u.hits
    overlay.res_x, overlay.res_y = d.w, d.h
    overlay.data = u:render()
    overlay:update()
    -- The video beside it, not under it
    mp.set_property_number("video-margin-ratio-right", d.w > 0 and width(s) / d.w or 0)
    place_osc()
end

local function hit_at(x, y)
    for i = #hits, 1, -1 do
        local h = hits[i]
        if x >= h.x0 and x < h.x1 and y >= h.y0 and y < h.y1 then return h end
    end
end

-- mpv's on-screen controller: while the panel shows, the box one at half its size, at the bottom, centered over the
-- video (the bottom bar would go under the panel). Its own settings come back as they were when it hides
local OSC_SCALE = 0.5
local OSC_KEYS = { "osc-layout", "osc-halign", "osc-valign", "osc-scalewindowed", "osc-scalefullscreen" }
local osc_before = nil
function place_osc()
    local w = width(scale())
    local opts = mp.get_property_native("script-opts") or {}
    if w > 0 and dims.w > 0 then
        -- osc.lua's box: 550 x 138 at a height of 720 / its scale, placed by get_align with no margin
        local playresy = 720 / OSC_SCALE
        local playresx = playresy * dims.w / dims.h
        local room = playresx / 2 - 275
        local want = {
            ["osc-layout"] = "box",
            ["osc-halign"] = string.format("%.3f", room > 0 and math.max(-1, -(w / dims.w) * playresx / 2 / room) or -1),
            -- 12 px above the window's bottom
            ["osc-valign"] = string.format("%.3f", math.min(1, (playresy / 2 - 69 - 12 * playresy / dims.h) / (playresy / 2 - 69))),
            ["osc-scalewindowed"] = tostring(OSC_SCALE), ["osc-scalefullscreen"] = tostring(OSC_SCALE),
        }
        -- Each change sets the controller up again. Compared with what's there: script-opts may be set anew by others
        local same = true
        for _, k in ipairs(OSC_KEYS) do same = same and opts[k] == want[k] end
        if same then return end
        if not osc_before then
            osc_before = {}
            for _, k in ipairs(OSC_KEYS) do osc_before[k] = opts[k] or false end
        end
        for k, v in pairs(want) do opts[k] = v end
        mp.set_property_native("script-opts", opts)
    elseif osc_before then
        for _, k in ipairs(OSC_KEYS) do opts[k] = osc_before[k] or nil end
        osc_before = nil
        mp.set_property_native("script-opts", opts)
    end
end

local update = render

-- The mouse: clicks and the wheel are the panel's while it's over it, the player's elsewhere
local captured = false

local function wheel(dir)
    return function(info)
        if info.event == "up" then return end
        local s = scale()
        scroll[tab] = math.max(0, (scroll[tab] or 0) + dir * 48 * s * (info.scale or 1))
        render()
    end
end

-- A slider held: its drag(x) follows the mouse until the button is let go
local dragging = nil

local function click(info)
    if info.event == "up" then dragging = nil; return end
    if info.event ~= "down" and info.event ~= "press" then return end
    local pos = mp.get_property_native("mouse-pos")
    local h = pos and hit_at(pos.x, pos.y)
    if not h then return end
    if type(h.cmd) == "table" and h.cmd.drag then
        h.cmd.drag(pos.x)
        if info.event == "down" then dragging = h.cmd.drag end
    elseif type(h.cmd) == "function" then h.cmd() else mp.command(h.cmd) end
    -- What the plugins change shows on their next publish; render now for what's the panel's own
    update()
end

-- mpv moves the window when it's dragged by the video: not while the mouse is over the panel (a slider would drag
-- the window instead)
local dragging_before = nil

local function capture(on)
    if on == captured then return end
    captured = on
    if on then
        dragging_before = mp.get_property_native("window-dragging")
        if dragging_before ~= nil then mp.set_property_native("window-dragging", false) end
        mp.add_forced_key_binding("MBTN_LEFT", "click", click, { complex = true })
        mp.add_forced_key_binding("MBTN_LEFT_DBL", "click-dbl", function() end)
        mp.add_forced_key_binding("WHEEL_UP", "wheel-up", wheel(-1), { complex = true })
        mp.add_forced_key_binding("WHEEL_DOWN", "wheel-down", wheel(1), { complex = true })
    else
        for _, name in ipairs({ "click", "click-dbl", "wheel-up", "wheel-down" }) do mp.remove_key_binding(name) end
        if dragging_before ~= nil then mp.set_property_native("window-dragging", dragging_before) end
        dragging_before = nil
    end
end

mp.observe_property("mouse-pos", "native", function(_, pos)
    if not pos then return end
    if dragging then return dragging(pos.x) end
    local inside = pos.hover and shown and pos.x >= dims.w - width(scale())
    capture(inside or false)
    local h = inside and hit_at(pos.x, pos.y) or nil
    local id = h and h.id or nil
    if id ~= hover then
        hover = id
        render()
    end
end)

mp.add_key_binding("TAB", "toggle", function()
    shown = not shown
    if shown then open = true end
    capture(false)
    update()
end)

mp.register_script_message("tab", function(name)
    if PARTS[name] then tab, open, shown = name, true, true; update() end
end)

-- What it draws, as an .ass file, at a window size given
mp.register_script_message("dump", function(path, w, h, s)
    read_keys()
    w, h, s = tonumber(w), tonumber(h), tonumber(s) or 1
    local u = draw(w, h, s, nil) or draw(w, h, s, nil)
    local f = io.open(path, "w")
    if not f then return mp.msg.error("can't write " .. path) end
    f:write("[Script Info]\nScriptType: v4.00+\nPlayResX: " .. w .. "\nPlayResY: " .. h .. "\nScaledBorderAndShadow: yes\n\n")
    f:write("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n")
    f:write("Style: Default," .. ui.FONT .. ",20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n\n")
    f:write("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n")
    for line in u:render():gmatch("[^\n]+") do
        f:write("Dialogue: 0,0:00:00.00,9:00:00.00,Default,,0,0,0,," .. line .. "\n")
    end
    f:close()
    mp.msg.info("wrote " .. path)
end)

-- What it shows comes from the plugins' state and the player's
for _, prop in ipairs({ "user-data/dlogm", "user-data/gyroflow", "user-data/dji_telemetry", "user-data/highlights", "user-data/workspace",
                        "user-data/footage_menu", "user-data/agent", "osd-dimensions",
                        "display-hidpi-scale", "border", "path", "pause", "fullscreen", "ontop", "loop-file", "speed",
                        "ab-loop-a", "ab-loop-b", "playlist-pos", "playlist-count" }) do
    mp.observe_property(prop, "native", function() update() end)
end
mp.register_event("file-loaded", function()
    read_keys()
    scroll = {}
    update()
end)
-- The highlight that plays now
mp.add_periodic_timer(1, function() if shown and open and (tab == "clip" or tab == "project") then render() end end)
read_keys()
-- For footage-menu: right click is mpv's own menu while the panel is there
mp.set_property_native("user-data/side_panel/loaded", true)
