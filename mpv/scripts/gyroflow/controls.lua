-- controls: the gyroflow plugin's keys and messages that change the transform as it plays (main.lua wires them):
-- on, wider, off; the level, the horizon, the lens, under water, the colour fringes, the edges, the denoise's
-- Stronger, Performance & Quality, Use as default, Revert
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'
local still = require 'still'
local quality = require 'quality'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, DENOISE_FRAMES, EDGE_LABELS, EDGE_MODES, HORIZON, LEVELS, LEVEL_LABELS, UNDERWATER_LABELS, dn_sync, edge_blur, o, session
-- The other parts' functions, looked up in W as they're called
local function as_plays(...) return W.as_plays(...) end
local function defaults_bundle(...) return W.defaults_bundle(...) end
local function filter_lens(...) return W.filter_lens(...) end
local function filtering(...) return W.filtering(...) end
local function gpu_render_scale(...) return W.gpu_render_scale(...) end
local function has_project(...) return W.has_project(...) end
local function mpv_frame(...) return W.mpv_frame(...) end
local function over_project(...) return W.over_project(...) end
local function project_owns(...) return W.project_owns(...) end
local function publish(...) return W.publish(...) end
local function settings_changed(...) return W.settings_changed(...) end
local function sticks(...) return W.sticks(...) end
local function take(...) return W.take(...) end
local function view_label(...) return W.view_label(...) end
-- Set further down, or as it installs
local QUALITY

-- Sticks for the next clips, like the dlogm plugin's modes
local function set_edges(mode)
    S.edge_mode = mode
    settings_changed()
    publish()
    mp.osd_message("Gyroflow edges: " .. EDGE_LABELS[S.edge_mode], 2)
end

function M.edges_key()
    if platform.photo_shown() then return end
    local i = 1
    for k, m in ipairs(EDGE_MODES) do
        if m == S.edge_mode then i = k end
    end
    set_edges(EDGE_MODES[i % #EDGE_MODES + 1])
end

local function no_source()
    if S.unstabilized then return mp.osd_message("Gyroflow: " .. S.unstabilized, 3) end
    mp.osd_message("Gyroflow: no project or motion data for this file", 1.5)
end

-- The stabilization on (normal), on with the wider view (wide), or off for this clip; normal and wide stick
local function show(v)
    if not S.current_source then return no_source() end
    if S.current_source.kind == "lens" then
        return mp.osd_message("Gyroflow: no motion data in this clip, only its lens is corrected (Ctrl+e: optical flow in Gyroflow)", 2.5)
    end
    local enabled = v ~= "off"
    if enabled then S.view = v end
    if enabled and not has_project(S.current_source) then sticks("view", v) end
    S.stab_off = not enabled
    settings_changed()
    mpv_frame()
    publish()
    mp.msg.info("stabilization " .. view_label(v) .. (filter_lens() == "only" and ", the lens corrected" or ""))
    mp.osd_message("Gyroflow: " .. view_label(v) .. (filter_lens() == "only" and ", the lens corrected" or ""), 1.5)
end

-- The lens corrected or as recorded (the transform's, apart from the stabilization); over a Gyroflow project too, saved
-- with the clip, and not the session's then
local function set_lens(v)
    if v == "toggle" then v = S.lens_mode == "corrected" and "as-recorded" or "corrected" end
    if v ~= "corrected" and v ~= "as-recorded" then return end
    if not S.current_source then
        S.lens_mode = v
        sticks("lens", v)
        publish()
        return mp.osd_message("Gyroflow: no lens profile for this clip", 2)
    end
    local before = filter_lens()
    S.lens_mode = v
    if not has_project(S.current_source) then sticks("lens", v) end
    if before ~= filter_lens() then settings_changed() end
    mpv_frame()
    publish()
    mp.osd_message("Gyroflow: the lens " .. (v == "corrected" and "corrected" or "as recorded"), 1.5)
end

-- on -> wider view -> off -> on
function M.toggle_key()
    if platform.photo_shown() then return end
    if S.stab_off then show("normal") else show(S.view == "normal" and "wide" or "off") end
end

function M.view(v)
    if v == "normal" or v == "wide" or v == "off" then show(v) end
end

-- The warp written again for it (gyroflow-gpu-table has every level of the clip loaded)
local function set_level(l)
    if not S.current_source then return no_source() end
    if has_project(S.current_source) then
        return mp.osd_message("Gyroflow: the project's own smoothing and zoom (edit them in Gyroflow, Ctrl+e)", 2)
    end
    S.level = l
    sticks("level", l)
    settings_changed()
    publish()
    mp.msg.info("level " .. S.level)
    mp.osd_message("Gyroflow: " .. LEVEL_LABELS[S.level], 1.5)
end

-- weak -> medium -> strong -> weak
function M.level_key()
    if platform.photo_shown() then return end
    for i, l in ipairs(LEVELS) do
        if l == S.level then return set_level(LEVELS[i % #LEVELS + 1]) end
    end
end

function M.level(l)
    if LEVEL_LABELS[l] then set_level(l) end
end

-- Level (locked) or Free; "project", from before, as the preset has it
function M.horizon_lock(value)
    if project_owns("the horizon") then return end
    if (S.current_source and S.current_source.optical and not S.current_source.optical.in_camera_turn) then
        return mp.notify({ id = "gyroflow:horizon", kind = "info", seconds = 4, title = "No horizon to lock",
                           detail = "This clip's motion was measured from the picture: it has no gravity in it" })
    end
    if value == "project" then value = S.DEFAULTS.preset_horizon end
    if value == "toggle" then
        S.horizon = not S.horizon
    elseif HORIZON[value] ~= nil then
        S.horizon = HORIZON[value]
    else
        return
    end
    sticks("horizon_lock", S.horizon and "yes" or "no")
    settings_changed()
    publish()
    mp.osd_message("Gyroflow: horizon " .. (S.horizon and "level" or "free"), 1.5)
end

local function set_underwater(value)
    if value == "project" or project_owns("the lens") then return end
    if (S.current_source and S.current_source.optical) then
        -- Kept for the clip (the next measurement's medium), but the motion plays in the medium it was measured in
        mp.commandv("script-message-to", "workspace", "input-set", "underwater", value)
        local wet = S.current_source.optical.in_water == true
        return mp.notify({ id = "gyroflow:water", kind = "info", seconds = 5, title = "Kept for the next measurement",
                           detail = "The motion was measured " .. (wet and "in water" or "in air") .. ": it plays so until it's measured again" })
    end
    S.underwater = value
    if value ~= "no" then S.underwater_back = value end
    -- Input: the source's correction, into its own file at once
    mp.commandv("script-message-to", "workspace", "input-set", "underwater", value)
    settings_changed()
    publish()
    local says = S.current_source and S.current_source.kind == "default" and "preset" or "project"
    local label = UNDERWATER_LABELS[value]:gsub("project", says)
    local note = ""
    if value == "auto" and S.current_source and not S.current_source.water then
        note = ": this camera doesn't record it, so as the " .. says .. " says"
    end
    mp.msg.info(label .. note)
    mp.osd_message("Gyroflow: " .. label .. note, 2)
end

function M.underwater(value)
    if UNDERWATER_LABELS[value] then set_underwater(value) end
end

-- Above water <-> what it was (under water, as the camera says, or as the project says). Not named "lens": a key
-- binding's name is a script message too (mpv's defaults.lua), and it took the Lens drop-down's message over
function M.above_water_key()
    if platform.photo_shown() then return end
    set_underwater(S.underwater == "no" and S.underwater_back or "no")
end

-- The color fringes taken out or not: the warp written again. It changes nothing where the lens is above water
local function set_chromatic(value)
    S.chromatic = value
    sticks("chromatic", value)
    settings_changed()
    publish()
    local note = ""
    if S.chromatic and S.underwater == "no" then note = " (the lens is above water: nothing to take out)" end
    mp.msg.info("color fringes under water " .. (S.chromatic and "taken out" or "as recorded"))
    mp.osd_message("Gyroflow: color fringes under water " .. (S.chromatic and "taken out" or "as recorded") .. note, 2)
end

function M.chromatic(value)
    if value == "toggle" then
        set_chromatic(not S.chromatic)
    elseif value == "yes" or value == "no" then
        set_chromatic(value == "yes")
    end
end

function M.fringes_key() if not platform.photo_shown() then set_chromatic(not S.chromatic) end end

-- The temporal denoise's Stronger (the panel's Transform): kept with the clip's transform; the shader takes it at once
-- (gyroflow_dn_strong, nothing written again)
function M.denoise_strong(value)
    local v
    if value == "toggle" then v = not S.denoise_strong elseif value == "yes" or value == "no" then v = value == "yes" else return end
    S.denoise_strong = v
    sticks("denoise_strong", v)
    dn_sync()
    publish()
    local note = ""
    if v and not (S.gpu and S.gpu.denoise) then note = still.enabled() and " (where it's stabilized)" or " (with Ctrl+p on)" end
    mp.msg.info("temporal denoise: " .. (v and "stronger" or "as usual"))
    mp.osd_message("Temporal denoise: " .. (v and "stronger, for this clip" or "as usual") .. note, 2)
end

function M.edges(mode)
    if EDGE_LABELS[mode] then set_edges(mode) end
end

-- The quality (quality.lua, the menu's Quality): what's picked is kept in gyroflow.conf at once, as a
-- setting of this computer's

local function set_quality(key, value)
    if not (QUALITY[key] and QUALITY[key][value]) then return end
    if key == "edges" then
        set_edges(value)
    elseif key == "resolution" then
        o.resolution = value
        gpu_render_scale()
    elseif key == "chromatic_skip" then
        o.chromatic_skip = value == "yes"
        settings_changed()
    elseif key == "temporal_denoise" then
        -- Light or full: the shader's parameters (dn_sync); off or back from it: still.lua says so, and the shader is
        -- written again without the denoise or with it
        o.temporal_denoise = value
        o.still_radius = DENOISE_FRAMES[value] or o.still_radius
        dn_sync()
        still.available_changed()
    else
        o.ask_when_slow = value == "yes"
    end
    if not platform.save_options("gyroflow", { { key, value } }) then mp.msg.warn("couldn't write gyroflow.conf") end
    quality.changed()
    publish()
    mp.msg.info("quality: " .. key .. " " .. value)
    if key ~= "edges" then mp.osd_message("Gyroflow: " .. QUALITY[key][value], 2) end
end


function M.remember()
    local b = as_plays()
    local saved = platform.save_options("gyroflow", {
        { "view", b.view }, { "level", b.level }, { "horizon_lock", b.horizon_lock },
        { "chromatic", b.chromatic and "yes" or "no" }, { "aspect", b.aspect }, { "orientation", b.orientation }, { "projection", b.projection },
        { "lens", b.lens }, { "edges", S.edge_mode }, { "edges_blur", string.format("%g", edge_blur) },
        { "still", b.denoise and "yes" or "no" }, { "denoise_strong", b.denoise_strong and "yes" or "no" },
    })
    if saved then
        o.still, o.denoise_strong = b.denoise, b.denoise_strong
        o.view, o.level, o.horizon_lock, o.chromatic, o.aspect, o.orientation, o.projection, o.lens = b.view, b.level, b.horizon_lock, b.chromatic, b.aspect, b.orientation, b.projection, b.lens
        S.DEFAULTS = defaults_bundle()
        for k, v in pairs(b) do session[k] = v end
        publish()
    end
    mp.osd_message(saved and "Gyroflow: the transform as it plays is the default now (gyroflow.conf)" or "Gyroflow: couldn't write gyroflow.conf", 2)
end

-- The edge fill as the default (Performance & Quality's): this computer's, not a clip's. The temporal denoise's switch
-- is the clip's transform's (Use as default there)
function M.remember_quality()
    local saved = platform.save_options("gyroflow", {
        { "edges", S.edge_mode }, { "edges_blur", string.format("%g", edge_blur) },
    })
    mp.osd_message(saved and "Gyroflow: the edge fill is the default now (gyroflow.conf)" or "Gyroflow: couldn't write gyroflow.conf", 2)
end

-- A bundle played now, for this viewing (the panel's Revert to saved): what a Gyroflow project has stays its own
function M.apply_view(json)
    local b = utils.parse_json(json or "")
    if type(b) ~= "table" then return end
    local turn = S.frame.rotation
    local off = take(b)
    S.frame.rotation = turn
    if has_project(S.current_source) then
        -- The bundle's view (on, wider, off over the project) and the output's lens stay; the rest is the project's
        if not b.view then S.view, off = "normal", false end
        S.horizon = nil
        over_project(b)
        S.frame.aspect, S.frame.orientation = "original", "as-is"
    end
    S.stab_off = off
    settings_changed()
    mpv_frame()
    publish()
end

-- Performance & Quality (quality.lua): what it asks of the plugin as it plays (main.lua runs it where it always ran)
function M.init_quality()
    quality.init(o, {
        stabilizing = function() return filtering() end,
        -- What the shader renders, mpv scaling it to the screen
        short_side = function()
            if not S.current_source or not (S.gpu and S.gpu.shader and not S.gpu.sphere) then return nil end
            return S.gpu_render_short
        end,
        edges = function() return S.edge_mode end,
        -- The color fringes being taken out where it can: the warp's, the lens not set above water
        chromatic = function() return S.chromatic and S.gpu ~= nil and S.underwater ~= "no" and not o.chromatic_skip end,
        -- The temporal denoise as it runs while it plays: full, light, or nil for none
        denoise = function() return S.gpu and S.gpu.denoise and o.temporal_denoise ~= "off" and o.temporal_denoise or nil end,
        set = set_quality,
    })
end

function M.install(w)
    W = w
    S = W.S
    DENOISE_FRAMES, EDGE_LABELS, EDGE_MODES, HORIZON, LEVELS, LEVEL_LABELS, UNDERWATER_LABELS, dn_sync, edge_blur, o, session = W.DENOISE_FRAMES, W.EDGE_LABELS, W.EDGE_MODES, W.HORIZON, W.LEVELS, W.LEVEL_LABELS, W.UNDERWATER_LABELS, W.dn_sync, W.edge_blur, W.o, W.session
    QUALITY = {
        resolution = { auto = "auto: fits the display, at most 1080p while the temporal denoise is on", display = "fits the display", ["1080"] = "up to 1080p", ["720"] = "up to 720p", video = "the video's own size" },
        chromatic_skip = { yes = "color aberration correction skipped on this screen, for smooth playback", no = "color aberration correction as the clip's transform says" },
        ask_when_slow = { yes = "pauses and asks when it can't keep up", no = "plays on when it can't keep up" },
        temporal_denoise = { full = "temporal denoise: full, 5 frames on each side", light = "temporal denoise: light, 3 frames on each side",
                             off = "temporal denoise: off on this computer" },
        edges = EDGE_LABELS,
    }
    W.set_lens, W.set_quality = set_lens, set_quality
end

return M
