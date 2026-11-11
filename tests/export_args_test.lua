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
package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
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

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
