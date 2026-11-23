-- The underwater colour's rule (mpv/scripts/dlogm/uw_estimate.lua) on made-up frames: what it must and mustn't do.
-- Its numbers were checked against the research scripts (research_notes/underwater_color/tile_variants.v7) on 113
-- GoPro frames: within 0.005 for red and blue, 0.013 for the leak.
--     luajit tests/uwcolor_test.lua
package.path = "mpv/scripts/dlogm/?.lua;" .. package.path
local E = require 'uw_estimate'

local checks, failed = 0, 0
local function check(cond, what)
    checks = checks + 1
    if not cond then failed = failed + 1; print("FAIL " .. what) end
end

local W, H = 64, 36
local SW = E.SDR_WHITE
-- A frame from a function of (x, y) giving linear Rec.709 (SDR white = 1), as the grid would have it
local function frame(fn)
    local R, G, B = {}, {}, {}
    for y = 0, H - 1 do
        for x = 0, W - 1 do
            local r, g, b = E.mul({ 0.6274, 0.3293, 0.0433, 0.0691, 0.9195, 0.0114, 0.0164, 0.0880, 0.8956 }, fn(x, y))
            local i = y * W + x + 1
            R[i], G[i], B[i] = r * SW, g * SW, b * SW
        end
    end
    return R, G, B
end
local function est(fn) local R, G, B = frame(fn); return E.estimate(R, G, B, W, H) end
-- A little texture where wanted: a checker in luminance
local function tex(x, y, on) return on and (((x + y) % 2 == 0) and 1.3 or 0.7) or 1 end

-- Green murky water (HERO4-like), smooth in the top half, textured bottom
local p, s = est(function(x, y)
    local t = tex(x, y, y >= H / 2)
    return 0.10 * t, 0.20 * t, 0.12 * t
end)
check(s.f == 1, "green water: under water")
check(p.db > 0.3, "green water: blue added (" .. p.db .. ")")
check(s.water > 0.3, "green water: the smooth half seen as water (" .. s.water .. ")")
check(s.water_hue_after >= E.WATER_HUE_MIN - 1, "green water: the water's hue brought to the floor (" .. tostring(s.water_hue_after) .. ")")

-- Purple-ish blue water (Egypt-like): red left in deep water reads purple, the leak takes it out
p, s = est(function(x, y)
    local t = tex(x, y, y >= H / 2)
    return 0.10 * t, 0.16 * t, 0.40 * t
end)
check(p.k > 0, "purple water: some green taken out of red (" .. p.k .. ")")
check(s.water_a_after <= E.A_MAX + 0.002 or p.k >= E.K_CAP - 1e-6, "purple water: no longer purple (" .. tostring(s.water_a_after) .. ")")
check(p.k <= E.K_CAP, "purple water: the leak within its cap")

-- Above water: a sunlit frame with real red (skin, a red towel) in its lower half; nothing moves
p, s = est(function(x, y)
    local t = tex(x, y, true)
    if y >= H / 2 then return 0.80 * t, 0.20 * t, 0.10 * t end
    return 0.45 * t, 0.50 * t, 0.60 * t
end)
check(s.f == 0, "above water: seen as such (" .. s.f .. ")")
check(p.dr == 0 and p.db == 0 and p.k == 0 and p.ev == 0, "above water: left as recorded")

-- Lightness: a dark frame is lifted, at most 2 stops
p = est(function(x, y) return 0.08, 0.12, 0.15 end)
check(p.ev > 0 and p.ev <= 2, "dark frame: lifted, within 2 stops (" .. p.ev .. ")")

-- An empty (black) frame: nothing
p = est(function() return 0, 0, 0 end)
check(p.dr == 0 and p.db == 0 and p.k == 0, "black frame: nothing")

-- Oklab of white is (1, 0, 0)
local L, a, b = E.oklab(1, 1, 1)
check(math.abs(L - 1) < 1e-3 and math.abs(a) < 1e-3 and math.abs(b) < 1e-3, "oklab: white")

print(string.format("%d checks, %d failed", checks, failed))
os.exit(failed == 0 and 0 or 1)
