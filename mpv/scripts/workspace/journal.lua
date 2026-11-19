-- The responsiveness journal (docs/responsiveness-journal.md): each thing asked of the player (a key, a panel button,
-- a file opened, a seek), when it was asked, the steps it took and when its result was on screen, written as JSON
-- lines to this computer's log folder, for tools/journal_report.py to say where the waiting is. Nothing is sent
-- anywhere.
--
-- Each plugin requires it once, near its top (before it binds keys), from the workspace plugin's folder as longpath.lua
-- is found; then by default:
--   - every key binding the plugin registers (mp.add_key_binding, mp.add_forced_key_binding) is an action, named
--     "<plugin>/<binding name>", asked when the key went down;
--   - mp.add_timeout's and mp.command_native_async's callbacks keep the action they were started in, so the steps
--     they record belong to it;
--   - every subprocess (mp.command_native[_async] {name="subprocess"}) is a step with its duration;
--   - io.open, utils.file_info and utils.readdir are counted and timed, local or on a network share (NAS);
--   - an OSD overlay drawn (mp.create_osd_overlay(...):update()) or an OSD message is a "show": something on screen
--     for the action going on.
-- The workspace plugin calls core() too: the run's header, the player's start, files opened, seeks, the first frame,
-- whether it then plays without dropping frames, the Library's open handed over, the folder kept small.
--
--   local J = require 'journal'
--   J.act(name[, fields])          an action asked now (from code that isn't a key binding: a panel button, a message)
--   J.label(name)                  the action going on is called this (the panel's click: which button)
--   J.step(name[, fields])         a point in the action going on; fields.d = how long it took (ms), if known
--   local done = J.span(name[, fields]); ...; done([fields])   a step timed from here to done()
--   J.shown(what[, fields])        something is on screen now for the action going on (the first of each kind counts)
--   J.now()                        the journal's clock: ms, monotonic, in Unix time (see below)
--
-- The clock: w is mpv's monotonic clock (mp.get_time) plus one reading of the wall clock taken when the plugin loads,
-- so it never jumps and the Library's records (same idea) line up with the player's within a millisecond or so.
--
-- Cheap by design: a record is a Lua table turned into a line (utils.format_json, ~3 µs) and put in a list; the list is
-- written every 2 s from a timer, in one write, not where the record is made. Nothing is recorded per frame. It
-- measures itself: a "cost" record every minute and at the end says how much time it took.

local M = {}

local has_mp, mp = pcall(require, 'mp')
if not has_mp then mp = nil end
local utils = mp and require 'mp.utils' or nil

local function noop() end
local function pack(...) return { n = select("#", ...), ... } end
local unpack = unpack or table.unpack
local VERSION = 1
-- Shows and steps without an action of their own go to the last action asked this recently (s)
local WINDOW = 15
-- Written this often (s)
local FLUSH_EVERY = 2
-- A file of the journal stops growing past this (bytes); the folder is kept under DIR_CAP, files older than KEEP_DAYS go
local FILE_CAP = 20 * 1024 * 1024
local DIR_CAP = 200 * 1024 * 1024
local KEEP_DAYS = 45
-- One file operation this slow is a step of its own (ms)
local SLOW_IO_MS = 50

-- (an mpv stood in for by a test without what the journal needs: off)
local needs = { "get_time", "observe_property", "set_property_native", "get_property_native", "add_periodic_timer",
                "add_timeout", "register_event", "command_native_async", "add_key_binding", "add_forced_key_binding" }
M.enabled = mp ~= nil and os.getenv("AFP_JOURNAL") ~= "0" and utils ~= nil and utils.format_json ~= nil
for _, k in ipairs(needs) do if M.enabled and type(mp[k]) ~= "function" then M.enabled = false end end

local api = { "act", "label", "step", "span", "shown", "core", "flush" }
if not M.enabled then
    for _, k in ipairs(api) do M[k] = noop end
    M.span = function() return noop end
    M.now = function() return os.time() * 1000 end
    return M
end

local windows = package.config:sub(1, 1) == "\\"
local raw_open, raw_remove = io.open, os.remove
local get_time = mp.get_time
local script = mp.get_script_name and mp.get_script_name() or "?"
local pid = utils.getpid and utils.getpid() or 0

-- The wall clock now, ms, as precise as this mpv lets Lua read it (LuaJIT's ffi), else to the second
local ffi_ok, ffi = pcall(require, 'ffi')
if not ffi_ok then ffi = nil end
local function wall_ms()
    if ffi then
        local ok, ms = pcall(function()
            if windows then
                pcall(ffi.cdef, "void GetSystemTimePreciseAsFileTime(uint64_t *t);")
                local t = ffi.new("uint64_t[1]")
                ffi.C.GetSystemTimePreciseAsFileTime(t)
                return tonumber(t[0]) / 1e4 - 11644473600000
            end
            pcall(ffi.cdef, "struct afp_timeval { long tv_sec; int32_t tv_usec; }; int gettimeofday(struct afp_timeval *tv, void *tz);")
            local tv = ffi.new("struct afp_timeval")
            ffi.C.gettimeofday(tv, nil)
            return tonumber(tv.tv_sec) * 1000 + tonumber(tv.tv_usec) / 1000
        end)
        if ok and ms then return ms, true end
    end
    return os.time() * 1000, false
end

local anchor, precise = wall_ms()
anchor = anchor - get_time() * 1000
-- To the second only: the first plugin's reading is everyone's, so the plugins agree at least with one another
if not precise then
    local shared = mp.get_property_native("user-data/afp/journal/anchor")
    if type(shared) == "number" then anchor = shared else mp.set_property_native("user-data/afp/journal/anchor", anchor) end
end
local function now() return anchor + get_time() * 1000 end
M.now = now

-- When this process started (ms, the journal's clock): macOS's proc_pid_rusage (ri_proc_start_abstime, in
-- mach_absolute_time's units), Linux's /proc/self/stat; nil elsewhere
local function process_start()
    if ffi and not windows and jit and jit.os == "OSX" then
        local ok, w = pcall(function()
            pcall(ffi.cdef, [[
                int proc_pid_rusage(int pid, int flavor, void *buffer);
                uint64_t mach_absolute_time(void);
                struct afp_timebase { uint32_t numer; uint32_t denom; };
                int mach_timebase_info(struct afp_timebase *info);
            ]])
            -- rusage_info_v0: a 16-byte uuid, 8 uint64, then ri_proc_start_abstime
            local buf = ffi.new("uint64_t[12]")
            if ffi.C.proc_pid_rusage(pid, 0, buf) ~= 0 then return nil end
            local start = buf[2 + 8]
            local tb = ffi.new("struct afp_timebase")
            ffi.C.mach_timebase_info(tb)
            local nowabs = ffi.C.mach_absolute_time()
            local ago_ms = tonumber(nowabs - start) * tb.numer / tb.denom / 1e6
            return now() - ago_ms
        end)
        if ok then return w end
        return nil
    end
    local f = raw_open("/proc/self/stat", "r")
    if not f then return nil end
    local stat = f:read("*l")
    f:close()
    local up = raw_open("/proc/uptime", "r")
    local uptime = up and tonumber((up:read("*l") or ""):match("^[%d.]+"))
    if up then up:close() end
    -- Field 22 (after the name in brackets, field 20 of the rest): clock ticks since boot (100 a second as a rule)
    local rest = stat and stat:match("%) (.*)")
    local fields = {}
    for v in (rest or ""):gmatch("%S+") do fields[#fields + 1] = v end
    local ticks = tonumber(fields[20])
    if not (ticks and uptime) then return nil end
    return now() - (uptime - ticks / 100) * 1000
end

-- The folder: AFP_JOURNAL_DIR, else this computer's logs (docs/responsiveness-journal.md)
local function journal_dir()
    local d = os.getenv("AFP_JOURNAL_DIR")
    if d and d ~= "" then return d end
    if windows then
        local base = os.getenv("LOCALAPPDATA") or os.getenv("APPDATA")
        return base and (base .. "\\ActionFootagePlayer\\Logs\\journal") or nil
    end
    local home = os.getenv("HOME")
    if jit and jit.os == "OSX" or (home and utils.file_info(home .. "/Library/Logs")) then
        return home and (home .. "/Library/Logs/ActionFootagePlayer/journal") or nil
    end
    local state = os.getenv("XDG_STATE_HOME")
    if not (state and state ~= "") then state = home and (home .. "/.local/state") end
    return state and (state .. "/ActionFootagePlayer/journal") or nil
end

local DIR = journal_dir()
local started = process_start()
M.process_start = started
-- One file a process, the same name in each plugin: player-<when it started>-<pid>.jsonl
local stamp = os.date("%Y%m%d-%H%M%S", math.floor((started or now()) / 1000))
if not started then stamp = os.date("%Y%m%d") end
local kind = os.getenv("AFP_BATCH") and os.getenv("AFP_BATCH") ~= "" and "batch" or "player"
local FILE = DIR and (DIR .. (windows and "\\" or "/") .. string.format("%s-%s-%d.jsonl", kind, stamp, pid)) or nil
M.file = FILE

---------------------------------------------------------------------------------------------------------------------
-- Records

local buf, nbuf = {}, 0
local cost = { records = 0, bytes = 0, rec_ms = 0, flush_ms = 0, flush_max_ms = 0, flushes = 0, dropped = 0 }
local written, capped = 0, false
local encode = utils.format_json

local function rec(r)
    local t0 = get_time()
    r.s = script
    local ok, line = pcall(encode, r)
    if ok and line then
        nbuf = nbuf + 1
        buf[nbuf] = line
        cost.records = cost.records + 1
    else
        cost.dropped = cost.dropped + 1
    end
    cost.rec_ms = cost.rec_ms + (get_time() - t0) * 1000
end
M._rec = rec

local made_dir = false
local function make_dir()
    if made_dir or not DIR then return end
    made_dir = true
    local args = windows and { "cmd", "/d", "/c", "mkdir", DIR } or { "mkdir", "-p", DIR }
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stdout = true, capture_stderr = true }, noop)
end

-- The list written, in one write (a line from another plugin never lands in the middle of one: appended, < 64 KB)
local function flush()
    if nbuf == 0 or not FILE then return end
    if capped then
        buf, nbuf = {}, 0
        return
    end
    local t0 = get_time()
    local f = raw_open(FILE, "ab")
    if not f then
        make_dir()
        -- Kept for the next try, up to a point
        if nbuf > 20000 then buf, nbuf = {}, 0 end
        return
    end
    if written == 0 then
        local info = utils.file_info(FILE)
        written = info and info.size or 0
    end
    f:setvbuf("full", 65536)
    local from = 1
    while from <= nbuf do
        local to, size = from, 0
        while to <= nbuf and (size + #buf[to] + 1 < 60000 or to == from) do
            size = size + #buf[to] + 1
            to = to + 1
        end
        local chunk = table.concat(buf, "\n", from, to - 1) .. "\n"
        f:write(chunk)
        f:flush()
        written = written + #chunk
        cost.bytes = cost.bytes + #chunk
        from = to
    end
    if written > FILE_CAP then
        capped = true
        f:write(encode({ k = "capped", w = now(), s = script, bytes = written }) .. "\n")
    end
    f:close()
    buf, nbuf = {}, 0
    local ms = (get_time() - t0) * 1000
    cost.flush_ms = cost.flush_ms + ms
    cost.flushes = cost.flushes + 1
    if ms > cost.flush_max_ms then cost.flush_max_ms = ms end
end
M.flush = flush

---------------------------------------------------------------------------------------------------------------------
-- Actions

-- The action this plugin's code runs for now (a key's handler, a callback started from one), and the last one asked
-- anywhere in the player (user-data/afp/journal/act, each plugin's)
local cur = nil
local last = nil
local count = 0
local seen = {}       -- action id -> { [what] = true }: shows already recorded
local seen_order = {}

local function new_id()
    count = count + 1
    return string.format("P%d.%s.%d", pid, script, count)
end

-- The action a step or a show belongs to: this plugin's one going on, else the last asked within WINDOW
local function owner()
    if cur then return cur end
    -- Read now, not as observed: another plugin's action may be newer than its notification here (a file opening
    -- reaches this plugin as an event before the property's change does)
    local v = mp.get_property_native("user-data/afp/journal/act")
    if type(v) == "table" and v.a and type(v.w) == "number" then last = v end
    if last and now() - last.w < WINDOW * 1000 then return last.a, true end
    return nil
end
M.owner = owner

local function publish(a, name, w)
    last = { a = a, n = name, w = w }
    mp.set_property_native("user-data/afp/journal/act", last)
end

mp.observe_property("user-data/afp/journal/act", "native", function(_, v)
    if type(v) == "table" and v.a and (not last or v.a ~= last.a) then last = v end
end)

-- An action asked at w (now when nil); fn, if given, runs as it (returns what fn returns)
function M.act(name, fields, w, fn, ...)
    local a = new_id()
    local asked = w or now()
    local r = { k = "act", a = a, n = name, w = asked }
    if fields then for k, v in pairs(fields) do if r[k] == nil then r[k] = v end end end
    publish(a, name, asked)
    if not fn then
        rec(r)
        return a
    end
    local before = cur
    cur = a
    M._pending = r
    local res = pack(pcall(fn, ...))
    cur = before
    M._pending = nil
    -- How long the handler itself took (the plugin's Lua, not what it started)
    r.d = now() - asked
    if r.n ~= name then publish(a, r.n, asked) end
    rec(r)
    if not res[1] then error(res[2], 0) end
    return unpack(res, 2, res.n)
end

-- The action going on (a handler running) is called this: the panel's click names its button
function M.label(name, fields)
    local r = M._pending
    if not r then return end
    r.n = name
    if fields then for k, v in pairs(fields) do if r[k] == nil then r[k] = v end end end
end

function M.step(name, fields)
    local r = { k = "step", n = name, w = now(), a = owner() }
    if fields then
        for k, v in pairs(fields) do if r[k] == nil then r[k] = v end end
        -- A duration given: the step started that long ago
        if type(fields.d) == "number" then r.w = r.w - fields.d end
    end
    rec(r)
end

function M.span(name, fields)
    local t0, a = now(), owner()
    return function(more)
        local r = { k = "step", n = name, w = t0, d = now() - t0, a = a }
        if fields then for k, v in pairs(fields) do if r[k] == nil then r[k] = v end end end
        if more then for k, v in pairs(more) do if r[k] == nil then r[k] = v end end end
        rec(r)
    end
end

-- Something on screen now for the action going on: only the first of each kind (what) an action gets is recorded
function M.shown(what, fields)
    local a, by_window = owner()
    if not a then return end
    local s = seen[a]
    if not s then
        s = {}
        seen[a] = s
        seen_order[#seen_order + 1] = a
        if #seen_order > 64 then seen[table.remove(seen_order, 1)] = nil end
    end
    if s[what] then return end
    s[what] = true
    -- by_window: not from the action's own code (a handler, its timers, its subprocesses' callbacks) but the last
    -- action asked: likely its result when soon after it, maybe not when late (the analysis weighs it so)
    local r = { k = "show", n = what, w = now(), a = a, weak = by_window or nil }
    if fields then for k, v in pairs(fields) do if r[k] == nil then r[k] = v end end end
    rec(r)
end

---------------------------------------------------------------------------------------------------------------------
-- File operations: local or on a network share, counted per action and written with the next flush

local net = nil   -- the network shares' mount points (core() publishes them), longest first
mp.observe_property("user-data/afp/journal/net", "native", function(_, v)
    if type(v) == "table" then
        table.sort(v, function(x, y) return #x > #y end)
        net = v
    end
end)

local function where(path)
    if type(path) ~= "string" then return "local" end
    if windows and (path:sub(1, 2) == "\\\\" or path:sub(1, 2) == "//") and path:sub(1, 4) ~= "\\\\?\\" then return "nas" end
    if windows and path:sub(1, 8) == "\\\\?\\UNC\\" then return "nas" end
    for _, m in ipairs(net or {}) do
        if path:sub(1, #m) == m and (#path == #m or path:sub(#m + 1, #m + 1) == "/") then return "nas" end
    end
    return "local"
end
M.where = where

local io_by = {}  -- action id or "-" -> totals
local function io_note(op, path, ms)
    local a = owner() or "-"
    local t = io_by[a]
    if not t then
        t = { nas_n = 0, nas_ms = 0, local_n = 0, local_ms = 0, slow_ms = 0 }
        io_by[a] = t
    end
    local w = where(path)
    t[w .. "_n"] = t[w .. "_n"] + 1
    t[w .. "_ms"] = t[w .. "_ms"] + ms
    if ms > t.slow_ms then t.slow_ms, t.slow_op, t.slow_path = ms, op, path end
    if ms >= SLOW_IO_MS then M.step("io-slow", { op = op, path = path, d = ms, where = w }) end
end

local function io_flush()
    for a, t in pairs(io_by) do
        -- Only what's worth counting: anything on the share, or a millisecond locally
        if t.nas_n == 0 and t.local_ms < 1 then goto next end
        local r = { k = "io", w = now(), a = a ~= "-" and a or nil }
        for k, v in pairs(t) do r[k] = v end
        rec(r)
        ::next::
    end
    io_by = {}
end

---------------------------------------------------------------------------------------------------------------------
-- The hooks: what's asked through mpv's API is journaled by default

-- Keys: the binding's function runs as an action (its key going down, a press; not up, not held repeats)
local function wrap_binding(add)
    return function(key, name, fn, flags)
        local label = name
        if type(name) ~= "string" and name ~= nil then
            flags, fn, label = fn, name, nil
        end
        if type(fn) ~= "function" then return add(key, name, fn, flags) end
        local act_name = script .. "/" .. tostring(label or key)
        local complex = type(flags) == "table" and flags.complex
        local wrapped = function(info, ...)
            if complex and type(info) == "table" and info.event ~= "down" and info.event ~= "press" then
                return fn(info, ...)
            end
            local w = now()
            return M.act(act_name, { via = "key", key = complex and type(info) == "table" and info.key_name or key }, w, fn, info, ...)
        end
        if label then return add(key, label, wrapped, flags) end
        return add(key, wrapped, flags)
    end
end
mp.add_key_binding = wrap_binding(mp.add_key_binding)
mp.add_forced_key_binding = wrap_binding(mp.add_forced_key_binding)

-- Timers and async commands keep the action they were started in
local function keep(fn)
    local a = cur or owner()
    if type(fn) ~= "function" or not a then return fn end
    return function(...)
        local before = cur
        cur = a
        local res = pack(pcall(fn, ...))
        cur = before
        if not res[1] then error(res[2], 0) end
        return unpack(res, 2, res.n)
    end
end
local add_timeout = mp.add_timeout
mp.add_timeout = function(seconds, cb, ...) return add_timeout(seconds, keep(cb), ...) end

local function proc_name(cmd)
    local exe = type(cmd) == "table" and type(cmd.args) == "table" and cmd.args[1]
    return exe and tostring(exe):match("([^/\\]+)$") or "?"
end

local native_async = mp.command_native_async
mp.command_native_async = function(cmd, cb)
    if type(cmd) == "table" and cmd.name == "subprocess" then
        local done = M.span("proc:" .. proc_name(cmd))
        local inner = keep(cb)
        return native_async(cmd, function(ok, r, err)
            done({ status = type(r) == "table" and r.status or nil, killed = type(r) == "table" and r.killed_by_us or nil })
            if inner then return inner(ok, r, err) end
        end)
    end
    return native_async(cmd, keep(cb))
end
local native = mp.command_native
mp.command_native = function(cmd, ...)
    if type(cmd) == "table" and cmd.name == "subprocess" then
        local done = M.span("proc:" .. proc_name(cmd), { sync = true })
        local r, err = native(cmd, ...)
        done({ status = type(r) == "table" and r.status or nil })
        return r, err
    end
    return native(cmd, ...)
end

-- File operations, timed
io.open = function(path, ...)
    local t0 = get_time()
    local f, err = raw_open(path, ...)
    io_note("open", path, (get_time() - t0) * 1000)
    return f, err
end
do
    local file_info, readdir = utils.file_info, utils.readdir
    utils.file_info = function(path, ...)
        local t0 = get_time()
        local r, err = file_info(path, ...)
        io_note("stat", path, (get_time() - t0) * 1000)
        return r, err
    end
    utils.readdir = function(path, ...)
        local t0 = get_time()
        local r, err = readdir(path, ...)
        io_note("readdir", path, (get_time() - t0) * 1000)
        return r, err
    end
end

-- On screen: an overlay drawn, an OSD message
local create_overlay = mp.create_osd_overlay
if create_overlay then
    mp.create_osd_overlay = function(format)
        local ov = create_overlay(format)
        local update = ov.update
        ov.update = function(self, ...)
            local r = { update(self, ...) }
            M.shown("overlay:" .. script)
            return unpack(r)
        end
        return ov
    end
end
local osd_message = mp.osd_message
mp.osd_message = function(...)
    osd_message(...)
    M.shown("osd")
end

-- Written every FLUSH_EVERY seconds, and as the player closes; what the journal cost, every minute
local last_cost = get_time()
local function cost_record(final)
    rec({ k = "cost", w = now(), records = cost.records, bytes = cost.bytes, rec_ms = cost.rec_ms, flush_ms = cost.flush_ms,
          flush_max_ms = cost.flush_max_ms, flushes = cost.flushes, dropped = cost.dropped, final = final or nil })
end
mp.add_periodic_timer(FLUSH_EVERY, function()
    io_flush()
    if get_time() - last_cost > 60 then
        last_cost = get_time()
        cost_record()
    end
    flush()
end)
mp.register_event("shutdown", function()
    io_flush()
    cost_record(true)
    flush()
end)

rec({ k = "load", w = now(), v = VERSION, precise = precise or nil })

---------------------------------------------------------------------------------------------------------------------
-- The player itself (called by the workspace plugin, once a process)

local function prop(name) return mp.get_property_native(name) end

-- The network shares mounted now (macOS, Linux: mount's list; Windows: \\ paths are found by their look)
local function find_shares()
    if windows then return end
    local f = raw_open("/proc/mounts", "r")
    if f then
        local list = {}
        for line in f:lines() do
            local _, at, fs = line:match("^(%S+) (%S+) (%S+)")
            if fs and (fs:match("^cifs") or fs:match("^smb") or fs:match("^nfs") or fs == "fuse.sshfs" or fs:match("^9p")) then
                list[#list + 1] = at:gsub("\\040", " ")
            end
        end
        f:close()
        mp.set_property_native("user-data/afp/journal/net", list)
        return
    end
    native_async({ name = "subprocess", args = { "/sbin/mount" }, capture_stdout = true, playback_only = false }, function(_, r)
        local list = {}
        for line in ((r and r.stdout) or ""):gmatch("[^\n]+") do
            local at, fs = line:match(" on (.-) %(([%w]+)")
            if at and (fs == "smbfs" or fs == "nfs" or fs == "afpfs" or fs == "webdav") then list[#list + 1] = at end
        end
        mp.set_property_native("user-data/afp/journal/net", list)
    end)
end

-- The folder kept: files older than KEEP_DAYS go, then the oldest until it's under DIR_CAP
local function prune()
    if not DIR then return end
    local names = utils.readdir(DIR, "files") or {}
    local files, total = {}, 0
    local cutoff = os.time() - KEEP_DAYS * 86400
    for _, n in ipairs(names) do
        if n:match("%.jsonl$") then
            local p = DIR .. (windows and "\\" or "/") .. n
            local info = utils.file_info(p)
            if info then
                if info.mtime < cutoff and p ~= FILE then raw_remove(p)
                else
                    files[#files + 1] = { p = p, t = info.mtime, size = info.size }
                    total = total + info.size
                end
            end
        end
    end
    table.sort(files, function(x, y) return x.t < y.t end)
    for _, f in ipairs(files) do
        if total <= DIR_CAP then break end
        if f.p ~= FILE then
            raw_remove(f.p)
            total = total - f.size
        end
    end
end
M._prune = prune

-- The Library's double-click, handed over (Library's journal.rs writes it before it starts the player; with `open`,
-- macOS gives an already open player the file without our environment): taken by the file it names, once
local function handoff(path)
    if not DIR then return nil end
    local p = DIR .. (windows and "\\" or "/") .. "handoff.json"
    local f = raw_open(p, "r")
    if not f then return nil end
    local text = f:read("*a")
    f:close()
    local h = utils.parse_json(text or "")
    if type(h) ~= "table" or type(h.w) ~= "number" then return nil end
    if now() - h.w > 60000 then raw_remove(p) return nil end
    local match = type(h.paths) ~= "table" or #h.paths == 0
    for _, q in ipairs(type(h.paths) == "table" and h.paths or {}) do
        if q == path then match = true end
    end
    if not match then return nil end
    raw_remove(p)
    return h
end

local cored = false
function M.core()
    if cored then return end
    cored = true
    local batch = kind == "batch"
    rec({ k = "run", w = now(), app = "player", v = VERSION, pid = pid, start = started, batch = batch or nil,
          parent = os.getenv("AFP_JOURNAL_PARENT"), version = (prop("script-opts") or {})["footage_menu-version"],
          mpv = prop("mpv-version"), platform = jit and jit.os or nil, precise = precise or nil })
    find_shares()
    mp.add_timeout(20, prune)
    -- The start: from the process starting (or this plugin loading) to the first frame
    local start_a = M.act("player/start", { via = "core", batch = batch or nil }, started or now())
    local files = 0
    local opened = {}  -- the paths opened this run: a file opened again is warm (its sidecar, motion, pipelines read)
    local file = nil   -- the file opening: { a, w, first frame seen }
    local smooth_timer = nil

    local function smooth(a)
        if smooth_timer then smooth_timer:kill() end
        local t0, ticks, quiet = now(), 0, 0
        local drops0 = prop("frame-drop-count") or 0
        local vo0 = prop("vo-delayed-frame-count") or 0
        local before = { drops0, vo0 }
        smooth_timer = mp.add_periodic_timer(1, function()
            ticks = ticks + 1
            if prop("pause") then return end
            local d, v = prop("frame-drop-count") or 0, prop("vo-delayed-frame-count") or 0
            local new = (d - before[1]) + (v - before[2])
            before = { d, v }
            quiet = new == 0 and quiet + 1 or 0
            if quiet >= 2 or ticks >= 15 then
                smooth_timer:kill()
                rec({ k = "step", n = quiet >= 2 and "smooth" or "not-smooth", a = a, w = now() - (quiet >= 2 and 2000 or 0),
                      drops = d - drops0, delayed = v - vo0, fps = prop("estimated-vf-fps"), display_fps = prop("display-fps"),
                      after_ms = now() - t0 })
            end
        end)
    end

    mp.register_event("start-file", function()
        files = files + 1
        local recent = last and now() - last.w < 250 and last or nil
        local a
        local path = prop("path")
        local h = not batch and handoff(path) or nil
        -- The first file, or one in place of a file not shown yet (a folder's playlist made, a moment redirected):
        -- the same action
        if files == 1 then a = start_a
        elseif file and not file.first then a = file.a
        elseif recent then a = recent.a
        else a = M.act("file/open", { via = "core" }) end
        -- The Library's action that asked for it: the analysis measures from its click
        if h then rec({ k = "step", n = "handoff", a = a, w = now(), parent = h.a, asked = h.w }) end
        if file and not file.first and file.a == a then file.path = path
        else file = { a = a, w = now(), first = false, path = path } end
        local again = opened[path or ""]
        opened[path or ""] = true
        rec({ k = "step", n = "start-file", a = a, w = now(), nth = files, where = where(path), again = again or nil })
    end)
    mp.register_event("file-loaded", function()
        if not file then return end
        local path = prop("path")
        local ws = prop("user-data/workspace")
        rec({ k = "step", n = "file-loaded", a = file.a, w = now(), since_ms = now() - file.w, where = where(path),
              size = prop("file-size"), duration = prop("duration"),
              moment = type(ws) == "table" and ws.redirected or nil, nth = files })
    end)
    mp.register_event("video-reconfig", function()
        if file and not file.first then rec({ k = "step", n = "video-reconfig", a = file.a, w = now() }) end
    end)
    local seek_a = nil
    mp.register_event("seek", function()
        if file and not file.first then return end
        local recent = last and now() - last.w < 300 and last or nil
        if recent then
            seek_a = recent.a
            rec({ k = "step", n = "seek", a = seek_a, w = now(), pos = prop("time-pos") })
        else
            seek_a = M.act("seek", { via = "core", pos = prop("time-pos") })
        end
    end)
    mp.register_event("playback-restart", function()
        if file and not file.first then
            file.first = true
            local vp = prop("video-params") or {}
            rec({ k = "step", n = "first-frame", a = file.a, w = now(), since_ms = now() - file.w, nth = files,
                  w_px = vp.w, h_px = vp.h, fps = prop("container-fps"), codec = prop("video-format"), hwdec = prop("hwdec-current"),
                  gamma = vp.gamma })
            rec({ k = "show", n = "frame", a = file.a, w = now() })
            smooth(file.a)
        elseif seek_a then
            rec({ k = "show", n = "frame", a = seek_a, w = now() })
            seek_a = nil
        else
            M.shown("frame")
        end
    end)
    local pause_seen = false
    mp.observe_property("pause", "bool", function(_, paused)
        if not pause_seen then pause_seen = true return end
        -- Asked by a key of the plugins (an action just now): its result; else mpv's own (Space, a click, the IPC):
        -- an action of its own, applied as it's asked (what's drawn after isn't seen: the bar, at most a frame)
        -- (a file opening: the gyroflow plugin's own hold until its warp is there)
        if (last and now() - last.w < 300) or (file and not file.first) then M.shown("pause")
        else
            local a = M.act(paused and "pause" or "play", { via = "core" })
            rec({ k = "show", n = "pause", a = a, w = now() })
        end
    end)
end

return M
