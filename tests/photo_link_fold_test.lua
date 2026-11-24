-- A photo link's crop folded into its camera (mpv/scripts/gyroflow/crop.lua, M.fold): crop mode's frame (in the middle)
-- becomes the camera's zoom, what's left of the other side a centered cut; a crop off the middle is left alone. The
-- live part trembled when the crop was cut out of the whole stabilized frame (2026-11-20).
-- From the repository's folder: luajit tests/photo_link_fold_test.lua

-- crop.lua talks to mpv as it loads: a stand-in that takes every call and returns nothing
local function anything()
    return setmetatable({}, { __index = function(t, k) local f = function() return nil end; rawset(t, k, f); return f end })
end
package.preload['mp'] = function() return anything() end
package.preload['mp.utils'] = function() return anything() end
package.preload['mp.msg'] = function() return anything() end
package.preload['mp.options'] = function() return anything() end
package.preload['mp.assdraw'] = function() return anything() end
package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
local crop = require 'crop'

local failures, checks = 0, 0
local function near(what, got, want, eps)
    checks = checks + 1
    if type(got) ~= "number" or math.abs(got - want) > (eps or 1e-9) then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function expect(what, ok)
    checks = checks + 1
    if not ok then failures = failures + 1; print("FAIL " .. what) end
end

-- Anton's link (DJI 0102 at 49.22 s): crop mode's original aspect, 0.8 either way
local cam = { yaw = -14.890073, pitch = -6.239985, roll = -3, zoom = 0.455143 }
local f = crop.fold(cam, { x = 0.1, y = 0.1, w = 0.8, h = 0.8 })
expect("original aspect folds", f ~= nil)
near("its zoom: the camera's times 0.8", f.camera.zoom, 0.455143 * 0.8)
near("yaw kept", f.camera.yaw, -14.890073)
near("pitch kept", f.camera.pitch, -6.239985)
near("roll kept", f.camera.roll, -3)
expect("nothing left to cut", f.rest == nil)
expect("the link's own camera untouched", cam.zoom == 0.455143)

-- 9:16 on a 16:9 clip (crop mode: MARGIN of the height, that over the ratio across): the height into the zoom, the
-- width's rest cut in the middle
local w = 0.8 * (9 / 16) * (9 / 16)
f = crop.fold(cam, { x = (1 - w) / 2, y = 0.1, w = w, h = 0.8 })
expect("9:16 folds", f ~= nil)
near("9:16 zoom: the height's share", f.camera.zoom, 0.455143 * 0.8)
expect("9:16 rest", f.rest ~= nil)
near("9:16 rest: full height", f.rest.h, 1)
near("9:16 rest: the width over the height's share", f.rest.w, w / 0.8)
near("9:16 rest: in the middle", f.rest.x + f.rest.w / 2, 0.5)
near("9:16 rest: from the top", f.rest.y, 0)

-- A landscape crop wider than tall: the width into the zoom
f = crop.fold(cam, { x = 0.1, y = 0.3, w = 0.8, h = 0.4 })
near("wide zoom: the width's share", f.camera.zoom, 0.455143 * 0.8)
near("wide rest: full width", f.rest.w, 1)
near("wide rest: half the height", f.rest.h, 0.5)
near("wide rest: in the middle", f.rest.y, 0.25)

-- Not crop mode's frame: off the middle, or none, or no camera: left alone
expect("off the middle: not folded", crop.fold(cam, { x = 0.15, y = 0.1, w = 0.7, h = 0.75 }) == nil)
expect("no crop: not folded", crop.fold(cam, nil) == nil)
expect("no camera: not folded", crop.fold(nil, { x = 0.1, y = 0.1, w = 0.8, h = 0.8 }) == nil)
expect("an empty crop: not folded", crop.fold(cam, { x = 0.5, y = 0.5, w = 0, h = 0 }) == nil)
-- A camera without a zoom (zoom 1)
f = crop.fold({ yaw = 1, pitch = 2, roll = 3 }, { x = 0.1, y = 0.1, w = 0.8, h = 0.8 })
near("no zoom: 1 times 0.8", f.camera.zoom, 0.8)

print(("photo_link_fold_test: %d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
