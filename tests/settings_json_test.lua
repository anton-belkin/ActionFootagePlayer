-- A saved view as the settings JSON gyroflow-export takes (mpv/scripts/gyroflow/settings.lua of_view, then
-- exportsettings.lua apply), case by case from tests/settings_cases.json. vs-gyroflow's gyroflow-export tests check
-- each case's settings set the options its flags (the player's before) did: what was exported is what's exported.
-- From the repository's folder: luajit tests/settings_json_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;mpv/scripts/workspace/?.lua;" .. package.path
local reg = require 'registry'
local settings = require 'settings'
local es = require 'exportsettings'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- A value written one way, keys in order, numbers to four places
local function show(v)
    if type(v) == "table" then
        local keys = {}
        for k in pairs(v) do keys[#keys + 1] = tostring(k) end
        table.sort(keys)
        local parts = {}
        for _, k in ipairs(keys) do parts[#parts + 1] = k .. "=" .. show(v[k] == nil and v[tonumber(k)] or v[k]) end
        return "{" .. table.concat(parts, ",") .. "}"
    end
    if type(v) == "number" then return string.format("%.4f", v) end
    return tostring(v)
end

local function merged(a, b)
    local out = {}
    for k, v in pairs(a or {}) do out[k] = v end
    for k, v in pairs(b or {}) do
        if type(v) == "table" and type(out[k]) == "table" then out[k] = merged(out[k], v) else out[k] = v end
    end
    return out
end

local f = assert(io.open("tests/settings_cases.json"))
local doc = reg.decode(f:read("*a"), "tests/settings_cases.json")
f:close()
expect("cases", #doc.cases >= 10, true)
for _, c in ipairs(doc.cases) do
    local s = settings.of_view(c.view, merged(doc.here, c.here))
    es.apply(s, c.export, c.kind)
    expect(c.name, show(s), show(c.settings))
    -- Only keys gyroflow-export reads
    for k in pairs(s) do expect(c.name .. ": " .. k .. " is the export's", settings.EXPORT[k], true) end
end

-- The table is asked only what it reads: the export's own keys stay out of its request (and of its caches' keys)
local r = settings.request({ project = "/a.gyroflow", level = "weak", lens = "stabilized", edges = "sphere", gpu_denoise = true,
                             denoise_strong = true, temporal_denoise = "light", denoise_grain = 0.5, curve = "dlogm", translation = true,
                             chromatic = true, sphere_width = 4096 })
expect("request", show(r), show({ project = "/a.gyroflow", level = "weak", lens = "stabilized", edges = "sphere", gpu_denoise = true }))

-- Every setting the export takes has a flag it stood for (tests/settings_json_test.py looks for them)
local flags = settings.flags()
expect("--with-translation stands for translation", flags["--with-translation"], "translation")
expect("--grain stands for denoise_grain", flags["--grain"], "denoise_grain")
expect("--project is the settings'", flags["--project"], "project")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
