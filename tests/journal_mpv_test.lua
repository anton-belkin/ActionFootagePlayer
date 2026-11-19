-- The responsiveness journal (mpv/scripts/workspace/journal.lua) in a real mpv, no window: what its hooks hand mpv is
-- what the plugin gave (an overlay drawn: in 0.41 the journal's timed update, put on the overlay as a field, was sent
-- with it and refused, "disallowed Lua type found: function", and every plugin that drew died), and all mpv gives back
-- reaches the plugin. Run by tests/journal_mpv_test.sh (mpv --script=this, --vo=null --idle=yes).

local mp = require 'mp'
local utils = require 'mp.utils'
local msg = require 'mp.msg'

local root = os.getenv("AFP_TEST_ROOT") or "."
package.path = root .. "/mpv/scripts/workspace/?.lua;" .. package.path
local J = require 'journal'
local script = mp.get_script_name()

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        msg.error(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function finish()
    msg.info(failures == 0 and ("ok: " .. checks .. " checks") or (failures .. " of " .. checks .. " failed"))
    mp.commandv("quit", failures == 0 and "0" or "1")
end

expect("journal on", J.enabled, true)

-- An overlay: drawn, its result back, nothing of the journal's on it, removed
local ov = mp.create_osd_overlay("ass-events")
ov.data = "{\\an7}journal test"
ov.res_x, ov.res_y = 1280, 720
local ok, res, err = pcall(function() return ov:update() end)
expect("overlay drawn (was: disallowed Lua type found: function)", ok, true)
if not ok then msg.error(tostring(res)) end
expect("overlay: mpv's result back", ok and type(res) == "table" or err, true)
expect("overlay: no field of the journal's", rawget(ov, "update"), nil)
ov.compute_bounds = true
ok, res = pcall(function() return ov:update() end)
expect("overlay: compute_bounds result back", ok and type(res) == "table" and res.x0 ~= nil, true)
expect("overlay removed", pcall(function() ov:remove() end), true)
-- (mpv's own user of overlays: the legacy call)
expect("set_osd_ass", pcall(mp.set_osd_ass, 1280, 720, "x"), true)

-- An OSD message
expect("osd_message", pcall(mp.osd_message, "journal test", 0.1), true)

-- A subprocess, waited for: mpv's result whole
local r = mp.command_native({ name = "subprocess", args = { "/bin/echo", "hi" }, capture_stdout = true, playback_only = false })
expect("subprocess: status", type(r) == "table" and r.status, 0)
expect("subprocess: stdout", type(r) == "table" and r.stdout, "hi\n")
-- A command that fails: its default and error back
local def, cerr = mp.command_native({ name = "no-such-command" }, "default")
expect("failed command: default back", def, "default")
expect("failed command: error back", type(cerr), "string")

-- Files: all of what they return
expect("file_info", type(utils.file_info(root)), "table")
local fi, ferr = utils.file_info(root .. "/no/such")
expect("file_info: error back", fi == nil and type(ferr) == "string", true)
expect("readdir", type(utils.readdir(root, "files")), "table")
local nf, nerr, ncode = io.open(root .. "/no/such", "rb")
expect("io.open: three results back", nf == nil and type(nerr) == "string" and type(ncode) == "number", true)

-- A key: its handler runs as an action, a timer from it runs; then async subprocesses, one aborted
local key_ran, timer_ran = false, false
local key_ov = mp.create_osd_overlay("ass-events")
mp.add_key_binding("ctrl+alt+j", "probe", function()
    key_ran = true
    key_ov.data = "{\\an7}drawn for a key"
    key_ov:update()
    mp.add_timeout(0.01, function() timer_ran = true end)
end)
local tm = mp.add_timeout(10, function() end)
expect("timer: killable", pcall(function() tm:kill() end), true)

mp.add_timeout(0.1, function() mp.commandv("keypress", "ctrl+alt+j") end)
mp.add_timeout(0.5, function()
    expect("key ran", key_ran, true)
    expect("its timer ran", timer_ran, true)
    local got = nil
    local h = mp.command_native_async({ name = "subprocess", args = { "/bin/echo", "async" }, capture_stdout = true, playback_only = false },
        function(success, result) got = { success, type(result) == "table" and result.stdout } end)
    expect("async: handle back", h ~= nil, true)
    -- (mpv's handle: abort_async_command takes it; this mpv doesn't stop a running subprocess, journal or not)
    local h2 = mp.command_native_async({ name = "subprocess", args = { "/bin/sleep", "0.2" }, playback_only = false }, function() end)
    expect("async: abort takes the handle", pcall(mp.abort_async_command, h2), true)
    mp.add_timeout(1, function()
        expect("async: callback", got and got[1], true)
        expect("async: stdout", got and got[2], "async\n")
        -- What the journal wrote: the key's action, the overlay drawn, the subprocesses timed, no trouble of its own
        J.flush()
        local text = ""
        local f = io.open(J.file, "rb")
        if f then text = f:read("*a") f:close() end
        local function has(pat) return text:find(pat, 1, true) ~= nil end
        expect("journal: the key's action", has('"n":"' .. script .. '/probe"'), true)
        expect("journal: the overlay drawn", has('"n":"overlay:' .. script .. '"'), true)
        expect("journal: the subprocess", has('"n":"proc:echo"'), true)
        expect("journal: no trouble of its own", J._troubles(), 0)
        finish()
    end)
end)
mp.add_timeout(15, function() msg.error("timed out") failures = failures + 1 finish() end)
