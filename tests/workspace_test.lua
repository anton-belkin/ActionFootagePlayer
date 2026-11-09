-- The workspace plugin's library over tests/workspace_cases.txt, on a folder tree it fakes, plus its path handling
-- on Windows paths. From the repository's folder: luajit tests/workspace_test.lua (or lua 5.1 and later)

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local workspace = require 'workspace'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- The cases' file, read as its comment says
local BASE = "/base"
local cases, case = {}, nil
local section
for line in io.lines("tests/workspace_cases.txt") do
    if line:match("^#") or line:match("^%s*$") then
        -- skip
    elseif line:match("^case ") then
        case = { name = line:sub(6), dirs = {}, files = {}, texts = {}, clips = {}, volumes = {}, mounts = {} }
        table.insert(cases, case)
        section = nil
    elseif line:match("^home ") then
        case.home = line:sub(6)
    elseif line:match("^mount ") then
        local source, dir = line:match("^mount (%S+) (.-)%s*$")
        table.insert(case.mounts, { source = source, dir = dir })
    elseif line == "tree" then
        section = "tree"
    elseif line:match("^clip ") then
        section = { path = line:sub(6), expect = {} }
        table.insert(case.clips, section)
    elseif line:match("^volume ") then
        local system, path, answer = line:match("^volume (%S+) (.-) (%S+)$")
        table.insert(case.volumes, { system = system, path = path, yes = answer == "yes" })
    elseif section == "tree" then
        local path = line:match("^%s+(.-)%s*$")
        local name, text = path:match("^(.-) = (.*)$")
        if name then case.files[name] = true; case.texts[name] = text
        elseif path:sub(-1) == "/" then case.dirs[path:sub(1, -2)] = true else case.files[path] = true end
    elseif type(section) == "table" then
        local key, value = line:match("^%s+(%S+) (.-)%s*$")
        table.insert(section.expect, { key, value })
    end
end

-- JSON as the cases write it, made a Lua table: enough for objects, arrays, strings, numbers, true and false
local function json(text)
    local lua = text:gsub('"([^"]*)"%s*:', '["%1"]='):gsub("%[", "{"):gsub("%]", "}"):gsub('{"', '{"'):gsub("{%[", "{[")
    -- The keys' brackets turned to braces above: put them back
    lua = lua:gsub('{{"([^"]*)"}=', '{["%1"]='):gsub(',{"([^"]*)"}=', ',["%1"]=')
    local f = (loadstring or load)("return " .. lua)
    local ok, v = pcall(f)
    return ok and v or nil
end

local function in_base(rel)
    if rel == "-" then return nil end
    return BASE .. "/" .. rel
end

for _, c in ipairs(cases) do
    -- Every folder a path is in is a folder too
    local dirs = { [BASE] = true }
    local function add_parents(rel)
        local dir = rel:match("^(.*)/[^/]+$")
        while dir do
            dirs[BASE .. "/" .. dir] = true
            dir = dir:match("^(.*)/[^/]+$")
        end
    end
    for rel in pairs(c.dirs) do dirs[BASE .. "/" .. rel] = true; add_parents(rel) end
    local files = {}
    for rel in pairs(c.files) do files[BASE .. "/" .. rel] = true; add_parents(rel) end
    local texts = {}
    for rel, t in pairs(c.texts) do texts[BASE .. "/" .. rel] = t end
    local mounts = {}
    for _, m in ipairs(c.mounts) do mounts[#mounts + 1] = { source = m.source, dir = BASE .. "/" .. m.dir } end
    local ws = workspace.new({
        is_dir = function(p) return dirs[p] == true end,
        is_file = function(p) return files[p] == true end,
        home = c.home and in_base(c.home) or BASE,
        windows = false,
        read_json = function(p) return texts[p] and json(texts[p]) or nil end,
        files = function(d)
            local out = {}
            for f in pairs(files) do if f:match("^(.*)/[^/]+$") == d then out[#out + 1] = f:match("([^/]+)$") end end
            return out
        end,
        mounts = function() return mounts end,
    })
    for _, clip in ipairs(c.clips) do
        local path = in_base(clip.path)
        for _, e in ipairs(clip.expect) do
            local key, value, what = e[1], e[2], c.name .. ": " .. clip.path .. " " .. e[1]
            if key == "project" then
                expect(what, ws.project_root(path), in_base(value))
            elseif key == "relative" then
                local root = ws.project_root(path)
                expect(what, root and ws.tree_path(path, ws.locations(root)), value)
            elseif key == "projects" then
                local list = {}
                for i, p in ipairs(ws.projects_of(path)) do list[i] = p:sub(#BASE + 2) end
                expect(what, table.concat(list, "|"), value)
            elseif key == "original" then
                local tree, file = value:match("^(.-) (%S+/%S.-)$")
                tree, file = value:match("^(.-%.%w+) (.+)$")
                local got = ws.files_at(tree, ws.locations(ws.project_root(path)))[1]
                expect(what, got and got.file, in_base(file))
            elseif key == "own" then
                local own, list = ws.own_file(path, ws.locations(ws.project_root(path)))
                expect(what, own or (list and "conflict"), value == "conflict" and "conflict" or in_base(value))
            elseif key == "new" then
                expect(what, ws.new_project_root(path), in_base(value))
            elseif key == "gyroflow" then
                expect(what, ws.gyroflow_project(path), in_base(value))
            elseif key == "gyroflow-new" then
                expect(what, ws.gyroflow_new(path), in_base(value))
            elseif key == "sidecar" then
                expect(what, ws.sidecar_file(path), in_base(value))
            else
                error("unknown expectation " .. key)
            end
        end
    end
    for _, v in ipairs(c.volumes) do
        local ws_v = workspace.new({ is_dir = function() return false end, is_file = function() return false end,
                                     windows = v.system == "windows" })
        expect(c.name .. ": " .. v.system .. " " .. v.path, ws_v.is_volume_root(v.path), v.yes)
    end
end

-- Paths: normalized, made absolute, split, made relative
local unix = workspace.new({ is_dir = function() return false end, is_file = function() return false end })
expect("unix normalize", unix.normalize("/a/./b//c/../d"), "/a/b/d")
expect("unix normalize root", unix.normalize("/.."), "/")
expect("unix absolute", unix.absolute("Day 1/../A.MP4", "/Volumes/NAS/Egypt"), "/Volumes/NAS/Egypt/A.MP4")
expect("unix absolute kept", unix.absolute("/x/A.MP4", "/elsewhere"), "/x/A.MP4")
expect("unix parent", unix.parent("/Volumes/NAS"), "/Volumes")
expect("unix parent of top", unix.parent("/Volumes"), "/")
expect("unix parent of root", unix.parent("/"), nil)
expect("unix relative", unix.relative("/a/b/Day 1/A.MP4", "/a/b"), "Day 1/A.MP4")
expect("unix relative from /", unix.relative("/a/A.MP4", "/"), "a/A.MP4")
expect("unix relative outside", unix.relative("/a/bc/A.MP4", "/a/b"), nil)
expect("unix url", unix.project_root("https://example.com/a.mp4"), nil)
expect("unix gyroflow of a name without extension", unix.gyroflow_new("/a/clip"), "/a/clip.gyroflow")
expect("unix gyroflow of a dotted folder", unix.gyroflow_new("/a.b/clip"), "/a.b/clip.gyroflow")

local win = workspace.new({ is_dir = function() return false end, is_file = function() return false end, windows = true })
expect("windows normalize", win.normalize("z:/Egypt/./Day 1/../A.MP4"), "z:\\Egypt\\A.MP4")
expect("windows normalize drive", win.normalize("D:"), "D:\\")
expect("windows normalize unc", win.normalize("\\\\nas\\video\\Egypt\\..\\A.MP4"), "\\\\nas\\video\\A.MP4")
expect("windows absolute", win.absolute("Day 1\\A.MP4", "Z:\\Egypt"), "Z:\\Egypt\\Day 1\\A.MP4")
expect("windows absolute unc kept", win.absolute("\\\\nas\\video\\A.MP4", "C:\\x"), "\\\\nas\\video\\A.MP4")
expect("windows parent", win.parent("Z:\\Egypt"), "Z:\\")
expect("windows parent of drive", win.parent("Z:\\"), nil)
expect("windows parent in unc", win.parent("\\\\nas\\video\\Egypt"), "\\\\nas\\video\\")
expect("windows parent of unc", win.parent("\\\\nas\\video\\"), nil)
expect("windows relative", win.relative("Z:\\Egypt\\Day 1\\A.MP4", "z:\\egypt"), "Day 1/A.MP4")
expect("windows new above DCIM", win.new_project_root("E:\\DCIM\\100GOPRO\\GX01.MP4"), "E:\\")
expect("windows gyroflow", win.gyroflow_new("Z:\\a.b\\clip.MP4"), "Z:\\a.b\\clip.gyroflow")
expect("windows sidecar", win.sidecar_file("Z:\\a.b\\clip.MP4"), "Z:\\a.b\\clip.afp.json")

-- The project search on Windows: the drive's top is looked at, the home folder isn't
local wfiles = { ["E:\\Project.afproject"] = true, ["C:\\Users\\me\\Project.afproject"] = true }
local wfs = { is_dir = function() return false end, is_file = function(p) return wfiles[p] == true end,
              home = "C:\\Users\\me", windows = true }
local w = workspace.new(wfs)
expect("windows card project", w.project_root("E:\\DCIM\\100GOPRO\\GX01.MP4"), "E:\\")
expect("windows card relative", w.relative("E:\\DCIM\\100GOPRO\\GX01.MP4", "E:\\"), "DCIM/100GOPRO/GX01.MP4")
expect("windows home stops", w.project_root("c:\\users\\me\\Videos\\A.MP4"), nil)

-- Where a project can be started: the volume's top down to the clip's folder, the home folder and above left out
local pc = workspace.new({ is_dir = function() return false end, is_file = function() return false end, home = "/Users/me" })
expect("choices on a volume", table.concat(pc.project_choices("/Volumes/NAS/Trips/Egypt/Day 1/A.MP4"), "|"),
       "/Volumes/NAS|/Volumes/NAS/Trips|/Volumes/NAS/Trips/Egypt|/Volumes/NAS/Trips/Egypt/Day 1")
expect("choices under home", table.concat(pc.project_choices("/Users/me/Movies/Egypt/A.MP4"), "|"), "/Users/me/Movies|/Users/me/Movies/Egypt")
expect("choices on a drive", table.concat(win.project_choices("E:\\DCIM\\100GOPRO\\GX01.MP4"), "|"), "E:\\|E:\\DCIM|E:\\DCIM\\100GOPRO")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
