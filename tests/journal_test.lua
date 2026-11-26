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
    osd_message = function() end,
    register_event = function(name, f) events[name] = events[name] or {}; table.insert(events[name], f) end,
}
-- mpv's overlays as its defaults.lua makes them: methods in a metatable; update sends every field of the overlay as
-- the command, and mpv refuses a function among them
local overlay_mt = {}
overlay_mt.__index = overlay_mt
local overlay_sent = {}
function overlay_mt.update(ov)
    local cmd = {}
    for k, v in pairs(ov) do
        if type(v) == "function" then error("disallowed Lua type found: function") end
        cmd[k] = v
    end
    cmd._name = "osd-overlay"
    overlay_sent[#overlay_sent + 1] = cmd
    return { ok = true }, nil
end
function overlay_mt.remove(ov) overlay_sent[#overlay_sent + 1] = { _name = "osd-overlay", id = ov.id, format = "none" } end
mp.create_osd_overlay = function(format)
    return setmetatable({ format = format, id = 1, data = "", res_x = 0, res_y = 720 }, overlay_mt)
end
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

-- A card for another plugin's action (the side panel's: notify_client hands the id over): recorded for it, not weak
J.shown_for("P1.gyroflow.7", "card:done")
J.shown_for("P1.gyroflow.7", "card:done")

-- An overlay drawn: mpv gets only the overlay's own fields (a function among them killed the plugin drawing), the
-- update's results come back, the draw is a show for the action going on; remove still there
clock = 400
bindings.still()
local ov = mp.create_osd_overlay("ass-events")
ov.data = "{\\an7}hello"
local draw_ok, draw_res = pcall(function() return ov:update() end)
expect("overlay drawn", draw_ok, true)
expect("overlay: its result back", type(draw_res) == "table" and draw_res.ok, true)
expect("overlay: no field of the journal's", rawget(ov, "update"), nil)
local sent_fn = false
for _, c in ipairs(overlay_sent) do for _, v in pairs(c) do if type(v) == "function" then sent_fn = true end end end
expect("overlay: nothing but data sent", sent_fn, false)
expect("overlay: its data sent", overlay_sent[1] and overlay_sent[1].data, "{\\an7}hello")
expect("overlay: remove kept", pcall(function() ov:remove() end), true)
local ov2 = mp.create_osd_overlay("ass-events")
expect("overlays share one metatable", getmetatable(ov2), getmetatable(ov))

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
local function count_for(k, n, a)
    local c = 0
    for _, l in ipairs(lines) do if field(l, "k") == k and field(l, "n") == n and field(l, "a") == a then c = c + 1 end end
    return c
end
expect("one show of a kind", count_for("show", "overlay:test", id), 1)
expect("a show after the window: nobody's", count("show", "osd"), 0)
expect("click named by the panel", find("act", "panel/export") ~= nil, true)
expect("click: one action (not the up)", count("act", "panel/export"), 1)
expect("the label is the action going on", a_seen ~= nil, true)
local proc = find("step", "proc:gyroflow-gpu-table")
expect("subprocess timed (started 4 ms into the handler)", proc and math.floor(field(proc, "d") + 0.5), 246)
expect("its callback the action's", cb_owner ~= nil and cb_owner == field(proc, "a"), true)
expect("load record", find("load") ~= nil, true)
expect("overlay drawn: a show for its action", count("show", "overlay:test"), 2)
local card = find("show", "card:done")
expect("card for its action", card and field(card, "a"), "P1.gyroflow.7")
expect("card: once", count("show", "card:done"), 1)
expect("card: not weak", card and card:find('"weak"') == nil, true)
for _, l in ipairs(lines) do
    expect("a line is an object", l:sub(1, 1) == "{" and l:sub(-1) == "}", true)
end

-- The journal broken (its property refused, as a fault of its own would): the plugins' calls go on as without it,
-- a plugin's own error still raised as its own
local set_native, get_native = mp.set_property_native, mp.get_property_native
local function broken(k) if tostring(k):find("afp/journal") then error("journal property broken") end end
mp.set_property_native = function(k, v) broken(k) return set_native(k, v) end
mp.get_property_native = function(k) broken(k) return get_native(k) end
local troubles0 = J._troubles()
local ran, timer_ran = false, false
mp.add_key_binding("x", "works", function(v) ran = v; mp.add_timeout(0, function() timer_ran = true end) return "result" end)
mp.add_key_binding("y", "fails", function() error("the plugin's own") end)
expect("broken: a key still runs, its result back", bindings.works("arg"), "result")
expect("broken: with its arguments", ran, "arg")
for _, cb in ipairs(timers) do cb() end
timers = {}
expect("broken: its timer still runs", timer_ran, true)
local ok_own, err_own = pcall(bindings.fails)
expect("broken: a plugin's error still its own", not ok_own and tostring(err_own):find("the plugin's own", 1, true) ~= nil, true)
expect("broken: step, shown, label, span don't raise", pcall(function()
    J.step("x"); J.shown("y"); J.label("z"); J.shown_for(nil, "w"); J.span("v")({ more = 1 }); J.owner()
end), true)
local ov3 = mp.create_osd_overlay("ass-events")
expect("broken: an overlay still drawn", pcall(function() ov3:update() end), true)
local f = io.open(J.file, "rb")
expect("broken: io.open still opens", f ~= nil, true)
if f then f:close() end
local nf, nerr, ncode = io.open(dir .. "/no/such/file", "rb")
expect("io.open: all three results back", nf == nil and type(nerr) == "string" and type(ncode) == "number", true)
expect("broken: said, counted", J._troubles() > troubles0, true)
mp.set_property_native, mp.get_property_native = set_native, get_native

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

-- The trace off (this first load: no journal.conf, no AFP_TRACE): nothing of it written
expect("trace off by default", J.trace, false)
expect("actor by default", J.actor, "user")
J.say("osd", "x"); J.note("set", { n = "a" })
expect("trace off: mark gives nothing", J.mark(), nil)
periodic()
lines = {}
for l in io.lines(J.file) do lines[#lines + 1] = l end
expect("trace off: no texts, notes, markers", count("text") + count("set") + count("mark"), 0)
expect("trace off: an action carries no file", (find("act", "test/still") or ""):find('"file"') == nil, true)

-- The usage trace on (AFP_TRACE=1, AFP_ACTOR=claude): the journal loaded again, as a plugin of its own
env.AFP_TRACE, env.AFP_ACTOR = "1", "claude"
package.loaded.journal = nil
mp.get_script_name = function() return "traced" end
local log_events = nil
mp.enable_messages = function(level) log_events = level end
local T = require 'journal'
expect("trace on", T.trace, true)
expect("actor", T.actor, "claude")
props.path, props["time-pos"], props.pause = "/Volumes/DJI_Inbox/A.MP4", 12.34567, true
props["script-opts"] = { ["footage_menu-version"] = "0.14.2" }
T.core(require 'registry')
expect("mpv's warnings asked for", log_events, "warn")
mp.add_key_binding("Ctrl+l", "level", function()
    mp.osd_message("Level: strong")
    mp.set_property_native("user-data/gyroflow/level", "strong")
end)
mp.set_property_native("user-data/gyroflow/level", "medium")
clock = 500
bindings.level()
-- The same setting published again: not a change
mp.set_property_native("user-data/gyroflow/level", "strong")
-- A table setting (the lens): as JSON; the underwater colour from its own table
mp.set_property_native("user-data/gyroflow/lens", { camera = "GoPro", lens = "Wide" })
mp.set_property_native("user-data/uwcolor", { uw = true, uw_strength = 0.7, stats = { n = 1 } })
mp.set_property_native("user-data/uwcolor", { uw = true, uw_strength = 0.5, stats = { n = 2 } })
-- mpv's log: a warning, the same again, an error; a debug line isn't asked for
local function logged(level, prefix, text) for _, f in ipairs(events["log-message"] or {}) do f({ level = level, prefix = prefix, text = text }) end end
logged("warn", "gyroflow", "gpu table late\n")
logged("warn", "gyroflow", "gpu table late\n")
logged("error", "cplayer", "Failed to open x.MP4.")
logged("v", "vo", "chatter")
-- An IPC client that says who it is: the actions not from a key are its
mp.set_property_native("user-data/afp/journal/ipc", "ipc")
T.act("pause", { via = "core" })
bindings.level()
mp.set_property_native("user-data/afp/journal/ipc", "")
T.act("play", { via = "core" })
T.say("card", "Exported", { kind = "done", a = "P1.gyroflow.9" })
T.note("answer", { n = "Name this moment", text = "Reef" })
T.note("tab", { n = "transform" })
local mid, shot = T.mark({ via = "key" })
T.mark_note(mid, { note = "the horizon tilts" })
T.flush()
lines = {}
for l in io.lines(T.file) do lines[#lines + 1] = l end
local function find_s(k, n, s_)
    for _, l in ipairs(lines) do
        if field(l, "k") == k and field(l, "s") == (s_ or "traced") and (not n or field(l, "n") == n) then return l end
    end
end
local function count_s(k, n)
    local c = 0
    for _, l in ipairs(lines) do if field(l, "k") == k and field(l, "s") == "traced" and (not n or field(l, "n") == n) then c = c + 1 end end
    return c
end
local run = find_s("run")
expect("run: who acted", run and field(run, "actor"), "claude")
expect("run: the trace on", run and run:find('"trace":true') ~= nil, true)
local lvl = find_s("act", "traced/level")
expect("action: the file", lvl and field(lvl, "file"), "/Volumes/DJI_Inbox/A.MP4")
expect("action: the position, to the ms", lvl and field(lvl, "pos"), 12.346)
expect("action: paused", lvl and lvl:find('"paused":true') ~= nil, true)
expect("IPC: its action", find_s("act", "pause") and field(find_s("act", "pause"), "actor"), "ipc")
expect("IPC: a key still the run's", (function()
    local n = 0
    for _, l in ipairs(lines) do if field(l, "n") == "traced/level" and field(l, "k") == "act" then n = n + 1; if l:find('"actor"') then return "ipc" end end end
    return n
end)(), 2)
expect("IPC: gone, the run's again", find_s("act", "play") and find_s("act", "play"):find('"actor"') == nil, true)
local osd = find_s("text", "osd")
expect("the OSD's words", osd and field(osd, "text"), "Level: strong")
expect("the OSD's words: its action's", osd and field(osd, "a"), lvl and field(lvl, "a"))
local sets = {}
for _, l in ipairs(lines) do if field(l, "k") == "set" and field(l, "s") == "traced" then sets[#sets + 1] = l end end
local function set_of(n, new)
    for _, l in ipairs(sets) do if field(l, "n") == n and (new == nil or field(l, "new") == new) then return l end end
end
expect("a setting first seen", set_of("gyroflow.level", "medium") ~= nil and set_of("gyroflow.level", "medium"):find('"old"') == nil, true)
local changed = set_of("gyroflow.level", "strong")
expect("a setting changed: old", changed and field(changed, "old"), "medium")
expect("a setting changed: by the key's action", changed and field(changed, "a"), lvl and field(lvl, "a"))
expect("published again: not a change", count_s("set", "gyroflow.level"), 2)
expect("a table setting as JSON", set_of("gyroflow.lens") ~= nil and set_of("gyroflow.lens"):find("Wide", 1, true) ~= nil, true)
expect("the underwater colour's strength changed", set_of("uwcolor.uw_strength", 0.5) and field(set_of("uwcolor.uw_strength", 0.5), "old"), 0.7)
expect("its other keys not again", count_s("set", "uwcolor.uw"), 1)
expect("its stats not a setting", count_s("set", "uwcolor.stats"), 0)
local warn = find_s("log")
expect("a warning", warn and field(warn, "prefix"), "gyroflow")
expect("its words, the newline off", warn and field(warn, "text"), "gpu table late")
expect("the same line again: once", count_s("log"), 2)
expect("the error", find_s("log") and (function() for _, l in ipairs(lines) do if field(l, "k") == "log" and field(l, "level") == "error" then return field(l, "text") end end end)(), "Failed to open x.MP4.")
local card2 = find_s("text", "card")
expect("a card's words, for its action", card2 and field(card2, "a"), "P1.gyroflow.9")
expect("an answer", find_s("answer") and field(find_s("answer"), "text"), "Reef")
expect("the tab", find_s("tab") and field(find_s("tab"), "n"), "transform")
local m = find_s("mark")
expect("a marker", m and field(m, "id"), mid)
expect("its picture beside the journal", shot and shot:sub(1, #dir) == dir and shot:match("mark%-[%d%-]+%.png$") ~= nil, true)
expect("the marker: the file and position", m and field(m, "pos"), 12.346)
expect("the marker: who", m and field(m, "actor"), "claude")
expect("its note", find_s("mark-note") and field(find_s("mark-note"), "note"), "the horizon tilts")
expect("the start-file's path", (function()
    for _, f in ipairs(events["start-file"] or {}) do f() end
    T.flush()
    for l in io.lines(T.file) do if field(l, "n") == "start-file" and field(l, "s") == "traced" then return field(l, "path") end end
end)(), "/Volumes/DJI_Inbox/A.MP4")
for _, l in ipairs(lines) do expect("a line is an object", l:sub(1, 1) == "{" and l:sub(-1) == "}", true) end

os.execute("rm -rf '" .. dir .. "'")
print(failures == 0 and ("ok: " .. checks .. " checks") or (failures .. " of " .. checks .. " failed"))
os.exit(failures == 0 and 0 or 1)
