-- The agent plugin (mpv/scripts/agent/main.lua) over a fake mpv: a player window sets up the IPC socket and
-- session.json, and removes session.json on quit unless another player has it since; a batch of Action Footage
-- Library's (AFP_BATCH) touches neither, so a task run while a window is open leaves the agent its way in.
-- From the repository's folder: luajit tests/agent_test.lua (or lua 5.1 and later)

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local function read(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local s = f:read("*a")
    f:close()
    return s
end
local function write(path, text)
    local f = assert(io.open(path, "wb"))
    f:write(text)
    f:close()
end

-- A folder of its own for <data>
local DATA = os.tmpname()
os.remove(DATA)
assert(os.execute("mkdir -p '" .. DATA .. "'"))
local SESSION = DATA .. "/session.json"
local SOCKET = DATA .. "/mpv.sock"

-- The environment the plugin reads
local env = {}
local real_getenv = os.getenv
os.getenv = function(name) if env[name] ~= nil then return env[name] end return real_getenv(name) end

-- A fake mpv, enough for the plugin: properties, events, bindings, script messages, the subprocesses it runs
local function fake_mp(pid)
    local f = { props = { pid = pid, platform = "darwin", ["input-ipc-server"] = "", ["mpv-version"] = "mpv test" },
                events = {}, bindings = {}, messages = {}, set = {}, log_messages = false }
    f.mp = {
        msg = { warn = function() end, info = function() end, verbose = function() end, debug = function() end, error = function() end },
        get_property = function(name, def) local v = f.props[name]; if v == nil then return def end return tostring(v) end,
        get_property_native = function(name) return f.props[name] end,
        get_property_number = function(name) return tonumber(f.props[name]) end,
        set_property = function(name, v)
            f.set[name] = v
            f.props[name] = v
            -- mpv: a socket's file is made again by the one that sets it
            if name == "input-ipc-server" and v ~= "" then write(v, "socket of " .. pid) end
            return true
        end,
        set_property_native = function(name, v) f.set[name] = v; f.props[name] = v; return true end,
        command_native = function(cmd)
            if cmd[1] == "expand-path" then return cmd[2] end
            if cmd.name == "subprocess" then
                local ok = os.execute("'" .. table.concat(cmd.args, "' '") .. "'")
                return { status = (ok == true or ok == 0) and 0 or 1 }
            end
        end,
        command_native_async = function() end,
        commandv = function() end,
        enable_messages = function() f.log_messages = true end,
        register_event = function(name, fn) f.events[name] = f.events[name] or {}; table.insert(f.events[name], fn) end,
        observe_property = function() end,
        unobserve_property = function() end,
        add_key_binding = function(key, name, fn) f.bindings[name] = { key = key, fn = fn } end,
        add_forced_key_binding = function(key, name, fn) f.bindings[name] = { key = key, fn = fn } end,
        remove_key_binding = function(name) f.bindings[name] = nil end,
        register_script_message = function(name, fn) f.messages[name] = fn end,
        create_osd_overlay = function() return { update = function() end, remove = function() end } end,
        osd_message = function() end,
        input_enable_section = function() end,
        get_script_name = function() return "agent" end,
        get_time = function() return 0 end,
    }
    return f
end

local utils = {
    join_path = function(a, b) return a .. "/" .. b end,
    file_info = function(path) local f = io.open(path, "rb"); if f then f:close(); return {} end end,
    -- Flat tables are all the plugin writes
    format_json = function(t)
        local parts = {}
        for k, v in pairs(t) do
            table.insert(parts, ("%q:%s"):format(k, type(v) == "number" and tostring(v) or ("%q"):format(tostring(v))))
        end
        return "{" .. table.concat(parts, ",") .. "}"
    end,
    parse_json = function(s)
        local pid = s and s:match('"pid":(%d+)')
        return pid and { pid = tonumber(pid) } or nil
    end,
}

local function load_plugin(f)
    package.loaded['mp'] = f.mp
    package.loaded['mp.utils'] = utils
    package.loaded['mp.options'] = { read_options = function() end }
    package.loaded['mp.input'] = { get = function() end, terminate = function() end }
    assert(loadfile("mpv/scripts/agent/main.lua"))()
end

local function quit(f)
    for _, fn in ipairs(f.events.shutdown or {}) do fn() end
    -- mpv: the socket it made goes with it
    if f.props["input-ipc-server"] == SOCKET and read(SOCKET) == "socket of " .. f.props.pid then os.remove(SOCKET) end
end

env.AGENT_DATA = DATA

-- A player window: the socket, session.json, the note's key, the log kept for it
env.AFP_BATCH = false
local window = fake_mp(100)
load_plugin(window)
expect("window: socket set", window.set["input-ipc-server"], SOCKET)
expect("window: socket made", read(SOCKET), "socket of 100")
expect("window: session.json written", utils.parse_json(read(SESSION)) and utils.parse_json(read(SESSION)).pid, 100)
expect("window: user-data/agent", type(window.set["user-data/agent"]), "table")
expect("window: Ctrl+r bound", window.bindings.report and window.bindings.report.key, "Ctrl+r")
expect("window: log kept", window.log_messages, true)

-- A batch while the window's open: nothing of the agent's
env.AFP_BATCH = DATA .. "/request.json"
local batch = fake_mp(200)
load_plugin(batch)
expect("batch: socket not set", batch.set["input-ipc-server"], nil)
expect("batch: user-data/agent not set", batch.set["user-data/agent"], nil)
expect("batch: no shutdown handler", batch.events.shutdown, nil)
expect("batch: no bindings", next(batch.bindings), nil)
expect("batch: no script messages", next(batch.messages), nil)
expect("batch: no log kept", batch.log_messages, false)
quit(batch)
expect("after the batch: the window's socket", read(SOCKET), "socket of 100")
expect("after the batch: the window's session.json", utils.parse_json(read(SESSION)) and utils.parse_json(read(SESSION)).pid, 100)

-- AFP_BATCH set but empty: a window as any other
env.AFP_BATCH = ""
local empty = fake_mp(300)
load_plugin(empty)
expect("empty AFP_BATCH: socket set", empty.set["input-ipc-server"], SOCKET)
expect("empty AFP_BATCH: session.json is its", utils.parse_json(read(SESSION)).pid, 300)
-- The first window quits: session.json is the later one's now, and stays
quit(window)
expect("first window quits: session.json stays the later one's", utils.parse_json(read(SESSION)).pid, 300)
quit(empty)
expect("last window quits: session.json removed", read(SESSION), nil)

os.getenv = real_getenv
os.execute("rm -rf '" .. DATA .. "'")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
