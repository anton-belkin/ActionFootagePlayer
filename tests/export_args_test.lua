-- The gyroflow plugin's exports (mpv/scripts/gyroflow/export.lua) over a fake mpv: every route hands gyroflow-export
-- the clip's clean-up as it plays (the temporal denoise, its Stronger, the water's color fringes), the clip playing
-- (Shift+E) from the plugin's state, another clip (the Library's batch, a photo of a mark) from its saved view, else
-- the session's, else the app's defaults; Performance & Quality's temporal denoise says how much (light, full, off).
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
    format_json = function() return "{}" end,
    parse_json = function(s) return requests[s] end,
    getpid = function() return 1 end,
}
package.loaded['mp'] = mp
package.loaded['mp.utils'] = utils
package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path
assert(loadfile("mpv/scripts/gyroflow/export.lua"))()

-- The arguments of the last gyroflow-export run, as a string, and whether it has a run of them
local function last() return " " .. table.concat(ran[#ran] or {}, " ") .. " " end
local function has(args, part) return args:find(" " .. part .. " ", 1, true) ~= nil end

-- The Library's batch: a clip's saved view
local function batch(view)
    requests.req = { id = 1, clip = "/clips/a.MP4", output = "/out/a.mov", reply = "workspace", view = view }
    messages["export-batch"]("req")
    return last()
end
props["user-data/gyroflow/session"] = { denoise = false, denoise_strong = false, chromatic = true }
props["user-data/gyroflow/defaults"] = { denoise = true, denoise_strong = false, chromatic = true }
props["user-data/gyroflow/temporal_denoise"] = "full"

local a = batch({ denoise = true, denoise_strong = true, chromatic = false })
expect("batch: the view's denoise, full", has(a, "--denoise full"), true)
expect("batch: its Stronger", has(a, "--denoise-strong"), true)
expect("batch: its fringes as recorded", has(a, "--chromatic no"), true)
a = batch({})
expect("batch: nothing saved, the session's denoise (off)", has(a, "--denoise"), false)
expect("batch: nothing saved, the session's fringes", has(a, "--chromatic yes"), true)
props["user-data/gyroflow/session"] = {}
a = batch({})
expect("batch: no session either, the defaults' denoise", has(a, "--denoise full"), true)
props["user-data/gyroflow/temporal_denoise"] = "light"
a = batch({ denoise = true })
expect("batch: Light on this computer", has(a, "--denoise light"), true)
expect("batch: no Stronger", has(a, "--denoise-strong"), false)
props["user-data/gyroflow/temporal_denoise"] = "off"
a = batch({ denoise = true, denoise_strong = true })
expect("batch: Off on this computer, no denoise", has(a, "--denoise"), false)
expect("batch: nor its Stronger", has(a, "--denoise-strong"), false)
expect("batch: the clip last", a:match("(%S+) $"), "/clips/a.MP4")

-- Shift+E on the clip playing: the plugin's state, whatever the session says
props["user-data/gyroflow/temporal_denoise"] = "full"
props.path = "/clips/b.MP4"
props["user-data/gyroflow/filter"] = "stabilized"
props["user-data/gyroflow/denoise"] = true
props["user-data/gyroflow/denoise_strong"] = false
props["user-data/gyroflow/chromatic"] = true
bindings.export()
a = last()
expect("Shift+E: gyroflow-export", a:find("gyroflow-export", 1, true) ~= nil, true)
expect("Shift+E: the clip's denoise", has(a, "--denoise full"), true)
expect("Shift+E: no Stronger", has(a, "--denoise-strong"), false)
expect("Shift+E: fringes taken out", has(a, "--chromatic yes"), true)
expect("Shift+E: the clip last", a:match("(%S+) $"), "/clips/b.MP4")

-- A preview of a moment, as the clip plays
props["user-data/gyroflow/denoise"] = false
props["user-data/gyroflow/chromatic"] = false
requests.prev = { id = 2, clip = "/clips/b.MP4", output = "/m/x.afplink.mp4", reply = "workspace" }
messages.preview("prev")
a = last()
expect("preview: --preview still", has(a, "--preview still"), true)
expect("preview: the denoise off as it plays", has(a, "--denoise"), false)
expect("preview: fringes as recorded as it plays", has(a, "--chromatic no"), true)

-- A photo of a mark: its view's fringes (its motion, when it has one, is exported with its denoise)
props["user-data/gyroflow/session"] = { denoise = true, chromatic = true }
requests.photo = { id = 3, clip = "/clips/c.MP4", t = 4.5, reply = "workspace", view = { chromatic = false } }
messages["photo-at"]("photo")
a = last()
expect("photo-at: --photo", has(a, "--photo 4.5000"), true)
expect("photo-at: the view's fringes", has(a, "--chromatic no"), true)
expect("photo-at: the session's denoise for its motion", has(a, "--denoise full"), true)

-- The export's settings (docs/design/export-settings.md): the player's own, kept in gyroflow.conf by export-setting, and the
-- Library's, in its request
local saved = {}
require('platform').save_options = function(name, kv) for _, p in ipairs(kv) do saved[name .. "." .. p[1]] = p[2] end end
props["user-data/gyroflow/export_settings"] = nil
props["user-data/gyroflow/denoise"] = true
props["user-data/gyroflow/edges"] = "sphere"
-- (each a clip of its own: one being exported asks before cancelling)
props.path = "/clips/d.MP4"
bindings.export()
a = last()
expect("nothing set: no size", has(a, "--resolution"), false)
expect("nothing set: no encoder mode", has(a, "--encode"), false)
expect("nothing set: no interpolation", a:find("interpolation", 1, true), nil)
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
a = last()
expect("Shift+E: the size", has(a, "--resolution 1080"), true)
expect("Shift+E: the encoder", has(a, "--encode quality"), true)
expect("Shift+E quickest: no denoise", has(a, "--denoise"), false)
expect("Shift+E quickest: bilinear", has(a, "--gpu-interpolation bilinear"), true)
expect("Shift+E: the clip still last", a:match("(%S+) $"), "/clips/e.MP4")
-- The Library's request carries its own
props["user-data/gyroflow/temporal_denoise"] = "off"
requests.req = { id = 1, clip = "/clips/a.MP4", output = "/out/a.mov", reply = "workspace", view = { denoise = true, temporal_denoise = "light" },
                 export = { size = "720", encode = "fast", quality = "highest" } }
messages["export-batch"]("req")
a = last()
expect("batch: the Library's size", has(a, "--resolution 720"), true)
expect("batch: its encoder (fast: nothing)", has(a, "--encode"), false)
expect("batch highest: the full denoise over the recipe's light", has(a, "--denoise full"), true)
expect("batch highest: Lanczos", has(a, "--interpolation lanczos"), true)
requests.req.export = nil
messages["export-batch"]("req")
a = last()
expect("batch without settings: as before", has(a, "--resolution") or has(a, "--encode") or a:find("interpolation", 1, true) ~= nil, false)
expect("batch without settings: the recipe's light", has(a, "--denoise light"), true)
requests.photo = { id = 3, clip = "/clips/c.MP4", t = 4.5, reply = "workspace", view = {}, export = { size = "1080", quality = "quickest" } }
messages["photo-at"]("photo")
a = last()
expect("photo-at: the Library's size", has(a, "--resolution 1080"), true)
expect("photo-at quickest: not cleaned up", has(a, "--radius 0"), true)
requests.photo.export = nil
messages["photo-at"]("photo")
a = last()
expect("photo-at without the Library's: this player's photo settings (highest)", has(a, "--radius 7"), true)

-- The underwater colour (#273: the dlogm plugin's curve "uw"): every route hands it over, a video's numbers measured
-- by gyroflow-export as it plays, a photo's as the shader has them now; another clip's from its saved view
props["user-data/dlogm/look"] = "hdr"
props["user-data/dlogm/curve"] = "uw"
props["user-data/uwcolor"] = { uw = true, uw_strength = 0.6, uw_highlights = true }
props["glsl-shader-opts"] = { uw_dr = "0.2214", uw_db = "0.0335", uw_k = "0.5000", uw_ev = "1.4845", uw_s = "0.600", uw_hp = "2.5" }
props.path = "/clips/uw1.MP4"
bindings.export()
a = last()
expect("uw Shift+E: the curve", has(a, "--curve uw"), true)
expect("uw Shift+E: its strength", has(a, "--uw-strength 0.60"), true)
expect("uw Shift+E: its highlights", has(a, "--uw-highlights yes"), true)
expect("uw Shift+E: numbers measured as it plays, not fixed", has(a, "--uw-params"), false)
requests.prev = { id = 4, clip = "/clips/uw1.MP4", output = "/m/y.afplink.mp4", reply = "workspace" }
messages.preview("prev")
a = last()
expect("uw preview: the curve", has(a, "--curve uw"), true)
expect("uw preview: its frame measured", has(a, "--uw-params"), false)
props["user-data/workspace"] = nil
props["time-pos"] = 8
props["user-data/gyroflow/live_photo"] = "live"
local before = #ran
bindings["photo-full"]()
a = last()
expect("uw photo: run", #ran > before, true)
expect("uw photo: --photo", has(a, "--photo 8.0000"), true)
expect("uw photo: the curve", has(a, "--curve uw"), true)
expect("uw photo: the numbers on screen", has(a, "--uw-params 0.2214,0.0335,0.5000,1.4845"), true)
expect("uw photo: its strength", has(a, "--uw-strength 0.60"), true)
props["user-data/dlogm/look"] = "off"
props.path = "/clips/uw2.MP4"
bindings.export()
a = last()
expect("uw off (L): colours as recorded", has(a, "--curve none"), true)
expect("uw off (L): no underwater colour", has(a, "--uw-strength"), false)
props["user-data/dlogm/look"], props["user-data/dlogm/curve"] = "hdr", ""
requests.req = { id = 5, clip = "/clips/uw3.MP4", output = "/out/uw3.mov", reply = "workspace", view = { uw = true, uw_strength = 0.5 } }
messages["export-batch"]("req")
a = last()
expect("uw batch: the clip's", has(a, "--uw-color yes"), true)
expect("uw batch: its strength", has(a, "--uw-strength 0.50"), true)
expect("uw batch: its highlights (none saved: no)", has(a, "--uw-highlights no"), true)
requests.req.view = { uw = false, uw_strength = 0.5 }
messages["export-batch"]("req")
a = last()
expect("uw batch: off, nothing", has(a, "--uw-color") or has(a, "--uw-strength"), false)
requests.photo = { id = 6, clip = "/clips/uw3.MP4", t = 2, reply = "workspace", view = { uw = true, uw_highlights = true } }
messages["photo-at"]("photo")
a = last()
expect("uw photo-at: the clip's", has(a, "--uw-color yes") and has(a, "--uw-highlights yes"), true)
expect("uw photo-at: its frame measured", has(a, "--uw-params"), false)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
