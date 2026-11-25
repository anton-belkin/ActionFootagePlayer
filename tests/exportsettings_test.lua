-- An export's settings (mpv/scripts/gyroflow/exportsettings.lua, docs/export-settings.md): the choices, their
-- defaults (what was exported before them), and each one into gyroflow-export's arguments.
-- From the repository's folder: luajit tests/exportsettings_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path
local es = require 'exportsettings'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function line(args) return " " .. table.concat(args, " ") .. " " end
local function has(args, part) return line(args):find(" " .. part .. " ", 1, true) ~= nil end

-- The choices and the defaults
local values = {}
for k, list in pairs(es.CHOICES) do
    values[k] = {}
    for _, c in ipairs(list) do table.insert(values[k], c[1]) end
end
expect("sizes", table.concat(values.size, ","), "original,2160,1080,720")
expect("encoder modes", table.concat(values.encode, ","), "fast,quality")
expect("quality presets", table.concat(values.quality, ","), "displayed,quickest,highest")
local d = es.normalize(nil, "video")
expect("default size: as before", d.size, "original")
expect("default encoder: as before", d.encode, "fast")
expect("default quality: as displayed", d.quality, "displayed")
expect("a photo has no encoder", es.normalize({ encode = "quality" }, "photo").encode, nil)
expect("a stale value: its default", es.normalize({ size = "8K", quality = "best" }, "video").size, "original")
expect("…the other kept", es.normalize({ size = "720", quality = "best" }, "video").size, "720")
expect("defaults are default", es.is_default({}, "video"), true)
expect("…a size isn't", es.is_default({ size = "1080" }, "video"), false)

-- The defaults add nothing: an export is what it was
local base = { "--curve", "dlogm", "--edges", "sphere", "--denoise", "light", "--chromatic", "yes" }
local function copy(t) local o = {} for i, v in ipairs(t) do o[i] = v end return o end
local a = copy(base)
es.apply(a, {}, "video")
expect("defaults: nothing added", line(a), line(base))
es.apply(a, { size = "original", encode = "fast", quality = "displayed" }, "video")
expect("defaults said: nothing added either", line(a), line(base))
expect("displayed: this computer's denoise amount", es.amount({ quality = "displayed" }, "light"), "light")

-- Size and encoder
a = copy(base)
es.apply(a, { size = "1080", encode = "quality" }, "video")
expect("size", has(a, "--resolution 1080"), true)
expect("encoder", has(a, "--encode quality"), true)
a = copy(base)
es.apply(a, { size = "720", encode = "quality" }, "photo")
expect("a photo's size", has(a, "--resolution 720"), true)
expect("a photo: no encoder", has(a, "--encode"), false)

-- Quickest: each display setting at its lightest
a = copy(base)
table.insert(a, "--denoise-strong")
es.apply(a, { quality = "quickest" }, "video")
expect("quickest: no denoise", has(a, "--denoise"), false)
expect("…nor its Stronger", has(a, "--denoise-strong"), false)
expect("quickest: edges spread out, not the trail", has(a, "--edges smooth"), true)
expect("quickest: fringes left", has(a, "--chromatic no"), true)
expect("quickest: bilinear on the GPU path", has(a, "--gpu-interpolation bilinear"), true)
expect("quickest: bilinear on the pipes", has(a, "--interpolation bilinear"), true)
expect("quickest: no denoise amount", es.amount({ quality = "quickest" }, "full"), "off")
a = { "--edges", "off" }
es.apply(a, { quality = "quickest" }, "video")
expect("quickest: no fill stays no fill", has(a, "--edges off"), true)
a = { "--radius", "0", "--image", "/tmp/f.png" }
es.apply(a, { quality = "quickest" }, "photo")
expect("quickest photo: not cleaned up", has(a, "--radius 0"), true)
expect("quickest photo: the frame on screen kept", has(a, "--image /tmp/f.png"), true)

-- Highest: each at its best
a = copy(base)
es.apply(a, { quality = "highest" }, "video")
expect("highest: full denoise", has(a, "--denoise full"), true)
expect("highest: the trail", has(a, "--edges sphere"), true)
expect("highest: fringes as the clip says", has(a, "--chromatic yes"), true)
expect("highest: Lanczos on the GPU path", has(a, "--gpu-interpolation lanczos"), true)
expect("highest: Lanczos on the pipes", has(a, "--interpolation lanczos"), true)
expect("highest: full even where this computer has it off", es.amount({ quality = "highest" }, "off"), "full")
a = { "--edges", "smooth" }
es.apply(a, { quality = "highest" }, "video")
expect("highest: smooth becomes the trail", has(a, "--edges sphere"), true)
expect("highest: no denoise where the clip has none", has(a, "--denoise"), false)
a = { "--radius", "0", "--image", "/tmp/f.png" }
es.apply(a, { quality = "highest" }, "photo")
expect("highest photo: 7 frames each side", has(a, "--radius 7"), true)
expect("highest photo: rendered by gyroflow-export", has(a, "--image"), false)

-- What can't be had
expect("360: its own size", es.unavailable("size", "360") ~= nil, true)
expect("flat: its own size", es.unavailable("size", "flat") ~= nil, true)
expect("an HDR export: any size", es.unavailable("size", "hdr"), nil)
expect("360: nor its quality (its own renderer)", es.unavailable("quality", "360") ~= nil, true)
expect("SDR: all", es.unavailable("encode", "sdr"), nil)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
