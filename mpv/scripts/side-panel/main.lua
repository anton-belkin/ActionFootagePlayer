-- side-panel: the plugins' settings in a panel at the right of the window, beside the video (it's moved over for it,
-- not covered): a rail of icons at the edge, one for each part (the clip, its look, the stabilization, its quality,
-- the telemetry, the player), and the panel of the one chosen, scrolled with the wheel. What the right-click menu has, as
-- controls: it reads the same state the plugins keep in user-data/<plugin>/ and runs the same commands. An mpv
-- plugin: this folder goes into ~~/scripts/.
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
--   tab=stabilization  the part it opens on: clip, look, stabilization, quality, telemetry, player
--
-- Script messages (`script-message-to side_panel <message>`):
--   tab <name>                      open the panel on that part
--   dump <path> <w> <h> [<scale>]   write what it draws at that window size as an .ass file (for checking its look
--                                   without a window)

local mp = require 'mp'
local options = require 'mp.options'
local ui = require 'ui'

local o = { shown = true, open = true, tab = "stabilization" }
options.read_options(o, "side_panel")

local C = ui.C
local RAIL, PANEL, HEADER, FOOTER, PAD = 48, 292, 44, 56, 16
local platform = mp.get_property("platform") or ""
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
    if name == "clip" then
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
    elseif name == "quality" then
        -- A gauge: an arc, and its needle
        local R, r = 8, 6.4
        local k1, k2 = R * 0.5523, r * 0.5523
        u:shape("m " .. at(-R, 3) .. " l " .. at(-R, 0) .. " b " .. at(-R, -k1) .. " " .. at(-k1, -R) .. " " .. at(0, -R)
                .. " b " .. at(k1, -R) .. " " .. at(R, -k1) .. " " .. at(R, 0) .. " l " .. at(R, 3) .. " " .. at(r, 3) .. " " .. at(r, 0)
                .. " b " .. at(r, -k2) .. " " .. at(k2, -r) .. " " .. at(0, -r) .. " b " .. at(-k2, -r) .. " " .. at(-r, -k2) .. " " .. at(-r, 0)
                .. " l " .. at(-r, 3), color)
        u:shape("m " .. at(-0.9, 1.6) .. " l " .. at(3.6, -4.6) .. " " .. at(4.6, -3.8) .. " " .. at(0.9, 2.6), color)
        u:shape(ui.circle_path(cx, cy + 2.1 * s, 2 * s), color)
    elseif name == "telemetry" then
        for i, bar in ipairs({ { -7.5, 6 }, { -1.75, 13 }, { 4, 9 } }) do
            u:box(cx + bar[1] * s, cy + (7 - bar[2]) * s, 3.5 * s, bar[2] * s, 1, color)
        end
    elseif name == "player" then
        u:box(cx - 8.5 * s, cy - 4.75 * s, 17 * s, 1.5 * s, 0, color)
        u:box(cx - 8.5 * s, cy + 3.25 * s, 17 * s, 1.5 * s, 0, color)
        u:shape(ui.circle_path(cx - 3 * s, cy - 4 * s, 2.8 * s), color)
        u:shape(ui.circle_path(cx + 3.5 * s, cy + 4 * s, 2.8 * s), color)
    elseif name == "keys" then
        u:shape(ui.rrect_path(cx - 9 * s, cy - 6 * s, 18 * s, 12 * s, 2.5 * s), color, stroke)
        for _, x in ipairs({ -5.5, -1, 3.5 }) do u:box(cx + x * s, cy - 3 * s, 2 * s, 2 * s, 0, color) end
        u:box(cx - 4.5 * s, cy + 1.5 * s, 9 * s, 1.6 * s, 0, color)
    elseif name == "help" then
        u:shape(ui.circle_path(cx, cy, 8 * s), color, stroke)
        u:text(cx, cy + 0.5 * s, "?", { align = 5, size = 12, bold = true, color = color })
    end
end

local TABS = {
    { "clip", "Clip" }, { "look", "Look" }, { "stabilization", "Stabilization" }, { "quality", "Quality" },
    { "telemetry", "Telemetry" }, { "player", "Player" },
}
local TAB_TITLES = {}
for _, t in ipairs(TABS) do TAB_TITLES[t[1]] = t[2] end

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
    if h and h.times then table.insert(parts, #h.times == 1 and "1 highlight" or (#h.times .. " highlights")) end
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

    if h and h.times then
        y = u:heading(x, w, y + 10 * s, "Highlights", keys["script-binding highlights/next"])
        local pos = mp.get_property_number("time-pos", 0)
        for i, time in ipairs(h.times) do
            y = u:radio(x, w, y, "hl:" .. i, "Highlight " .. i, math.abs(pos - time) < 0.5, "script-message-to highlights go " .. i, { note = clock(time) })
        end
    end

    y = u:heading(x, w, y + 10 * s, "In the folder")
    local n, count = mp.get_property_number("playlist-pos", 0), mp.get_property_number("playlist-count", 0)
    u:button(x, half, y, "prev", "Previous clip", "playlist-prev", { disabled = n <= 0 })
    y = u:button(x + half + 8 * s, half, y, "next", "Next clip", "playlist-next", { disabled = n >= count - 1 })
    y = u:button(x, w, y, "frame", "Save the frame as an image", "screenshot", { key = keys["screenshot"] })
    return y
end

local function look_title(d, look)
    if look == "hdr" then return (d.log or d.hdr == "") and "HDR" or ("HDR (" .. d.hdr:upper() .. "), as it is") end
    if look == "maker" then return "SDR, " .. (d.maker ~= "" and d.maker or "the maker") .. "'s LUT" end
    if look == "nbcu" then return "SDR, NBCU (neutral)" end
    if look == "tonemap" then return d.log and "SDR, HDR tone-mapped by mpv (as on Windows)" or "SDR, tone-mapped by mpv" end
    if look == "dv" then return d.log and "HDR + Dolby Vision (DJI fit), as macOS shows it" or "With its Dolby Vision, as macOS shows it" end
    return look
end

local function look_part(u, x, w, y)
    local s = u.s
    local d = state("dlogm")
    if not mp.get_property("path") then return u:caption(x, w, y, "Nothing plays") end
    if not d then return u:caption(x, w, y, "Needs the dlogm plugin") end
    y = u:heading(x, w, y, "Look", keys["script-binding dlogm/sdr"])
    if not d.log and (d.hdr or "") == "" then
        y = u:caption(x, w, y, "Not log, as the camera says")
    else
        if d.screen == "sdr" then y = u:caption(x, w, y, "SDR screen: HDR shows as macOS tone-maps it")
        elseif d.screen == "hdr" then y = u:caption(x, w, y, "HDR screen: SDR shows as an SDR screen would") end
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
    local g = state("gyroflow")
    if g and g.available then
        y = u:switch(x, w, y, "still", "Clean still frame on pause", g.still, "script-binding gyroflow/still", keys["script-binding gyroflow/still"])
    end
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
    local lenses = { auto = 1, yes = 2, no = 3 }
    y = u:segmented(x, w, y, "lens", {
        { "Auto", "script-message-to gyroflow underwater auto", not g.water },
        { "In water", "script-message-to gyroflow underwater yes" },
        { "Above", "script-message-to gyroflow underwater no" },
    }, lenses[g.underwater])
    y = u:caption(x, w, y, g.water and "Auto: as the camera says it was in the water" or ("Auto: this camera doesn't say, so as the " .. (project and "project" or "preset")))
    y = u:switch(x, w, y, "fringes", "Reduce color aberration", g.chromatic == true, "script-message-to gyroflow chromatic toggle",
                 keys["script-binding gyroflow/fringes"], not g.chromatic_available or g.underwater == "no")
    y = u:switch(x, w, y, "still", "Clean still frame on pause", g.still, "script-binding gyroflow/still", keys["script-binding gyroflow/still"])

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

local PARTS = { clip = clip_part, look = look_part, stabilization = stabilization_part, quality = quality_part,
                telemetry = telemetry_part, player = player_part }
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
    for i, t in ipairs(TABS) do
        rail_button(i, t[1], t[2], function() choose(t[1]) end, open and tab == t[1], t0 + (8 + (i - 1) * 44) * s)
    end
    rail_button(0, "keys", "Keys", "script-message-to footage_menu keys", false, h - (8 + 36 + 44) * s)
    rail_button(0, "help", "Help", "script-message-to footage_menu help", false, h - (8 + 36) * s)
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

local function click(info)
    if info.event ~= "down" and info.event ~= "press" then return end
    local pos = mp.get_property_native("mouse-pos")
    local h = pos and hit_at(pos.x, pos.y)
    if not h then return end
    if type(h.cmd) == "function" then h.cmd() else mp.command(h.cmd) end
    -- What the plugins change shows on their next publish; render now for what's the panel's own
    update()
end

local function capture(on)
    if on == captured then return end
    captured = on
    if on then
        mp.add_forced_key_binding("MBTN_LEFT", "click", click, { complex = true })
        mp.add_forced_key_binding("MBTN_LEFT_DBL", "click-dbl", function() end)
        mp.add_forced_key_binding("WHEEL_UP", "wheel-up", wheel(-1), { complex = true })
        mp.add_forced_key_binding("WHEEL_DOWN", "wheel-down", wheel(1), { complex = true })
    else
        for _, name in ipairs({ "click", "click-dbl", "wheel-up", "wheel-down" }) do mp.remove_key_binding(name) end
    end
end

mp.observe_property("mouse-pos", "native", function(_, pos)
    if not pos then return end
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
for _, prop in ipairs({ "user-data/dlogm", "user-data/gyroflow", "user-data/dji_telemetry", "user-data/highlights", "osd-dimensions",
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
mp.add_periodic_timer(1, function() if shown and open and tab == "clip" then render() end end)
read_keys()
