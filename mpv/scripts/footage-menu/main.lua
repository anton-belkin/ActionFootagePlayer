-- footage-menu: a right-click menu for the plugins (dlogm, gyroflow, dji-telemetry, highlights, agent) and the player: what
-- their keys do, with the keys beside it, and the settings that have no key. An mpv plugin: this folder goes into
-- ~~/scripts/.
--
-- The menu is mpv's own (context_menu.lua; on Windows the native one), so it needs mpv 0.41 or later; with an older
-- mpv right click stays as it was. It's filled each time it opens (menu-data) from what the plugins keep in
-- user-data/<plugin>/, so it shows what each one does now, and only the plugins that are there.
--
-- Keys (rebind with `<key> script-binding footage_menu/<name>` in input.conf):
--   right click, MENU   menu   the menu
--
-- Options (~~/script-opts/footage_menu.conf):
--   setup=   a setup script the menu offers to run again (ActionFootagePlayer's setup.sh, run with `all`)

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { setup = "" }
options.read_options(o, "footage_menu")

local REPO = "https://github.com/anton-belkin/ActionFootagePlayer"
local platform = mp.get_property("platform") or ""
-- DJI's telemetry names its schema, not the camera
local CAMERAS = { ["dvtm_ac206.proto"] = "DJI Osmo Action 6", ["dvtm_oq101.proto"] = "DJI Osmo 360" }
local CURVES = { { "dlogm", "D-Log M" }, { "dlog", "D-Log" }, { "gplog", "GP-Log" }, { "gplog2", "GP-Log2" }, { "ilog", "I-Log (10-bit)" } }
local CURVE_NAMES = {}
for _, c in ipairs(CURVES) do CURVE_NAMES[c[1]] = c[2] end
local EDGES = { { "sphere", "Trail by direction" }, { "trail", "Trail of earlier frames" }, { "original", "Original frame" },
                { "zoom", "Zoomed frame" }, { "off", "Background color" } }
local CORNERS = { { "top-left", "Top left" }, { "top-right", "Top right" }, { "bottom-left", "Bottom left" }, { "bottom-right", "Bottom right" } }
local FIELDS = { { "depth", "Depth" }, { "water", "In or out of water" }, { "pressure", "Pressure" }, { "altitude", "Altitude" },
                 { "gps", "GPS" }, { "speed", "Speed" }, { "temperature", "Sensor temperature" }, { "heartrate", "Heart rate" },
                 { "exposure", "Exposure" } }
local SPEEDS = { 0.25, 0.5, 1, 1.5, 2, 4 }
local KIND_LABELS = { project = "Gyroflow project", default = "gyro" }
-- What the keys do, for Keys…: the plugins' and the player's that the menu has
local KEY_LABELS = {
    { "cycle pause", "Play / pause" },
    { "ab-loop", "Mark the start of a part, its end, then clear it" },
    { "script-binding gyroflow/export", "Export the clip as it plays, or the marked part" },
    { "script-binding highlights/next", "Next highlight the camera marked" },
    { "script-binding highlights/prev", "Previous highlight" },
    { "script-binding dlogm/hdr", "Log in HDR" },
    { "script-binding dlogm/sdr", "Log in the standard look (DJI's LUT)" },
    { "script-binding dlogm/off", "Log handling off for this clip" },
    { "script-binding dlogm/curve", "The next log curve, by hand" },
    { "script-binding dlogm/denoise", "Noise reduction on / off" },
    { "script-binding gyroflow/toggle", "Stabilization on, wider view, off" },
    { "script-binding gyroflow/edges", "What fills the edges" },
    { "script-binding gyroflow/edit", "Open in Gyroflow" },
    { "script-binding dji_telemetry/toggle", "Telemetry on / off" },
    { "playlist-prev", "Previous clip in the folder" },
    { "playlist-next", "Next clip in the folder" },
    { "script-binding agent/report", "A note for the agent: areas of the frame and a comment" },
    { "script-binding footage_menu/menu", "This menu" },
}

local function state(plugin)
    return mp.get_property_native("user-data/" .. plugin)
end

local function clock(s)
    s = math.floor(s)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

-- The key each command is bound to, the binding that wins (SPACE among equals): "Ctrl+g"; a capital letter as
-- Shift+ it
local function shortcuts()
    local keys, priority = {}, {}
    for _, b in ipairs(mp.get_property_native("input-bindings") or {}) do
        -- The scripts' keys are "nonscalable script-binding <script>/<name>" there
        if b.cmd then b.cmd = b.cmd:gsub("^nonscalable ", "") end
        if b.cmd and not b.key:find("^MBTN") and not b.key:find("^MOUSE")
           and (not priority[b.cmd] or b.priority > priority[b.cmd] or (b.priority == priority[b.cmd] and b.key == "SPACE")) then
            keys[b.cmd] = b.key:match("^%u$") and ("Shift+" .. b.key) or b.key
            priority[b.cmd] = b.priority
        end
    end
    return keys
end

local SEPARATOR = { type = "separator" }

-- opts: shortcut, checked, disabled
local function item(title, cmd, opts)
    opts = opts or {}
    local it = { title = title, cmd = cmd, shortcut = opts.shortcut, state = {} }
    if opts.checked then table.insert(it.state, "checked") end
    if opts.disabled then table.insert(it.state, "disabled") end
    return it
end

local function info(title)
    return item(title, nil, { disabled = true })
end

local function submenu(title, items, opts)
    local it = item(title, nil, opts)
    it.type = "submenu"
    it.submenu = items
    return it
end

local function local_file(path)
    return path and not path:find("://", 1, true)
end

-- What the clip is, as the plugins found it: "DJI Osmo Action 6 · D-Log M · gyro · telemetry · 3 highlights"
local function passport(d, g, t, h)
    local parts = {}
    if t and t.model and t.model ~= "" then
        table.insert(parts, CAMERAS[t.model] or (t.model:find("^dvtm_") and "DJI" or t.model))
    end
    if d then
        if d.log then
            table.insert(parts, (CURVE_NAMES[d.curve] or d.curve) .. (d.curve ~= d.recorded and " (by hand)" or ""))
        elseif d.hdr and d.hdr ~= "" then
            table.insert(parts, "HDR (" .. d.hdr:upper() .. ")")
        else
            table.insert(parts, "not log")
        end
    end
    if g then table.insert(parts, g.available and (KIND_LABELS[g.kind] or "gyro") or "no motion data") end
    if t and t.available then table.insert(parts, "telemetry") end
    if h and h.times then table.insert(parts, #h.times == 1 and "1 highlight" or (#h.times .. " highlights")) end
    return #parts > 0 and table.concat(parts, " · ") or nil
end

local function look_menu(d, key)
    local items = {}
    if not d.log and d.hdr and d.hdr ~= "" then
        -- A file tagged HDR: as it is, or tone-mapped to SDR as without the plugin
        table.insert(items, item("HDR (" .. d.hdr:upper() .. "), as it is", "script-binding dlogm/hdr", { checked = d.look == "hdr", shortcut = key["script-binding dlogm/hdr"] }))
        table.insert(items, item("Tone-mapped to SDR", "script-binding dlogm/sdr", { checked = d.look == "sdr", shortcut = key["script-binding dlogm/sdr"] }))
    elseif not d.log then
        table.insert(items, info("Not log, as the camera says"))
    else
        local name = CURVE_NAMES[d.curve] or d.curve
        table.insert(items, item("HDR", "script-binding dlogm/hdr", { checked = d.look == "hdr", shortcut = key["script-binding dlogm/hdr"] }))
        local sdr_title = "Standard look (DJI's LUT)"
        if not d.sdr then sdr_title = d.curve == "dlogm" and "Standard look (DJI's LUT isn't downloaded)" or ("Standard look (none for " .. name .. ")") end
        table.insert(items, item(sdr_title, "script-binding dlogm/sdr", { checked = d.look == "sdr", disabled = not d.sdr, shortcut = key["script-binding dlogm/sdr"] }))
        table.insert(items, item("As recorded (log off)", "script-binding dlogm/off", { checked = d.look == "off", shortcut = key["script-binding dlogm/off"] }))
    end
    table.insert(items, SEPARATOR)
    local curves = {
        item(d.recorded ~= "" and ("From the clip (" .. (CURVE_NAMES[d.recorded] or d.recorded) .. ")") or "None (not log)",
             "script-message-to dlogm curve auto", { checked = d.curve == d.recorded }),
        SEPARATOR,
    }
    for _, c in ipairs(CURVES) do
        table.insert(curves, item(c[2], "script-message-to dlogm curve " .. c[1], { checked = d.curve == c[1] and d.curve ~= d.recorded }))
    end
    table.insert(items, submenu("Log curve", curves, { shortcut = key["script-binding dlogm/curve"] }))
    table.insert(items, SEPARATOR)
    table.insert(items, item("Noise reduction", "script-binding dlogm/denoise", { checked = d.denoise, shortcut = key["script-binding dlogm/denoise"] }))
    table.insert(items, SEPARATOR)
    table.insert(items, item("Remember as default", "script-message-to dlogm remember"))
    return items
end

local function stabilization_menu(g, key)
    local toggle, lens = key["script-binding gyroflow/toggle"], key["script-binding gyroflow/lens"]
    if not g.available then
        return {
            info("No motion data in this clip"),
            g.waiting and info("Waiting for the project from Gyroflow…")
                or item("Stabilize in Gyroflow (optical flow)…", "script-binding gyroflow/edit", { shortcut = key["script-binding gyroflow/edit"] }),
        }
    end
    local edges = {}
    for _, e in ipairs(EDGES) do
        table.insert(edges, item(e[2], "script-message-to gyroflow edges " .. e[1], { checked = g.edges == e[1] }))
    end
    return {
        item(g.kind == "project" and "On (Gyroflow project)" or "On", "script-message-to gyroflow view normal", { checked = g.view == "normal", shortcut = toggle }),
        item("Wider view", "script-message-to gyroflow view wide", { checked = g.view == "wide", shortcut = toggle }),
        item("Off", "script-message-to gyroflow view off", { checked = g.view == "off", shortcut = toggle }),
        SEPARATOR,
        item("Lock horizon", "script-message-to gyroflow horizon-lock " .. (g.horizon_lock == "yes" and "no" or "yes"), { checked = g.horizon_lock == "yes" }),
        submenu("Lens", {
            item("Under water", "script-message-to gyroflow underwater yes", { checked = g.underwater == "yes", shortcut = lens }),
            -- Only a camera that records when it's in the water has it; elsewhere auto is the project's or preset's
            item(g.water and "Auto: as the camera says (In water / Out of water)" or "Auto: this camera doesn't say, so as the "
                 .. (g.kind == "default" and "preset" or "project") .. " says",
                 "script-message-to gyroflow underwater auto", { checked = g.underwater == "auto", disabled = not g.water, shortcut = lens }),
            item("Above water", "script-message-to gyroflow underwater no", { checked = g.underwater == "no", shortcut = lens }),
            SEPARATOR,
            -- Only under water: above water no color moves either way
            item("Take out color fringes under water", "script-message-to gyroflow chromatic toggle",
                 { checked = g.chromatic == true, disabled = not g.chromatic_available or g.underwater == "no",
                   shortcut = key["script-binding gyroflow/fringes"] }),
        }, { shortcut = lens }),
        SEPARATOR,
        submenu("Edges", edges, { shortcut = key["script-binding gyroflow/edges"] }),
        item("Clean still frame on pause", "script-binding gyroflow/still", { checked = g.still, shortcut = key["script-binding gyroflow/still"] }),
        SEPARATOR,
        item("Edit in Gyroflow…", "script-binding gyroflow/edit", { shortcut = key["script-binding gyroflow/edit"] }),
        item("Remember as default", "script-message-to gyroflow remember"),
    }
end

local function telemetry_menu(t, key)
    local corners, fields = {}, {}
    for _, c in ipairs(CORNERS) do
        table.insert(corners, item(c[2], "script-message-to dji_telemetry position " .. c[1], { checked = t.position == c[1] }))
    end
    for _, f in ipairs(FIELDS) do
        table.insert(fields, item(f[2], "script-message-to dji_telemetry field " .. f[1], { checked = t.shown and t.shown[f[1]] }))
    end
    return {
        item("Show", "script-binding dji_telemetry/toggle", { checked = t.visible, shortcut = key["script-binding dji_telemetry/toggle"] }),
        submenu("Position", corners),
        submenu("Units", {
            item("Metric", "script-message-to dji_telemetry units metric", { checked = t.units == "metric" }),
            item("Imperial: ft, mph, °F", "script-message-to dji_telemetry units imperial", { checked = t.units == "imperial" }),
        }),
        submenu("Fields", fields),
        SEPARATOR,
        item("Remember as default", "script-message-to dji_telemetry remember"),
    }
end

local function export_menu(d, g)
    local reveal = platform == "darwin" and "Show last export in Finder" or platform == "windows" and "Show last export in Explorer"
                   or "Open the folder of the last export"
    return {
        item("Export in standard look (Rec.709)", "script-message-to gyroflow export sdr",
             { disabled = g.exporting or not (d and d.log and d.sdr) }),
        SEPARATOR,
        item(reveal, "script-message-to gyroflow reveal-last-export", { disabled = (g.last_export or "") == "" }),
    }
end

local function tracks_menu(kind, command)
    local items = { item("Off", command .. " no", { checked = true }) }
    local any = false
    for _, tr in ipairs(mp.get_property_native("track-list") or {}) do
        if tr.type == kind then
            any = true
            if tr.selected then items[1].state = {} end
            local title = tr.title or tr.lang or ("Track " .. tr.id)
            if tr.title and tr.lang then title = tr.title .. " (" .. tr.lang .. ")" end
            table.insert(items, item(title, command .. " " .. tr.id, { checked = tr.selected }))
        end
    end
    return any and items or nil
end

local function highlights_menu(h, key)
    local items = {
        item("Next", "script-binding highlights/next", { shortcut = key["script-binding highlights/next"] }),
        item("Previous", "script-binding highlights/prev", { shortcut = key["script-binding highlights/prev"] }),
        SEPARATOR,
    }
    for i, t in ipairs(h.times) do
        table.insert(items, item(i .. "    " .. clock(t), "script-message-to highlights go " .. i))
    end
    return items
end

local function clip_menu(path, key)
    local pos, count = mp.get_property_number("playlist-pos", 0), mp.get_property_number("playlist-count", 0)
    local reveal = platform == "darwin" and "Show in Finder" or platform == "windows" and "Show in Explorer" or "Open its folder"
    return {
        item("Next clip in the folder", "playlist-next", { disabled = pos >= count - 1, shortcut = key["playlist-next"] }),
        item("Previous clip in the folder", "playlist-prev", { disabled = pos <= 0, shortcut = key["playlist-prev"] }),
        SEPARATOR,
        item(reveal, "script-message-to footage_menu reveal", { disabled = not local_file(path) }),
        item("Copy path", "script-message-to footage_menu copy-path", { disabled = not path }),
        item("Save frame as image", "screenshot", { shortcut = key["screenshot"] }),
    }
end

local function player_menu(key)
    local speeds = {}
    local speed = mp.get_property_number("speed", 1)
    for _, s in ipairs(SPEEDS) do
        table.insert(speeds, item(string.format("%g%%", s * 100), "set speed " .. s, { checked = math.abs(speed - s) < 1e-3 }))
    end
    local items = { submenu("Speed", speeds) }
    local audio, subs = tracks_menu("audio", "set aid"), tracks_menu("sub", "set sid")
    if audio then table.insert(items, submenu("Audio track", audio)) end
    if subs then table.insert(items, submenu("Subtitles", subs)) end
    table.insert(items, item("Loop the clip", "cycle-values loop-file inf no", { checked = mp.get_property("loop-file") == "inf" }))
    table.insert(items, SEPARATOR)
    table.insert(items, item("Fullscreen", "cycle fullscreen", { checked = mp.get_property_native("fullscreen"), shortcut = key["cycle fullscreen"] }))
    table.insert(items, item("Always on top", "cycle ontop", { checked = mp.get_property_native("ontop"), shortcut = key["cycle ontop"] }))
    return items
end

local function app_menu(path, key)
    local items = {
        item("Note for the agent…", "script-binding agent/report", { disabled = not path or not state("agent"), shortcut = key["script-binding agent/report"] }),
        SEPARATOR,
        item("Keys…", "script-message-to footage_menu keys"),
        item("Help", "script-message-to footage_menu help"),
        item("Report this camera…", "script-message-to footage_menu report", { disabled = not local_file(path) }),
    }
    if o.setup ~= "" then table.insert(items, item("Set up again…", "script-message-to footage_menu setup")) end
    return items
end

local function build()
    local key = shortcuts()
    local d, g, t, h = state("dlogm"), state("gyroflow"), state("dji_telemetry"), state("highlights")
    local path = mp.get_property("path")
    local items = {}
    local add = function(it) table.insert(items, it) end

    if path then
        local about = passport(d, g, t, h)
        if about then add(info(about)); add(SEPARATOR) end
    end
    add(item(mp.get_property_native("pause") and "Play" or "Pause", "cycle pause", { shortcut = key["cycle pause"], disabled = not path }))
    if path then
        local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
        add(item("Mark start of part", "set ab-loop-a ${=time-pos}", { shortcut = key["ab-loop"] }))
        add(item("Mark end of part", "set ab-loop-b ${=time-pos}", { shortcut = key["ab-loop"], disabled = not a }))
        if a or b then add(item("Clear part", "set ab-loop-a no; set ab-loop-b no")) end
        if g and local_file(path) then
            local export = key["script-binding gyroflow/export"]
            if g.exporting then
                add(item("Cancel export", "script-message-to gyroflow cancel-export", { shortcut = export }))
            else
                add(item((a and b and b > a) and ("Export part " .. clock(a) .. "–" .. clock(b)) or "Export clip", "script-binding gyroflow/export", { shortcut = export }))
            end
        end
        if h and h.times then add(submenu("Highlights (" .. #h.times .. ")", highlights_menu(h, key))) end
        add(SEPARATOR)
        if d then add(submenu("Look", look_menu(d, key))) end
        if g and local_file(path) then add(submenu("Stabilization", stabilization_menu(g, key))) end
        if t then
            if t.available then add(submenu("Telemetry", telemetry_menu(t, key))) else add(info("Telemetry: none in this clip")) end
        end
        add(SEPARATOR)
        if g and local_file(path) then add(submenu("Export", export_menu(d, g))) end
        add(submenu("Clip", clip_menu(path, key)))
    end
    add(submenu("Player", player_menu(key)))
    add(SEPARATOR)
    add(submenu("ActionFootagePlayer", app_menu(path, key)))
    -- No two separators in a row, none at either end
    local out = {}
    for _, it in ipairs(items) do
        if it ~= SEPARATOR or (#out > 0 and out[#out] ~= SEPARATOR) then table.insert(out, it) end
    end
    if out[#out] == SEPARATOR then table.remove(out) end
    return out
end

-- context_menu.lua, or the native menu where mpv has one (Windows), as mpv's own select.lua opens it
local function use_menu_script()
    return mp.get_property_native("load-context-menu")
        or (mp.get_property("current-gpu-context") == "d3d11" and mp.get_property_native("fullscreen") and mp.get_property_native("d3d11-exclusive-fs"))
end

if mp.get_property("load-context-menu") == nil and platform ~= "windows" then
    mp.msg.verbose("this mpv has no context menu (0.41 and later have): right click stays as it was")
    return
end

local function open(info_)
    if info_.event == "repeat" then return end
    if info_.event == "down" or info_.event == "press" then mp.set_property_native("menu-data", build()) end
    if not info_.is_mouse then
        if info_.event == "down" or info_.event == "press" then
            mp.command(use_menu_script() and "script-message-to context_menu open" or "context-menu")
        end
    elseif use_menu_script() then
        mp.commandv("script-message-to", "context_menu", info_.event == "down" and "open" or "select")
    elseif info_.event == (platform == "windows" and "up" or "down") then
        mp.command("context-menu")
    end
end
mp.add_key_binding("MBTN_RIGHT", "menu", open, { complex = true })
mp.add_key_binding("MENU", "menu-key", open, { complex = true })

-- What the menu runs that's no plugin's

local function run(args)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false }, function() end)
end

local function open_url(url)
    if platform == "darwin" then run({ "open", url })
    elseif platform == "windows" then run({ "rundll32", "url.dll,FileProtocolHandler", url })
    else run({ "xdg-open", url }) end
end

mp.register_script_message("reveal", function()
    local path = mp.get_property("path")
    if not local_file(path) then return end
    path = utils.join_path(mp.get_property("working-directory") or "", path)
    if platform == "darwin" then run({ "open", "-R", path })
    elseif platform == "windows" then run({ "explorer", "/select," .. path })
    else run({ "xdg-open", utils.split_path(path) }) end
end)

mp.register_script_message("copy-path", function()
    local path = mp.get_property("path")
    if not path then return end
    if local_file(path) then path = utils.join_path(mp.get_property("working-directory") or "", path) end
    -- The clipboard property is mpv 0.40's
    local ok = mp.set_property("clipboard/text", path)
    mp.osd_message(ok and ("Copied: " .. path) or path, ok and 2 or 6)
end)

mp.register_script_message("keys", function()
    local key = shortcuts()
    local lines = {}
    for _, k in ipairs(KEY_LABELS) do
        if key[k[1]] then table.insert(lines, key[k[1]] .. "    " .. k[2]) end
    end
    table.insert(lines, "Right click    This menu")
    mp.osd_message(table.concat(lines, "\n"), 10)
end)

mp.register_script_message("help", function() open_url(REPO .. "#readme") end)

local function url_encode(s)
    return (s:gsub("[^%w%-%._~]", function(c) return string.format("%%%02X", c:byte()) end))
end

-- A new issue on GitHub, with what the plugins found in the clip
mp.register_script_message("report", function()
    local path = mp.get_property("path")
    if not local_file(path) then return end
    local d, g, t, h = state("dlogm"), state("gyroflow"), state("dji_telemetry"), state("highlights")
    local camera = t and t.model ~= "" and (CAMERAS[t.model] or t.model) or "(which camera?)"
    local lines = {
        "Camera: " .. camera,
        "Clip: " .. mp.get_property("filename", ""),
        "Log curve from the metadata: " .. (d and (d.recorded ~= "" and (CURVE_NAMES[d.recorded] or d.recorded) or "none") or "dlogm plugin not loaded"),
        "Motion data: " .. (g and (g.available and (KIND_LABELS[g.kind] or g.kind) or "none") or "gyroflow plugin not loaded"),
        "Telemetry: " .. (t and (t.available and ((t.model ~= "" and t.model) or "yes") or "none") or "dji_telemetry plugin not loaded"),
        "Highlights: " .. (h and (h.times and (#h.times .. " (" .. h.camera .. ")") or "none") or "highlights plugin not loaded"),
        "Player: " .. (mp.get_property("mpv-version") or "mpv") .. ", " .. platform,
        "",
        "What it did, and what you expected:",
        "",
    }
    open_url(REPO .. "/issues/new?title=" .. url_encode("Camera: " .. camera) .. "&body=" .. url_encode(table.concat(lines, "\n")))
    mp.osd_message("A new issue opened on GitHub with what's known about this clip", 3)
end)

mp.register_script_message("setup", function()
    if o.setup == "" then return end
    mp.osd_message("Setting up...", 10)
    mp.command_native_async({ name = "subprocess", args = { o.setup, "all" }, playback_only = false, capture_stdout = true, capture_stderr = true }, function(_, r)
        local report = ((r and r.stdout or "") .. (r and r.stderr or "")):gsub("%s+$", "")
        mp.osd_message(report ~= "" and report or "Set up", 8)
    end)
end)
