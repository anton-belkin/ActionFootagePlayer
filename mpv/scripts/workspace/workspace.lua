-- Which project a clip belongs to, and where that project's files for it are (docs/moments-and-workspaces.md).
-- Plain Lua with no mpv in it, so that luajit tests it (tests/workspace_test.lua) over the same cases as its Rust
-- twin in gyroflow-export (vs-gyroflow/src/workspace.rs): keep the two in step.
--
-- A project is a folder with `.actionfootageplayer/` in it, the trip folder: its footage is under it, and the
-- player's files for a clip sit at the clip's own path in it (its Gyroflow project, its marks), next to the clip.
-- Separate project folders (Sources.txt) come later; every path here is already the same for them.
--
--   local ws = require('workspace').new({ is_dir = f, is_file = f, home = "/Users/me", windows = false })
--   ws.project_root("/Volumes/NAS/Egypt/Day 1/A.MP4")   -> "/Volumes/NAS/Egypt" or nil
--   ws.new_project_root(clip)                         -> where a project for the clip would be made
--   ws.gyroflow_projects(clip)                        -> the clip's Gyroflow project, the paths it may have
--   ws.gyroflow_new(clip)                             -> where a new one is written

local M = {}

-- The folder a project's state is kept in, which also marks a folder as one
M.STATE_DIR = ".actionfootageplayer"

function M.new(fs)
    local ws = {}
    local windows = fs.windows
    local SEP = windows and "\\" or "/"

    local function is_sep(c) return c == "/" or (windows and c == "\\") end

    -- A path's root part: "/", "C:\", "\\server\share\", or "" for a relative one
    local function root_of(path)
        if windows then
            local unc = path:match("^[/\\][/\\][^/\\]+[/\\][^/\\]+[/\\]?")
            if unc then return unc end
            local drive = path:match("^%a:[/\\]?")
            if drive then return drive end
        end
        return is_sep(path:sub(1, 1)) and path:sub(1, 1) or ""
    end

    local function is_absolute(path)
        if windows then return root_of(path):match("[/\\]$") ~= nil or path:match("^[/\\][/\\]") ~= nil end
        return path:sub(1, 1) == "/"
    end

    -- The path with "." and ".." resolved and one separator between parts (the platform's own on Windows)
    function ws.normalize(path)
        local root = root_of(path)
        local parts = {}
        for part in path:sub(#root + 1):gmatch(windows and "[^/\\]+" or "[^/]+") do
            if part == ".." then
                if #parts > 0 and parts[#parts] ~= ".." then
                    table.remove(parts)
                elseif root == "" then
                    table.insert(parts, part)
                end
            elseif part ~= "." then
                table.insert(parts, part)
            end
        end
        if windows then
            root = root:gsub("/", "\\")
            if root:match("^%a:$") then root = root .. "\\" end
            if root:match("^\\\\") and not root:match("\\$") then root = root .. "\\" end
        end
        local rest = table.concat(parts, SEP)
        if root == "" and rest == "" then return "." end
        return root .. rest
    end

    -- path made absolute against cwd, then normalized
    function ws.absolute(path, cwd)
        if not is_absolute(path) and cwd then path = cwd .. SEP .. path end
        return ws.normalize(path)
    end

    function ws.join(dir, name)
        if is_sep(dir:sub(-1)) then return dir .. name end
        return dir .. SEP .. name
    end

    -- The folder a normalized path is in, nil for a root
    function ws.parent(path)
        local root = root_of(path)
        if #path <= #root then return nil end
        local i = #path
        while i > #root and not is_sep(path:sub(i, i)) do i = i - 1 end
        if i <= #root then return root ~= "" and root or "." end
        return path:sub(1, i - 1)
    end

    function ws.name(path)
        return path:match(windows and "([^/\\]+)$" or "([^/]+)$") or path
    end

    local function same(a, b)
        if windows then return a:lower() == b:lower() end
        return a == b
    end

    -- A volume's top folder, where looking further up for a project ends: a disk or a share as the system mounts
    -- it (/Volumes/<name>, /media/<user>/<name>, /run/media/<user>/<name>, /mnt/<name>, C:\, \\nas\video), or /
    function ws.is_volume_root(dir)
        if dir == "/" then return true end
        if windows then
            return dir:match("^%a:[/\\]?$") ~= nil or dir:match("^[/\\][/\\][^/\\]+[/\\][^/\\]+[/\\]?$") ~= nil
        end
        return dir:match("^/Volumes/[^/]+$") ~= nil or dir:match("^/media/[^/]+/[^/]+$") ~= nil
            or dir:match("^/run/media/[^/]+/[^/]+$") ~= nil or dir:match("^/mnt/[^/]+$") ~= nil
    end

    -- The project the clip (a normalized absolute path) belongs to: the nearest folder above it, or its own, with
    -- the state folder in it. The search ends at the volume's top folder (included: a card's project is on its
    -- top) or at the home folder (not included: a project of everything in it would be no trip). nil: none
    function ws.project_root(clip)
        if not clip or clip:find("://", 1, true) then return nil end
        local home = fs.home and ws.normalize(fs.home)
        local dir = ws.parent(clip)
        while dir do
            if home and same(dir, home) then return nil end
            if fs.is_dir(ws.join(dir, M.STATE_DIR)) then return dir end
            if ws.is_volume_root(dir) then return nil end
            dir = ws.parent(dir)
        end
        return nil
    end

    -- Where a project for the clip would be made at the first save: the clip's folder, or the folder above DCIM
    -- for a clip in a camera's own tree (DCIM/100GOPRO, DCIM/DJI_001), the card or a copy of it
    function ws.new_project_root(clip)
        local dir = ws.parent(clip)
        local up = dir
        for _ = 1, 2 do
            if not up then break end
            if ws.name(up):upper() == "DCIM" then return ws.parent(up) or up end
            up = ws.parent(up)
        end
        return dir
    end

    -- The clip's path relative to its project's folder ("Day 1/A.MP4", with "/" on every system), nil when it
    -- isn't under it
    function ws.relative(clip, root)
        local prefix = is_sep(root:sub(-1)) and root or root .. SEP
        if not same(clip:sub(1, #prefix), prefix) then return nil end
        local rel = clip:sub(#prefix + 1)
        return windows and rel:gsub("\\", "/") or rel
    end

    -- The clip's Gyroflow project, the paths it may have, in the order looked at: <name>.gyroflow and
    -- <name.ext>.gyroflow, next to the clip (Gyroflow's own two ways of naming it). In a separate project folder
    -- this will be the same names at the clip's place in it
    function ws.gyroflow_projects(clip)
        local base = clip:match(windows and "^(.*)%.[^./\\]+$" or "^(.*)%.[^./]+$") or clip
        return { base .. ".gyroflow", clip .. ".gyroflow" }
    end

    -- The first of them that exists, nil: none
    function ws.gyroflow_project(clip)
        if not clip or clip:find("://", 1, true) then return nil end
        for _, candidate in ipairs(ws.gyroflow_projects(clip)) do
            if fs.is_file(candidate) then return candidate end
        end
        return nil
    end

    -- Where a new Gyroflow project for the clip goes (Ctrl+e): the first name
    function ws.gyroflow_new(clip)
        return ws.gyroflow_projects(clip)[1]
    end

    return ws
end

return M
