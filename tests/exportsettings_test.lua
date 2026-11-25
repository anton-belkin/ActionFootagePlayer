-- An export's settings (mpv/scripts/gyroflow/exportsettings.lua, docs/design/export-settings.md): the choices, their
-- defaults (what was exported before them), and each one into the settings gyroflow-export takes.
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

-- The defaults change nothing: an export is what it was (exportsettings apply, on the settings JSON gyroflow-export takes)
local base = { curve = "dlogm", edges = "sphere", denoise = true, temporal_denoise = "light", chromatic = true }
local function copy(t) local o = {} for k, v in pairs(t) do o[k] = v end return o end
local function line(t)
    local keys = {}
    for k in pairs(t) do keys[#keys + 1] = k end
    table.sort(keys)
    local out = {}
    for _, k in ipairs(keys) do out[#out + 1] = k .. "=" .. tostring(t[k]) end
    return table.concat(out, " ")
end
local a = copy(base)
es.apply(a, {}, "video")
expect("defaults: nothing changed", line(a), line(base))
es.apply(a, { size = "original", encode = "fast", quality = "displayed" }, "video")
expect("defaults said: nothing changed either", line(a), line(base))
expect("displayed: this computer's denoise amount", es.amount({ quality = "displayed" }, "light"), "light")

-- Size and encoder
a = copy(base)
es.apply(a, { size = "1080", encode = "quality" }, "video")
expect("size", a.resolution, "1080")
expect("encoder", a.encode, "quality")
a = copy(base)
es.apply(a, { size = "720", encode = "quality" }, "photo")
expect("a photo's size", a.resolution, "720")
expect("a photo: no encoder", a.encode, nil)

-- Quickest: each display setting at its lightest
a = copy(base)
a.denoise_strong = true
es.apply(a, { quality = "quickest" }, "video")
expect("quickest: no denoise", a.temporal_denoise, "off")
expect("quickest: edges spread out, not the trail", a.edges, "smooth")
expect("quickest: fringes left", a.chromatic, false)
expect("quickest: bilinear on the GPU path", a.interpolation, "bilinear")
expect("quickest: bilinear on the pipes", a.engine_interpolation, "bilinear")
expect("quickest: no denoise amount", es.amount({ quality = "quickest" }, "full"), "off")
a = { edges = "off" }
es.apply(a, { quality = "quickest" }, "video")
expect("quickest: no fill stays no fill", a.edges, "off")
a = {}
es.apply(a, { quality = "quickest" }, "photo")
expect("quickest photo: not cleaned up", a.still_radius, 0)

-- Highest: each at its best
a = copy(base)
es.apply(a, { quality = "highest" }, "video")
expect("highest: full denoise", a.temporal_denoise, "full")
expect("highest: the trail", a.edges, "sphere")
expect("highest: fringes as the clip says", a.chromatic, true)
expect("highest: Lanczos on the GPU path", a.interpolation, "lanczos")
expect("highest: Lanczos on the pipes", a.engine_interpolation, "lanczos")
expect("highest: full even where this computer has it off", es.amount({ quality = "highest" }, "off"), "full")
a = { edges = "smooth", denoise = false, temporal_denoise = "light" }
es.apply(a, { quality = "highest" }, "video")
expect("highest: smooth becomes the trail", a.edges, "sphere")
expect("highest: no denoise where the clip has none", a.temporal_denoise, "light")
a = {}
es.apply(a, { quality = "highest" }, "photo")
expect("highest photo: 7 frames each side", a.still_radius, 7)

-- What can't be had
expect("360: its own size", es.unavailable("size", "360") ~= nil, true)
expect("flat: its own size", es.unavailable("size", "flat") ~= nil, true)
expect("an HDR export: any size", es.unavailable("size", "hdr"), nil)
expect("360: nor its quality (its own renderer)", es.unavailable("quality", "360") ~= nil, true)
expect("SDR: all", es.unavailable("encode", "sdr"), nil)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
