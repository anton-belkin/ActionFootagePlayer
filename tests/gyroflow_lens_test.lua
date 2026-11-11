-- The output's lens over a clip's own Gyroflow project (#196), in the gyroflow plugin (mpv/scripts/gyroflow/main.lua)
-- over a fake mpv: the bundle saved for the clip (its .afp.json's view, as the workspace plugin publishes it) or the
-- Lens drop-down's pick reaches the filter's arguments (its user-data JSON, which the GPU path's gyroflow-gpu-table
-- --player request is made from): a projection in place of the project's (linear too), the project's own (none given),
-- the lens as recorded. From the repository's folder: luajit tests/gyroflow_lens_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- JSON stand-ins: Lua table literals
local function serialize(v)
    if type(v) == "table" then
        local parts = {}
        for k, x in pairs(v) do
            parts[#parts + 1] = "[" .. (type(k) == "string" and string.format("%q", k) or tostring(k)) .. "]=" .. serialize(x)
        end
        return "{" .. table.concat(parts, ",") .. "}"
    end
    return type(v) == "string" and string.format("%q", v) or tostring(v)
end
local function parse(s)
    local f = loadstring("return " .. (s or ""))
    if not f then return nil end
    local ok, v = pcall(f)
    return ok and v or nil
end

-- A folder with the clip, its Gyroflow project, and the plugin's files the script looks for
local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")
local function write(p, text) local f = io.open(p, "w"); f:write(text); f:close() end
local clip = dir .. "/GX010001.MP4"
write(clip, "")
write(dir .. "/GX010001.gyroflow", serialize({ video_info = { width = 3840, height = 2160, fps = 50 },
                                                 stabilization = { output_projection = "Equidistant", horizon_lock_amount = 0 },
                                                 output = { output_width = 3840, output_height = 2160 } }))
write(dir .. "/libvs_gyroflow.dylib", "")
write(dir .. "/gyroflow.vpy", "")

-- The fake mpv: properties, observers, events, hooks, script messages; anything else does nothing
local props = { platform = "darwin", path = clip, filename = "GX010001.MP4", vo = "gpu-next", ["mpv-version"] = "mpv 0.41.0",
                ["osd-dimensions"] = { w = 1920, h = 1080 }, ["display-width"] = 1920, ["display-height"] = 1080, pause = true }
local observers, events, hooks, messages = {}, {}, {}, {}
local function changed(name)
    for _, o in ipairs(observers) do
        if o.name == name or name:sub(1, #o.name + 1) == o.name .. "/" then o.fn(o.name, props[o.name]) end
    end
end
local mp = {
    msg = setmetatable({}, { __index = function() return function() end end }),
    get_script_directory = function() return dir end,
    get_script_name = function() return "gyroflow" end,
    get_property_native = function(name, def) local v = props[name]; if v == nil then return def end; return v end,
    get_property = function(name, def) local v = props[name]; if v == nil or type(v) == "table" then return def end; return tostring(v) end,
    get_property_number = function(name, def) return tonumber(props[name]) or def end,
    get_property_bool = function(name, def) local v = props[name]; if v == nil then return def end; return v == true end,
    -- The clip's own filter chain is the chain while it plays
    set_property_native = function(name, v) props[name] = v; if name == "file-local-options/vf" then props.vf = v end; changed(name); return true end,
    observe_property = function(name, _, fn) observers[#observers + 1] = { name = name, fn = fn } end,
    register_event = function(name, fn) events[name] = events[name] or {}; table.insert(events[name], fn) end,
    add_hook = function(name, prio, fn) hooks[#hooks + 1] = { name = name, prio = prio, fn = fn } end,
    register_script_message = function(name, fn) messages[name] = fn end,
    add_timeout = function(_, fn) local t = { kill = function() end, is_enabled = function() return false end, resume = function() end }; fn(); return t end,
    add_periodic_timer = function() return { kill = function() end, resume = function() end, is_enabled = function() return false end, stop = function() end } end,
    get_time = function() return 0 end,
    command_native = function() return nil end,
    command_native_async = function() return {} end,
    create_osd_overlay = function() return { update = function() end, remove = function() end } end,
}
mp.set_property = function(name, v) props[name] = v; changed(name); return true end
mp.set_property_number = mp.set_property_native
mp.set_property_bool = mp.set_property_native
setmetatable(mp, { __index = function() return function() end end })
package.preload.mp = function() return mp end
package.preload["mp.options"] = function()
    return { read_options = function(o, name) if name == "gyroflow" then o.gpu = "no"; o.workers = 1 end end }
end
package.preload["mp.utils"] = function()
    return {
        join_path = function(a, b) return a .. "/" .. b end,
        split_path = function(p) return p:match("^(.*/)([^/]*)$") end,
        file_info = function(p)
            local f = io.open(p, "r")
            if not f then return nil end
            f:close()
            return { is_file = true, is_dir = false, size = 0, mtime = 0 }
        end,
        readdir = function() return {} end,
        parse_json = parse,
        format_json = serialize,
        getpid = function() return 1 end,
        get_env_list = function() return {} end,
        to_string = tostring,
    }
end

dofile("mpv/scripts/gyroflow/main.lua")

-- The clip loaded (the on_load hook), with what the workspace plugin publishes for it: view, the bundle saved, or none
local function load(view)
    props["user-data/workspace"] = { path = clip, clip = clip, view = view }
    props["file-local-options/vf"], props.vf = nil, nil
    for _, h in ipairs(hooks) do
        if h.name == "on_load" then h.fn({ defer = function() end, cont = function() end }) end
    end
end
-- The filter's arguments as set now
local function filter()
    for _, f in ipairs(props.vf or {}) do
        if f.name == "vapoursynth" then return parse(f.params["user-data"]) end
    end
end

-- No bundle saved: the project's own lens, corrected; no projection given
load(nil)
local ud = filter()
expect("a project clip: stabilized with it", ud and ud.project, dir .. "/GX010001.gyroflow")
expect("none saved: the project's own projection", ud and ud.projection, nil)
expect("…the lens corrected", ud and ud.lens, "stabilized")
expect("published: the project's own", props["user-data/gyroflow/projection"], "project")
expect("published: corrected", props["user-data/gyroflow/lens_mode"], "corrected")

-- A bundle saved with a fisheye: over the project, as for any clip
load({ view = "normal", level = "medium", projection = "stereographic", lens = "corrected" })
ud = filter()
expect("saved stereographic: given (1)", ud and ud.projection, 1)
expect("…stabilized", ud and ud.lens, "stabilized")
-- Linear in place of the project's equidistant: given too (0), not left to the project
load({ view = "normal", projection = "linear", lens = "corrected" })
ud = filter()
expect("saved linear over a project: given (0)", ud and ud.projection, 0)
-- The project's own, saved
load({ view = "normal", projection = "project", lens = "corrected" })
ud = filter()
expect("saved project's own: none given", ud and ud.projection, nil)
-- As recorded, saved
load({ view = "normal", projection = "project", lens = "as-recorded" })
ud = filter()
expect("saved as recorded: the lens left as recorded", ud and ud.lens, "recorded")
expect("published: as recorded", props["user-data/gyroflow/lens_mode"], "as-recorded")
-- The stabilization off over the project, the lens corrected: the lens alone, in its projection
load({ view = "off", projection = "equisolid", lens = "corrected" })
ud = filter()
expect("off: the lens alone", ud and ud.lens, "only")
expect("…in the projection picked (3)", ud and ud.projection, 3)

-- The Lens drop-down's picks while it plays (none saved): the filter set up again with them
load(nil)
messages.lens("orthographic")
ud = filter()
expect("picked orthographic: given (4)", ud and ud.projection, 4)
expect("published", props["user-data/gyroflow/projection"], "orthographic")
messages.lens("as-recorded")
ud = filter()
expect("picked as recorded", ud and ud.lens, "recorded")
messages.lens("stereographic")
ud = filter()
expect("from as recorded to a projection: corrected", ud and ud.lens, "stabilized")
expect("…in it (1)", ud and ud.projection, 1)
messages.lens("project")
ud = filter()
expect("picked the project's own: none given", ud and ud.projection, nil)
-- Not the session's: a project clip's pick doesn't carry over to clips without one
expect("the session's projection untouched", (props["user-data/gyroflow/session"] or {}).projection, "linear")
expect("the session's lens untouched", (props["user-data/gyroflow/session"] or {}).lens, "corrected")

-- Without a project: linear is the preset's (none given), "project" isn't a choice
os.remove(dir .. "/GX010001.gyroflow")
load(nil)
messages.lens("project")
ud = filter()
expect("no project: no project's own", props["user-data/gyroflow/projection"], "linear")
messages.lens("stereographic")
ud = filter()
-- (no motion data without the probe: nothing to stabilize with here; the projection is what's published)
expect("no project: a projection sticks to the session", (props["user-data/gyroflow/session"] or {}).projection, "stereographic")

os.execute("rm -rf '" .. dir .. "'")
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
