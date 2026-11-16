-- The photo rendered ahead of Save (mpv/scripts/gyroflow/export.lua's pr, docs/photo-upscaling.md "The render ahead")
-- over a fake mpv whose timers and subprocesses the test runs by hand: a render starts a moment after the frame is
-- paused in Save frame with an upscaler, its preview goes over the frame, Save writes it, a change or playing drops
-- it, Save while it renders waits for it, Cancel keeps it dropped until something changes.
-- From the repository's folder: luajit tests/photo_render_test.lua

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local props = {
    platform = "darwin", path = "/clips/c.MP4", pause = true, ["time-pos"] = 12.5,
    ["osd-dimensions"] = { w = 1600, h = 900, ml = 0, mr = 0, mt = 0, mb = 0 },
    ["user-data/gyroflow/crop"] = { active = true, frame = { x = 0.1, y = 0.1, w = 0.5, h = 0.5 }, camera = { yaw = 0, pitch = 0, roll = 0, zoom = 0.78 } },
    ["user-data/gyroflow/photo_upscaler"] = "swinir",
    ["user-data/gyroflow/filter"] = "stabilized",
}
local observers, events, messages, timers, runs, commands = {}, {}, {}, {}, {}, {}
local function set(name, v)
    props[name] = v
    for _, o in ipairs(observers) do if o.name == name then o.fn(name, v) end end
end
local mp = {
    msg = { warn = function() end, info = function() end, verbose = function() end, debug = function() end, error = function() end },
    get_property = function(name, def) local v = props[name]; if v == nil then return def end return tostring(v) end,
    get_property_native = function(name) return props[name] end,
    get_property_number = function(name) return tonumber(props[name]) end,
    set_property = function(name, v) props[name] = v return true end,
    set_property_native = function(name, v) props[name] = v return true end,
    command_native = function(c) if type(c) == "table" and c[1] then table.insert(commands, c[1]) end return nil end,
    command_native_async = function(cmd, cb) if cmd.name == "subprocess" then table.insert(runs, { args = cmd.args, cb = cb }) end return #runs end,
    abort_async_command = function() end,
    commandv = function() end,
    register_event = function(name, fn) events[name] = fn end,
    observe_property = function(name, _, fn) table.insert(observers, { name = name, fn = fn }) end,
    add_key_binding = function() end,
    register_script_message = function(name, fn) messages[name] = fn end,
    create_osd_overlay = function() return { update = function() end, remove = function() end } end,
    osd_message = function() end,
    add_timeout = function(_, fn) local t = { fn = fn, on = true }; t.kill = function() t.on = false end; table.insert(timers, t); return t end,
    add_periodic_timer = function() return { kill = function() end, resume = function() end, stop = function() end } end,
    get_script_directory = function() return "/plugins/gyroflow" end,
    get_script_name = function() return "gyroflow" end,
    get_time = function() return 0 end,
}
local utils = {
    join_path = function(a, b) return a .. "/" .. b end,
    file_info = function(path) if path:match("gyroflow%-export$") then return { is_file = true } end end,
    format_json = function() return "{}" end,
    parse_json = function() return nil end,
    getpid = function() return 1 end,
}
package.loaded['mp'] = mp
package.loaded['mp.utils'] = utils
package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
assert(loadfile("mpv/scripts/gyroflow/export.lua"))()

local function fire()
    local due = timers
    timers = {}
    for _, t in ipairs(due) do if t.on then t.fn() end end
end
local function args(run) return " " .. table.concat(run and run.args or {}, " ") .. " " end
local function has(a, part) return a:find(" " .. part .. " ", 1, true) ~= nil end
local function render() return props["user-data/gyroflow/photo_render"] or {} end
local function overlay(n) local c = 0; for _, x in ipairs(commands) do if x == n then c = c + 1 end end return c end
local function finish(run, label)
    run.cb(true, { status = 0, stderr = "c.MP4: photo\n  dlogm: SDR NBCU, HDR HLG · 1920x1080 · " .. label .. " · rendered, not saved\n  -> /tmp/r\n" })
end

-- Paused in Save frame with SwinIR-L: nothing yet, a render a moment later
set("user-data/gyroflow/crop", props["user-data/gyroflow/crop"])
expect("not at once", #runs, 0)
fire()
expect("a render a moment later", #runs, 1)
local a = args(runs[1])
expect("into a folder of its own", a:find(" %-%-render%-to [^ ]*afp%-photo%-render%-1%-1 ") ~= nil, true)
expect("its preview the frame's size on screen", has(a, "--preview-size 800x450"), true)
expect("the upscaler chosen", has(a, "--upscale swinir"), true)
expect("not into _Stills", a:find("--out", 1, true), nil)
expect("no motion", a:find("--live", 1, true), nil)
expect("running", render().state, "running")

-- Done: shown over the frame
finish(runs[1], "stretched 2.50x, SwinIR-L (draws detail)")
expect("ready", render().state, "ready")
expect("what it is", render().label, "stretched 2.50x, SwinIR-L (draws detail)")
expect("shown over the frame", overlay("overlay-add"), 1)

-- Save: from the render, no frame rendered again
messages.photo()
local s = args(runs[2])
expect("Save writes the render", s:find(" %-%-from%-render [^ ]*afp%-photo%-render%-1%-1 ") ~= nil, true)
expect("no screenshot for it", s:find("--image", 1, true), nil)
expect("into its place", s:find("--render-to", 1, true), nil)

-- The frame moved: dropped at once, rendered again a moment later
set("user-data/gyroflow/crop", { active = true, frame = { x = 0.2, y = 0.1, w = 0.5, h = 0.5 }, camera = { yaw = 0, pitch = 0, roll = 0, zoom = 0.78 } })
expect("hidden at once", overlay("overlay-remove"), 1)
expect("dropped at once", render().state, nil)
fire()
expect("rendered again", #runs, 3)

-- Save while it renders: waits for it, one render
messages.photo()
expect("no second run", #runs, 3)
expect("saving when done", render().saving, true)
finish(runs[3], "stretched 2.50x, SwinIR-L (draws detail)")
expect("saved when done", args(runs[4]):find("--from-render", 1, true) ~= nil, true)

-- Playing on: dropped, nothing rendered while it plays
set("pause", false)
expect("dropped when it plays", render().state, nil)
fire()
expect("nothing while playing", #runs, 4)

-- Paused again: a render; Cancel drops it and it stays dropped
set("pause", true)
fire()
expect("paused again: a render", #runs, 5)
messages["photo-render-cancel"]()
expect("cancelled", render().state, nil)
set("pause", true)
fire()
expect("stays cancelled", #runs, 5)
-- Until something changes
set("user-data/gyroflow/photo_upscaler", "realesrgan")
fire()
expect("another upscaler: a render", #runs, 6)
expect("with it", has(args(runs[6]), "--upscale realesrgan"), true)

-- None: no render
set("user-data/gyroflow/photo_upscaler", "none")
fire()
expect("none: nothing", #runs, 6)
expect("none: dropped", render().state, nil)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
