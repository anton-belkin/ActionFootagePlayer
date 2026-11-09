-- Which project a clip belongs to, and where that project's files for it are (docs/moments-and-workspaces.md).
-- Plain Lua with no mpv in it, so that luajit tests it (tests/workspace_test.lua) over the same cases as its Rust
-- twin in gyroflow-export (vs-gyroflow/src/workspace.rs): keep the two in step.
--
-- A project is a folder with `Project.afproject` in it. Its footage is in its locations (docs/library-management.md):
-- folders, each placed at a point of the project's tree, in an order, the first that has a clip winning. Without
-- locations, the project's own folder is its one location, at the top of the tree: the trip folder, the footage under
-- it. What belongs to a clip (its Gyroflow project, its own file: marks, input, its view) lives beside the clip, one
-- per clip over all the locations. A clip finds its project from the folders above it: a Project.afproject, or a
-- `<name>.afplocation` file at the top of a location, which says where its project is. Before 0.9 the mark was a
-- folder, `_ActionFootagePlayer/`: one still makes a project (projectfile.lua moves it into the file).
--
--   local ws = require('workspace').new({ is_dir = f, is_file = f, home = "/Users/me", windows = false,
--                                         read_json = f, files = f, mounts = f })
--   ws.project_root("/Volumes/NAS/Egypt/Day 1/A.MP4")   -> "/Volumes/NAS/Egypt" or nil
--   ws.locations(project)                             -> its locations here: { name, at, dir, copies }
--   ws.tree_path(clip, locations)                     -> the clip's path in the project's tree ("Action 6/A.MP4")
--   ws.files_at(tree_path, locations)                 -> the file at that path in each location that has it
--   ws.own_file(clip, locations)                      -> where the clip's own file is read and written, or nil, list
--                                                        when there are two
--
-- read_json(path) -> a table or nil; files(dir) -> the names of the files in it; mounts() -> { { source =
-- "//host/share", dir = "/Volumes/share" } } (the shares this computer has mounted; not on Windows, which opens
-- \\host\share itself). Without them only projects next to their footage are found.
--   ws.new_project_root(clip)                         -> where a project for the clip would be made
--   ws.gyroflow_projects(clip)                        -> the clip's Gyroflow project, the paths it may have
--   ws.gyroflow_new(clip)                             -> where a new one is written

local M = {}

-- The file a project's state is kept in, which also marks a folder as one, and which a double click opens in Action
-- Footage Library. Not hidden: the project is its files, where the user sees them
M.PROJECT_FILE = "Project.afproject"
-- The folder it was before 0.9
M.LEGACY_DIR = "_ActionFootagePlayer"
-- The file at the top of a location that says which project it's in (one per project that uses the location)
M.LOCATION_EXT = ".afplocation"

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

    -- A network path's parts: "//nas/video/Footage" or "\\\\nas\\video\\Footage" -> "nas", "video", "Footage"
    local function network(path)
        local host, share, rest = path:match("^[/\\][/\\]([^/\\]+)[/\\]([^/\\]+)[/\\]?(.*)$")
        return host, share, rest
    end
    -- A host's name as a mount has it, compared plainly: "antonadmin@DISKSTATIONDXP._smb._tcp.local" -> "diskstationdxp"
    local function host_key(h)
        h = (h or ""):lower():gsub("^[^@]*@", "")
        return (h:gsub("%._smb%._tcp%.local%.?$", ""):gsub("%.local%.?$", ""):gsub("%.$", ""))
    end
    local mounts_cache = nil
    local function mounted()
        if not mounts_cache then mounts_cache = fs.mounts and fs.mounts() or {} end
        return mounts_cache
    end

    -- Where a location's path is on this computer: a network path through the share mounted (`hosts`: other names
    -- of a host), one relative to the project folder, or an absolute one as it is. nil: not reachable here
    function ws.place(path, project, hosts)
        if type(path) ~= "string" or path == "" then return nil end
        local host, share, rest = network(path)
        if host then
            if windows then return ws.normalize("\\\\" .. host .. "\\" .. share .. (rest ~= "" and "\\" .. rest or "")) end
            local names = { [host_key(host)] = true }
            for k, list in pairs(type(hosts) == "table" and hosts or {}) do
                local all = { k }
                for _, x in ipairs(type(list) == "table" and list or {}) do all[#all + 1] = x end
                local hit = false
                for _, x in ipairs(all) do if host_key(x) == host_key(host) then hit = true end end
                if hit then for _, x in ipairs(all) do names[host_key(x)] = true end end
            end
            for _, m in ipairs(mounted()) do
                local mh, ms = network(m.source or "")
                if mh and names[host_key(mh)] and ms:lower() == share:lower() then
                    return ws.normalize(rest ~= "" and ws.join(m.dir, rest) or m.dir)
                end
            end
            return nil
        end
        if is_absolute(path) then return ws.normalize(path) end
        return project and ws.normalize(ws.join(project, path)) or nil
    end

    local function read_doc(project)
        return fs.read_json and fs.read_json(ws.join(project, M.PROJECT_FILE)) or nil
    end

    -- The project's locations on this computer, in their order: { name, at ("" or "Action 6"), dir (nil when not
    -- reachable here), copies }. A project without any: its own folder, at the top
    function ws.locations(project, doc)
        doc = doc or read_doc(project) or {}
        local list = {}
        if type(doc.locations) == "table" and #doc.locations > 0 then
            for _, l in ipairs(doc.locations) do
                if type(l) == "table" then
                    local at = (type(l.at) == "string" and l.at or ""):gsub("^/+", ""):gsub("/+$", "")
                    list[#list + 1] = { name = l.name, at = at, dir = ws.place(l.path, project, doc.hosts), copies = l.copies == true, path = l.path }
                end
            end
        else
            list[1] = { name = "Project", at = "", dir = project, copies = false, path = "." }
        end
        return list
    end

    local function under(file, dir)
        local prefix = is_sep(dir:sub(-1)) and dir or dir .. SEP
        if not same(file:sub(1, #prefix), prefix) then return nil end
        local rel = file:sub(#prefix + 1)
        return windows and rel:gsub("\\", "/") or rel
    end

    -- The clip's path in the project's tree: the location holding it (the one whose folder is deepest, when one is
    -- in another), its place plus the path under it. nil: in none of them
    function ws.tree_path(clip, locations)
        local best, best_len = nil, -1
        for _, l in ipairs(locations or {}) do
            if l.dir then
                local rel = under(clip, l.dir)
                if rel and #l.dir > best_len then
                    best, best_len = (l.at ~= "" and l.at .. "/" .. rel or rel), #l.dir
                end
            end
        end
        return best
    end

    -- The file at a path of the tree in each location that has it, in the locations' order: { { file, location } }
    function ws.files_at(tree, locations)
        local out = {}
        for _, l in ipairs(locations or {}) do
            if l.dir then
                local rel
                if l.at == "" then rel = tree
                elseif tree:sub(1, #l.at + 1) == l.at .. "/" then rel = tree:sub(#l.at + 2) end
                if rel then
                    local file = ws.join(l.dir, windows and rel:gsub("/", "\\") or rel)
                    if fs.is_file(file) then out[#out + 1] = { file = file, location = l } end
                end
            end
        end
        return out
    end

    -- Where the clip's own file is, over all the locations: the one there is; none, beside the clip in the first
    -- location that has it and isn't copies only (where it's written at the first save). Two or more: nil, and their
    -- list (the player then saves nothing for the clip until one goes). Without locations: beside the clip
    function ws.own_file(clip, locations)
        local tree = locations and ws.tree_path(clip, locations)
        if not tree then return ws.sidecar_file(clip) end
        local copies = ws.files_at(tree, locations)
        local found, home = {}, nil
        for _, c in ipairs(copies) do
            local own = ws.sidecar_file(c.file)
            if fs.is_file(own) then found[#found + 1] = own end
            if not home and not c.location.copies then home = own end
        end
        -- The older name, <clip>.marks.json, counts as the clip's own file too
        if #found == 0 then
            for _, c in ipairs(copies) do
                local old = ws.sidecar_file(c.file):gsub("%.afp%.json$", ".marks.json")
                if fs.is_file(old) then found[#found + 1] = ws.sidecar_file(c.file) end
            end
        end
        if #found > 1 then return nil, found end
        return found[1] or home or ws.sidecar_file(clip)
    end

    -- The projects whose markers are in a folder: its Project.afproject, its .afplocation files' projects (those that
    -- exist), in the order of their names
    local function projects_in(dir)
        local out = {}
        if fs.is_file(ws.join(dir, M.PROJECT_FILE)) or fs.is_dir(ws.join(dir, M.LEGACY_DIR)) then out[#out + 1] = dir end
        if fs.files and fs.read_json then
            local names = {}
            for _, n in ipairs(fs.files(dir) or {}) do
                if n:lower():sub(-#M.LOCATION_EXT) == M.LOCATION_EXT and n:sub(1, 1) ~= "." then names[#names + 1] = n end
            end
            table.sort(names)
            for _, n in ipairs(names) do
                local p = fs.read_json(ws.join(dir, n))
                local where = type(p) == "table" and ws.place(p.project, dir, p.hosts)
                if where and fs.is_file(ws.join(where, M.PROJECT_FILE)) then
                    local dup = false
                    for _, x in ipairs(out) do if same(x, where) then dup = true end end
                    if not dup then out[#out + 1] = where end
                end
            end
        end
        return out
    end

    -- The projects the clip (a normalized absolute path) belongs to, all found in the nearest folder above it (or its
    -- own) that has any: a Project.afproject (the project next to its footage), or .afplocation files. The search
    -- ends at the volume's top folder (included: a card's project is on its top) or at the home folder (not
    -- included: a project of everything in it would be no trip). {}: none
    function ws.projects_of(clip)
        if not clip or clip:find("://", 1, true) then return {} end
        local home = fs.home and ws.normalize(fs.home)
        local dir = ws.parent(clip)
        while dir do
            if home and same(dir, home) then return {} end
            local found = projects_in(dir)
            if #found > 0 then return found end
            if ws.is_volume_root(dir) then return {} end
            dir = ws.parent(dir)
        end
        return {}
    end

    -- The first of them (the one a clip opened from Finder goes to unless chosen otherwise), nil: none
    function ws.project_root(clip)
        return ws.projects_of(clip)[1]
    end

    -- The folders a project for the clip can be started in, from the top down: the volume's top folder (or the
    -- folder below the home folder) to the clip's own. A project there covers every folder in it
    function ws.project_choices(clip)
        local list = {}
        local home = fs.home and ws.normalize(fs.home)
        local dir = ws.parent(clip)
        while dir do
            if home and same(dir, home) then break end
            table.insert(list, 1, dir)
            if ws.is_volume_root(dir) then break end
            dir = ws.parent(dir)
        end
        return list
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

    -- The clip's own file, its marks and the view it's saved to play in: <name>.afp.json next to it (sidecar.lua; in
    -- a separate project folder, later, at its place there)
    function ws.sidecar_file(clip)
        local base = clip:match(windows and "^(.*)%.[^./\\]+$" or "^(.*)%.[^./]+$") or clip
        return base .. ".afp.json"
    end

    return ws
end

return M
