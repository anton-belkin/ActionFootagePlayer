-- The settings JSON (docs/reference/contracts.md, "The settings"; its keys: registry.json's settings_json): one object
-- saying how a clip is shown, which gyroflow-gpu-table takes as its request (with what only playback adds) and
-- gyroflow-export takes with --settings. The clip playing has it from the gyroflow plugin (main.lua gpu_settings,
-- published as user-data/gyroflow/settings); a saved view here (of_view). export.lua sends either as it is, with the
-- look and the export's own settings over it (exportsettings.lua apply).
-- Plain Lua, no mpv (registry.lua on package.path): tests/settings_json_test.lua, cases in tests/settings_cases.json
-- that the export's tests read too (the flags each case stood for before).

local M = {}

local reg = require 'registry'

-- The output projections, as Gyroflow numbers them (gyroflow/main.lua's PROJECTIONS); linear is the preset's own
local PROJECTIONS = { stereographic = 1, equidistant = 2, equisolid = 3, orthographic = 4 }

-- The keys the table reads (its request), and the export's
M.TABLE, M.EXPORT = {}, {}
for _, k in ipairs(reg.settings_json or {}) do
    if k.table then M.TABLE[k.key] = true end
    if k.export then M.EXPORT[k.key] = true end
end

-- What the table is asked: the settings' keys it reads, nothing else (the clean-up's amount, the look and the
-- export's choices aren't its, and would only make its caches miss)
function M.request(s)
    local out = {}
    for k, v in pairs(s or {}) do if M.TABLE[k] then out[k] = v end end
    return out
end

-- A saved view (a moment's, a Library recipe's: the transform bundle with its source's input, the opts, and the frame
-- the workspace plugin worked out, frame_aspect) as the settings. `here`: this computer's for what the view doesn't
-- say: { edges, edges_blur, temporal_denoise, denoise_grain, session, defaults } (the session's clean-up, else the app's
-- defaults). What isn't said is gyroflow-export's own default (the curve from the clip's metadata, the preset's)
function M.of_view(v, here)
    v, here = type(v) == "table" and v or {}, type(here) == "table" and here or {}
    local session, defaults = here.session or {}, here.defaults or {}
    local s = {}
    if v.curve and v.curve ~= "auto" then s.curve = v.curve end
    -- The underwater colour, as the source's input has it (a moment: its original's): on a plain clip, measured as it plays
    if v.uw == true then
        s.uw = true
        if type(v.uw_strength) == "number" then s.uw_strength = v.uw_strength end
        s.uw_highlights = v.uw_highlights == true
    end
    -- The lens apart from the stabilization: off with the lens corrected is the lens alone, as recorded while on keeps
    -- it (a bundle from before the lens was a key: off was as recorded)
    local lens = v.lens or (v.view == "off" and "as-recorded" or "corrected")
    if v.view == "off" then
        s.lens = lens == "corrected" and "only" or "off"
    else
        if v.view == "wide" then s.wide = true end
        s.lens = lens == "as-recorded" and "recorded" or "stabilized"
    end
    if v.level then s.level = v.level end
    -- The edge fill: the view's (a recipe's opts), else as it's set here
    s.edges = v.edges or here.edges
    if s.edges then s.edges_blur = v.edges_blur or here.edges_blur end
    if v.horizon_lock == "yes" or v.horizon_lock == "no" then s.horizon_lock = v.horizon_lock == "yes" and 100 or 0 end
    if v.underwater then s.underwater = v.underwater end
    if tonumber(v.rotation) and tonumber(v.rotation) ~= 0 then s.rotation = tonumber(v.rotation) end
    -- The frame's W:H for that source (its ratio turned for its orientation); an aspect from before, as it showed
    local aspect = v.frame_aspect or (v.orientation == nil and v.aspect) or nil
    if aspect and aspect ~= "original" then s.aspect = aspect end
    if PROJECTIONS[v.projection] and lens == "corrected" then s.projection = PROJECTIONS[v.projection] end
    -- The clean-up: the view's, else the session's, else the app's defaults; how much of the denoise: the view's (a
    -- recipe's), else this computer's; the grain it keeps, this computer's
    local function pick(k)
        if type(v[k]) == "boolean" then return v[k] end
        if type(session[k]) == "boolean" then return session[k] end
        return defaults[k] == true
    end
    s.denoise = pick("denoise")
    s.denoise_strong = pick("denoise_strong")
    if type(v.chromatic) == "boolean" then s.chromatic = v.chromatic
    elseif type(session.chromatic) == "boolean" then s.chromatic = session.chromatic
    else s.chromatic = defaults.chromatic ~= false end
    s.temporal_denoise = v.temporal_denoise or here.temporal_denoise or "full"
    s.denoise_grain = tonumber(v.denoise_grain) or tonumber(here.denoise_grain)
    return s
end

-- The flags that gyroflow-export's settings stand for (registry's cli), for tests/settings_json_test.py
function M.flags()
    local out = {}
    for _, k in ipairs(reg.settings_json or {}) do
        if k.export then for _, f in ipairs(k.cli or {}) do out[f] = k.key end end
    end
    return out
end

return M
