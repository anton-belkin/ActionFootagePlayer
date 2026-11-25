-- The project lookup kept between the player's runs (mpv/scripts/workspace/projectcache.lua), on a folder tree it
-- fakes: a share's clip folder found through an .afplocation at the share's top, the project on another share.
-- From the repository's folder: luajit tests/projectcache_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local workspace = require 'workspace'
local projectcache = require 'projectcache'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local NOW = 2000000000
-- path -> { dir = true } or { text = "..." }, each with its mtime and size
local tree
local function reset()
    tree = {
        ["/Volumes/photo"] = { dir = true, mtime = 100 },
        ["/Volumes/photo/DATA"] = { dir = true, mtime = 100 },
        ["/Volumes/photo/DATA/Lib.afplocation"] = { text = '{"project":"/Volumes/ssd/Lib","id":"lib"}', mtime = 100, size = 40 },
        ["/Volumes/photo/DATA/Trip"] = { dir = true, mtime = 100 },
        ["/Volumes/photo/DATA/Trip/100GOPRO"] = { dir = true, mtime = 100 },
        ["/Volumes/photo/DATA/Trip/100GOPRO/GX01.MP4"] = { text = "", mtime = 100, size = 1 },
        ["/Volumes/photo/DATA/Trip/100GOPRO/GX02.MP4"] = { text = "", mtime = 100, size = 1 },
        ["/Volumes/ssd"] = { dir = true, mtime = 100 },
        ["/Volumes/ssd/Lib"] = { dir = true, mtime = 100 },
        ["/Volumes/ssd/Lib/Project.afproject"] = { text = '{"id":"lib"}', mtime = 100, size = 12 },
        ["/Volumes/ssd/Other"] = { dir = true, mtime = 100 },
        ["/Volumes/ssd/Other/Project.afproject"] = { text = '{"id":"other"}', mtime = 100, size = 14 },
    }
end

local calls
-- The shares mounted ({ source, dir })
local mounts = {}
local function info(p)
    calls.info = calls.info + 1
    local e = tree[p]
    if not e then return nil end
    return { is_dir = e.dir == true, is_file = not e.dir, mtime = e.mtime, size = e.size or 0 }
end
local function fake_fs()
    return {
        is_dir = function(p) local e = tree[p]; return e ~= nil and e.dir == true end,
        is_file = function(p) local e = tree[p]; return e ~= nil and not e.dir end,
        home = "/Users/me",
        read_json = function(p)
            calls.read = calls.read + 1
            local e = tree[p]
            if not e or e.dir then return nil end
            local project, id = e.text:match('"project":"([^"]*)"'), e.text:match('"id":"([^"]*)"')
            return project and { project = project, id = id } or { id = id }
        end,
        files = function(d)
            calls.list = calls.list + 1
            local out = {}
            for f in pairs(tree) do if f:match("^(.*)/[^/]+$") == d then out[#out + 1] = f:match("([^/]+)$") end end
            return out
        end,
        mounts = function() calls.mounts = calls.mounts + 1; return mounts end,
        projects_dir = "/Users/me/Library/Application Support/ActionFootage/projects",
    }
end

-- What's saved between runs (the JSON file)
local saved
local function copy(t)
    if type(t) ~= "table" then return t end
    local out = {}
    for k, v in pairs(t) do out[k] = copy(v) end
    return out
end
-- One run of the player: a new cache over what's saved, a new workspace
local function run()
    calls = { info = 0, list = 0, read = 0, mounts = 0 }
    local fs = fake_fs()
    local ws = workspace.new(fs)
    local cache = projectcache.new({ info = info, parent = ws.parent, now = function() return NOW end,
                                     load = function() return copy(saved) end, save = function(t) saved = copy(t) end }).install(fs, ws)
    return ws, cache
end

local CLIP = "/Volumes/photo/DATA/Trip/100GOPRO/GX01.MP4"

-- First run: looked up, kept
reset(); saved = nil
local ws, cache = run()
expect("first run finds the project", ws.project_root(CLIP), "/Volumes/ssd/Lib")
expect("first run lists the folders", calls.list, 3)
expect("first run is a miss", cache.misses, 1)
expect("kept", saved and saved.folders and saved.folders["/Volumes/photo/DATA/Trip/100GOPRO"] ~= nil, true)

-- Next run: no listing, no reading, a few stats
ws, cache = run()
expect("second run finds it", ws.project_root(CLIP), "/Volumes/ssd/Lib")
expect("second run lists nothing", calls.list, 0)
expect("second run reads nothing", calls.read, 0)
expect("second run is a hit", cache.hits, 1)
expect("second run's stats", calls.info <= 6, true)
-- Another clip in the same folder: the same answer
expect("same folder, other clip", ws.project_root("/Volumes/photo/DATA/Trip/100GOPRO/GX02.MP4"), "/Volumes/ssd/Lib")
expect("same folder, other clip: no listing", calls.list, 0)

-- A Project.afproject made in the clip's folder (its time changes): looked up again, the new project found
tree["/Volumes/photo/DATA/Trip/100GOPRO/Project.afproject"] = { text = "{}", mtime = 200, size = 2 }
tree["/Volumes/photo/DATA/Trip/100GOPRO"].mtime = 200
ws, cache = run()
expect("folder changed: the new project", ws.project_root(CLIP), "/Volumes/photo/DATA/Trip/100GOPRO")
expect("folder changed: a miss", cache.misses, 1)

-- The .afplocation pointed elsewhere (its time and size change, its folder's not)
reset(); saved = nil
run(); ws.project_root(CLIP)
ws = run(); ws.project_root(CLIP)
tree["/Volumes/photo/DATA/Lib.afplocation"] = { text = '{"project":"/Volumes/ssd/Other","id":"other"}', mtime = 300, size = 44 }
ws, cache = run()
expect("location file changed: the other project", ws.project_root(CLIP), "/Volumes/ssd/Other")
expect("location file changed: a miss", cache.misses, 1)

-- The project taken away (its file gone, its share's folder not looked at)
reset(); saved = nil
ws = run(); ws.project_root(CLIP)
tree["/Volumes/ssd/Lib/Project.afproject"] = nil
ws, cache = run()
expect("project gone: none", ws.project_root(CLIP), nil)
expect("project gone: a miss", cache.misses, 1)

-- No project: kept too, and found once one is made above (the folder that gets it changes)
reset(); saved = nil
tree["/Volumes/photo/DATA/Lib.afplocation"] = nil
ws = run()
expect("no project", ws.project_root(CLIP), nil)
ws, cache = run()
expect("no project, kept", ws.project_root(CLIP), nil)
expect("no project, kept: no listing", calls.list, 0)
tree["/Volumes/photo/DATA/Lib.afplocation"] = { text = '{"project":"/Volumes/ssd/Lib","id":"lib"}', mtime = 400, size = 40 }
tree["/Volumes/photo/DATA"].mtime = 400
ws = run()
expect("a location made above: found", ws.project_root(CLIP), "/Volumes/ssd/Lib")

-- A folder changed this last minute isn't kept (a time in whole seconds can't tell a change later that second)
reset(); saved = nil
tree["/Volumes/photo/DATA/Trip/100GOPRO"].mtime = NOW - 5
ws = run(); ws.project_root(CLIP)
expect("recent folder: not kept", saved == nil or saved.folders["/Volumes/photo/DATA/Trip/100GOPRO"] == nil, true)

-- In one run the answer checked once is taken again without stats
reset(); saved = nil
ws = run(); ws.project_root(CLIP)
ws, cache = run()
ws.project_root(CLIP)
local after_first = calls.info
ws.project_root(CLIP)
expect("checked this run: no stats again", calls.info, after_first)

-- The project's share mounted by another name of the NAS than the pointer has (24 Nov 2026: ssd remounted by IP, the
-- pointer saying DISKSTATIONDXP): found through this computer's record of it, by its id. Mounted by name again, the
-- answer kept before doesn't hold
local RECORD = "/Users/me/Library/Application Support/ActionFootage/projects/lib.json"
reset(); saved = nil
tree["/Volumes/photo/DATA/Lib.afplocation"] = { text = '{"project":"//DISKSTATIONDXP/ssd/Lib","id":"lib"}', mtime = 100, size = 50 }
mounts = { { source = "//antonadmin@192.168.50.186/photo", dir = "/Volumes/photo" },
           { source = "//antonadmin@192.168.50.186/ssd", dir = "/Volumes/ssd" } }
ws = run()
expect("NAS by another name, no record: none", ws.project_root(CLIP), nil)
tree[RECORD] = { text = '{"project":"/Volumes/ssd/Lib"}', mtime = 100, size = 30 }
ws = run()
expect("NAS by another name: found through the record", ws.project_root(CLIP), "/Volumes/ssd/Lib")
ws, cache = run()
expect("NAS by another name: kept", ws.project_root(CLIP), "/Volumes/ssd/Lib")
expect("NAS by another name: kept, a hit", cache.hits, 1)
mounts = { { source = "//antonadmin@DISKSTATIONDXP._smb._tcp.local/photo", dir = "/Volumes/photo" },
           { source = "//antonadmin@DISKSTATIONDXP._smb._tcp.local/ssd", dir = "/Volumes/ssd" } }
ws, cache = run()
expect("mounted by name again: found by its path", ws.project_root(CLIP), "/Volumes/ssd/Lib")
expect("mounted by name again: a miss", cache.misses, 1)
tree["/Volumes/ssd/Lib/Project.afproject"] = { text = '{"id":"p-2"}', mtime = 100, size = 12 }
saved = nil
ws = run()
expect("a project of another id there: none", ws.project_root(CLIP), nil)
tree[RECORD] = nil
-- An answer saved by the version before (it didn't know the mounts): asked again
reset()
saved = { v = 1, folders = { ["/Volumes/photo/DATA/Trip/100GOPRO"] = { projects = {}, deps = {}, at = NOW - 1000 } } }
ws, cache = run()
expect("version 1 answers dropped", ws.project_root(CLIP), "/Volumes/ssd/Lib")
mounts = {}

print(("%d checks, %d failures"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
