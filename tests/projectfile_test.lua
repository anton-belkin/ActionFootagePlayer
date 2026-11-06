-- The project's own file (mpv/scripts/workspace/projectfile.lua): read, written with what it doesn't know kept, the
-- folder of before moved into it. In a temporary folder. From the repository's folder: luajit tests/projectfile_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local projectfile = require 'projectfile'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- JSON stand-ins: Lua table literals, enough to carry the tables through a file
local function serialize(v)
    if type(v) == "table" then
        local parts = {}
        for k, x in pairs(v) do
            parts[#parts + 1] = "[" .. (type(k) == "string" and string.format("%q", k) or tostring(k)) .. "]=" .. serialize(x)
        end
        return "{" .. table.concat(parts, ",") .. "}"
    end
    return type(v) == "string" and string.format("%q", v) or tostring(v)
end
local function exists(p) local f = io.open(p, "r"); if f then f:close() end; return f ~= nil end
local function is_dir(p) return os.execute("test -d '" .. p .. "'") == 0 or os.execute("test -d '" .. p .. "'") == true end
local function write(p, text) local f = io.open(p, "w"); f:write(text); f:close() end

local pf = projectfile.new({
    format_json = serialize,
    parse_json = function(s)
        local f = loadstring("return " .. s)
        if not f then return nil end
        local ok, v = pcall(f)
        return ok and v or nil
    end,
    join = function(a, b) return a .. "/" .. b end,
    is_dir = is_dir,
    is_file = exists,
    rmdir = function(d) os.execute("rmdir '" .. d .. "' 2>/dev/null") end,
})

local root = os.tmpname()
os.remove(root)
os.execute("mkdir -p '" .. root .. "'")

expect("path", pf.path(root), root .. "/Project.afproject")
local doc = pf.read(root)
expect("none: blank", #doc.clips_made, 0)
expect("none: format", doc.format, "actionfootage-project")
expect("none: nothing written", exists(pf.path(root)), false)
expect("no old folder: nothing to move", pf.migrate(root), false)

-- Written: the lists kept, a field it doesn't know kept
expect("update", pf.update(root, function(d) d.clips_made[#d.clips_made + 1] = "Day 1/A.MP4\t100"; d.mine = "kept" end), true)
expect("update again", pf.update(root, function(d) d.settings.clips_flat = true end), true)
doc = pf.read(root)
expect("made kept", doc.clips_made[1], "Day 1/A.MP4\t100")
expect("unknown kept", doc.mine, "kept")
expect("setting", doc.settings.clips_flat, true)
expect("no temporary left", exists(pf.path(root) .. ".tmp"), false)

-- The folder of before, with no file: read from it, then moved into the file and removed
local root2 = root .. "/old"
local old = root2 .. "/_ActionFootagePlayer"
os.execute("mkdir -p '" .. old .. "'")
write(old .. "/clips-made.txt", "Day 1/A.MP4\t100\nDay 1/B.MP4\t200\n")
write(old .. "/clips-failed.txt", "Day 2/C.MP4\tno gyro data\n")
write(old .. "/stills-made.txt", "Day 1/A.MP4\t100\t1.500\n")
write(old .. "/moment-names.txt", "Turtle\nTurtle 2\n")
write(old .. "/settings.json", serialize({ moments_alone = true }))
doc = pf.read(root2)
expect("old: made", #doc.clips_made, 2)
expect("old: failed path", doc.clips_failed[1][1], "Day 2/C.MP4")
expect("old: failed why", doc.clips_failed[1][2], "no gyro data")
expect("old: stills", doc.stills_made[1], "Day 1/A.MP4\t100\t1.500")
expect("old: last name", doc.moment_names[#doc.moment_names], "Turtle 2")
expect("old: settings", doc.settings.moments_alone, true)
expect("moved", pf.migrate(root2), true)
expect("moved: file there", exists(pf.path(root2)), true)
expect("moved: folder gone", is_dir(old), false)
doc = pf.read(root2)
expect("moved: made", #doc.clips_made, 2)
expect("moved: settings", doc.settings.moments_alone, true)

-- A folder with a file of someone else's in it stays, with that file; a file beside it gets what only the folder had
local root3 = root .. "/both"
os.execute("mkdir -p '" .. root3 .. "/_ActionFootagePlayer'")
write(root3 .. "/_ActionFootagePlayer/clips-made.txt", "A.MP4\t1\nB.MP4\t2\n")
write(root3 .. "/_ActionFootagePlayer/notes.txt", "mine")
pf.update(root3, function(d) d.clips_made = { "A.MP4\t1" } end)
expect("both: moved", pf.migrate(root3), true)
doc = pf.read(root3)
expect("both: merged", #doc.clips_made, 2)
expect("both: folder kept for its other file", exists(root3 .. "/_ActionFootagePlayer/notes.txt"), true)
expect("both: moved file gone", exists(root3 .. "/_ActionFootagePlayer/clips-made.txt"), false)

-- Not ours: a file of that name with another format reads as none
local root4 = root .. "/other"
os.execute("mkdir -p '" .. root4 .. "'")
write(root4 .. "/Project.afproject", serialize({ format = "something-else" }))
expect("not ours", #pf.read(root4).clips_made, 0)

os.execute("rm -r '" .. root .. "'")
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
