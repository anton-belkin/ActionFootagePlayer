-- The gyroflow plugin's exports (mpv/scripts/gyroflow/export.lua) over a fake mpv: every route hands gyroflow-export
-- one settings JSON (--settings): the clip playing (Shift+E, a photo, a preview) the gyroflow plugin's own settings as
-- they are (user-data/gyroflow/settings, what its warp plays with), another clip (the Library's batch, a photo of a
-- mark) its saved view's (settings.lua of_view: else the session's, else the app's defaults; Performance & Quality's
-- temporal denoise says how much, a recipe's its own). No route passes a flag the settings stand for.
-- From the repository's folder: luajit tests/export_args_test.lua

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- The JSON the messages carry: the test hands the tables over by a name
local requests = {}
local props = { platform = "darwin" }
local messages, bindings, ran = {}, {}, {}
local mp = {
    msg = { warn = function() end, info = function() end, verbose = function() end, debug = function() end, error = function() end },
    get_property = function(name, def) local v = props[name]; if v == nil then return def end return tostring(v) end,
    get_property_native = function(name) return props[name] end,
    get_property_number = function(name) return tonumber(props[name]) end,
    set_property = function(name, v) props[name] = v return true end,
    set_property_native = function(name, v) props[name] = v return true end,
    command_native = function() return nil end,
    command_native_async = function(cmd) if cmd.name == "subprocess" then table.insert(ran, cmd.args) end return #ran end,
    abort_async_command = function() end,
    commandv = function() end,
    register_event = function() end,
    observe_property = function() end,
    add_key_binding = function(_, name, fn) bindings[name] = fn end,
    register_script_message = function(name, fn) messages[name] = fn end,
    create_osd_overlay = function() return { update = function() end, remove = function() end } end,
    osd_message = function() end,
    notify = function() end,
    add_timeout = function() return { kill = function() end } end,
    add_periodic_timer = function() return { kill = function() end, resume = function() end, stop = function() end } end,
    get_script_directory = function() return "/plugins/gyroflow" end,
    get_script_name = function() return "gyroflow" end,
    get_time = function() return 0 end,
}
local utils = {
    join_path = function(a, b) return a .. "/" .. b end,
    -- gyroflow-export is there
    file_info = function(path) if path:match("gyroflow%-export$") then return { is_file = true } end end,
    -- The JSON a route hands over: the table itself, by a name
    format_json = function(t) requests[#requests + 1] = t; return "json" .. #requests end,
    parse_json = function(s) return requests[s] or requests[tonumber((s or ""):match("^json(%d+)$") or "")] end,
    getpid = function() return 1 end,
}
package.loaded['mp'] = mp
package.loaded['mp.utils'] = utils
package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path
assert(loadfile("mpv/scripts/gyroflow/export.lua"))()

-- The arguments of the last gyroflow-export run, as a string, and whether it has a run of them
local function last() return " " .. table.concat(ran[#ran] or {}, " ") .. " " end
local function has(args, part) return args:find(" " .. part .. " ", 1, true) ~= nil end
-- The settings the last run was handed (none: an empty table), and every flag the settings stand for
local function sent()
    local args = ran[#ran] or {}
    for i, a in ipairs(args) do if a == "--settings" then return requests[tonumber(args[i + 1]:match("%d+"))] end end
    return {}
end
local carried = require('settings').flags()
local function no_settings_flag(what)
    for _, a in ipairs(ran[#ran] or {}) do
        if carried[a] then expect(what .. ": no " .. a .. ", the settings carry it", a, nil) end
    end
end

-- The Library's batch: a clip's saved view
local function batch(view)
    requests.req = { id = 1, clip = "/clips/a.MP4", output = "/out/a.mov", reply = "workspace", view = view }
    messages["export-batch"]("req")
    no_settings_flag("batch")
    return sent()
end
props["user-data/gyroflow/session"] = { denoise = false, denoise_strong = false, chromatic = true }
props["user-data/gyroflow/defaults"] = { denoise = true, denoise_strong = false, chromatic = true }
props["user-data/gyroflow/temporal_denoise"] = "full"
props["user-data/gyroflow/settings"] = { denoise_grain = 0.4 }

local s = batch({ denoise = true, denoise_strong = true, chromatic = false })
expect("batch: the view's denoise", s.denoise, true)
expect("batch: …full", s.temporal_denoise, "full")
expect("batch: its Stronger", s.denoise_strong, true)
expect("batch: its fringes as recorded", s.chromatic, false)
expect("batch: the grain this computer keeps (#314)", s.denoise_grain, 0.4)
s = batch({})
expect("batch: nothing saved, the session's denoise (off)", s.denoise, false)
expect("batch: nothing saved, the session's fringes", s.chromatic, true)
props["user-data/gyroflow/session"] = {}
s = batch({})
expect("batch: no session either, the defaults' denoise", s.denoise, true)
props["user-data/gyroflow/temporal_denoise"] = "light"
s = batch({ denoise = true })
expect("batch: Light on this computer", s.temporal_denoise, "light")
props["user-data/gyroflow/temporal_denoise"] = "off"
s = batch({ denoise = true, denoise_strong = true })
expect("batch: Off on this computer", s.temporal_denoise, "off")
expect("batch: the clip last", last():match("(%S+) $"), "/clips/a.MP4")
requests.req = { id = 1, clip = "/clips/a.MP4", output = "/out/a.mov", reply = "workspace", view = { view = "off" }, sdr = true, project = "/m/own.gyroflow" }
messages["export-batch"]("req")
s = sent()
expect("batch SDR: NBCU's", s.sdr, "nbcu")
expect("batch: a moment's own project in the settings", s.project, "/m/own.gyroflow")
expect("batch: off as recorded, nothing done", s.lens, "off")

-- Shift+E on the clip playing: the gyroflow plugin's settings as they are, whatever the session says
local playing = { project = "/clips/b.MP4", level = "strong", lens = "stabilized", edges = "sphere", edges_blur = 2, denoise = true,
                  temporal_denoise = "full", denoise_strong = false, denoise_grain = 0.25, chromatic = true, translation = true, aspect = "9:16" }
props["user-data/gyroflow/settings"] = playing
props.path = "/clips/b.MP4"
bindings.export()
a = last()
s = sent()
no_settings_flag("Shift+E")
expect("Shift+E: gyroflow-export", a:find("gyroflow-export", 1, true) ~= nil, true)
for k, v in pairs(playing) do expect("Shift+E: the player's " .. k .. " as it is", s[k], v) end
expect("Shift+E: Ctrl+j's jerks (#313)", s.translation, true)
expect("Shift+E: the clip last", a:match("(%S+) $"), "/clips/b.MP4")
-- The look as it plays: SDR by the maker's LUT, Dolby Vision's RPU
props["user-data/dlogm/look"], props["user-data/dlogm/curve"] = "maker", "dlogm"
props.path = "/clips/b2.MP4"
bindings.export()
s = sent()
expect("Shift+E maker: its SDR", s.sdr, "maker")
expect("Shift+E maker: the curve", s.curve, "dlogm")
props["user-data/dlogm/look"], props["macos-dovi-rpu-file"], props["macos-dovi-live-l1"] = "dv", "/dv/fit.rpu", true
props.path = "/clips/b3.MP4"
bindings.export()
s = sent()
expect("Shift+E dv: the RPU", s.dv_rpu, "/dv/fit.rpu")
expect("Shift+E dv: each frame's L1", s.dv_live_l1, true)
expect("Shift+E dv: HDR", s.sdr, nil)
messages.export("youtube")
props.path = "/clips/b4.MP4"
messages.export("youtube")
a, s = last(), sent()
expect("YouTube: --youtube", has(a, "--youtube auto"), true)
expect("YouTube: HDR, no RPU", s.dv_rpu == nil and s.sdr == nil, true)
props["user-data/dlogm/look"], props["user-data/dlogm/curve"], props["macos-dovi-rpu-file"] = nil, nil, nil

-- A preview of a moment, as the clip plays (in SDR), its own project
props["user-data/gyroflow/settings"] = { project = "/clips/b.MP4", lens = "stabilized", denoise = false, chromatic = false }
requests.prev = { id = 2, clip = "/clips/b.MP4", output = "/m/x.afplink.mp4", reply = "workspace", project = "/m/x.gyroflow" }
messages.preview("prev")
a, s = last(), sent()
no_settings_flag("preview")
expect("preview: --preview still", has(a, "--preview still"), true)
expect("preview: the denoise off as it plays", s.denoise, false)
expect("preview: fringes as recorded as it plays", s.chromatic, false)
expect("preview: the moment's own project", s.project, "/m/x.gyroflow")
requests.prev = { id = 2, clip = "/clips/b.MP4", output = "/m/x.afplink.mp4", reply = "workspace", project = false }
messages.preview("prev")
expect("preview: no project of its own: the clip's own, found by gyroflow-export", sent().project, nil)

-- A photo of a mark: its view's (its motion, when it has one, is exported with its denoise)
props["user-data/gyroflow/session"] = { denoise = true, chromatic = true }
props["user-data/gyroflow/temporal_denoise"] = "full"
requests.photo = { id = 3, clip = "/clips/c.MP4", t = 4.5, reply = "workspace", view = { chromatic = false } }
messages["photo-at"]("photo")
a, s = last(), sent()
no_settings_flag("photo-at")
expect("photo-at: --photo", has(a, "--photo 4.5000"), true)
expect("photo-at: the view's fringes", s.chromatic, false)
expect("photo-at: the session's denoise for its motion", s.denoise == true and s.temporal_denoise == "full", true)
-- #319: a recipe's denoise, whatever this computer plays: gyroflow-export cleans the still up as the settings say
props["user-data/gyroflow/temporal_denoise"] = "off"
requests.photo = { id = 3, clip = "/clips/c.MP4", t = 4.5, reply = "workspace", view = { denoise = true, temporal_denoise = "light" } }
messages["photo-at"]("photo")
s = sent()
expect("photo-at: the recipe's light denoise (#319)", s.temporal_denoise, "light")
expect("photo-at: no radius from this computer (#319)", has(last(), "--radius"), false)
props["user-data/gyroflow/temporal_denoise"] = "full"

-- The export's settings (docs/design/export-settings.md): the player's own, kept in gyroflow.conf by export-setting, and the
-- Library's, in its request
local saved = {}
require('platform').save_options = function(name, kv) for _, p in ipairs(kv) do saved[name .. "." .. p[1]] = p[2] end end
props["user-data/gyroflow/export_settings"] = nil
props["user-data/gyroflow/settings"] = { lens = "stabilized", denoise = true, temporal_denoise = "full", edges = "sphere", chromatic = true }
-- (each a clip of its own: one being exported asks before cancelling)
props.path = "/clips/d.MP4"
bindings.export()
s = sent()
expect("nothing set: no size", s.resolution, nil)
expect("nothing set: no encoder mode", s.encode, nil)
expect("nothing set: no interpolation", s.interpolation, nil)
messages["export-setting"]("video", "size", "1080")
messages["export-setting"]("video", "encode", "quality")
messages["export-setting"]("video", "quality", "quickest")
messages["export-setting"]("video", "size", "8K")
messages["export-setting"]("photo", "quality", "highest")
expect("kept in gyroflow.conf", saved["gyroflow.export_size"], "1080")
expect("…the encoder", saved["gyroflow.export_encode"], "quality")
expect("…the photo's quality under its own key", saved["gyroflow.photo_quality"], "highest")
expect("a value it doesn't have: ignored", props["user-data/gyroflow/export_settings"].video.size, "1080")
props.path = "/clips/e.MP4"
bindings.export()
s = sent()
no_settings_flag("Shift+E quickest")
expect("Shift+E: the size", s.resolution, "1080")
expect("Shift+E: the encoder", s.encode, "quality")
expect("Shift+E quickest: no denoise", s.temporal_denoise, "off")
expect("Shift+E quickest: bilinear", s.interpolation, "bilinear")
expect("Shift+E: the clip still last", last():match("(%S+) $"), "/clips/e.MP4")
-- The Library's request carries its own
props["user-data/gyroflow/temporal_denoise"] = "off"
requests.req = { id = 1, clip = "/clips/a.MP4", output = "/out/a.mov", reply = "workspace", view = { denoise = true, temporal_denoise = "light" },
                 export = { size = "720", encode = "fast", quality = "highest" } }
messages["export-batch"]("req")
s = sent()
expect("batch: the Library's size", s.resolution, "720")
expect("batch: its encoder (fast: nothing)", s.encode, nil)
expect("batch highest: the full denoise over the recipe's light", s.temporal_denoise, "full")
expect("batch highest: Lanczos", s.engine_interpolation, "lanczos")
requests.req.export = nil
messages["export-batch"]("req")
s = sent()
expect("batch without settings: as before", s.resolution == nil and s.encode == nil and s.interpolation == nil, true)
expect("batch without settings: the recipe's light", s.temporal_denoise, "light")
requests.photo = { id = 3, clip = "/clips/c.MP4", t = 4.5, reply = "workspace", view = {}, export = { size = "1080", quality = "quickest" } }
messages["photo-at"]("photo")
s = sent()
expect("photo-at: the Library's size", s.resolution, "1080")
expect("photo-at quickest: not cleaned up", s.still_radius, 0)
requests.photo.export = nil
messages["photo-at"]("photo")
expect("photo-at without the Library's: this player's photo settings (highest)", sent().still_radius, 7)

-- The underwater colour (#273: the dlogm plugin's curve "uw"): every route hands it over, a video's numbers measured
-- by gyroflow-export as it plays, a photo's as the shader has them now; another clip's from its saved view
props["user-data/dlogm/look"] = "hdr"
props["user-data/dlogm/curve"] = "uw"
props["user-data/uwcolor"] = { uw = true, uw_strength = 0.6, uw_highlights = true }
props["glsl-shader-opts"] = { uw_dr = "0.2214", uw_db = "0.0335", uw_k = "0.5000", uw_ev = "1.4845", uw_s = "0.600", uw_hp = "2.5" }
props.path = "/clips/uw1.MP4"
bindings.export()
s = sent()
expect("uw Shift+E: the curve", s.curve, "uw")
expect("uw Shift+E: its strength", s.uw_strength, 0.6)
expect("uw Shift+E: its highlights", s.uw_highlights, true)
expect("uw Shift+E: numbers measured as it plays, not fixed", s.uw_params, nil)
requests.prev = { id = 4, clip = "/clips/uw1.MP4", output = "/m/y.afplink.mp4", reply = "workspace" }
messages.preview("prev")
s = sent()
expect("uw preview: the curve", s.curve, "uw")
expect("uw preview: its frame measured", s.uw_params, nil)
props["user-data/workspace"] = nil
props["time-pos"] = 8
props["user-data/gyroflow/live_photo"] = "live"
local before = #ran
bindings["photo-full"]()
a, s = last(), sent()
no_settings_flag("photo-full")
expect("uw photo: run", #ran > before, true)
expect("uw photo: --photo", has(a, "--photo 8.0000"), true)
expect("uw photo: the curve", s.curve, "uw")
expect("uw photo: the numbers on screen", table.concat(s.uw_params or {}, ","), "0.2214,0.0335,0.5,1.4845")
expect("uw photo: its strength", s.uw_strength, 0.6)
expect("uw photo: its SDR image", s.sdr_image, "macos")
props["user-data/dlogm/look"] = "off"
props.path = "/clips/uw2.MP4"
bindings.export()
s = sent()
expect("uw off (L): colours as recorded", s.curve, "none")
expect("uw off (L): no underwater colour", s.uw_strength, nil)
props["user-data/dlogm/look"], props["user-data/dlogm/curve"] = "hdr", ""
requests.req = { id = 5, clip = "/clips/uw3.MP4", output = "/out/uw3.mov", reply = "workspace", view = { uw = true, uw_strength = 0.5 } }
messages["export-batch"]("req")
s = sent()
expect("uw batch: the clip's", s.uw, true)
expect("uw batch: its strength", s.uw_strength, 0.5)
expect("uw batch: its highlights (none saved: no)", s.uw_highlights, false)
requests.req.view = { uw = false, uw_strength = 0.5 }
messages["export-batch"]("req")
s = sent()
expect("uw batch: off, nothing", s.uw == nil and s.uw_strength == nil, true)
requests.photo = { id = 6, clip = "/clips/uw3.MP4", t = 2, reply = "workspace", view = { uw = true, uw_highlights = true } }
messages["photo-at"]("photo")
s = sent()
expect("uw photo-at: the clip's", s.uw == true and s.uw_highlights == true, true)
expect("uw photo-at: its frame measured", s.uw_params, nil)

-- A 360 export (#320): the sphere as the player makes it (its width, its stitching distance) and what straight ahead
-- follows, with the look as it plays
props["user-data/dlogm/look"], props["user-data/dlogm/curve"] = "hdr", "ilog"
props["user-data/gyroflow/settings"] = { lens = "stabilized", sphere_width = 4096, stitching_distance = 3.5, level = "strong" }
props["user-data/view360"] = { available = true, stamped = true, heading = "north" }
props.path = "/clips/x.insv"
messages.export("360")
a, s = last(), sent()
no_settings_flag("360")
expect("360: --360", has(a, "--360"), true)
expect("360: the sphere's width (#320)", s.sphere_width, 4096)
expect("360: its stitching distance (#320)", s.stitching_distance, 3.5)
expect("360: straight ahead", s.heading, "north")
expect("360: the curve", s.curve, "ilog")
expect("360: not the flat warp's settings", s.level, nil)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
