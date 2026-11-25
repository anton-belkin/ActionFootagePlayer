-- momentlist: the clip's moments as the workspace plugin lists them (the project's catalog, else a look through its
-- folders, read again as they change), as chapters too; the names the name dialog suggests from; the scan's cache,
-- kept between sessions with this computer's choices. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local moments_lib = require 'moments'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, windows, catalog, moments
local function publish(...) return W.publish(...) end
local function extras(...) return W.extras(...) end
local function adopt_gyroflow_trim(...) return W.adopt_gyroflow_trim(...) end
local function clock(...) return W.clock(...) end
local function fingerprint_args(...) return W.fingerprint_args(...) end
local function fingerprint_of(...) return W.fingerprint_of(...) end

-- How often the _Moments/ folders are looked at for files added, removed or renamed in Finder
local WATCH_SECONDS = 5

-- The project's _Moments/ as last read: every moment in it, and its folders' times
local found = { list = {}, dirs = {} }
local scanned_project = nil
-- The clip's moments came from the project's catalog (asked again every few seconds), not a look through its folder
local from_catalog = false
local last_asked = 0
-- The project's moments' names and how many (the catalog's, asked once a clip has its own)
local project_moments = nil
local watch_timer = nil
-- The chapters this put in chapter-list, kept there when another plugin sets it
local ours = {}
local applying = false
-- The name last typed this session
local last_typed = nil
-- The name typed for a moment or an export (momentsave.lua), kept as the next suggestion's
local function set_last_typed(text) last_typed = text end

-- What's read of every moment file, kept between sessions (moments.lua's cache, in mpv's cache folder): a scan then
-- only lists the folders and stats the files
local CACHE_FILE = mp.command_native({ "expand-path", "~~cache/actionfootageplayer-moments.json" })
local cache = {}
do
    local f = CACHE_FILE and io.open(CACHE_FILE, "r")
    if f then
        local t = utils.parse_json(f:read("*a") or "")
        f:close()
        if type(t) == "table" then cache = t end
    end
end
local cache_dirty = false
local function save_cache()
    if not cache_dirty or not CACHE_FILE then return end
    cache_dirty = false
    local f = io.open(CACHE_FILE .. ".tmp", "w")
    if not f then return end
    f:write(utils.format_json(cache) or "{}")
    f:close()
    os.remove(CACHE_FILE)
    os.rename(CACHE_FILE .. ".tmp", CACHE_FILE)
end
-- Marks it changed when the scan reads a file
local tracked_cache = setmetatable({}, {
    __index = cache,
    __newindex = function(_, k, v) cache[k] = v; cache_dirty = true end,
})


-- Chapters: the moments' ins and outs, put in beside what's there (the file's own, the highlights plugin's)
local function same_chapter(a, b) return a.title == b.title and math.abs(a.time - b.time) < 0.001 end

local function apply_chapters(list)
    local chapters = {}
    for _, c in ipairs(mp.get_property_native("chapter-list") or {}) do
        local mine = false
        for _, o in ipairs(ours) do if same_chapter(c, o) then mine = true end end
        if not mine then chapters[#chapters + 1] = { title = c.title, time = c.time } end
    end
    for _, c in ipairs(list) do chapters[#chapters + 1] = c end
    table.sort(chapters, function(a, b) return a.time < b.time end)
    ours = list
    applying = true
    mp.set_property_native("chapter-list", chapters)
    applying = false
end

function M.chapters_changed(_, chapters)
    if applying or #ours == 0 then return end
    -- Another plugin set them anew: ours again, if they're gone
    for _, o in ipairs(ours) do
        local there = false
        for _, c in ipairs(chapters or {}) do if same_chapter(c, o) then there = true end end
        if not there then return apply_chapters(ours) end
    end
end
-- A new file (opening.lua's on_load): none of the chapters there are ours
local function forget_chapters() ours = {} end

-- The project's moments read again, and the clip's published, as chapters too
local function refresh()
    local project = state.project
    if not project or not state.clip then
        found, scanned_project = { list = {}, dirs = {} }, nil
        state.moments = nil
        if #ours > 0 then apply_chapters({}) end
        publish()
        return
    end
    local clip = state.clip
    local function show(list)
        local info = utils.file_info(state.clip)
        local mine = moments.of_clip(list, { relative = state.relative, size = info and info.size, created = state.created, hash = state.hash })
        local out, chapters = {}, {}
        -- Just saved (preview-done), not listed yet: shown all the same, for a minute at most
        local listed = {}
        for _, m in ipairs(mine) do listed[m.file] = true end
        for file, f in pairs(state.fresh or {}) do
            if listed[file] or f.clip ~= state.clip or mp.get_time() - f.at > 60 then state.fresh[file] = nil
            else mine[#mine + 1] = { name = f.name, file = file, link = { ["in"] = f["in"], out = f.out, snap = f.snap and 1 or nil, alone = f.alone, stab = f.stab } } end
        end
        table.sort(mine, function(x, y) return (x.link["in"] or 0) < (y.link["in"] or 0) end)
        for _, m in ipairs(mine) do
            out[#out + 1] = { name = m.name, file = m.file, ["in"] = m.link["in"], out = m.link.out,
                              snap = m.link.snap == 1, alone = m.link.alone == true, stab = m.link.stab }
            -- A moment playing is a video of its own: the clip's moments aren't on its seek bar
            if not state.offset then
                chapters[#chapters + 1] = { title = m.name, time = m.link["in"] }
                chapters[#chapters + 1] = { title = m.name .. " end", time = m.link.out }
            end
        end
        state.moments = out
        state.moments_total = project_moments and project_moments.total or (not from_catalog and #found.list) or state.moments_total
        apply_chapters(chapters)
        if extras then extras() end
        publish()
    end
    last_asked = mp.get_time()
    -- The project's catalog first (docs/design/library-management.md §9): this clip's moments only, by its path, in the
    -- background; then a look at the project's own folders (what Finder changed), and again when it found something
    catalog.derived(project, nil, state.relative, function(list)
        if state.clip ~= clip or state.project ~= project then return end
        if list then
            local ms = {}
            for _, m in ipairs(list) do if m.link.v == 1 and m.link.kind == "moment" then ms[#ms + 1] = m end end
            found, scanned_project, from_catalog = { list = ms, dirs = {} }, project, true
            show(ms)
            -- A moment not where it was (moved or removed in Finder) stays shown until the look says which. Only
            -- then: what else changes on the disk the catalog's own care notes (the NAS Helper watches the shares, the
            -- Library looks after its roots), the player adds no walks of its own (each ran during playback, on the
            -- share the clip plays from)
            local missing = false
            for _, m in ipairs(list) do if m.missing then missing = true end end
            if missing then catalog.look(project, 0) end
            -- The project's moments' names and count, once (what the name dialog suggests from)
            if not project_moments or project_moments.project ~= project then
                project_moments = { project = project, names = {}, total = nil }
                catalog.ask({ "list", project, "--kind", "moment" }, function(all)
                    if not all or project_moments.project ~= project then return end
                    local names = {}
                    for _, m in ipairs(all) do names[#names + 1] = type(m.link.name) == "string" and m.link.name or m.name end
                    project_moments.names, project_moments.total = names, #all
                    state.moments_total = #all
                    publish()
                end)
            end
            return
        end
        -- No catalog yet (the Library hasn't made it): the project's folder looked through, as before
        found = moments.scan(project, tracked_cache)
        scanned_project, from_catalog = project, false
        save_cache()
        show(found.list)
    end)
end

-- A folder with moments in it changed (a file added, removed or renamed in it), or the project's top or one of its own
-- folders did (a folder of moments moved in): moments.scan's dirs
local function watch()
    if not scanned_project or scanned_project ~= state.project then return end
    -- From the catalog: asked again (what another computer, or the Library, wrote is in its journal), every 30 s:
    -- each ask reads the catalog from the project's share
    if from_catalog then
        if mp.get_time() - last_asked >= 30 then refresh() end
        return
    end
    for dir, mtime in pairs(found.dirs) do
        local info = utils.file_info(dir)
        if not info or info.mtime ~= mtime then return refresh() end
    end
end

function M.loaded()
    state.created = mp.get_property("metadata/by-key/creation_time") or state.created
    state.loaded = true
    if adopt_gyroflow_trim then adopt_gyroflow_trim() end
    refresh()
    if not watch_timer then watch_timer = mp.add_periodic_timer(WATCH_SECONDS, watch) end
end

-- The names of the project's moments, the newest last (by their files' times): what the name dialog suggests from
-- and ↑ goes through. Read from the moments themselves (docs/design/library-management.md §8), not kept in the project's file
local function moment_names()
    local list = {}
    -- From the catalog: the project's moments' names as it listed them (the clip's own after them, the newest)
    if from_catalog and project_moments and project_moments.project == state.project then
        local out, seen = {}, {}
        for _, n in ipairs(project_moments.names) do if not seen[n] then seen[n] = true; out[#out + 1] = n end end
        return out
    end
    for _, m in ipairs(found and found.list or {}) do
        local c = cache[m.file]
        list[#list + 1] = { name = type(m.link.name) == "string" and m.link.name or m.name, t = type(c) == "table" and c.mtime or 0 }
    end
    table.sort(list, function(x, y) return x.t < y.t end)
    local out, seen = {}, {}
    for i = #list, 1, -1 do
        local n = list[i].name
        if not seen[n] then seen[n] = true; table.insert(out, 1, n) end
    end
    return out
end

local function last_name()
    if last_typed then return last_typed end
    local names = moment_names()
    return names[#names]
end

local function remember_name(text)
    if text and text:match("%S") then last_typed = text end
end

-- mp.input's ↑ reads earlier names from a file: the moments' names, written out for it before it asks
local history_path = nil
local function history_file()
    if not state.project then return nil end
    if not history_path then
        local tmp = os.getenv(windows and "TEMP" or "TMPDIR") or "/tmp"
        history_path = utils.join_path(tmp, "afp-moment-names-" .. tostring(utils.getpid()) .. ".txt")
    end
    local f = io.open(history_path, "w")
    if not f then return nil end
    for _, n in ipairs(moment_names()) do f:write(n, "\n") end
    f:close()
    return history_path
end

-- What the dialog suggests: the last name typed with the next free number, else the moment's date and time by the
-- camera's clock
local function suggestion(a)
    local last = last_name()
    if last then
        local taken = {}
        for _, m in ipairs(found.list) do taken[m.name:lower()] = true end
        return moments_lib.next_name(last, taken)
    end
    local created = moments_lib.parse_time(state.created)
    if created then
        local t = created + a
        return moments_lib.time_name(t, moments_lib.zone_of_name(ws.name(state.clip), created) or moments_lib.local_zone(t))
    end
    return (ws.name(state.clip):gsub("%.[^.]*$", "")) .. " " .. clock(a):gsub(":", ".")
end

-- The clip's, read in the background once it's loaded (2 MiB): published as hash, for the highlights plugin's marks
function M.hash_loaded()
    local clip = state.clip
    local cmd = clip and fs.is_file(clip) and fingerprint_args(clip)
    if not cmd then return end
    mp.command_native_async(cmd, function(ok, r)
        local h = ok and fingerprint_of(r)
        if h and state.clip == clip then
            state.hash = h
            publish()
            -- Its moments asked for again by what it is (links name their original by its fingerprint)
            if not from_catalog and not scanned_project then refresh() end
        end
    end)
end

-- New moments stabilized on their own: this computer's choice (kept with the player's cache), not the project's
function M.moments_alone()
    local alone = not (cache.moments_alone == true)
    cache.moments_alone = alone
    cache_dirty = true
    save_cache()
    local t = { moments_alone = alone }
    state.moments_alone = alone
    publish()
    mp.osd_message(t.moments_alone and "New moments: stabilized on their own" or "New moments: stabilized as part of their clip", 3)
end

function M.install(w)
    W = w
    state, fs, ws, windows, catalog, moments = W.state, W.fs, W.ws, W.windows, W.catalog, W.moments
    -- Where this computer last found a link's clip away from the path the link names (moments.lua's original), by the
    -- clip's identity: kept with the scan's cache
    fs.found_get = function(project, key)
        local f = cache.found
        return type(f) == "table" and f[project .. "\t" .. key] or nil
    end
    fs.found_set = function(project, key, tree)
        if type(cache.found) ~= "table" then cache.found = {} end
        cache.found[project .. "\t" .. key] = tree
        cache_dirty = true
    end
    catalog.changed = refresh
    W.cache, W.tracked_cache, W.save_cache = cache, tracked_cache, save_cache
    W.refresh, W.forget_chapters, W.moment_names, W.remember_name, W.set_last_typed = refresh, forget_chapters, moment_names, remember_name, set_last_typed
    W.history_file, W.suggestion = history_file, suggestion
end

return M
