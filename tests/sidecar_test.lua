-- The clip's own file (mpv/scripts/workspace/sidecar.lua): read, the old .marks.json moved over, each part written on
-- its own with the rest kept, removed once it holds nothing. In a temporary folder. From the repository's folder:
-- luajit tests/sidecar_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local sidecar_lib = require 'sidecar'

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
local env = {
    format_json = serialize,
    parse_json = function(s)
        local f = loadstring("return " .. s)
        if not f then return nil end
        local ok, v = pcall(f)
        return ok and v or nil
    end,
}
local s = sidecar_lib.new(env)

local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")
local clip = dir .. "/DJI_0068_D.MP4"
local function exists(p) local f = io.open(p, "r"); if f then f:close() end; return f ~= nil end
local function write(p, text) local f = io.open(p, "w"); f:write(text); f:close() end

expect("path", s.path(clip), dir .. "/DJI_0068_D.afp.json")
expect("old path", s.old_path(clip), dir .. "/DJI_0068_D.marks.json")
expect("none", s.read(clip), nil)

-- An old marks file: read, then moved over by the first write, its marks kept
write(s.old_path(clip), serialize({ format = "actionfootageplayer-marks", version = 1, marks = { { t = 1.5 } }, extra = "kept" }))
local doc, file = s.read(clip)
expect("old read", doc and doc.marks[1].t, 1.5)
expect("old file", file, s.old_path(clip))
expect("view written", s.update(clip, function(d) d.view = { curve = "dlogm" } end), true)
expect("moved: new there", exists(s.path(clip)), true)
expect("moved: old gone", exists(s.old_path(clip)), false)
doc = s.read(clip)
expect("format", doc.format, "actionfootageplayer-clip")
expect("marks kept", doc.marks[1].t, 1.5)
expect("unknown kept", doc.extra, "kept")
expect("view", doc.view.curve, "dlogm")

-- Marks written: the view kept
s.update(clip, function(d) d.marks = { { t = 2 }, { t = 3 } } end)
doc = s.read(clip)
expect("marks", #doc.marks, 2)
expect("view kept", doc.view.curve, "dlogm")

-- fn returning false: nothing written
s.update(clip, function(d) d.view = nil; return false end)
expect("not written", s.read(clip).view.curve, "dlogm")

-- Neither marks nor view: removed
s.update(clip, function(d) d.marks = nil end)
expect("still there with a view", exists(s.path(clip)), true)
s.update(clip, function(d) d.view = nil end)
expect("removed", exists(s.path(clip)), false)

-- Someone else's file of that name: left alone
write(s.path(clip), serialize({ format = "something-else" }))
local d2, f2, problem = s.read(clip)
expect("foreign: no doc", d2, nil)
expect("foreign: said", problem ~= nil, true)
expect("foreign: not written", (s.update(clip, function(d) d.view = {} end)), false)
os.remove(s.path(clip))

-- Windows paths
local w = sidecar_lib.new({ format_json = serialize, parse_json = env.parse_json, windows = true })
expect("windows", w.path("Z:\\a.b\\clip.MP4"), "Z:\\a.b\\clip.afp.json")

os.execute("rmdir '" .. dir .. "'")
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
