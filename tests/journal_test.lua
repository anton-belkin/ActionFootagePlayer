-- The responsiveness journal's player part (mpv/scripts/workspace/journal.lua): with mpv stood in for, a key binding
-- is an action with its handler's time, the panel's label names it, steps and timers' callbacks belong to it, only the
-- first show of a kind counts, file operations are counted local or on a share, the lines written are JSON, the folder
-- is kept small; and what a record costs. From the repository's folder: luajit tests/journal_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- A small JSON writer and reader, as mp.utils' are
local function enc(v)
    local t = type(v)
    if t == "table" then
        local parts = {}
        for k, x in pairs(v) do parts[#parts + 1] = string.format("%q:%s", tostring(k), enc(x)) end
        return "{" .. table.concat(parts, ",") .. "}"
    elseif t == "string" then return string.format("%q", v)
    elseif t == "number" then return string.format("%.17g", v)
    elseif t == "boolean" then return tostring(v) end
    return "null"
end
local function field(line, k)
    local s = line:match('"' .. k .. '":"([^"]*)"')
    if s then return s end
    local n = line:match('"' .. k .. '":([%d.%-e]+)')
    return n and tonumber(n)
end

local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")

-- mpv, stood in for
local clock = 100.0
local props, observers, bindings, timers, events = {}, {}, {}, {}, {}
local mp = {
    get_time = function() return clock end,
    get_script_name = function() return "test" end,
    get_script_directory = function() return "." end,
    get_property_native = function(k) return props[k] end,
    set_property_native = function(k, v)
        props[k] = v
        for _, f in ipairs(observers[k] or {}) do f(k, v) end
    end,
    observe_property = function(k, _, f) observers[k] = observers[k] or {}; table.insert(observers[k], f) end,
    add_key_binding = function(key, name, fn) bindings[name or key] = fn end,
    add_forced_key_binding = function(key, name, fn) bindings[name or key] = fn end,
    add_timeout = function(_, cb) table.insert(timers, cb) return { kill = function() end } end,
    add_periodic_timer = function(_, cb) return { kill = function() end, cb = cb } end,
    command_native_async = function(cmd, cb) table.insert(timers, function() cb(true, { status = 0 }, nil) end) end,
    command_native = function() return { status = 0 } end,
    create_osd_overlay = function() return { update = function() end } end,
    osd_message = function() end,
    register_event = function(name, f) events[name] = events[name] or {}; table.insert(events[name], f) end,
}
local utils = {
    getpid = function() return 4242 end,
    format_json = enc,
    parse_json = function() return nil end,
    file_info = function(p)
        local f = io.open(p, "rb")
        if not f then return nil end
        local size = f:seek("end")
        f:close()
        return { size = size, mtime = os.time() }
    end,
    readdir = function() return {} end,
    join_path = function(a, b) return a .. "/" .. b end,
}
mp.utils = utils
package.loaded['mp'] = mp
package.loaded['mp.utils'] = utils

-- (the periodic timer the module starts: its flush)
local periodic
local add_periodic = mp.add_periodic_timer
mp.add_periodic_timer = function(s, cb) local t = add_periodic(s, cb); periodic = periodic or cb; return t end

local env = { AFP_JOURNAL_DIR = dir }
local getenv = os.getenv
os.getenv = function(k) if env[k] ~= nil then return env[k] end return getenv(k) end

local J = require 'journal'
expect("on", J.enabled, true)
expect("file in the folder", J.file:sub(1, #dir), dir)

-- A key: its binding is an action; its handler's time; a step in it, and one from a timer it started
mp.add_key_binding("ctrl+p", "still", function()
    clock = clock + 0.004
    J.step("inside")
    mp.add_timeout(0.1, function() J.step("later") end)
end)
local a_seen
mp.add_forced_key_binding("MBTN_LEFT", "click", function(info)
    J.label("panel/export")
    a_seen = J.owner()
end, { complex = true })

clock = 101
bindings.still()
clock = 101.5
J.shown("overlay:test")
J.shown("overlay:test")
clock = 102
for _, cb in ipairs(timers) do cb() end
timers = {}
-- A click: down is the action, up isn't
bindings.click({ event = "down", key_name = "MBTN_LEFT" })
bindings.click({ event = "up", key_name = "MBTN_LEFT" })
-- After the window, a show belongs to nobody
clock = 200
J.shown("osd")
-- A subprocess, timed, its callback the action's
clock = 300
bindings.still()
local cb_owner
mp.command_native_async({ name = "subprocess", args = { "/x/gyroflow-gpu-table", "--player" } }, function() cb_owner = J.owner() end)
clock = 300.25
for _, cb in ipairs(timers) do cb() end
timers = {}

-- File operations: on the share or not
props["user-data/afp/journal/net"] = nil
mp.set_property_native("user-data/afp/journal/net", { "/Volumes/DJI_Inbox", "/Volumes/ssd" })
expect("share", J.where("/Volumes/DJI_Inbox/a.MP4"), "nas")
expect("share itself", J.where("/Volumes/ssd"), "nas")
expect("not a prefix", J.where("/Volumes/ssd2/a"), "local")
expect("local", J.where("/Users/a/b"), "local")

periodic()
local lines = {}
for l in io.lines(J.file) do lines[#lines + 1] = l end
local function find(k, n)
    for _, l in ipairs(lines) do if field(l, "k") == k and (not n or field(l, "n") == n) then return l end end
end
local function count(k, n)
    local c = 0
    for _, l in ipairs(lines) do if field(l, "k") == k and (not n or field(l, "n") == n) then c = c + 1 end end
    return c
end

local act = find("act", "test/still")
expect("key action", act ~= nil, true)
expect("asked when the key went down (ms)", act and math.floor(field(act, "w") - J.now() + (clock - 101) * 1000 + 0.5), 0)
expect("its handler's time", act and math.floor(field(act, "d") + 0.5), 4)
expect("via", act and field(act, "via"), "key")
local id = act and field(act, "a")
expect("step inside", field(find("step", "inside") or "", "a"), id)
expect("step from its timer", field(find("step", "later") or "", "a"), id)
expect("one show of a kind", count("show", "overlay:test"), 1)
expect("a show after the window: nobody's", count("show", "osd"), 0)
expect("click named by the panel", find("act", "panel/export") ~= nil, true)
expect("click: one action (not the up)", count("act", "panel/export"), 1)
expect("the label is the action going on", a_seen ~= nil, true)
local proc = find("step", "proc:gyroflow-gpu-table")
expect("subprocess timed (started 4 ms into the handler)", proc and math.floor(field(proc, "d") + 0.5), 246)
expect("its callback the action's", cb_owner ~= nil and cb_owner == field(proc, "a"), true)
expect("load record", find("load") ~= nil, true)
for _, l in ipairs(lines) do
    expect("a line is an object", l:sub(1, 1) == "{" and l:sub(-1) == "}", true)
end

-- Cost: a record (format_json here is this test's Lua one; mpv's is C: the live run's "cost" records say)
local n = 20000
local t0 = os.clock()
for i = 1, n do J.step("bench", { d = 1.5 }) end
local us = (os.clock() - t0) / n * 1e6
print(string.format("journal.lua: %.2f µs a record (luajit, Lua JSON)", us))
expect("cheap", us < 50, true)
t0 = os.clock()
periodic()
print(string.format("journal.lua: %.1f ms to write %d records", (os.clock() - t0) * 1000, n))

os.execute("rm -rf '" .. dir .. "'")
print(failures == 0 and ("ok: " .. checks .. " checks") or (failures .. " of " .. checks .. " failed"))
os.exit(failures == 0 and 0 or 1)
