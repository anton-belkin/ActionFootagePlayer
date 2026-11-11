-- side-panel: the plugins' settings in a panel at the right of the window, beside the video (it's moved over for it,
-- not covered): a rail of icons at the edge, one for each part (the project: the trip's marks, moments and clip
-- previews, the workspace plugin's; the clip, its look, the stabilization, performance and quality, the telemetry,
-- the Dolby Vision trims, the 360 view of a 360° clip; support at the bottom), and the panel of the one chosen, scrolled
-- with the wheel. The plugins' settings as controls: it reads the state they keep in user-data/<plugin>/ and runs
-- their commands. While it's loaded, footage-menu leaves the right-click menu to mpv's own (it publishes
-- user-data/side_panel/loaded). An mpv plugin: this folder goes into ~~/scripts/.
--
-- Keys (rebind with `<key> script-binding side_panel/<name>` in input.conf):
--   TAB    toggle   the panel and its rail shown or hidden
-- Clicking the chosen icon again, or the arrow in the panel's header, leaves only the rail.
--
-- The navigation bar at the bottom of the video is this plugin's too (bar.lua, docs/navigation-bar.md), in place of mpv's
-- own on-screen controller (osc=no in the app; elsewhere it's hidden): over the video, shown by the mouse, while the
-- panel is closed; below the video, always there, while it's open.
--   F8     playlist   the Playlist part (mpv's own F8 shows the playlist as text)
--
-- Options (~~/script-opts/side_panel.conf):
--   shown=no         the rail shows when the player starts (off: the video alone, Tab shows the panel)
--   open=yes         and the panel beside it
--   tab=project      the part it opens on: project, clip, look, stabilization, quality, telemetry, dv, view360, support
--
-- Script messages (`script-message-to side_panel <message>`):
--   tab <name>                      open the panel on that part
--   popover [<id>]                  open a list over the panel (export: the Output tab's Export kinds), or close it
--   dump <path> <w> <h> [<scale>]   write what it draws at that window size as an .ass file (for checking its look
--                                   without a window)

local mp = require 'mp'
local options = require 'mp.options'
local ui = require 'ui'
local bar = require 'bar'
local tl = require 'timeline'
local exportkind = require 'exportkind'

local o = { shown = false, open = true, tab = "project" }
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
-- The aspects a frame can have (the Clip tab's output, Save frame's photo), in two rows
local ASPECT_ROWS = { { { "original", "Original" }, { "16:9" }, { "4:3" }, { "1:1" } }, { { "4:5" }, { "3:4" }, { "9:16" } } }
-- The output's lens (Gyroflow's output projection), the fisheyes' edges more and more compressed
local PROJECTIONS = { { "linear", "Linear", "straight lines" }, { "stereographic", "Fisheye 1", "stereographic" },
                      { "equidistant", "Fisheye 2", "equidistant" }, { "equisolid", "Fisheye 3", "equisolid" },
                      { "orthographic", "Fisheye 4", "orthographic" } }
local CORNERS = { { "top-left", "Top left" }, { "top-right", "Top right" }, { "bottom-left", "Bottom left" }, { "bottom-right", "Bottom right" } }
local FIELDS = { { "depth", "Depth" }, { "water", "In or out of water" }, { "pressure", "Pressure" }, { "altitude", "Altitude" }, { "height", "Height (drone)" },
                 { "gps", "GPS" }, { "speed", "Speed" }, { "temperature", "Sensor temperature" }, { "heartrate", "Heart rate" },
                 { "exposure", "Exposure" } }

-- The parts' names before (side_panel.conf's tab=, the other plugins' messages)
local RENAMED = { clip = "source", stabilization = "transform", look = "output" }
local shown, open, tab = o.shown, o.open, RENAMED[o.tab] or o.tab
local scroll = {}
local hover, hits = nil, {}
-- The list open over the panel (its id: "export", a drop-down's), or nil; set_popover opens or closes it (defined with
-- the mouse's bindings, below). The row the keys are on in it (a kind; nil: the one chosen), and the list as drawn
local popover, set_popover = nil, nil
local popover_focus, shown_list = nil, nil
-- The panel's scroll bar held
local scrolling_bar = false
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

local ICONS = { source = "clip", transform = "stabilization", output = "look" }
local function icon(u, name, cx, cy, color)
    name = ICONS[name] or name
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
    elseif name == "crop" then
        -- A camera: the body with its viewfinder bump, the lens
        u:shape(ui.rrect_path(cx - 9 * s, cy - 5 * s, 18 * s, 12.5 * s, 2.5 * s), color, stroke)
        u:shape("m " .. at(-4, -5) .. " l " .. at(-2.5, -8) .. " " .. at(2.5, -8) .. " " .. at(4, -5), color)
        u:shape(ui.circle_path(cx, cy + 1.2 * s, 3.6 * s), color, stroke)
        u:shape(ui.circle_path(cx + 6 * s, cy - 2.4 * s, 0.9 * s), color)
    elseif name == "view360" then
        -- A globe: its rim, a meridian and the equator
        u:shape(ui.circle_path(cx, cy, 8 * s), color, stroke)
        local k = 4 * 0.5523
        u:shape("m " .. at(0, -8) .. " b " .. at(k, -8) .. " " .. at(4, -k * 2) .. " " .. at(4, 0) .. " b " .. at(4, k * 2) .. " "
                .. at(k, 8) .. " " .. at(0, 8) .. " b " .. at(-k, 8) .. " " .. at(-4, k * 2) .. " " .. at(-4, 0) .. " b " .. at(-4, -k * 2)
                .. " " .. at(-k, -8) .. " " .. at(0, -8), color, { fill = false, border = 1.2 * s })
        u:box(cx - 8 * s, cy - 0.6 * s, 16 * s, 1.2 * s, 0, color)
    elseif name == "playlist" then
        -- A list: a play mark by its first line, two more lines below
        u:shape("m " .. at(-9, -7) .. " l " .. at(-3.5, -4) .. " " .. at(-9, -1), color)
        u:box(cx - 1 * s, cy - 4.75 * s, 10 * s, 1.5 * s, 0, color)
        u:box(cx - 9 * s, cy + 0.75 * s, 18 * s, 1.5 * s, 0, color)
        u:box(cx - 9 * s, cy + 5.25 * s, 18 * s, 1.5 * s, 0, color)
    elseif name == "support" then
        u:shape(ui.circle_path(cx, cy, 8 * s), color, stroke)
        u:text(cx, cy + 0.5 * s, "?", { align = 5, size = 12, bold = true, color = color })
    end
end

-- The parts: { name, title, hidden (not on the rail for now) }. Support is at the rail's bottom
-- Source, Transform, Output: how it's read (the source's input), how it's shown (its transform), where (this screen,
-- a file) (docs/moments-and-workspaces.md, "Source and transform")
local TABS = {
    { "project", "Project" }, { "playlist", "Playlist" }, { "source", "Source" }, { "transform", "Transform" }, { "output", "Output" }, { "crop", "Save frame" },
    { "quality", "Performance & Quality" }, { "telemetry", "Telemetry" }, { "player", "Player", true }, { "dv", "Dolby Vision" }, { "view360", "360° view" },
}
local TAB_TITLES = { support = "Support" }
for _, t in ipairs(TABS) do TAB_TITLES[t[1]] = t[2] end

-- The parts the rail has: Dolby Vision only where the dv look can be (the app's mpv on macOS), the 360 view only for a
-- 360° clip
local function shown_tabs()
    local list, d, v = {}, state("dlogm"), state("view360")
    for _, t in ipairs(TABS) do
        if not t[3] and (t[1] ~= "dv" or (d and d.dv))
           and (t[1] ~= "view360" or (type(v) == "table" and (v.available or v.note))) then
            table.insert(list, t)
        end
    end
    return list
end

local reveal_playlist
local function choose(name)
    if tab == name and open then open = false else tab, open = name, true end
    if name == "playlist" and open then reveal_playlist() end
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
    if g then table.insert(parts, g.kind == "project" and "Gyroflow project" or g.motion and "gyro" or "no motion data") end
    if t and t.available then table.insert(parts, "telemetry")
    elseif t and type(t.subtitles) == "table" and t.subtitles.camera then table.insert(parts, "camera data as subtitles") end
    if h and h.times and #h.times > 0 then table.insert(parts, #h.times == 1 and "1 mark" or (#h.times .. " marks")) end
    return table.concat(parts, " · ")
end

-- The index of value in list (of { value, ... }), nil when it isn't there
local function index_of(list, value)
    if value == nil then return nil end
    for i, it in ipairs(list) do
        if it[1] == value then return i end
    end
    return nil
end

-- A drop-down's rows from a list of { value, label, note }: each runs cmd .. " " .. value
local function choices(list, cmd)
    local rows = {}
    for _, it in ipairs(list) do
        rows[#rows + 1] = { kind = it[1], label = it[2] or tostring(it[1]), note = it[3], cmd = cmd .. " " .. tostring(it[1]) }
    end
    return rows
end

-- The source: what it is, and its input (docs/moments-and-workspaces.md, "Source and transform"): the log curve, the
-- turn it was recorded with, the lens under water. Saved into the source's own file at once, for all its moments and
-- clip previews; what a Gyroflow project has is the project's
local TURNS = { { 0, "As is" }, { 90, "90° ↻" }, { 180, "180°" }, { 270, "90° ↺" } }
-- An HDR file's log curves shown (Grade as log by hand…), until another file plays
local grade_open, grade_for = false, nil
local WATER = { { "auto", "Auto" }, { "yes", "Yes" }, { "no", "No" } }

local function source_part(u, x, w, y)
    local s = u.s
    local path = mp.get_property("path")
    if not path then return u:caption(x, w, y, "Nothing plays") end
    if grade_for ~= path then grade_open, grade_for = false, path end
    local d, g, t, h, ws = state("dlogm"), state("gyroflow"), state("dji_telemetry"), state("highlights"), state("workspace")
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
    if ws and ws.moment and not ws.moment.missing then
        y = u:button(x, w, y, "whole", "Open the whole clip at " .. clock(ws.moment["in"]), "script-message-to workspace open-original")
    end
    -- The input: for viewing as it's changed, saved into the source's own file with Save (for all its moments and clips)
    local own = local_file(path) and ws and ws.clip and not ws.export
    local saved_in = own and ws.input or nil
    local status = own and ws.input_status or nil
    local at = y + 6 * s
    local ix, iw = x + 10 * s, w - 20 * s
    y = at + 8 * s
    y = u:heading(ix, iw, y, "Input")
    if not own then
        y = u:caption(ix, iw, y - 4 * s, "Not saved: this file isn't a source")
    else
        y = u:caption(ix, iw, y - 4 * s, status == "changed" and "Changed for viewing. The dot marks what's saved"
            or status == "saved" and "Saved in the source, for all its moments and clips"
            or "Nothing saved: as the file says", status == "changed" and { color = C.text } or nil)
        local ihalf = (iw - 8 * s) / 2
        u:button(ix, ihalf, y, "input-save", "Save to the source", "script-message-to workspace input-save",
                 { primary = status == "changed", disabled = status ~= "changed" })
        y = u:button(ix + ihalf + 8 * s, ihalf, y, "input-revert", "Revert to saved", "script-message-to workspace input-revert",
                     { disabled = status ~= "changed" })
    end
    u:box(x, at, w, y - at + 4 * s, 8, C.card, { fill = false, border = 1 * s, border_color = C.line })
    y = y + 10 * s
    -- A saved value's dot, only while something differs from it
    local changed = status == "changed"
    local saved_input = changed and (saved_in or {}) or nil

    if d then
        y = u:heading(x, w, y + 4 * s, "Color")
        local hdr = (d.hdr or "") ~= ""
        local recorded = hdr and ("HDR, " .. d.hdr:upper() .. (d.prim == "bt.2020" and " · BT.2020" or d.prim ~= "" and (" · " .. d.prim) or ""))
            or (d.recorded or "") ~= "" and ("Log, " .. (CURVE_NAMES[d.recorded] or d.recorded))
            or "SDR, Rec.709"
        u:text(x, y + 9 * s, "Recorded", { size = 13 })
        u:text(x + w, y + 9 * s, recorded, { align = 6, size = 12, color = C.dim })
        y = y + 22 * s
        if hdr then
            u:text(x, y + 9 * s, "Dolby Vision", { size = 13 })
            u:text(x + w, y + 9 * s, (d.dv_profile or "") ~= "" and ("profile " .. d.dv_profile) or "none", { align = 6, size = 12, color = C.dim })
            y = y + 22 * s
        end
        local saved_curve = saved_input and (saved_input.curve or "auto")
        if hdr and not d.log and not grade_open then
            y = u:caption(x, w, y, "Tagged HDR, so not taken for log")
            y = u:button(x, w, y, "grade", "Grade as log by hand…", function() grade_open = true end)
        else
            local rows = { { kind = "auto", label = d.recorded ~= "" and ("From the clip: " .. (CURVE_NAMES[d.recorded] or d.recorded)) or "None, not log",
                             cmd = "script-message-to dlogm curve auto" } }
            for _, c in ipairs(CURVES) do
                if c[1] ~= d.recorded then rows[#rows + 1] = { kind = c[1], label = c[2], cmd = "script-message-to dlogm curve " .. c[1] } end
            end
            local chosen = d.curve == d.recorded and "auto" or d.curve
            y = u:select_row(x, w, y + 6 * s, "curve", "Log curve", rows, chosen, {
                key = keys["script-binding dlogm/curve"], saved = saved_curve,
                text = chosen == "auto" and (d.recorded ~= "" and (CURVE_NAMES[d.recorded] or d.recorded) or "None, not log") or nil })
        end
    end

    if g and local_file(path) then
        local project = g.kind == "project"
        if project then
            y = u:caption(x, w, y + 6 * s, "Turn and lens as defined in the Gyroflow project", { color = C.label })
        end
        local turn = project and 0 or (index_of(TURNS, g.rotation or 0) and g.rotation or 0)
        y = u:select_row(x, w, y + 8 * s, "turn", "Recorded turned", choices(TURNS, "script-message-to gyroflow rotate"), turn,
                         { disabled = project, saved = saved_input and (tonumber(saved_input.rotation) or 0) or nil })
        if (g.file_rotation or 0) ~= 0 then
            y = u:caption(x, w, y, "The file says it's turned " .. g.file_rotation .. "°: shown upright, and turned more by these")
        end

        y = u:heading(x, w, y + 8 * s, "Lens")
        local l = type(g.lens) == "table" and g.lens or {}
        local known = g.available == true
        local rows = { { "Camera", (l.camera or "") ~= "" and (l.camera:gsub("^%s+", "")) or (g.lens and "unknown" or "…") },
                       { "Profile", project and "From the project" or (not g.lens and "…") or l.lens == "in the file" and "From the file"
                           or (l.lens or "") ~= "" and (l.lens:gsub("_", " "))
                           -- No profile: the lens type it was recorded in (or given) when it's named, and the camera's own
                           -- number for the mode, for trying lenses on it
                           or l.type_label and ("Unknown (" .. l.type_label .. (l.mode and (", mode " .. l.mode) or "") .. ")") or l.mode and ("Unknown (mode " .. l.mode .. ")") or "Unknown" },
                       { "Motion data", project and "Gyroflow project" or g.motion and "Gyro, in-camera stabilization off"
                           or "none" .. ((l.camera or "") ~= "" and g.kind ~= "lens" and ": the camera stabilized it (EIS)" or "") } }
        for _, r in ipairs(rows) do
            u:text(x, y + 9 * s, r[1], { size = 13 })
            u:text(x + w, y + 9 * s, r[2], { align = 6, size = 12, color = r[2]:find("^Unknown") and C.saved or C.dim })
            y = y + 22 * s
        end
        if not known and g.lens then
            y = u:caption(x, w, y, "Without a lens profile the lens can't be corrected. Picking one by hand comes later")
        end
        local water = (not project and index_of(WATER, g.underwater)) and g.underwater or nil
        y = u:select_row(x, w, y + 6 * s, "water", "Under water", choices(WATER, "script-message-to gyroflow underwater"), water,
                         { key = keys["script-binding gyroflow/lens"], disabled = project or not known, text = project and "Project's" or nil,
                           saved = saved_input and (saved_input.underwater or "auto") or nil })
        y = u:caption(x, w, y, project and "As defined in the Gyroflow project" or not known and "Needs a known lens"
            or ("Auto uses the camera's telemetry" .. (g.water and "" or " (this camera doesn't record it: the preset's)")
                .. ". For a flat port (the camera's own glass, a flat housing); behind a dome port pick No"))
        y = u:button(x, w, y + 2 * s, "edit-lens", "Edit in Gyroflow…", "script-binding gyroflow/edit", { key = keys["script-binding gyroflow/edit"] })
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
    return y
end

-- In the folder, at the panel's bottom (Source, Output): conveniences
local DOCK = 150
local function dock(u, x, w, y)
    local s = u.s
    local half = (w - 8 * s) / 2
    y = u:heading(x, w, y, "In the folder")
    local n, count = mp.get_property_number("playlist-pos", 0), mp.get_property_number("playlist-count", 0)
    u:button(x, half, y, "prev", "Previous clip", "playlist-prev", { disabled = n <= 0 })
    y = u:button(x + half + 8 * s, half, y, "next", "Next clip", "playlist-next", { disabled = n >= count - 1 })
    local ws = state("workspace")
    y = u:switch(x, w, y, "autoplay", "Autoplay next", not (ws and ws.autoplay == false), "script-message-to workspace autoplay toggle")
    y = u:switch(x, w, y, "loop", "Loop the clip", mp.get_property("loop-file") == "inf", "cycle-values loop-file inf no")
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
        y = u:caption(ix, iw, y, "A project is a trip's folder. What the player makes goes into a Generated folder in its kind's folder (_Moments, _Clips, _Stills, _Exports), from where you move it "
            .. "anywhere in the project; and the file Project.afproject, which opens the trip in Action Footage Library. A clip's own files, its marks and Gyroflow project, "
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
                { (ws.moments_alone and "✓ " or "") .. "New moments stabilized on their own", "script-message-to workspace moments-alone" },
            })
        end
        local where = platform == "darwin" and "Finder" or "the folder"
        -- The trip's batches (clip previews, moments and photos of all marks, exports of all moments) are made in
        -- Action Footage Library, in the background, whatever plays here
        y = u:button(ix, iw, y + 2 * s, "library", "Open in Action Footage Library", "script-message-to workspace open-library", { primary = true })
        y = u:caption(ix, iw, y - 2 * s, "Clip previews" .. (st and st.new > 0 and (" (" .. st.new .. " new clips)") or "")
            .. ", moments and photos of all marks, exports of all moments: made there, in the background")
        if ws.clips then
            y = u:caption(ix, iw, y + 2 * s, "Making clip previews: " .. ws.clips.done .. " of " .. ws.clips.total, { color = C.text })
            y = u:button(ix, iw, y, "clips-stop", "Stop", "script-message-to workspace clips-stop")
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
        -- How it's stabilized: on its own or with its clip, written into it (its transform is on the Transform tab)
        if ws.snap then
            y = u:switch(x, w, y, "alone", "Stabilized on its own", ws.alone == true, "script-message-to workspace moment-alone toggle")
            y = u:caption(x, w, y - 2 * s, ws.own_project and "Its own Gyroflow project: changes to the clip's don't reach it"
                or "Its own transform, as made: the source's, changed later, doesn't reach it")
        else
            y = u:caption(x, w, y, "Plays as its source: its transform is the source's (Transform: Settings of its own)")
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
        -- A clip preview playing: its own transform (a variant of the clip's) or the source's, on the Transform tab
        local own = ws.link and ws.link.own
        y = u:caption(x, w, y, own and ("Its own transform" .. (ws.own_project and " and Gyroflow project (Ctrl+e opens it)" or "")
            .. ": a variant of the clip's. Copy the preview in Finder for another") or "The source's transform (Transform: Settings of its own)")
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
    local file_hdr = not d.log and (d.hdr or "") ~= ""
    local base = file_hdr and d.hdr == "pq" and "PQ" or "HLG"
    if look == "hdr" then return "HDR, " .. base .. (file_hdr and ((d.dv_profile or "") ~= "" and " (base layer)" or ", as recorded") or "") end
    if look == "dv" then return d.dv_own and "HDR, its own Dolby Vision" or ("HDR, " .. base .. "+DV") end
    if look == "dvsdr" then return d.dv_own and "SDR, its own Dolby Vision" or ("SDR, " .. base .. "+DV") end
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
        if d.hdr == "pq" and not d.log then return "PQ with Dolby Vision metadata (profile 8.1, HDR10 underneath), shown by macOS: in HDR on an HDR screen, "
            .. "toned down by the trims on an SDR one. " .. (d.dv_custom and "These are your own trims (Dolby Vision, in the rail)." or "The trims are the author's, and can be changed in Dolby Vision.") end
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

-- The Export button's kind chosen, per kind of clip ("360", "flat"): for this session
local export_choice = {}

-- Output: how it's shown on this screen (Display: the looks, this computer's), and a file of what plays (Export)
-- The clip's subtitles (the dji-telemetry plugin's v, docs: its main.lua): a switch, and what the track is when the
-- camera wrote it. Nothing when the clip has none (and heading: with its heading)
local function subtitles_block(u, x, w, y, heading)
    local s = u.s
    local t = state("dji_telemetry")
    local subs = type(t) == "table" and t.subtitles
    if type(subs) ~= "table" or (subs.count or 0) == 0 then return heading and y - 10 * s or y end
    local key = keys["script-binding dji_telemetry/subtitles"]
    if heading then y = u:heading(x, w, y, "Subtitles") end
    y = u:switch(x, w, y, "subtitles", subs.camera and subs.count == 1 and "Show the camera's text" or "Show subtitles",
                 subs.shown, "script-binding dji_telemetry/subtitles", key)
    if subs.camera then
        y = u:caption(x, w, y, "The camera wrote its data as subtitles: exposure, GPS, height. Off when a clip starts",
                      { color = C.label })
    end
    return y
end

local function output_part(u, x, w, y)
    local s = u.s
    local path = mp.get_property("path")
    if not path then return u:caption(x, w, y, "Nothing plays") end
    local d, g = state("dlogm"), state("gyroflow")
    if d then
        y = u:heading(x, w, y, "Display", keys["script-binding dlogm/sdr"])
        local from = d.log and ("Log, " .. (CURVE_NAMES[d.curve] or d.curve)) or (d.hdr or "") ~= ""
            and ("HDR, " .. d.hdr:upper() .. ((d.dv_profile or "") ~= "" and (" + Dolby Vision " .. d.dv_profile) or "")) or "SDR, Rec.709"
        y = u:caption(x, w, y - 4 * s, "From " .. from)
        if not d.log and (d.hdr or "") == "" then
            y = u:radio(x, w, y, "look:sdr", "SDR, as recorded", true, function() end)
            y = u:caption(x, w, y + 6 * s, "An SDR picture shows as it is. If it's log in fact, pick its curve in Source", { color = C.label })
        else
            if d.screen == "hdr" then y = u:caption(x, w, y, "HDR screen: SDR shows as an SDR screen would") end
            local rows = {}
            for _, look in ipairs(d.looks or {}) do
                rows[#rows + 1] = { kind = look, label = look_title(d, look), cmd = "script-message-to dlogm look " .. look,
                                    note = look == "hdr" and keys["script-binding dlogm/hdr"] or nil }
            end
            if d.log then
                rows[#rows + 1] = { kind = "off", label = "As recorded, log", cmd = "script-binding dlogm/off", note = keys["script-binding dlogm/off"] }
            end
            y = u:select(x, w, y, "look", rows, d.look)
            if d.log and d.maker == "" then
                y = u:caption(x, w, y + 2 * s, d.curve == "dlogm" and "DJI's LUT isn't downloaded" or ("No maker's LUT for " .. (CURVE_NAMES[d.curve] or d.curve) .. " yet"))
            end
            local note = look_note(d, d.look)
            if note then y = u:caption(x, w, y + 6 * s, note, { color = C.label }) end
        end
        if (d.look == "dv" or d.look == "dvsdr") and not d.dv_own and d.dv_trims then
            y = u:button(x, w, y + 4 * s, "to-dv", "Dolby Vision trims…", function() tab, open = "dv", true end)
        end
        -- The log curve is the source's input: shown here, changed on the Source tab (or Ctrl+l)
        local curve = d.curve ~= "" and (CURVE_NAMES[d.curve] or d.curve) or "none, not log"
        local from = d.curve ~= "" and d.curve == d.recorded and ", from the clip" or ""
        y = u:heading(x, w, y + 10 * s, "Log curve", keys["script-binding dlogm/curve"])
        y = u:caption(x, w, y - 4 * s, curve .. from .. ": the source's, saved with it", { color = C.label })
        y = u:button(x, w, y, "to-source", "Change in Source", function() tab, open = "source", true end)
    end

    y = subtitles_block(u, x, w, y + 10 * s, true)

    if g and local_file(path) then
        y = u:heading(x, w, y + 10 * s, "Export", keys["script-binding gyroflow/export"])
        y = u:caption(x, w, y - 2 * s, "A file of what plays: its source, its transform, in this look")
        local half = (w - 8 * s) / 2
        local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
        u:button(x, half, y, "mark-a", a and ("Start " .. clock(a)) or "Mark start", "set ab-loop-a ${=time-pos}")
        y = u:button(x + half + 8 * s, half, y, "mark-b", b and ("End " .. clock(b)) or "Mark end", "set ab-loop-b ${=time-pos}", { disabled = not a })
        if a or b then y = u:button(x, w, y, "clear", "Clear the part", "set ab-loop-a no; set ab-loop-b no") end
        if g.exporting then
            y = u:button(x, w, y, "export", "Cancel export", "script-message-to gyroflow cancel-export")
        else
            -- One button, its kind picked in its list (docs/360-reframe.md, "One Export button in the Output tab"),
            -- remembered for the session per kind of clip
            local v = state("view360")
            local rows = exportkind.rows(d, v)
            local clip_kind = exportkind.clip_kind(v)
            local kind = exportkind.chosen(rows, export_choice[clip_kind])
            for _, r in ipairs(rows) do
                local k = r.kind
                r.cmd = function() export_choice[clip_kind] = k; set_popover(nil) end
            end
            local busy = exportkind.busy(kind, v)
            -- Cut only when it's sure not to fit: the part's times are digits, which the estimate takes as wide
            local label, room = exportkind.label(kind, d, a, b, clock), w - (ui.SPLIT_ARROW + 12) * s
            if ui.text_width(label, 12, s, true) > room then label = fit(label, room, 12, s, true) end
            local top = y
            y = u:split_button(x, w, y, "export", label, "script-message-to gyroflow export " .. kind,
                               function() set_popover("export") end, { open = popover == "export", disabled = busy ~= nil })
            if popover == "export" then u:dropdown(x, x + w, top, top + 28 * s, "export-kind", rows, kind) end
            if busy then y = u:caption(x, w, y, busy) end
        end
        y = u:button(x, w, y, "last", "Show the last export", "script-message-to gyroflow reveal-last-export", { disabled = (g.last_export or "") == "" })
        y = u:caption(x, w, y, "A still or a Live Photo: Save frame")
    end
    return y
end

local LEVEL_NOTES = { weak = "10 % cropped, smoothness 50 %", medium = "20 % cropped, smoothness 75 %", strong = "30 % cropped, smoothness 100 %" }
local VIEWS = { { "normal", "On" }, { "wide", "Wider" }, { "off", "Off" } }
local LEVELS = { { "weak", "Weak" }, { "medium", "Medium" }, { "strong", "Strong" } }
local HORIZONS = { { "yes", "Level" }, { "no", "Free" } }
-- The frame's ratios, landscape-wise, drawn as their shape; and its orientation: as the source is, landscape, portrait
local RATIOS = { { "original", "Own", 1, 0.75 }, { "16:9", "16:9", 1, 0.5625 }, { "4:3", "4:3", 1, 0.75 }, { "5:4", "5:4", 1, 0.8 }, { "1:1", "1:1", 0.75, 0.75 } }
local ORIENTS = { { "as-is", "As is" }, { "landscape", "Landscape" }, { "portrait", "Portrait" } }
local WHERE = { session = "this session (nothing saved for it)", source = "the source", moment = "the moment", preview = "the clip preview" }
local TO = { source = "Save to the source", moment = "Save to the moment", preview = "Save to the clip preview" }

-- The Lens drop-down (the gyroflow plugin's `lens`): the lens corrected in a projection (over a Gyroflow project,
-- first the project's own), or as recorded. Returns its rows, the kind playing, and the kind saved (nil: none)
local function lens_rows(g, project, saved)
    local rows = {}
    if project then
        local info = type(g.project_info) == "table" and g.project_info or {}
        rows[1] = { kind = "project", label = "As in the project", note = (info.projection or "rectilinear"):lower(),
                    cmd = "script-message-to gyroflow lens project" }
    end
    for _, p in ipairs(PROJECTIONS) do
        rows[#rows + 1] = { kind = p[1], label = p[2], note = p[3], cmd = "script-message-to gyroflow lens " .. p[1] }
    end
    rows[#rows + 1] = { kind = "as-recorded", label = "As recorded", note = "not corrected", cmd = "script-message-to gyroflow lens as-recorded" }
    local function kind(lens, projection)
        if lens == "as-recorded" then return "as-recorded" end
        if projection == "project" or projection == nil then return project and "project" or "linear" end
        return projection
    end
    return rows, kind(g.lens_mode or "corrected", g.projection), saved and kind(saved.lens, saved.projection) or nil
end

local function frame_controls(u, x, w, y, g, saved, disabled)
    -- Drawn as it shows: turned when the frame is portrait (as is: when the source is)
    local sw, sh = tostring(g.source_aspect or ""):match("^(%d+):(%d+)$")
    local source_portrait = sw ~= nil and tonumber(sw) < tonumber(sh)
    local orient = g.orientation or "as-is"
    local portrait = orient == "portrait" or (orient == "as-is" and source_portrait)
    local rows = {}
    for _, r in ipairs(RATIOS) do
        local label = r[1] == "original" and "Its own" or r[2]
        if r[1] ~= "original" and r[1] ~= "1:1" and portrait then
            local a, b = r[1]:match("^(%d+):(%d+)$")
            label = b .. ":" .. a
        end
        rows[#rows + 1] = { kind = r[1], label = label, note = r[1] == "original" and (g.frame_aspect ~= "original" and g.frame_aspect or g.source_aspect or nil) or nil,
                            cmd = "script-message-to gyroflow aspect " .. r[1] }
    end
    local aspect = index_of(RATIOS, g.aspect or "original") and (g.aspect or "original") or nil
    y = u:select_row(x, w, y, "ratio", "Aspect", rows, aspect, { key = "a", disabled = disabled, saved = saved and saved.aspect or nil })
    y = u:select_row(x, w, y, "orient", "Orientation", choices(ORIENTS, "script-message-to gyroflow orientation"), orient,
                     { disabled = disabled or g.aspect == "1:1", saved = saved and saved.orientation or nil })
    if orient == "as-is" then
        y = u:caption(x, w, y - 2 * u.s, source_portrait and "As is: portrait, as the source is" or "As is: as the source is")
    end
    return y
end

-- The transform: where it's defined (a bundle saved for it, else the session's), Save, Revert, Use as default, Auto save,
-- a moment's or clip preview's own settings; then what it has, or what its Gyroflow project has
-- The filter makes the frame (stabilized, or the lens alone): else mpv's own crop and turn do
local function filtering_note(g) return (g.filter or "") ~= "" end

local function transform_part(u, x, w, y)
    local s = u.s
    local g, ws = state("gyroflow"), state("workspace")
    if not local_file(mp.get_property("path")) then return u:caption(x, w, y, "Nothing to transform") end
    if not g then return u:caption(x, w, y, "Needs the gyroflow plugin") end
    local edit = keys["script-binding gyroflow/edit"]
    local project = g.kind == "project"
    local saved = ws and type(ws.saved) == "table" and ws.saved or nil
    local status = ws and ws.view_status
    local target = ws and ws.view_target

    -- Where it's defined
    local at = y
    local ix, iw = x + 10 * s, w - 20 * s
    y = y + 8 * s
    y = u:heading(ix, iw, y, "Transform defined in")
    y = u:caption(ix, iw, y - 4 * s, WHERE[ws and ws.view_from or "session"] or "this session", { color = C.text })
    if ws and ws.link then
        y = u:switch(ix, iw, y, "view-own", "Settings of its own", ws.link.own == true, "script-message-to workspace view-own")
    end
    local note
    if ws and ws.fresh then note = "Saved as it opened (Auto save): changes are saved, and stick for the next ones"
    elseif status == "changed" then note = "Changed for viewing. The dot marks what's saved"
    elseif status == "saved" then note = "As saved"
    else note = target and "Nothing saved: changes stick for the next clips" or "Nothing to save into" end
    y = u:caption(ix, iw, y, note, status == "changed" and { color = C.text } or nil)
    local half = (iw - 8 * s) / 2
    u:button(ix, half, y, "view-save", target and TO[target] or "Save", "script-message-to workspace view-save",
             { primary = target ~= nil and status ~= "saved", disabled = not target or status == "saved" })
    y = u:button(ix + half + 8 * s, half, y, "view-revert", "Revert to saved", "script-message-to workspace view-revert",
                 { disabled = status ~= "changed" })
    y = u:button(ix, iw, y, "view-default", "Use as default", "script-message-to gyroflow remember")
    y = u:switch(ix, iw, y, "view-auto", "Auto save", ws and ws.auto_save == true, "script-message-to workspace view-auto", nil, not target)
    u:box(x, at, w, y - at + 4 * s, 8, C.card, { fill = false, border = 1 * s, border_color = C.line })
    y = y + 14 * s

    -- Stabilization: with motion data (a project, or the clip's own). Over a project too it's saved with the clip: on (the
    -- project as it is), wider (its zoom left out), off (the project kept, not used)
    local views = choices(VIEWS, "script-message-to gyroflow view")
    local stab_key = keys["script-binding gyroflow/toggle"]
    if not g.motion then
        y = u:select_row(x, w, y, "view", "Stabilization", views, "off", { disabled = true, key = stab_key })
        y = u:caption(x, w, y, "No motion data in this clip: Gyroflow can find it in the picture (optical flow)")
        if g.waiting then y = u:caption(x, w, y, "Waiting for the project from Gyroflow…")
        else y = u:button(x, w, y, "edit", "Stabilize in Gyroflow…", "script-binding gyroflow/edit", { key = edit }) end
    else
        y = u:select_row(x, w, y, "view", "Stabilization", views, g.view,
                         { key = stab_key, saved = saved and saved.view or (project and "normal" or nil) })
    end
    local off = g.view == "off"
    if g.motion and project then
        y = u:caption(x, w, y, g.view == "wide" and "The project without its zoom: the whole picture, the edges show at times"
            or off and "The project kept but not used: as recorded" or "The project as it is")
    elseif g.motion then
        y = u:caption(x, w, y, g.view == "wide" and "No zoom: the edges show at times" or off and "Not stabilized: the lens below still applies"
            or "Zoomed in so no edge shows")
        y = u:select_row(x, w, y + 2 * s, "level", "Level", choices(LEVELS, "script-message-to gyroflow level"), g.level,
                         { key = keys["script-binding gyroflow/level"], disabled = off, saved = saved and saved.level or nil })
        y = u:caption(x, w, y, LEVEL_NOTES[g.level] or "")
        y = u:select_row(x, w, y + 2 * s, "horizon", "Horizon", choices(HORIZONS, "script-message-to gyroflow horizon-lock"), g.horizon_lock,
                         { disabled = off, saved = saved and saved.horizon_lock or nil })
    end

    -- The lens, apart from the stabilization: corrected in a projection, or as recorded; it needs a lens profile. Over a
    -- Gyroflow project too, saved with the clip: the project's own projection, another, or the lens as recorded
    local known = g.available == true
    local rows, chosen, saved_lens = lens_rows(g, project, saved)
    y = u:select_row(x, w, y + 2 * s, "lens", "Lens", rows, known and chosen or "as-recorded",
                     { disabled = not known, saved = saved and saved_lens or nil, field = math.floor(w * 0.62) })
    local corrected = known and chosen ~= "as-recorded"
    if not known then
        y = u:caption(x, w, y, "The lens is unknown: Source › Lens")
    else
        local l = type(g.lens) == "table" and g.lens or {}
        local what = ((l.camera or "") ~= "" and (l.camera:gsub("^%s+", "")) or "The lens") .. (l.lens == "in the file" and " (from the file)" or "")
        local water = g.underwater == "yes" and ", under water" or g.underwater == "auto" and ", under water as the camera says" or ", above water"
        if project then
            what, water = chosen == "project" and "As the Gyroflow project has it" or "The project's lens correction, in this projection in place of its own", ""
        end
        y = u:caption(x, w, y, not corrected and "The lens as the camera recorded it"
            or (what .. water .. (g.filter == "only" and ": only the lens is corrected, nothing is smoothed" or "")))
    end

    if project then
        -- What the project has: values, changed only in Gyroflow
        local info = type(g.project_info) == "table" and g.project_info or {}
        local rows = { { "Smoothing", "its own" },
                       { "Horizon", (info.horizon or 0) > 0 and string.format("locked, %g %%", info.horizon) or "free" },
                       { "Frame", (info.width and info.height) and string.format("%d×%d", info.width, info.height) or "as the video" },
                       { "Projection", info.projection and info.projection:gsub("^%l", string.upper) or "Linear" } }
        local bat = y + 8 * s
        y = bat + 8 * s
        y = u:heading(ix, iw, y, "In the Gyroflow project")
        for _, r in ipairs(rows) do
            u:text(ix, y + 9 * s, r[1], { size = 13 })
            u:text(ix + iw, y + 9 * s, r[2], { align = 6, size = 12, color = C.dim })
            y = y + 22 * s
        end
        y = u:button(ix, iw, y + 4 * s, "edit", "Edit in Gyroflow…", "script-binding gyroflow/edit", { key = edit })
        u:box(x, bat, w, y - bat + 4 * s, 8, C.card, { fill = false, border = 1 * s, border_color = C.line })
        y = y + 6 * s
    else
        y = u:heading(x, w, y + 8 * s, "Frame")
        y = frame_controls(u, x, w, y, g, saved, false)
        if not filtering_note(g) then y = u:caption(x, w, y, "Frame and turn by mpv's own crop") end
        if g.motion then y = u:button(x, w, y + 4 * s, "edit", "Make a Gyroflow project…", "script-binding gyroflow/edit", { key = edit }) end
    end

    y = u:heading(x, w, y + 8 * s, "Water", keys["script-binding gyroflow/fringes"])
    local saved_fringes = nil
    if saved then saved_fringes = saved.chromatic == true end
    y = u:switch(x, w, y, "fringes", "Reduce color aberration", g.chromatic == true, "script-message-to gyroflow chromatic toggle",
                 nil, not g.chromatic_available or g.underwater == "no" or not filtering_note(g), saved_fringes)
    y = u:caption(x, w, y - 2 * s, g.chromatic_skip and "Skipped on this screen (Performance & Quality): exports and photos have it"
        or "The water's color fringes, where the lens is under water; exports and photos too")

    -- The temporal denoise: on or off (Ctrl+p, the clip's: kept with its transform, off where it harms, as on the
    -- water's surface); how much of it this computer does is Performance & Quality's, where Off leaves it out
    y = u:heading(x, w, y + 8 * s, "Temporal denoise", keys["script-binding gyroflow/still"])
    local none = g.temporal_denoise == "off"
    local saved_denoise = nil
    if saved and type(saved.denoise) == "boolean" then saved_denoise = saved.denoise end
    local denoise_on = g.still
    if type(g.denoise) == "boolean" then denoise_on = g.denoise end
    y = u:switch(x, w, y, "still", "Clean the noise", not none and denoise_on, "script-binding gyroflow/still", nil, none, saved_denoise)
    local gpu = type(g.gpu) == "table"
    y = u:caption(x, w, y - 2 * s, none and ("Off on this computer (Performance & Quality)" .. (denoise_on and "" or "; off for this clip too"))
        or gpu and "Each frame from the frames around it, moved onto it, while it plays and paused. The picture shows that many frames later; the sound follows. Exports too"
        or "The paused frame from the frames around it, moved onto it; exports each frame so")
    -- Stronger: the clip's (its transform), on the GPU path
    local saved_strong = nil
    if saved then saved_strong = saved.denoise_strong == true end
    y = u:switch(x, w, y, "strong", "Stronger, for a very noisy clip", g.denoise_strong == true, "script-message-to gyroflow denoise-strong toggle",
                 nil, none or not g.still or not gpu, saved_strong)
    y = u:caption(x, w, y - 2 * s, "No grain kept, more frames taken in, dark specks out and the flat areas smoothed: open water clean, fine texture softer")

    -- The hand's sideways jerks (the translation compensation, Ctrl+j): measured once for a clip shot up close, then
    -- on or off
    local tr = g.translation
    y = u:heading(x, w, y + 8 * s, "Sideways jerks", keys["script-binding gyroflow/translation"])
    if tr == "on" or tr == "off" then
        y = u:switch(x, w, y, "translation", "Take out the hand's sideways jerks", tr == "on", "script-binding gyroflow/translation")
        y = u:caption(x, w, y - 2 * s, "Measured for this clip: the picture moved against the hand's jitter, as far as the camera's accelerometer and the picture say")
    elseif tr == "measuring" then
        y = u:caption(x, w, y, string.format("Measuring them for this clip… %d %%", tonumber(g.translation_progress) or 0))
    elseif tr == "none" then
        y = u:button(x, w, y + 2 * s, "translation", "Measure for this clip", "script-binding gyroflow/translation")
        y = u:caption(x, w, y, "Up close, the hand's own jitter shakes the picture however well its turns are taken out. Measured once a clip (Osmo Action 6; about half the clip's length), kept")
    else
        y = u:caption(x, w, y, "On the GPU path, for a clip shot up close")
    end
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
    local function q(key) return "script-message-to gyroflow quality " .. key end
    y = u:select_row(x, w, y + 6 * s, "fps", "Frame rate", choices({ { "0", "Every frame" }, { "30", "30 fps" }, { "15", "15 fps" } }, q("fps_limit")),
                     tostring(g.fps_limit or 0))
    y = u:caption(x, w, y, "How often the stabilized frame is rendered")
    y = u:select_row(x, w, y + 4 * s, "size", "Size", choices({ { "auto", "Auto" }, { "display", "Screen" }, { "1080", "1080p" }, { "720", "720p" },
                                                                { "video", "Video" } }, q("resolution")), tostring(g.resolution))
    y = u:caption(x, w, y, "The size it's rendered at. Auto: the screen's, at most 1080p while the temporal denoise is on. Video: the video's own")
    y = u:select_row(x, w, y + 4 * s, "paused", "Paused frame", choices({ { "off", "As played" }, { "auto", "Full size" } }, q("paused_full_size")),
                     tostring(g.paused_full_size or "off"), { key = keys["script-binding gyroflow/full"] })
    y = u:caption(x, w, y, "Full size: rendered again at the video's own size a moment after pausing (Ctrl+f does it once)")
    y = u:select_row(x, w, y + 4 * s, "denoise", "Temporal denoise", choices({ { "off", "Off" }, { "light", "Light" }, { "full", "Full" } }, q("temporal_denoise")),
                     tostring(g.temporal_denoise or "full"), { field = math.floor(w * 0.42) })
    y = u:caption(x, w, y, "Full: 5 frames on each side. Light: 3, a quarter lighter on the GPU and a little less clean. Off: none on this computer, whatever Transform's switch says")
    y = u:select_row(x, w, y + 4 * s, "edges", "Edges", choices({ { "sphere", "Trail by direction" }, { "smooth", "Frame's edge spread out" },
                                                                  { "off", "Background color" } }, "script-message-to gyroflow edges"), g.edges,
                     { key = keys["script-binding gyroflow/edges"], field = math.floor(w * 0.62) })
    -- Only the sphere trail is written apart from the frames
    local sphere = g.edges == "sphere"
    y = u:select_row(x, w, y + 2 * s, "fill", "Edge fill rate", choices({ { "auto", "Auto" }, { "50", "50 %" }, { "25", "25 %" } }, q("edge_fill_rate")),
                     tostring(g.edge_fill_rate or "auto"), { disabled = not sphere })
    y = u:caption(x, w, y, sphere and "How many frames the edges' trail is written from; Auto: as the GPU has room"
                               or "Only for edges filled by the trail")
    y = u:heading(x, w, y + 8 * s, "More")
    -- On this screen only, for smooth playback: the clip's transform keeps the fringes taken out (exports, photos)
    y = u:switch(x, w, y, "fringes-skip", "Skip color aberration correction", g.chromatic_skip == true,
                 "script-message-to gyroflow quality chromatic_skip " .. (g.chromatic_skip and "no" or "yes"))
    y = u:switch(x, w, y, "ask", "Pause and ask when slow", g.ask_when_slow ~= false,
                 "script-message-to gyroflow quality ask_when_slow " .. (g.ask_when_slow == false and "yes" or "no"))
    return y
end

local function telemetry_part(u, x, w, y)
    local s = u.s
    local t = state("dji_telemetry")
    if not mp.get_property("path") then return u:caption(x, w, y, "Nothing plays") end
    if not t then return u:caption(x, w, y, "Needs the dji-telemetry plugin") end
    if not t.available then
        if type(t.subtitles) == "table" and t.subtitles.camera then
            y = u:caption(x, w, y, "This clip's camera data is in a subtitle track, as text: it shows in place of subtitles")
            return subtitles_block(u, x, w, y + 4 * s, false)
        end
        return u:caption(x, w, y, "No telemetry in this clip")
    end
    y = u:switch(x, w, y, "show", "Show over the video", t.visible, "script-binding dji_telemetry/toggle", keys["script-binding dji_telemetry/toggle"])
    y = u:select_row(x, w, y + 4 * s, "corner", "Corner", choices(CORNERS, "script-message-to dji_telemetry position"), t.position)
    y = u:select_row(x, w, y, "units", "Units", choices({ { "metric", "Metric" }, { "imperial", "Imperial" } }, "script-message-to dji_telemetry units"),
                     t.units == "imperial" and "imperial" or "metric")
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
    local speed, chosen = mp.get_property_number("speed", 1), nil
    local opts = {}
    for _, v in ipairs(SPEEDS) do
        if math.abs(speed - v) < 1e-3 then chosen = v end
        table.insert(opts, { kind = v, label = string.format("%g×", v), cmd = "set speed " .. v })
    end
    y = u:select_row(x, w, y, "speed", "Speed", opts, chosen, { text = not chosen and string.format("%g×", speed) or nil })
    y = u:heading(x, w, y + 8 * s, "Window")
    y = u:switch(x, w, y, "loop", "Loop the clip", mp.get_property("loop-file") == "inf", "cycle-values loop-file inf no")
    y = u:switch(x, w, y, "fs", "Fullscreen", mp.get_property_native("fullscreen"), "cycle fullscreen", keys["cycle fullscreen"])
    y = u:switch(x, w, y, "ontop", "Always on top", mp.get_property_native("ontop"), "cycle ontop", keys["cycle ontop"])
    local audio = {}
    for _, tr in ipairs(mp.get_property_native("track-list") or {}) do
        if tr.type == "audio" then table.insert(audio, tr) end
    end
    if #audio > 1 then
        local rows, aid = {}, nil
        for _, tr in ipairs(audio) do
            rows[#rows + 1] = { kind = tr.id, label = tr.title or tr.lang or ("Track " .. tr.id), cmd = "set aid " .. tr.id }
            if tr.selected then aid = tr.id end
        end
        y = u:select_row(x, w, y + 8 * s, "aid", "Sound", rows, aid)
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

-- Save frame (the gyroflow plugin's crop.lua, its crop mode): a frame over the video for a photo of the frame on
-- screen, the picture moved under it, or the camera turned where the clip is stabilized. For this frame only: the
-- whole video's crop is Gyroflow's
local function crop_part(u, x, w, y)
    local s = u.s
    local g = state("gyroflow")
    if not local_file(mp.get_property("path")) then return u:caption(x, w, y, "Open a clip to save a frame of it as a photo") end
    if not g then return u:caption(x, w, y, "Needs the gyroflow plugin") end
    local c = g.crop or {}
    -- The whole video's crop, rotation and zoom are the stabilization's, in Gyroflow
    local function whole_video(y)
        y = u:caption(x, w, y + 10 * s, "This frames the photo only. To crop the whole video, set its crop and zoom in Gyroflow")
        return u:button(x, w, y + 4 * s, "crop-gyroflow", g.available and "Open in Gyroflow…" or "Stabilize in Gyroflow…",
                        "script-binding gyroflow/edit", { key = keys["script-binding gyroflow/edit"] })
    end
    y = u:switch(x, w, y, "crop", "Frame the photo", c.active == true, "script-message-to gyroflow crop toggle", keys["script-binding gyroflow/crop"])
    if not c.active then
        y = u:caption(x, w, y, "A frame over the video, for a photo of the frame on screen. Where the clip is stabilized, "
                               .. "moving the picture turns the camera, and zoom is its angle of view")
        return whole_video(y)
    end
    local function set(key, v) mp.commandv("script-message-to", "gyroflow", "crop-set", key, string.format("%.5f", v)) end
    local function value_row(label, value)
        u:text(x, y + 8 * s, label, { size = 12, color = C.label })
        u:text(x + w, y + 8 * s, value, { size = 12, color = C.dim, align = 6 })
    end

    local aspects = {}
    for _, row in ipairs(ASPECT_ROWS) do
        for _, a in ipairs(row) do aspects[#aspects + 1] = a end
    end
    y = u:select_row(x, w, y + 10 * s, "crop-aspect", "Aspect", choices(aspects, "script-message-to gyroflow crop-aspect"), c.aspect or "original",
                     { key = "a" })

    local cam = c.camera
    if cam then
        local zmin, zmax, margin = c.zoom_min or 0.15, c.zoom_max or 4, c.margin or 0.8
        y = u:heading(x, w, y + 10 * s, "Camera")
        value_row("Zoom", string.format("%.0f %%", 100 / (cam.zoom * margin)))
        y = u:slider(x, w, y + 14 * s, "crop:zoom", math.log(zmax / cam.zoom) / math.log(zmax / zmin),
                     function(p) set("zoom", zmax * (zmin / zmax) ^ p) end, "Wider", "Closer")
        y = y + 6 * s
        value_row("Horizon", string.format("%.1f°", cam.roll))
        y = u:slider(x, w, y + 14 * s, "crop:roll", math.max(0, math.min(1, (cam.roll + 45) / 90)),
                     function(p) set("roll", math.floor((p * 90 - 45) * 10 + 0.5) / 10) end, "−45°", "45°")
        local function turn(v, plus, minus)
            if math.abs(v) < 0.05 then return nil end
            return string.format("%.1f° %s", math.abs(v), v > 0 and plus or minus)
        end
        local parts = {}
        for _, t in ipairs({ turn(cam.yaw, "right", "left"), turn(cam.pitch, "down", "up") }) do table.insert(parts, t) end
        y = u:caption(x, w, y + 4 * s, #parts > 0 and ("Turned " .. table.concat(parts, ", ")) or "Looking where the stabilization does")
        y = u:button(x, w, y + 4 * s, "crop-reset", "Back to the start", "script-message-to gyroflow crop-reset", { key = "BS" })
        y = u:caption(x, w, y + 4 * s, "Drag, or two fingers on a trackpad: turn the camera. Wheel or pinch: zoom. "
                                       .. "Alt+drag or turning two fingers: the horizon")
    else
        local f = c.flat or { zoom = 0 }
        y = u:heading(x, w, y + 10 * s, "Picture")
        value_row("Zoom", string.format("%.0f %%", 100 * 2 ^ (f.zoom or 0)))
        y = u:slider(x, w, y + 14 * s, "crop:flat-zoom", ((f.zoom or 0) + 1) / 4, function(p) set("zoom", p * 4 - 1) end, "Smaller", "Closer")
        y = u:button(x, w, y + 10 * s, "crop-reset", "Back to the start", "script-message-to gyroflow crop-reset", { key = "BS" })
        y = u:caption(x, w, y + 4 * s, "Not stabilized: the picture itself moves and zooms. Drag to move it, wheel or pinch to zoom")
    end
    -- The photo alone, or with the video around it as Apple's devices (macOS only) or Android's take it
    local mode = g.live_photo or "no"
    local modes = { { "no", "Still" }, { "live", "Live Photo" }, { "motion", "Motion Photo" } }
    if platform ~= "darwin" then table.remove(modes, 2) end
    y = u:select_row(x, w, y + 10 * s, "crop-live", "Photo", choices(modes, "script-message-to gyroflow live-photo"), mode)
    y = u:button(x, w, y + 6 * s, "crop-photo", "Save the frame as a photo", "script-message-to gyroflow photo", { primary = true, key = "Enter" })
    local what = mode == "live" and "A HEIC and a .mov of the 3 s around it, one Live Photo for Apple's devices,"
              or mode == "motion" and "An Ultra HDR JPEG with the 3 s around it inside, a Motion Photo for Android's,"
              or (platform == "darwin" and "A HEIC" or "An Ultra HDR JPEG")
    -- Where gyroflow-export puts it: the project's _Stills/Generated (the gyroflow plugin says), else
    -- stabilized/ next to the clip
    local ws = state("workspace")
    local where = type(ws) == "table" and ws.project and "the project's _Stills/Generated" or "the stabilized folder by the clip"
    y = u:caption(x, w, y, what .. " in " .. where .. ": the SDR look, brighter where the screen has HDR")
    y = u:button(x, w, y + 6 * s, "crop-leave", "Done framing", "script-message-to gyroflow crop no", { key = "Esc" })
    return whole_video(y)
end

-- The 360 view (the view360 plugin): a 360° clip as a view to look around in, the head turned by the mouse and the
-- trackpad; the projection, the FOV, the horizon, what the view follows
local PROJECTION_ROWS = { { { "auto", "Auto" }, { "rectilinear", "Rectilinear" } }, { { "stereographic", "Stereographic" }, { "planet", "Little planet" } } }
-- What the view follows: along the way you move (GPS), the camera, north (the compass, or GPS), the world
local HEADING_OPTIONS = { { "travel", "Way" }, { "camera", "Camera" }, { "north", "North" }, { "world", "World" } }
local FOLLOWS_NAMES = { travel = "the way", camera = "the camera", north = "north", world = "the world" }
-- The smoothing's strengths (reframe.rs: a Gaussian of 0.25, 0.6, 1.5 s)
local TIDY_TURNS = { { "light", "Light" }, { "medium", "Medium" }, { "strong", "Strong" } }
local FLAT_FRAMES = { { "16:9", 1, 9 / 16 }, { "9:16", 9 / 16, 1 }, { "1:1", 0.8, 0.8 } }
local FLAT_SIZES = { "1080p", "4K" }
-- Clear the recording asks first: a second click within this many seconds clears
local CLEAR_SECONDS = 4
local clear_asked = nil
-- The panel drawn again (update, set once it's defined below)
local redraw_flat = nil

-- The 360° tab's Flat video (docs/360-reframe.md, step 1): Record / Stop, what's recorded, playing it, the frame and
-- the size, the exports, undo and clear. v: user-data/view360; r: its recording
local function flat_video_part(u, x, w, y, v, r)
    local s = u.s
    local function msg(...) return "script-message-to view360 " .. table.concat({ ... }, " ") end
    local alt_shift = (keys["script-binding view360/play-recording"] or ""):gsub("%+(%u)$", function(c) return "+Shift+" .. c:lower() end)
    y = u:heading(x, w, y + 10 * s, "Flat video", keys["script-binding view360/record"])
    if r.taking then
        y = u:button(x, w, y, "view360-record", "■  Stop · " .. clock(r.seconds or 0), msg("view360-record", "stop"), { primary = true })
    else
        y = u:button(x, w, y, "view360-record", "●  Record", msg("view360-record", "start"), { primary = true, disabled = not r.can })
    end
    if not r.can then return u:caption(x, w, y, "Can't record here: " .. tostring(r.why or "this clip can't keep a recording")) end
    if (r.parts or 0) == 0 then
        y = u:caption(x, w, y, "Look around as it plays: what you see becomes a flat video. Pausing, seeking or the clip's end "
                               .. "stop a take")
    else
        local spans = {}
        for _, sp in ipairs(r.spans or {}) do table.insert(spans, clock(sp[1]) .. "–" .. clock(sp[2])) end
        local listed = #spans > 3 and (spans[1] .. ", " .. spans[2] .. " … " .. spans[#spans]) or table.concat(spans, " and ")
        y = u:caption(x, w, y, string.format("Recorded %s · %d part%s", listed, #spans, #spans == 1 and "" or "s"), { color = C.label })
        y = u:switch(x, w, y, "view360-play-recording", "Play the recording", r.playing == true, msg("view360-play-recording", "toggle"),
                     alt_shift ~= "" and alt_shift or nil, r.taking)
        y = u:caption(x, w, y, "Relative to what the view follows: " .. (FOLLOWS_NAMES[r.follows] or tostring(r.follows))
                               .. (r.playing and "" or ". A drag, a scroll or a pinch gives you the view"))

        -- How it's tidied: for the preview and the export alike, relative to what the view follows
        local tidy = type(r.tidy) == "table" and r.tidy or {}
        local smooth = tidy.turns ~= nil and tidy.turns ~= "off"
        y = u:switch(x, w, y, "view360-tidy-turns", "Smooth the turns", smooth, msg("view360-tidy", "turns", "toggle"), nil, r.taking)
        if smooth then
            y = u:select_row(x, w, y, "view360-tidy-strength", "How much", choices(TIDY_TURNS, msg("view360-tidy", "turns")), tidy.turns,
                             { disabled = r.taking })
        end
        y = u:switch(x, w, y, "view360-tidy-level", "Level the horizon", tidy.level == true, msg("view360-tidy", "level", "toggle"), nil, r.taking)
        y = u:switch(x, w, y, "view360-tidy-picture", "Hold on what's in the middle", tidy.picture == true, msg("view360-tidy", "picture", "toggle"), nil, r.taking)
        if r.measuring then
            y = u:caption(x, w, y, string.format("Measuring… %d %%. The smoothed turns play meanwhile", r.measuring))
        elseif r.tidying then
            y = u:caption(x, w, y, "Tidying the recording…")
        elseif tidy.picture then
            y = u:caption(x, w, y, "By the picture: the view follows what's in its middle, up to 8°")
        end
    end

    -- The frame and the size of the flat video
    local frames = {}
    for _, f in ipairs(FLAT_FRAMES) do frames[#frames + 1] = { f[1] } end
    y = u:select_row(x, w, y + 6 * s, "view360-frame", "Frame", choices(frames, msg("view360-frame")), r.frame)
    local sizes = {}
    for _, z in ipairs(FLAT_SIZES) do sizes[#sizes + 1] = { z } end
    y = u:select_row(x, w, y, "view360-size", "Size", choices(sizes, msg("view360-size")), r.size)

    -- The exports: the recorded view flat, or the whole sphere; a part marked (A-B) or the recorded parts' span
    local g = state("gyroflow") or {}
    local half = (w - 8 * s) / 2
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local part = (a and b and b > a) and (" " .. clock(a) .. "–" .. clock(b)) or ""
    if g.exporting then
        y = u:button(x, w, y + 4 * s, "view360-export-cancel", "Cancel export", "script-message-to gyroflow cancel-export")
    else
        u:button(x, half, y + 4 * s, "view360-export-flat", "Export flat" .. part, "script-message-to gyroflow export flat",
                 { primary = true, disabled = (r.parts or 0) == 0 or r.taking })
        y = u:button(x + half + 8 * s, half, y + 4 * s, "view360-export-360", "Export 360°" .. part, "script-message-to gyroflow export 360",
                     { disabled = r.taking })
    end
    y = u:caption(x, w, y, (part ~= "" and "The part marked" or "Flat: the recorded parts' span") .. ", in the look it plays in. "
                           .. "360°: the whole sphere, level, along what the view follows")
    y = u:button(x, w, y, "view360-last", "Show the last export", "script-message-to gyroflow reveal-last-export",
                 { disabled = (g.last_export or "") == "" })

    if (r.parts or 0) > 0 then
        local asking = clear_asked and mp.get_time() < clear_asked + CLEAR_SECONDS
        u:button(x, half, y + 4 * s, "view360-undo", "Undo the last take", msg("view360-undo"), { disabled = (r.undo or 0) == 0 or r.taking })
        y = u:button(x + half + 8 * s, half, y + 4 * s, "view360-clear", asking and "Sure? Click again" or "Clear the recording",
                     function()
                         if clear_asked and mp.get_time() < clear_asked + CLEAR_SECONDS then
                             clear_asked = nil
                             mp.commandv("script-message-to", "view360", "view360-clear")
                         else
                             clear_asked = mp.get_time()
                             mp.add_timeout(CLEAR_SECONDS + 0.05, function() if redraw_flat then redraw_flat() end end)
                         end
                     end)
    end
    return y
end

local function view360_part(u, x, w, y)
    local s = u.s
    local v = state("view360")
    if type(v) ~= "table" or not (v.available or v.note) then return u:caption(x, w, y, "Open a 360° clip to look around in it") end
    if not v.available then return u:caption(x, w, y, v.note) end
    local function msg(...) return "script-message-to view360 " .. table.concat({ ... }, " ") end
    y = u:switch(x, w, y, "view360", "Look around", v.active == true, msg("view360", "toggle"), keys["script-binding view360/toggle"])
    if not v.active then
        return u:caption(x, w, y, "Off: the sphere as it was recorded, unrolled flat")
    end
    local function value_row(label, value)
        u:text(x, y + 8 * s, label, { size = 12, color = C.label })
        u:text(x + w, y + 8 * s, value, { size = 12, color = C.dim, align = 6 })
    end

    local projections = {}
    for _, row in ipairs(PROJECTION_ROWS) do
        for _, p in ipairs(row) do projections[#projections + 1] = p end
    end
    y = u:select_row(x, w, y + 10 * s, "view360-projection", "Projection", choices(projections, msg("view360-projection")), v.projection,
                     { key = keys["script-binding view360/projection"] })
    local notes = { auto = "Straight lines straight up to 100°, then more and more of the sphere: from 160° all of it, as a stereographic",
                    rectilinear = "Straight lines straight, as a camera's lens; stretched towards the edges above about 110°",
                    stereographic = "Shapes kept to the edges, up to nearly the whole sphere",
                    planet = "Looking straight down, the way ahead at the top: drag around the middle to spin it" }
    y = u:caption(x, w, y, notes[v.projection] or "")

    y = u:heading(x, w, y + 10 * s, "View")
    local fmin, fmax = v.fov_min or 20, v.fov_max or 300
    value_row("Field of view", string.format("%.0f°", v.fov or 100))
    y = u:slider(x, w, y + 14 * s, "view360:fov", math.log((v.fov or 100) / fmin) / math.log(fmax / fmin),
                 function(p) mp.commandv("script-message-to", "view360", "view360-set", "fov", string.format("%.2f", fmin * (fmax / fmin) ^ p)) end,
                 "Closer", "Wider")
    y = y + 6 * s
    value_row("Horizon", string.format("%.1f°", v.roll or 0))
    y = u:slider(x, w, y + 14 * s, "view360:roll", math.max(0, math.min(1, ((v.roll or 0) + 45) / 90)),
                 function(p) mp.commandv("script-message-to", "view360", "view360-set", "roll", string.format("%.1f", p * 90 - 45)) end, "−45°", "45°")
    local function turn(a, plus, minus)
        if math.abs(a or 0) < 0.05 then return nil end
        return string.format("%.0f° %s", math.abs(a), a > 0 and plus or minus)
    end
    local parts = {}
    for _, t in ipairs({ turn(v.yaw, "right", "left"), turn(v.pitch, "up", "down") }) do table.insert(parts, t) end
    y = u:caption(x, w, y + 4 * s, #parts > 0 and ("Looking " .. table.concat(parts, ", ")) or "Looking ahead")
    y = u:button(x, w, y + 4 * s, "view360-reset", "Look ahead again", msg("view360-reset"), { key = "BS" })
    y = u:caption(x, w, y + 4 * s, "Drag, or two fingers on a trackpad: look around. Wheel or pinch: zoom. "
                                   .. "Alt+drag or turning two fingers: the horizon. Double click: ahead again")

    -- What the view follows comes with the camera's motion in each frame (a 360 camera's own file): a stitched clip has none.
    -- Along the way and north need what the clip records besides (GPS, a compass): greyed out without it, saying why
    local modes = type(v.modes) == "table" and v.modes or nil
    local function has(m) return m == "camera" or m == "world" or (modes and type(modes[m]) == "table" and modes[m].available == true) end
    local options = choices(HEADING_OPTIONS, msg("view360-heading"))
    for _, o in ipairs(options) do o.disabled = v.stamped and not has(o.kind) end
    -- The recording's own while a take records or the recording plays
    local r = type(v.recording) == "table" and v.recording or {}
    local locked = r.taking == true or r.playing == true
    y = u:select_row(x, w, y + 10 * s, "view360-heading", "Follows", options, v.heading,
                     { key = keys["script-binding view360/heading"], disabled = not v.stamped or locked })
    if locked then
        y = u:caption(x, w, y, (r.taking and "Recording" or "Playing the recording") .. ": following "
                               .. (FOLLOWS_NAMES[v.heading] or tostring(v.heading)), { color = C.label })
    end
    if not v.stamped then
        return u:caption(x, w, y, "This clip has no motion data in its frames: the camera as it was stitched, level and still, "
                                  .. "and only your head turns")
    end
    local function pct(c) return string.format("%d %%", math.floor((tonumber(c) or 0) * 100 + 0.5)) end
    local m = modes and modes[v.heading] or {}
    local now = tonumber(v.heading_confidence)
    local text
    if v.heading == "travel" then
        text = "Along the way you move: the GPS course, the camera's turns tied to it"
               .. (m.sigma and string.format(" within ±%.1f°", m.sigma) or "") .. ". Standing, the way it went is held"
        if now then text = text .. (now < 0.05 and " (standing, or no GPS fix, now)" or ("; sure now: " .. pct(now))) end
    elseif v.heading == "camera" then
        text = "Along the camera: where it points, smoothed over a few seconds"
    elseif v.heading == "north" then
        text = m.source == "gps" and "True north, from the GPS track" or "Magnetic north, by the camera's compass"
        if m.source ~= "gps" and m.against_gps then text = text .. string.format(" (%.0f° from the GPS's true north)", m.against_gps) end
        if now then text = text .. "; sure now: " .. pct(now) end
    else
        local wm = modes and modes.world or {}
        text = "In the world: where the camera looked at the start, held"
        if wm.held_by == "north" or wm.held_by == "gps" then
            text = text .. " in the real world, the gyro's drift taken out by " .. (wm.held_by == "north" and "the compass" or "GPS")
        elseif modes then
            text = text .. ": turns slowly with the gyro's drift (no compass or GPS to hold it)"
        end
    end
    y = u:caption(x, w, y, text)
    -- The modes this clip hasn't, and why
    if not modes then
        y = u:caption(x, w, y, "Reading the clip's GPS and compass…")
    else
        for _, h in ipairs(HEADING_OPTIONS) do
            local mm = modes[h[1]]
            if type(mm) == "table" and mm.available ~= true then
                y = u:caption(x, w, y, h[2] .. ": " .. tostring(mm.why or "not in this clip"))
            end
        end
    end
    return flat_video_part(u, x, w, y, v, r)
end

-- The playlist: the files mpv plays one after the other (the videos in the clip's folder: autocreate-playlist, then
-- those of its subfolders, stabilized/, as it goes on into them), what has played kept, the one playing marked; a
-- click opens one. Shown with the one playing in view, when it opens and as the next one starts
local reveal, redraw = true, false
function reveal_playlist() reveal = true end
-- What has played: listed, though not a video by its extension (a 360° clip, .insv), once the playlist goes on from it
local played = {}
mp.register_event("start-file", function()
    local file = mp.get_property("playlist/" .. (mp.get_property_number("playlist-playing-pos") or -1) .. "/filename")
    if file then played[file] = true end
end)
local function playlist_part(u, x, w, y)
    local s = u.s
    local list = mp.get_property_native("playlist") or {}
    local rows = tl.playlist_rows(list, tl.set_of(mp.get_property_native("video-exts")), played)
    if #rows == 0 then return u:caption(x, w, y, "Nothing plays") end
    local dir = tl.common_folder(list)
    if dir and local_file(dir) then
        u:text(x, y + 9 * s, fit_end(folder_label(dir), w, 13, s), { size = 13, bold = true })
        y = y + 20 * s
    end
    y = u:caption(x, w, y, #rows == 1 and "1 video" or (#rows .. " videos")) + 6 * s
    for _, r in ipairs(rows) do
        if r.current and reveal and u.clip then
            reveal = false
            local top, bottom = u.clip[2], u.clip[4]
            if y < top or y + 28 * s > bottom then
                scroll.playlist = math.max(0, (scroll.playlist or 0) + y - (top + bottom) / 2)
                redraw = true
            end
        end
        local label, room = r.name, w - (r.current and 13 * s or 0)
        if ui.text_width(label, 13, s) > room * 0.9 then
            local n = #label
            while n > 4 and ui.text_width(tl.cut_middle(label, n), 13, s) > room * 0.9 do n = n - 1 end
            label = tl.cut_middle(label, n)
        end
        y = list_row(u, x, w, y, "pl:" .. r.index, label, nil, r.current and "play" or false,
                     r.current and function() end or ("playlist-play-index " .. r.index))
    end
    return y
end

local PARTS = { project = project_part, playlist = playlist_part, source = source_part, transform = transform_part, output = output_part, crop = crop_part,
                dv = dv_part, quality = quality_part, telemetry = telemetry_part, player = player_part, support = support_part,
                view360 = view360_part }
-- Each part's "these as the defaults" (Transform has its own, Use as default)
local REMEMBER = { output = "script-message-to dlogm remember", quality = "script-message-to gyroflow remember-quality",
                   telemetry = "script-message-to dji_telemetry remember" }
-- Parts with In the folder at their bottom
local DOCKED = { source = true, output = true }

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
    -- The drop-downs: which is open, how one opens, the row the keys are on
    u.open_id, u.focus = popover, popover_focus
    u.set_popover = function(id) set_popover(id) end
    if not shown then
        if popover then set_popover(nil) end
        return u
    end
    local rx = w - RAIL * s
    -- What shows of the scrolled area, once it's drawn
    local seen = nil
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

        local footer = (DOCKED[tab] and mp.get_property("path") and DOCK * s) or (REMEMBER[tab] and mp.get_property("path") and FOOTER * s) or 0
        local top, bottom = t0 + HEADER * s + 1, h - footer
        seen = { top, bottom }
        local before = #u.events
        u.clip = { px, top, px + PANEL * s, bottom }
        local y0 = top + 14 * s
        local off = scroll[tab] or 0
        local fn = PARTS[tab]
        local y_end = fn and fn(u, x, cw, y0 - off) or y0
        u.clip = nil
        if redraw then
            redraw = false
            for i = #u.events, before + 1, -1 do table.remove(u.events, i) end
            u.hits = {}
            return nil
        end
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
            -- The scroll bar: held by its thumb, it follows the mouse; a click beside the thumb brings it there
            local bh = math.max(24 * s, room * room / content)
            local by = top + (room - bh) * off / max
            local lit = u:hovered("scrollbar") or scrolling_bar
            u:box(px + PANEL * s - (lit and 7 or 6) * s, by + 2 * s, (lit and 5 or 3) * s, bh - 4 * s, 2, lit and C.dim or C.line)
            local part, grab = tab, nil
            u:hit(px + PANEL * s - 12 * s, top, 12 * s, room, "scrollbar", {
                drag = function(_, my)
                    if not my then return end
                    if not grab then grab = (my >= by and my < by + bh) and (my - by) or bh / 2 end
                    scrolling_bar = true
                    scroll[part] = math.max(0, math.min(max, (my - grab - top) / math.max(1, room - bh) * max))
                end,
                release = function() scrolling_bar = false end,
                redraw = true,
            })
        end
        if footer > 0 and DOCKED[tab] then
            u:divider(px, PANEL * s, bottom)
            dock(u, x, cw, bottom + 10 * s)
        elseif footer > 0 then
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
    -- The list open over it all; closed when what it's for isn't there any more (another tab, the export started)
    local where = u:popover(t0 + 8 * s, h - 8 * s, seen)
    shown_list = u.shown_list
    if not where and popover then set_popover(nil) end
    return u
end

-- On screen

local dims = { w = 0, h = 0 }
local function scale()
    return mp.get_property_number("display-hidpi-scale", 1) or 1
end

-- The navigation bar: its own overlay (it's drawn again as the time goes, the panel isn't), below the panel's
local bar_overlay = mp.create_osd_overlay("ass-events")
bar_overlay.z = 9
local bar_hits = {}
-- Over the video (the panel closed), shown until then (mp.get_time()); "never" while the agent's note hides it
local bar_until, bar_never, bar_drawn = 0, false, false
local SHOW_FOR = 1.5
-- The mouse: where it is, whether it's over the bar; the track held (its time, while it's dragged)
local mouse, over_bar = nil, false
local drag_t = nil
-- The time shown as the time left
local remaining = false

-- Below the video, always there, while the panel's open; else over it
local function docked()
    return shown and open and not bar_never
end

local function bar_shown()
    if bar_never or dims.w <= 0 then return false end
    return docked() or over_bar or drag_t ~= nil or mp.get_time() < bar_until
end

local bar_timer
local function render_bar()
    local d = dims
    if d.w <= 0 then return end
    local s = scale()
    -- The video above it, not under it, while it's docked
    local margin = docked() and math.min(0.5, bar.HEIGHT * s / d.h) or 0
    if math.abs((mp.get_property_number("video-margin-ratio-bottom") or 0) - margin) > 1e-6 then
        mp.set_property_number("video-margin-ratio-bottom", margin)
    end
    if not bar_shown() then
        bar_hits = {}
        if bar_drawn then bar_overlay:remove(); bar_drawn = false end
        bar_timer:kill()
        return
    end
    local u = ui.new(s)
    u.hover = hover
    bar.draw(u, 0, d.w - width(s), d.h, bar.snapshot(), {
        docked = docked(), hover = hover, mouse = over_bar and mouse or nil, dragging = drag_t ~= nil, drag_t = drag_t,
        remaining = remaining, keys = keys,
        toggle_time = function() remaining = not remaining end,
        seek = function(t, exact)
            drag_t = not exact and t or nil
            mp.commandv("seek", tostring(t), exact and "absolute+exact" or "absolute+keyframes")
        end,
    })
    bar_hits = u.hits
    bar_overlay.res_x, bar_overlay.res_y = d.w, d.h
    bar_overlay.data = u:render()
    bar_overlay:update()
    bar_drawn = true
    if not bar_timer:is_enabled() then bar_timer:resume() end
end
-- The time as it plays, and the bar hidden once its time is up
bar_timer = mp.add_periodic_timer(0.1, function()
    -- Paused and docked, nothing moves: what changes is drawn as it changes (the properties observed, a seek done)
    if bar_drawn and docked() and mp.get_property_native("pause") and not over_bar then return end
    render_bar()
end)
bar_timer:kill()

-- Shown for a while (over the video: the mouse moved, a seek)
local function show_bar()
    bar_until = mp.get_time() + SHOW_FOR
    render_bar()
end

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
    render_bar()
end

local function hit_at(x, y)
    for _, list in ipairs({ bar_hits, hits }) do
        for i = #list, 1, -1 do
            local h = list[i]
            if x >= h.x0 and x < h.x1 and y >= h.y0 and y < h.y1 then return h end
        end
    end
end

-- mpv's own on-screen controller hidden: the bar is in its place (the app has osc=no; an mpv the plugin is put into
-- has it on)
if mp.get_property_native("osc") then
    local opts = mp.get_property_native("script-opts") or {}
    opts["osc-visibility"] = "never"
    mp.set_property_native("script-opts", opts)
end

local update = render
redraw_flat = update

-- The mouse: clicks and the wheel are the panel's and the bar's while it's over them, the player's elsewhere
local captured = false

local function in_panel(pos)
    return pos ~= nil and pos.hover and shown and pos.x >= dims.w - width(scale())
end

local function wheel(dir)
    return function(info)
        if info.event == "up" then return end
        -- The list open closes as the panel scrolls; over the video (the mouse is the panel's while it's open), that's all
        if popover then
            set_popover(nil)
            if not in_panel(mouse) and not over_bar then return render() end
        end
        if over_bar then
            -- Over the bar: a frame on (up, right) or back (down, left), as mpv's wheel seeks forward going up
            mp.command(dir < 0 and "frame-step" or "frame-back-step")
            return
        end
        if dir == 2 or dir == -2 then return end
        local s = scale()
        scroll[tab] = math.max(0, (scroll[tab] or 0) + dir * 48 * s * (info.scale or 1))
        render()
    end
end

-- A slider held: its drag(x, y) follows the mouse until the button is let go, then its release(x); drawn again as
-- it moves when it's the panel's own (redraw: the scroll bar)
local dragging, released, drag_redraw = nil, nil, false

local function click(info)
    if info.event == "up" then
        local pos = mp.get_property_native("mouse-pos")
        if released and pos then released(pos.x) end
        local again = drag_redraw
        dragging, released, drag_redraw = nil, nil, false
        if again then render() end
        render_bar()
        return
    end
    if info.event ~= "down" and info.event ~= "press" then return end
    local pos = mp.get_property_native("mouse-pos")
    local h = pos and hit_at(pos.x, pos.y)
    -- A list open: a click off it closes it, and does nothing else
    if popover and not (h and h.popover) then
        set_popover(nil)
        return update()
    end
    if not h then return end
    if type(h.cmd) == "table" and h.cmd.drag then
        h.cmd.drag(pos.x, pos.y)
        if info.event == "down" then dragging, released, drag_redraw = h.cmd.drag, h.cmd.release, h.cmd.redraw == true
        elseif h.cmd.release then h.cmd.release(pos.x) end
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
        mp.add_forced_key_binding("WHEEL_RIGHT", "wheel-right", wheel(-2), { complex = true })
        mp.add_forced_key_binding("WHEEL_LEFT", "wheel-left", wheel(2), { complex = true })
    else
        for _, name in ipairs({ "click", "click-dbl", "wheel-up", "wheel-down", "wheel-right", "wheel-left" }) do mp.remove_key_binding(name) end
        if dragging_before ~= nil then mp.set_property_native("window-dragging", dragging_before) end
        dragging_before = nil
    end
end

-- The keys in a list open: Up and Down move along its rows (past the greyed ones), Enter picks the one they're on
local LIST_KEYS = { { "UP", "popover-up" }, { "DOWN", "popover-down" }, { "ENTER", "popover-enter" }, { "KP_ENTER", "popover-kp-enter" } }

local function list_move(dir)
    local l = shown_list
    if not l then return end
    local rows = {}
    for _, r in ipairs(l.rows) do if not r.disabled then rows[#rows + 1] = r end end
    if #rows == 0 then return end
    local at = popover_focus
    if at == nil then at = l.chosen end
    local i
    for k, r in ipairs(rows) do if r.kind == at then i = k end end
    if not i then i = dir > 0 and 0 or #rows + 1 end
    popover_focus = rows[math.max(1, math.min(#rows, i + dir))].kind
    update()
end

local function list_enter()
    local l = shown_list
    local at = popover_focus
    if l and at == nil then at = l.chosen end
    local cmd
    for _, r in ipairs(l and l.rows or {}) do
        if r.kind == at and not r.disabled then cmd = r.cmd end
    end
    if type(cmd) == "function" then cmd() elseif cmd then mp.command(cmd) end
    set_popover(nil)
    update()
end

-- A list over the panel, opened (its id) or closed (nil): Esc closes it, and the mouse is the panel's while it's open,
-- so a click anywhere else closes it; the arrow keys and Enter are the list's
set_popover = function(id)
    if id == popover then return end
    popover, popover_focus = id, nil
    if id then
        mp.add_forced_key_binding("ESC", "popover-esc", function() set_popover(nil); update() end)
        mp.add_forced_key_binding("UP", "popover-up", function() list_move(-1) end, { repeatable = true })
        mp.add_forced_key_binding("DOWN", "popover-down", function() list_move(1) end, { repeatable = true })
        mp.add_forced_key_binding("ENTER", "popover-enter", list_enter)
        mp.add_forced_key_binding("KP_ENTER", "popover-kp-enter", list_enter)
    else
        mp.remove_key_binding("popover-esc")
        for _, k in ipairs(LIST_KEYS) do mp.remove_key_binding(k[2]) end
    end
    capture((in_panel(mouse) or over_bar or popover ~= nil) and true or false)
end

mp.observe_property("mouse-pos", "native", function(_, pos)
    if not pos then return end
    mouse = pos
    if dragging then
        dragging(pos.x, pos.y)
        if drag_redraw then return render() end
        return render_bar()
    end
    local s = scale()
    local inside = in_panel(pos)
    local was_over = over_bar
    over_bar = (pos.hover and not inside and not bar_never and pos.y >= dims.h - bar.HEIGHT * s) or false
    capture((inside or over_bar or popover ~= nil) and true or false)
    -- Over the video, a move shows it; leaving the window hides it
    if pos.hover then bar_until = mp.get_time() + SHOW_FOR else bar_until = 0 end
    local h = (inside or over_bar) and hit_at(pos.x, pos.y) or nil
    local id = h and h.id or nil
    if id ~= hover then
        hover = id
        render()
    elseif over_bar or was_over or not bar_drawn then
        render_bar()
    end
end)

-- A seek with the keys shows where it went
mp.register_event("seek", function() if not docked() then show_bar() end end)
mp.register_event("playback-restart", function() render_bar() end)

mp.add_key_binding("TAB", "toggle", function()
    shown = not shown
    if shown then open = true end
    capture(false)
    update()
end)

mp.register_script_message("tab", function(name)
    name = RENAMED[name] or name
    if PARTS[name] then tab, open, shown = name, true, true; reveal_playlist(); update() end
end)

mp.register_script_message("popover", function(id)
    set_popover((id or "") ~= "" and id or nil)
    update()
end)

-- In place of mpv's playlist as text
mp.add_key_binding("F8", "playlist", function()
    tab, open, shown = "playlist", true, true
    reveal_playlist()
    update()
end)

-- The bar hidden (never) while the agent's note is written, and back (auto)
mp.register_script_message("bar", function(v)
    bar_never = v == "never"
    if bar_never then over_bar = false; capture(false) end
    update()
end)

-- What it draws, as an .ass file, at a window size given; the bar too, and what the mouse would show over it at x, y
-- with hover the id there ("bar:track", "bar:mark", "pl:3")
mp.register_script_message("dump", function(path, w, h, s, mx, my, hovered)
    read_keys()
    w, h, s = tonumber(w), tonumber(h), tonumber(s) or 1
    local u = draw(w, h, s, hovered) or draw(w, h, s, hovered)
    if not bar_never then
        local b = ui.new(s)
        b.hover = hovered
        bar.draw(b, 0, w - width(s), h, bar.snapshot(), { docked = docked(), hover = hovered, keys = keys,
                 mouse = mx and { x = tonumber(mx), y = tonumber(my) } or nil, remaining = remaining })
        for _, e in ipairs(u.events) do b.events[#b.events + 1] = e end
        u = b
    end
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
                        "user-data/view360",
                        "user-data/footage_menu", "user-data/agent", "osd-dimensions",
                        "display-hidpi-scale", "border", "path", "pause", "fullscreen", "ontop", "loop-file", "speed",
                        "ab-loop-a", "ab-loop-b", "playlist-pos", "playlist-count", "playlist", "mute", "duration",
                        "chapter-list" }) do
    mp.observe_property(prop, "native", function() update() end)
end
-- Crop mode on: its part shows; off again: back to the one there was
local crop_on, before_crop = false, nil
mp.observe_property("user-data/gyroflow/crop", "native", function(_, c)
    local on = type(c) == "table" and c.active == true
    if on ~= crop_on then
        crop_on = on
        if on then
            before_crop = { tab = tab, open = open, shown = shown }
            tab, open, shown = "crop", true, true
        elseif before_crop and tab == "crop" then
            tab, open, shown = before_crop.tab, before_crop.open, before_crop.shown
            before_crop = nil
        end
    end
    -- The camera's numbers as it turns
    update()
end)

-- A 360° clip: its part shows, if the panel's open; another clip: back to the one there was
local sphere_on, before_sphere = false, nil
mp.observe_property("user-data/view360", "native", function(_, v)
    local on = type(v) == "table" and v.available == true
    if on == sphere_on then return end
    sphere_on = on
    if on then
        if open and tab ~= "view360" then before_sphere, tab = tab, "view360" end
    elseif before_sphere and tab == "view360" then
        tab, before_sphere = before_sphere, nil
    end
    update()
end)

mp.register_event("file-loaded", function()
    read_keys()
    scroll = {}
    set_popover(nil)
    reveal_playlist()
    update()
end)
-- The highlight that plays now
mp.add_periodic_timer(1, function() if shown and open and (tab == "source" or tab == "project" or tab == "output") then render() end end)
read_keys()
-- For footage-menu: right click is mpv's own menu while the panel is there
mp.set_property_native("user-data/side_panel/loaded", true)
