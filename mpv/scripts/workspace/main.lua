-- workspace: which project the clip that plays belongs to, and where that project's files for it are, for the other
-- plugins (docs/moments-and-workspaces.md). An mpv plugin: this folder goes into ~~/scripts/.
--
-- A project is a trip folder with `.actionfootageplayer/` in it: the nearest such folder above the clip, up to the
-- volume's top folder, and never the home folder itself. A clip in none has no project, and plays as before.
--
-- Before the other plugins load the clip (on_load, early), it's published in user-data/workspace/:
--   path            what mpv plays (its `path`): the rest is for this file only, so a plugin checks it's its own
--   clip            the clip itself, as an absolute path
--   project         the project's folder, absent for none
--   relative        the clip's path in it ("Day 1/A.MP4"), absent for none
--   gyroflow_projects  the paths the clip's Gyroflow project may have, in the order looked at: the gyroflow plugin
--                      stabilizes with the first that exists, then or later (one made while it plays)
--   gyroflow_new    where a new Gyroflow project for it goes (Ctrl+e, the gyroflow plugin)
-- Without this plugin, the gyroflow plugin looks next to the clip itself, which is the same place for now.
--
-- workspace.lua is the lookup itself (plain Lua; tests/workspace_test.lua), and gyroflow-export has its twin
-- (vs-gyroflow/src/workspace.rs) for the Finder quick action and the command line.

local mp = require 'mp'
local utils = require 'mp.utils'
local workspace = require 'workspace'

local windows = mp.get_property("platform") == "windows" or package.config:sub(1, 1) == "\\"

local ws = workspace.new({
    is_dir = function(p)
        local info = utils.file_info(p)
        return info ~= nil and info.is_dir
    end,
    is_file = function(p)
        local info = utils.file_info(p)
        return info ~= nil and info.is_file
    end,
    home = os.getenv(windows and "USERPROFILE" or "HOME"),
    windows = windows,
})

-- What's known about the file at `path`, as published
local function describe(path)
    if not path or path == "" then return {} end
    if path:find("://", 1, true) then return { path = path } end
    local clip = ws.absolute(path, mp.get_property("working-directory"))
    local root = ws.project_root(clip)
    return {
        path = path,
        clip = clip,
        project = root,
        relative = root and ws.relative(clip, root) or nil,
        gyroflow_projects = ws.gyroflow_projects(clip),
        gyroflow_new = ws.gyroflow_new(clip),
    }
end

local function publish(path)
    mp.set_property_native("user-data/workspace", describe(path))
end

-- Before the gyroflow plugin's (50): it reads this to find the clip's Gyroflow project
mp.add_hook("on_load", 10, function() publish(mp.get_property("path")) end)
