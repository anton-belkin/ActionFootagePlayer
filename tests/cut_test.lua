-- Cutting a spoiled part out (mpv/scripts/workspace/cut.lua): the parts kept, their names, the marks and moments that
-- follow. From the repository's folder: luajit tests/cut_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local cut = require 'cut'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- The parts kept
local p = cut.parts(10, 20, 47.58)
expect("middle: two parts", #p, 2)
expect("middle: first", p[1].from .. "-" .. p[1].to, "0-10")
expect("middle: second", p[2].from .. "-" .. tostring(p[2].to), "20-nil")
p = cut.parts(0, 20, 47.58)
expect("start: one part", #p, 1)
expect("start: from B", p[1].from, 20)
p = cut.parts(30, 47.58, 47.58)
expect("end: one part", #p, 1)
expect("end: to A", p[1].to, 30)
expect("a sliver at the start isn't kept", #cut.parts(0.3, 20, 47.58), 1)
expect("a sliver at the end isn't kept", #cut.parts(10, 47.3, 47.58), 1)
expect("all of it", #cut.parts(0.2, 47.4, 47.58), 0)

-- Names: a later part gets its own recording time, the camera's way
local none = function() return false end
expect("DJI name", cut.part_name("DJI_20261024094541_0093_D.MP4", 29.03, none), "DJI_20261024094610_0093_D.MP4")
expect("DJI over midnight", cut.part_name("DJI_20261231235950_0001_D.MP4", 70, none), "DJI_20270101000100_0001_D.MP4")
expect("DJI name taken", cut.part_name("DJI_20261024094541_0093_D.MP4", 29, function(n) return n == "DJI_20261024094610_0093_D.MP4" end),
       "DJI_20261024094541_0093_D 2.MP4")
expect("other camera", cut.part_name("GX010123.MP4", 12, none), "GX010123 2.MP4")
expect("other camera, taken", cut.part_name("GX010123.MP4", 12, function(n) return n == "GX010123 2.MP4" end), "GX010123 3.MP4")

-- Marks: a part's own, at its time, the rest of the file kept
local doc = {
    format = "actionfootageplayer-marks", version = 1, extra = "kept",
    clip = { file = "A.MP4", size = 100, duration = 47.58, created = "2026-10-22T13:28:42.000000Z", note = "kept" },
    marks = { { t = 5.0, camera = "dji" }, { t = 26.376, name = "Turtle" }, { t = 40.887 } },
}
local m = cut.part_marks(doc, 19.019, 35.0, { file = "A.MP4", size = 50, duration = 15.981, created = "2026-10-22T13:29:01.000000Z" })
expect("marks: one kept", #m.marks, 1)
expect("marks: at the part's time", m.marks[1].t, 7.357)
expect("marks: its name kept", m.marks[1].name, "Turtle")
expect("marks: clip size", m.clip.size, 50)
expect("marks: clip's unknown field", m.clip.note, "kept")
expect("marks: the doc's unknown field", m.extra, "kept")
expect("marks: original untouched", doc.marks[2].t, 26.376)
expect("marks: none in the part", cut.part_marks(doc, 10, 20, {}), nil)
expect("marks: camera flag", cut.part_marks(doc, 0, 10, {}).marks[1].camera, "dji")

-- Moments: to the part they overlap most, inside it
local parts = { { from = 0, to = 10 }, { from = 19.019, to = 47.58 } }
local i, a, b = cut.moment_part(2, 4, parts)
expect("moment in the first part", i .. " " .. a .. " " .. b, "1 2 4")
i, a, b = cut.moment_part(25, 30, parts)
expect("moment in the second part", i .. " " .. a .. " " .. b, "2 5.981 10.981")
i, a, b = cut.moment_part(8, 22, parts)
expect("moment over the cut: the larger side, clamped", i .. " " .. a .. " " .. b, "2 0 2.981")
expect("moment all in the part cut out", cut.moment_part(12, 18, parts), nil)

-- Creation times
expect("later", cut.later("2026-10-22T13:28:42.000000Z", 9.009), "2026-10-22T13:28:51.000000Z")
expect("later over a day", cut.later("2026-10-22T23:59:59.000000Z", 2), "2026-10-23T00:00:01.000000Z")
expect("later, no time", cut.later(nil, 3), nil)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
