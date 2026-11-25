-- The log curves are registry.json's (mpv/scripts/workspace/registry.json), read by the dlogm plugin, the side panel,
-- the right-click menu, gyroflow-export and the Library (tests/registry_test.py checks each reads it). What's still
-- apart, checked here (#291: the player's curve "uw" reached gyroflow-export, which didn't know it, "unknown curve uw"):
--   * every curve the dlogm plugin can play is handed to gyroflow-export as one of the registry's, or mapped to one on
--     the way (export.lua as_it_plays, a saved view's): checked by running export.lua over a fake mpv
--   * every curve's LUT (the registry's lut): luts/<lut> in the repository, or made by tools/make_hdr_lut.py in each
--     packager (install.sh, make_app.sh, make_release.sh, make_windows_app.sh); "uw" has none (made per clip)
--   * the curves the cameras' metadata name (DJI_CURVES, GOPRO_CURVES) are the registry's
--   * gyroflow-export's help names the log curves
-- From the repository's folder: luajit tests/curves_twin_test.lua

local failures, checks = 0, 0
local function expect(what, ok, detail)
    checks = checks + 1
    if not ok then
        failures = failures + 1
        print("FAIL " .. what .. (detail and (": " .. detail) or ""))
    end
end
local function read(path)
    local f = assert(io.open(path, "r"), "can't read " .. path)
    local t = f:read("*a")
    f:close()
    return t
end
local function set_of(list) local s = {} for _, v in ipairs(list) do s[v] = true end return s end
local function sorted(s) local l = {} for k in pairs(s) do l[#l + 1] = k end table.sort(l) return table.concat(l, " ") end
-- The text of `name = {` … to its closing brace (a table constructor's, nested ones within)
local function block(text, start_pattern, what)
    local _, e = text:find(start_pattern)
    assert(e, "can't find " .. what)
    local open = text:find("[{%[]", e)
    local close_of = { ["{"] = "}", ["["] = "]" }
    local o, c = text:sub(open, open), close_of[text:sub(open, open)]
    local depth = 0
    for i = open, #text do
        local ch = text:sub(i, i)
        if ch == o then depth = depth + 1 elseif ch == c then depth = depth - 1 end
        if depth == 0 then return text:sub(open, i) end
    end
    error("unclosed " .. what)
end

package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path
local registry = require 'registry'
local dlogm = read("mpv/scripts/dlogm/main.lua")
-- The player's curves, and the log ones (Ctrl+l's round, a source's curve)
local player, logs = {}, {}
for _, c in ipairs(registry.curves) do
    player[c.id] = true
    if c.log then logs[c.id] = true end
end
expect("the registry has curves", next(player) ~= nil)
expect("the player's uw is among them (the curve #291 was about)", player.uw == true and not logs.uw, sorted(player))

-- What the cameras' metadata says, as a curve: one of the plugin's
for _, name in ipairs({ "DJI_CURVES", "GOPRO_CURVES" }) do
    for id in block(dlogm, "local " .. name .. " = {", name):gmatch('"([%w_]+)"') do
        expect(name .. " names a curve the plugin has", player[id], id)
    end
end

-- gyroflow-export takes the registry's curves (vs-gyroflow/src/lists.rs); its help names the log ones
-- (its --help text: options.rs, one of gyroflow-export's parts)
local exporter = read("vs-gyroflow/src/bin/gyroflow-export/options.rs")
local export = player
local help = exporter:match("%-%-curve <curve>  the log curve: ([^\n]*)")
expect("gyroflow-export's help for --curve is there", help ~= nil)
for id in pairs(logs) do expect("gyroflow-export's help names " .. id, help and help:find(id .. ",", 1, true) ~= nil, help) end

-- Each export curve's LUT: in the repository, or made in every packager
local made = {}
local packagers = { "install.sh", "tools/make_app.sh", "tools/make_release.sh", "tools/make_windows_app.sh" }
for _, p in ipairs(packagers) do
    local text = read(p)
    local list = text:match('HDR_CURVES="([^"]*)"') or text:match("for curve in ([%w_ ]+); do%s+[^\n]*make_hdr_lut")
    made[p] = {}
    for id in (list or ""):gmatch("[%w_]+") do made[p][id] = true end
end
local hdr_lut = read("tools/make_hdr_lut.py")
for _, c in ipairs(registry.curves) do
    expect(c.id .. ": a LUT, or none only for uw (made per clip)", (c.lut ~= nil) == (c.id ~= "uw"))
    local id = c.id
    if c.lut then
        expect(c.id .. "'s LUT is luts/<id>_hlg.cube, as make_hdr_lut.py names it", c.lut == id .. "_hlg.cube", c.lut)
        local f = io.open("luts/" .. c.lut, "r")
        if f then
            f:close()
        else
            expect("make_hdr_lut.py makes " .. id, hdr_lut:find("\ndef " .. id .. "%(") ~= nil)
            for _, p in ipairs(packagers) do expect(p .. " makes " .. c.lut, made[p][id], "no luts/" .. c.lut .. " in the repository") end
        end
    end
end

-- export.lua over a fake mpv: what each player curve becomes in the settings gyroflow-export takes (--settings)
local props = { platform = "darwin" }
local messages, bindings, ran, requests = {}, {}, {}, {}
local mp = {
    msg = setmetatable({}, { __index = function() return function() end end }),
    get_property = function(name, def) local v = props[name]; if v == nil or type(v) == "table" then return def end return tostring(v) end,
    get_property_native = function(name) return props[name] end,
    get_property_number = function(name) return tonumber(props[name]) end,
    set_property = function(name, v) props[name] = v return true end,
    set_property_native = function(name, v) props[name] = v return true end,
    command_native = function() return nil end,
    command_native_async = function(cmd) if cmd.name == "subprocess" then table.insert(ran, cmd.args) end return #ran end,
    add_key_binding = function(_, name, fn) bindings[name] = fn end,
    register_script_message = function(name, fn) messages[name] = fn end,
    create_osd_overlay = function() return { update = function() end, remove = function() end } end,
    add_timeout = function() return { kill = function() end } end,
    add_periodic_timer = function() return { kill = function() end, resume = function() end, stop = function() end } end,
    get_script_directory = function() return "/plugins/gyroflow" end,
    get_script_name = function() return "gyroflow" end,
    get_time = function() return 0 end,
}
setmetatable(mp, { __index = function() return function() end end })
package.loaded['mp'] = mp
package.loaded['mp.utils'] = {
    join_path = function(a, b) return a .. "/" .. b end,
    file_info = function(path) if path:match("gyroflow%-export$") then return { is_file = true } end end,
    -- The settings a route hands over (--settings): the table itself, by a name
    format_json = function(t) requests[#requests + 1] = t; return "json" .. #requests end,
    parse_json = function(s) return requests[s] end,
    getpid = function() return 1 end,
}
assert(loadfile("mpv/scripts/gyroflow/export.lua"))()
expect("export.lua binds Shift+E (export)", type(bindings.export) == "function")
expect("export.lua takes export-batch", type(messages["export-batch"]) == "function")

local function curve_arg()
    local args = ran[#ran] or {}
    for i = 1, #args do
        if args[i] == "--curve" then return args[i + 1] end
        if args[i] == "--settings" then return (requests[tonumber(args[i + 1]:match("%d+"))] or {}).curve end
    end
end
for id in pairs(player) do
    -- Shift+E on a clip playing in that curve
    props["user-data/dlogm/look"], props["user-data/dlogm/curve"] = "hdr", id
    props["user-data/uwcolor"] = { uw = id == "uw", uw_strength = 1, uw_highlights = false }
    props.path = "/clips/" .. id .. ".MP4"
    local before = #ran
    if bindings.export then pcall(bindings.export) end
    local got = #ran > before and curve_arg() or nil
    expect("Shift+E in " .. id .. ": a curve gyroflow-export takes", got and (export[got] or got == "none"), "--curve " .. tostring(got))
    -- Another clip (the Library's batch) whose saved view has it
    if logs[id] then
        requests.req = { id = 1, clip = "/clips/b.MP4", output = "/out/b.mov", reply = "workspace", view = { curve = id } }
        before = #ran
        if messages["export-batch"] then pcall(messages["export-batch"], "req") end
        got = #ran > before and curve_arg() or nil
        expect("a saved view in " .. id .. ": a curve gyroflow-export takes", got and (export[got] or got == "none"), "--curve " .. tostring(got))
    end
end

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
