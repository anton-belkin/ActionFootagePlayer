-- Which project a clip's folder belongs to (workspace.lua's projects_of), kept between the player's runs.
--
-- Each double click in Action Footage Library starts a new player, and the lookup runs before mpv may open the file
-- (main.lua's on_load): on a NAS it is a few listings and stats of the clip's folder and the ones above it, 0.3-6 s
-- cold (journal, 19-24 Nov 2026). Kept here: the answer for the folder and what it was read from, each folder listed
-- with its modification time (a file added, removed or renamed in it changes it: a Project.afproject or an
-- .afplocation made or taken away), each file read (an .afplocation) with its time and size, each other path whose
-- being there counted. The answer holds while all of them are as they were: a stat each, not a listing.
--
--   local cache = require('projectcache').new({ info = f, parent = f, now = f, load = f, save = f })
--   ws.projects_of = cache.wrap(ws.projects_of)
--   -- and the file calls the lookup makes, as it makes them:
--   cache.saw("list", dir, info)      -- a folder's names read
--   cache.saw("read", file, info)     -- a file's text read
--   cache.saw("exists", path, info)   -- whether a path is there (info nil: it isn't)
--   cache.recording()                 -- whether a lookup is under way (then a read file's info is wanted)
--
-- info(path) -> { is_dir, is_file, mtime, size } or nil; parent(path) -> its folder; now() -> s (os.time, the clock
-- the files' times are on); load() -> the table saved, or nil; save(table). Plain Lua with no mpv in it
-- (tests/projectcache_test.lua).

local M = {}

-- Folders changed this recently aren't kept: a time in whole seconds (mpv's file_info) doesn't tell a change later in
-- the same second, and the NAS's clock may be a little off this computer's
M.RECENT_S = 60
-- Answers checked this recently in this run are taken as they are (a clip's lookup runs a few times as it opens)
M.FRESH_S = 30
-- Folders kept, the ones used longest ago dropped first
M.MAX = 2000

function M.new(o)
    local self = {}
    local data = nil
    local checked = {}
    local deps = nil

    local function loaded()
        if not data then
            local t = o.load and o.load()
            data = type(t) == "table" and type(t.folders) == "table" and t or { v = 1, folders = {} }
        end
        return data
    end

    -- Whether a lookup is being recorded (a file read then needs its time and size)
    function self.recording() return deps ~= nil end

    function self.saw(kind, path, info)
        if not deps or type(path) ~= "string" then return end
        local d = { k = kind, p = path, e = info ~= nil, dir = info and info.is_dir or nil }
        if info and (kind == "list" or kind == "read") then d.m, d.s = info.mtime, info.size end
        deps[#deps + 1] = d
    end

    -- Whether a recorded path is as it was
    local function same(d)
        local info = o.info(d.p)
        if d.k == "exists" then return (info ~= nil) == d.e and (not info or (info.is_dir == true) == (d.dir == true)) end
        if not info then return false end
        if d.k == "list" then return info.is_dir == true and info.mtime == d.m end
        return info.is_file == true and info.mtime == d.m and info.size == d.s
    end

    -- What an answer depends on: a path's being there is the listed folder's time when its folder is listed too
    local function needed(list)
        local listed, out, seen = {}, {}, {}
        for _, d in ipairs(list) do if d.k == "list" and d.e then listed[d.p] = true end end
        for _, d in ipairs(list) do
            local key = d.k .. "\0" .. d.p
            if not seen[key] then
                seen[key] = true
                local covered = d.k == "exists" and listed[o.parent(d.p) or ""]
                if not covered then out[#out + 1] = d end
            end
        end
        return out
    end

    local function recent(list, now)
        for _, d in ipairs(list) do
            if type(d.m) == "number" and d.m > now - M.RECENT_S then return true end
        end
        return false
    end

    local function prune(folders)
        local keys = {}
        for k in pairs(folders) do keys[#keys + 1] = k end
        if #keys <= M.MAX then return end
        table.sort(keys, function(a, b) return (folders[a].at or 0) > (folders[b].at or 0) end)
        for i = M.MAX + 1, #keys do folders[keys[i]] = nil end
    end

    -- projects_of with the folder's answer kept
    function self.wrap(projects_of)
        return function(clip)
            if type(clip) ~= "string" or clip:find("://", 1, true) then return projects_of(clip) end
            local dir = o.parent(clip)
            if not dir then return projects_of(clip) end
            local folders = loaded().folders
            local e = folders[dir]
            if type(e) == "table" and type(e.projects) == "table" and type(e.deps) == "table" then
                local t = o.clock and o.clock() or o.now()
                local ok = checked[dir] and t - checked[dir] < M.FRESH_S
                if not ok then
                    ok = true
                    for _, d in ipairs(e.deps) do if not same(d) then ok = false; break end end
                end
                if ok then
                    checked[dir] = t
                    local out = {}
                    for i, p in ipairs(e.projects) do out[i] = p end
                    self.hits = (self.hits or 0) + 1
                    return out
                end
                folders[dir] = nil
                checked[dir] = nil
            end
            deps = {}
            local ok, found = pcall(projects_of, clip)
            local list = deps
            deps = nil
            if not ok then error(found, 0) end
            self.misses = (self.misses or 0) + 1
            local now = o.now()
            list = needed(list)
            if not recent(list, now) then
                local projects = {}
                for i, p in ipairs(found) do projects[i] = p end
                folders[dir] = { projects = projects, deps = list, at = now }
                checked[dir] = o.clock and o.clock() or now
                prune(folders)
                if o.save then o.save(data) end
            end
            return found
        end
    end

    -- The lookup's file calls recorded as they're made: fs's is_dir, is_file, files and read_json (workspace.lua
    -- calls them through the table), and ws.projects_of kept
    function self.install(fs, ws)
        local is_dir, is_file, files, read_json = fs.is_dir, fs.is_file, fs.files, fs.read_json
        local function exists(f)
            return function(p)
                local r = f(p)
                if deps then self.saw("exists", p, o.info(p)) end
                return r
            end
        end
        if is_dir then fs.is_dir = exists(is_dir) end
        if is_file then fs.is_file = exists(is_file) end
        if files then
            fs.files = function(p)
                local r = files(p)
                if deps then self.saw("list", p, o.info(p)) end
                return r
            end
        end
        if read_json then
            fs.read_json = function(p)
                local r = read_json(p)
                if deps then self.saw("read", p, o.info(p)) end
                return r
            end
        end
        ws.projects_of = self.wrap(ws.projects_of)
        return self
    end

    return self
end

return M
