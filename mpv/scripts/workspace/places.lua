-- places: where things are, for the workspace plugin (main.lua wires it): the platform, the file calls the
-- project lookup makes (fs: each gives mpv a turn while the project is counted), the lookup itself (workspace.lua's,
-- ws), the project's locations, a file's path in its project's tree, a clip's own file, a link's original
local mp = require 'mp'
local utils = require 'mp.utils'
local workspace = require 'workspace'
local link = require 'link'
local moments_lib = require 'moments'

local M = {}
-- Set by install: what plays (main.lua's W.state)
local state

local platform = mp.get_property("platform") or ""
local windows = platform == "windows" or package.config:sub(1, 1) == "\\"
local macos = platform == "darwin"

-- The project counted for the side panel (clippreviews.lua's extras) runs as a coroutine that gives mpv a turn every
-- TURN_CALLS of these file calls: on a NAS each is a round trip, a few hundred clips take a minute, and mpv waits for
-- a script to finish what it's doing before it quits or the next clip's hooks run
local TURN_CALLS = 16
local counting = { co = nil, calls = 0 }
local function turn()
    if counting.co and coroutine.running() == counting.co then
        counting.calls = counting.calls + 1
        if counting.calls % TURN_CALLS == 0 then coroutine.yield() end
    end
end

local fs = {
    is_dir = function(p)
        turn()
        local info = utils.file_info(p)
        return info ~= nil and info.is_dir
    end,
    is_file = function(p)
        turn()
        local info = utils.file_info(p)
        return info ~= nil and info.is_file
    end,
    info = function(p) turn(); return utils.file_info(p) end,
    readdir = function(p, what) turn(); return utils.readdir(p, what) end,
    read_link = function(p)
        turn()
        local ok, raw = pcall(link.read, p)
        return ok and raw or nil
    end,
    parse_json = function(s) return (utils.parse_json(s)) end,
    home = os.getenv(windows and "USERPROFILE" or "HOME"),
    windows = windows,
}
-- For the project's locations (workspace.lua): a JSON file read, a folder's files, the shares mounted here
fs.read_json = function(p)
    turn()
    local f = io.open(p, "r")
    if not f then return nil end
    local t = f:read("*a") or ""
    f:close()
    local v = utils.parse_json(t)
    return type(v) == "table" and v or nil
end
-- A folder's names, for the project lookup (its .afplocation files: workspace.lua's projects_in, in every folder
-- from the clip's up to its project's). Its names only ("all": readdir "files" stats each entry, 2-4 ms each on the
-- NAS, 5.6 s for a GoPro folder of 1689 files; a folder named .afplocation reads as no JSON). A clip's lookup runs a
-- few times as it opens (twice for a clip preview), and again for the next clip: kept while the folder's
-- modification time stays the same (it changes as a file is added or removed), a few minutes at most; between runs
-- the lookup's answer is kept (projectcache.lua, below)
local listed = {}
local LISTED_SECONDS = 300
fs.files = function(p)
    turn()
    local info = utils.file_info(p)
    local c = listed[p]
    if c and info and c.mtime == info.mtime and mp.get_time() - c.at < LISTED_SECONDS then return c.names end
    local names = utils.readdir(p, "all")
    listed[p] = info and names and { names = names, mtime = info.mtime, at = mp.get_time() } or nil
    return names
end
-- Kept a few seconds: a lookup asks for them per share path (workspace.lua's place) and the project cache once per
-- answer it checks (projectcache.lua), a run of /sbin/mount each
fs.mounts = function()
    local kept = fs.mounts_kept
    if kept and mp.get_time() - kept.at < 10 then return kept.list end
    local out = {}
    fs.mounts_kept = { list = out, at = mp.get_time() }
    -- Shares reached otherwise than mounted from the network (a container on the NAS itself, where they're its own
    -- folders, bound in): AFP_SHARES="//NAS/photo=/shares/photo;//NAS/DJI_Inbox=/shares/DJI_Inbox" (afp-stream)
    for src, dir in (os.getenv("AFP_SHARES") or ""):gmatch("(//[^=;]+)=([^;]+)") do out[#out + 1] = { source = src, dir = dir } end
    if windows then return out end
    local text
    if macos then
        local r = mp.command_native({ name = "subprocess", args = { "/sbin/mount" }, playback_only = false, capture_stdout = true, capture_stderr = true })
        text = r and r.stdout or ""
        for src, dir in text:gmatch("(//[^\n]-) on (/[^\n]-) %(") do out[#out + 1] = { source = src, dir = dir } end
    else
        local f = io.open("/proc/mounts", "r")
        text = f and f:read("*a") or ""
        if f then f:close() end
        for src, dir in text:gmatch("(//%S+) (%S+) cifs") do
            out[#out + 1] = { source = src:gsub("\\040", " "), dir = dir:gsub("\\040", " ") }
        end
    end
    return out
end
-- The Library's records of this computer's projects, by id (where each one's folder is here): a pointer whose path
-- doesn't reach its project finds it there (workspace.lua's known)
fs.projects_dir = (windows and os.getenv("APPDATA") or macos and (os.getenv("HOME") or "") .. "/Library/Application Support"
                   or os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") or "") .. "/.config") .. "/ActionFootage/projects"
local ws = workspace.new(fs)
-- The project a clip's folder is in, kept between the player's runs (each Library double click is a new one) while the
-- folders and files it was found from are as they were: a stat each instead of the lookup
require('projectcache').new({
    info = utils.file_info, parent = ws.parent, now = os.time, clock = mp.get_time,
    load = function()
        local path = mp.command_native({ "expand-path", "~~cache/actionfootageplayer-projects.json" })
        local f = path and io.open(path, "r")
        if not f then return nil end
        local t = utils.parse_json(f:read("*a") or "")
        f:close()
        return t
    end,
    save = function(t)
        local path = mp.command_native({ "expand-path", "~~cache/actionfootageplayer-projects.json" })
        local f = path and io.open(path .. ".tmp", "w")
        if not f then return end
        f:write(utils.format_json(t) or "{}")
        f:close()
        os.remove(path)
        os.rename(path .. ".tmp", path)
    end,
}).install(fs, ws)
-- A path in a project ("Day 1/A.MP4") with this system's separators
local function native(rel) return windows and (rel:gsub("/", "\\")) or rel end
-- A project's locations, read again when its file changes
local locs_cache = {}
local function locations_of(root)
    if not root then return nil end
    local info = utils.file_info(ws.join(root, workspace.PROJECT_FILE))
    local stamp = info and (tostring(info.mtime) .. "/" .. tostring(info.size)) or "-"
    local c = locs_cache[root]
    if c and c.stamp == stamp then return c.list end
    local doc = fs.read_json(ws.join(root, workspace.PROJECT_FILE)) or {}
    local list = ws.locations(root, doc)
    local never_rename = false
    local tag_stores = nil
    -- This computer's own copies of the project's clips (the Library's On this computer), looked at first
    local id = type(doc.id) == "string" and doc.id:match("^[%w%-]+$")
    if id then
        local base = windows and os.getenv("APPDATA") or macos and (os.getenv("HOME") or "") .. "/Library/Application Support"
                     or os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") or "") .. "/.config"
        local mine = fs.read_json(ws.join(ws.join(ws.join(base, "ActionFootage"), "projects"), id .. ".json"))
        never_rename = mine and mine.never_rename == true
        tag_stores = mine and type(mine.tag_stores) == "table" and mine.tag_stores or nil
        if mine and type(mine.locations) == "table" then
            for i = #mine.locations, 1, -1 do
                local l = mine.locations[i]
                if type(l) == "table" and type(l.path) == "string" then
                    table.insert(list, 1, { name = l.name, at = (type(l.at) == "string" and l.at or ""):gsub("^/+", ""):gsub("/+$", ""),
                                            dir = ws.place(l.path, root), copies = true, path = l.path, here = true })
                end
            end
        end
    end
    locs_cache[root] = { stamp = stamp, list = list, never_rename = never_rename, tag_stores = tag_stores }
    return list
end
fs.locations_of = locations_of
-- "I never rename my footage" (the Library's Project settings, this computer's choices): a clip's name counts when
-- it's looked for (moments.lua's original)
fs.never_rename = function(root)
    locations_of(root)
    return locs_cache[root] ~= nil and locs_cache[root].never_rename
end
-- The tag stores this computer syncs for the project (tags.lua; the Library's Project settings → Tags): `tag_stores`
-- in this computer's own file for it, never Project.afproject
fs.tag_stores = function(root)
    if not root then return {} end
    locations_of(root)
    return locs_cache[root] and locs_cache[root].tag_stores or {}
end
fs.created = link.created
fs.read_photo_link = link.photo
-- A file's path in its project's tree ("Action 6/A.MP4"): the location holding it, its place plus the path under it
local function tree_of(file, root)
    if not file or not root then return nil end
    return ws.tree_path(file, locations_of(root))
end
local moments = moments_lib.new(ws, fs)
-- The clip's own file is wherever it is over the project's locations (workspace.lua's own_file): beside the archive's
-- clip while its working copy plays; two of them, none written
local function own_path(clip)
    if state and state.clip == clip and state.project then
        if state.sidecar_file then return state.sidecar_file end
    end
    local root = ws.project_root(clip)
    if not root then return nil end
    return ws.own_file(clip, locations_of(root))
end

-- A link's clip (moments.lua's original); found away from the path the link names, the path is written into the
-- link (docs/design/library-management.md §8), so that the next look finds it at once, from any computer. Not for a clip
-- found in this computer's own copy, nor where the file can't be written (it's remembered here then)
-- A moment's (clip preview's, export's) original: where its link says. Originals move only when the library is
-- mended after (the Library's Restore, which finds them by what they are and rewrites the links: docs §9)
local function original_of(l, project, file)
    local original = moments.original(l, project, { hint_only = true })
    if not original and type(l) == "table" and l.clip then
        mp.msg.warn("the original " .. tostring(l.clip) .. " isn't where the link of " .. tostring(file) .. " says")
        mp.osd_message("Its original (" .. tostring(l.clip) .. ") isn't where it was: the Library's Restore finds moved originals", 6)
    end
    return original
end

-- A moment's or clip preview's own Gyroflow project: beside it, its name with .gyroflow (Turtle.gyroflow beside
-- Turtle.afplink.mp4)
local function own_project(file)
    return ws.join(ws.parent(file), moments_lib.display_name(file) .. ".gyroflow")
end

function M.install(W)
    state = W.state
    W.windows, W.macos, W.fs, W.ws, W.moments, W.counting = windows, macos, fs, ws, moments, counting
    W.native, W.locations_of, W.tree_of, W.own_path, W.original_of, W.own_project = native, locations_of, tree_of, own_path, original_of, own_project
end

return M
