-- gyroflow.conf as the app's defaults, case by case from tests/gyroflow_conf_cases.json: the gyroflow plugin
-- (mpv/scripts/gyroflow/main.lua, loaded over a fake mpv) publishes user-data/gyroflow/defaults, edges, edges_blur and
-- temporal_denoise from it; the Library's renders::defaults_of is checked against the same cases (cargo test in
-- library/), so the recipe the Library hashes says what the player renders. The fake read_options reads the conf as
-- mpv's player/lua/options.lua does. From the repository's folder: luajit tests/gyroflow_conf_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path

-- The JSON reader of tests/render_settings_test.lua: objects, arrays, strings (\" only), numbers, true, false, null
local NULL = {}
local function parse(s)
    local i = 1
    local function ws() i = s:find("[^ \t\r\n]", i) or #s + 1 end
    local value
    local function str()
        local j, out = i + 1, {}
        while true do
            local c = s:sub(j, j)
            if c == '"' then i = j + 1; return table.concat(out) end
            if c == "\\" then j = j + 1; c = s:sub(j, j) end
            out[#out + 1] = c
            j = j + 1
        end
    end
    function value()
        ws()
        local c = s:sub(i, i)
        if c == "{" then
            local o = {}
            i = i + 1; ws()
            if s:sub(i, i) == "}" then i = i + 1; return o end
            while true do
                ws(); local k = str(); ws(); i = i + 1
                local v = value()
                if v ~= NULL then o[k] = v end
                ws(); c = s:sub(i, i); i = i + 1
                if c == "}" then return o end
            end
        elseif c == "[" then
            local a = {}
            i = i + 1; ws()
            if s:sub(i, i) == "]" then i = i + 1; return a end
            while true do
                a[#a + 1] = value()
                ws(); c = s:sub(i, i); i = i + 1
                if c == "]" then return a end
            end
        elseif c == '"' then return str()
        elseif s:sub(i, i + 3) == "true" then i = i + 4; return true
        elseif s:sub(i, i + 4) == "false" then i = i + 5; return false
        elseif s:sub(i, i + 3) == "null" then i = i + 4; return NULL
        else
            local n = s:match("^-?[%d%.eE+-]+", i)
            i = i + #n
            return tonumber(n)
        end
    end
    return value()
end
local function show(v)
    if type(v) ~= "table" then return type(v) == "string" and string.format("%q", v) or tostring(v) end
    local keys = {}
    for k in pairs(v) do keys[#keys + 1] = k end
    table.sort(keys)
    local parts = {}
    for _, k in ipairs(keys) do parts[#parts + 1] = k .. "=" .. show(v[k]) end
    return "{" .. table.concat(parts, ",") .. "}"
end

-- mpv's options.lua read_options over a conf's lines: a line starting with # is a comment, key=value split at the
-- first =, nothing trimmed, an unknown key ignored; a boolean only yes or no, a number only a number, else the default
local function read_conf(o, lines)
    for _, line in ipairs(lines) do
        if line:sub(#line) == "\r" then line = line:sub(1, #line - 1) end
        if line:find("#") ~= 1 then
            local eq = line:find("=")
            if eq then
                local key, val = line:sub(1, eq - 1), line:sub(eq + 1)
                local d = o[key]
                if type(d) == "boolean" then
                    if val == "yes" then o[key] = true elseif val == "no" then o[key] = false end
                elseif type(d) == "number" then
                    if tonumber(val) then o[key] = tonumber(val) end
                elseif d ~= nil then
                    o[key] = val
                end
            end
        end
    end
end

-- The plugin's folder: what it looks for, and no Gyroflow preset (preset_horizon: no)
local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")
local function write(p, text) local f = io.open(p, "w"); f:write(text); f:close() end
write(dir .. "/libvs_gyroflow.dylib", "")
write(dir .. "/gyroflow.vpy", "")

-- The plugin loaded once for a conf: what it publishes
local function load(conf)
    local props = { platform = "darwin", vo = "gpu-next", ["mpv-version"] = "mpv 0.41.0", ["osd-dimensions"] = { w = 1920, h = 1080 },
                    ["display-width"] = 1920, ["display-height"] = 1080, pause = true }
    local mp = {
        msg = setmetatable({}, { __index = function() return function() end end }),
        get_script_directory = function() return dir end,
        get_script_name = function() return "gyroflow" end,
        get_property_native = function(name, def) local v = props[name]; if v == nil then return def end; return v end,
        get_property = function(name, def) local v = props[name]; if v == nil or type(v) == "table" then return def end; return tostring(v) end,
        get_property_number = function(name, def) return tonumber(props[name]) or def end,
        get_property_bool = function(name, def) local v = props[name]; if v == nil then return def end; return v == true end,
        set_property_native = function(name, v) props[name] = v; return true end,
        add_timeout = function() return { kill = function() end, is_enabled = function() return false end, resume = function() end } end,
        add_periodic_timer = function() return { kill = function() end, resume = function() end, is_enabled = function() return false end, stop = function() end } end,
        get_time = function() return 0 end,
        command_native = function() return nil end,
        command_native_async = function() return {} end,
        create_osd_overlay = function() return { update = function() end, remove = function() end } end,
    }
    mp.set_property = mp.set_property_native
    mp.set_property_number = mp.set_property_native
    mp.set_property_bool = mp.set_property_native
    setmetatable(mp, { __index = function() return function() end end })
    -- Every module loaded afresh, the plugin's own with this conf
    for name in pairs(package.loaded) do
        if not ({ _G = true, string = true, table = true, math = true, io = true, os = true, coroutine = true, package = true,
                  debug = true, bit = true, jit = true, ["jit.opt"] = true, ffi = true })[name] then package.loaded[name] = nil end
    end
    package.preload.mp = function() return mp end
    package.preload["mp.options"] = function()
        return { read_options = function(o, name) if name == "gyroflow" then read_conf(o, conf); o.gpu = "no"; o.workers = 1 end end }
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
            parse_json = function() return nil end,
            format_json = function() return "{}" end,
            getpid = function() return 1 end,
            get_env_list = function() return {} end,
            to_string = tostring,
        }
    end
    dofile("mpv/scripts/gyroflow/main.lua")
    return props
end

local f = assert(io.open("tests/gyroflow_conf_cases.json", "r"))
local doc = parse(f:read("*a"))
f:close()

local failures, checks = 0, 0
for _, case in ipairs(doc.cases) do
    local props = load(case.conf)
    local got = {
        defaults = props["user-data/gyroflow/defaults"],
        edges = props["user-data/gyroflow/edges"],
        edges_blur = props["user-data/gyroflow/edges_blur"],
        temporal_denoise = props["user-data/gyroflow/temporal_denoise"],
    }
    for k, want in pairs(case.want) do
        checks = checks + 1
        if show(got[k]) ~= show(want) then
            failures = failures + 1
            print(("FAIL %s: %s\n  got  %s\n  want %s"):format(case.name, k, show(got[k]), show(want)))
        end
    end
end

os.execute("rm -rf '" .. dir .. "'")
print(("%d checks, %d failed"):format(checks, failures))
os.exit((failures == 0 and checks >= 30) and 0 or 1)
