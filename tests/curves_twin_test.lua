-- The log curves, kept in several places, agree (#291: the player's curve "uw" reached gyroflow-export, which
-- didn't know it, "unknown curve uw"):
--   * every curve the dlogm plugin can play (mpv/scripts/dlogm/main.lua CURVES, and the ones it adds, CURVE.uw) is
--     handed to gyroflow-export as a curve it takes (vs-gyroflow/src/bin/gyroflow-export.rs CURVES), or mapped to one
--     on the way (export.lua as_it_plays, a saved view's): checked by running export.lua over a fake mpv, so a mapping
--     added there counts as well as the curve added to the export
--   * the export's "unknown curve" message names its curves
--   * every curve of the export has its LUT: luts/<id>_hlg.cube in the repository, or made by tools/make_hdr_lut.py in
--     each packager (install.sh, make_app.sh, make_release.sh, make_windows_app.sh), "uw" excepted (made per clip)
--   * the curves the cameras' metadata name (DJI_CURVES, GOPRO_CURVES) are the plugin's
--   * the Library's lists of curves: Source settings' (library/ui/app.js SRC_CURVES), what a clip's own file may say
--     (library/src/tags.rs), the thumbnails' (library/ui/albums.js ALBUM_CURVES: a LUT each)
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

-- The player's curves
local dlogm = read("mpv/scripts/dlogm/main.lua")
local player = {}
for id in block(dlogm, "\nlocal CURVES = {", "dlogm CURVES"):gmatch('id = "([%w_]+)"') do player[id] = true end
local logs = {}
for k in pairs(player) do logs[k] = true end
-- The ones added beside them (the underwater colour), not in Ctrl+l's round
for name in dlogm:gmatch("\nCURVE%.([%w_]+) = ") do player[name] = true end
for name in dlogm:gmatch('\nCURVE%["([%w_]+)"%] = ') do player[name] = true end
expect("the player has curves", next(player) ~= nil)
expect("the player's uw is among them (the curve #291 was about)", player.uw == true, sorted(player))

-- What the cameras' metadata says, as a curve: one of the plugin's
for _, name in ipairs({ "DJI_CURVES", "GOPRO_CURVES" }) do
    for id in block(dlogm, "local " .. name .. " = {", name):gmatch('"([%w_]+)"') do
        expect(name .. " names a curve the plugin has", player[id], id)
    end
end

-- gyroflow-export's
local exporter = read("vs-gyroflow/src/bin/gyroflow-export.rs")
local export = {}
for id in block(exporter, "const CURVES: &%[%(&str, &str%)%] = &", "gyroflow-export CURVES"):gmatch('%("([%w_]+)", "') do export[id] = true end
expect("gyroflow-export has curves", next(export) ~= nil)
-- Its message names them
local listed = exporter:match('unknown curve {c} %(([^)]*)%)')
expect("gyroflow-export's unknown-curve message is there", listed ~= nil)
if listed then
    local named = {}
    for id in listed:gmatch("[%w_]+") do if id ~= "none" then named[id] = true end end
    expect("gyroflow-export's message names its curves", sorted(named) == sorted(export), "message: " .. sorted(named) .. "; CURVES: " .. sorted(export))
end

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
for id in pairs(export) do
    if id ~= "uw" then
        local f = io.open("luts/" .. id .. "_hlg.cube", "r")
        if f then
            f:close()
        else
            expect("make_hdr_lut.py makes " .. id, hdr_lut:find("\ndef " .. id .. "%(") ~= nil)
            for _, p in ipairs(packagers) do expect(p .. " makes " .. id .. "_hlg.cube", made[p][id], "no luts/" .. id .. "_hlg.cube in the repository") end
        end
    end
end

-- export.lua over a fake mpv: what each player curve becomes on gyroflow-export's command line
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
    format_json = function() return "{}" end,
    parse_json = function(s) return requests[s] end,
    getpid = function() return 1 end,
}
package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
assert(loadfile("mpv/scripts/gyroflow/export.lua"))()
expect("export.lua binds Shift+E (export)", type(bindings.export) == "function")
expect("export.lua takes export-batch", type(messages["export-batch"]) == "function")

local function curve_arg()
    local args = ran[#ran] or {}
    for i = 1, #args do if args[i] == "--curve" then return args[i + 1] end end
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

-- The Library's lists of the log curves (a clip's own choice, not uw: the source's switch of its own)
local app = read("library/ui/app.js")
local src = {}
for id in block(app, "const SRC_CURVES = ", "SRC_CURVES"):gmatch('%["([%w_]+)", "') do if id ~= "auto" then src[id] = true end end
expect("Source settings offer the player's log curves (app.js SRC_CURVES)", sorted(src) == sorted(logs), "app.js: " .. sorted(src) .. "; player: " .. sorted(logs))
local tags = read("library/src/tags.rs")
local own = tags:match('"curve" => v%.as_str%(%)%.is_some_and%(|s| %[([^%]]*)%]')
expect("tags.rs knows which curves a clip's own file may name", own ~= nil)
local own_set = {}
for id in (own or ""):gmatch('"([%w_]+)"') do if id ~= "auto" then own_set[id] = true end end
expect("a clip's own file may name the player's log curves (tags.rs)", sorted(own_set) == sorted(logs), "tags.rs: " .. sorted(own_set) .. "; player: " .. sorted(logs))
local albums = read("library/ui/albums.js")
for id in (albums:match("const ALBUM_CURVES = (%b[])") or ""):gmatch('"([%w_]+)"') do
    expect("a thumbnail's curve (albums.js ALBUM_CURVES) has its LUT: " .. id, export[id] and id ~= "uw")
end

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
