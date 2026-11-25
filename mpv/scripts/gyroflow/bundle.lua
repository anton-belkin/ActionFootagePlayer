-- bundle: the transform the gyroflow plugin plays with (main.lua wires it): the app's defaults and the session's, a
-- clip's saved bundle taken, the clip's input, what the warp is asked for, and what's published in user-data/gyroflow/
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'
local still = require 'still'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, DATA, PRESETS, PROJECTIONS, dn_sync, o, trans
-- The other parts' functions, looked up in W as they're called
local function file_exists(...) return W.file_exists(...) end
local function gpu_missing(...) return W.gpu_missing(...) end
local function gpu_refresh(...) return W.gpu_refresh(...) end
local function gpu_settings(...) return W.gpu_settings(...) end
local function interpolation(...) return W.interpolation(...) end
local function preset_horizon(...) return W.preset_horizon(...) end
local function shown_aspect(...) return W.shown_aspect(...) end
local function source_aspect(...) return W.source_aspect(...) end
local function split_aspect(...) return W.split_aspect(...) end
-- Set further down, or as it installs
local publish, edge_blur, default_underwater

-- The warp fills the uncovered edges itself (warp.glsl's edge fill)
local EDGE_MODES = { "off", "sphere", "smooth" }
local EDGE_LABELS = { off = "background color", sphere = "trail by direction, blurred",
                      smooth = "frame's edge spread out, blurred" }

-- On, or on with a wider view (the warp's wide FOV, see vs-gyroflow's wide_fov); off is per clip

-- The level of a clip without a project (Ctrl+k)
local LEVELS = { "weak", "medium", "strong" }
local LEVEL_LABELS = { weak = "weak (10 % crop, smoothness 50 %)", medium = "medium (20 % crop, smoothness 75 %)",
                       strong = "strong (30 % crop, smoothness 100 %)" }

-- The water's color fringes taken out (Ctrl+a), or left as recorded


-- The transform bundle (docs/design/moments-and-workspaces.md, "Source and transform"): the stabilization (normal, wide; off is
-- a saved bundle's or this viewing's), level, horizon lock, color fringes, aspect, projection, and the lens (corrected,
-- or as recorded) apart from the stabilization. The app's defaults are
-- gyroflow.conf's; the session's start as them and change only by hand, on a file that plays with them (nothing saved
-- for it: the workspace plugin publishes no view). A file with a bundle of its own plays with it, and a change there is
-- for viewing only (or Save, Auto save)
local function defaults_bundle()
    local horizon_default = (o.horizon_lock == "yes" or o.horizon_lock == "no") and o.horizon_lock or preset_horizon()
    return {
        view = o.view == "wide" and "wide" or "normal",
        level = LEVEL_LABELS[o.level] and o.level or "medium",
        horizon_lock = horizon_default,
        chromatic = o.chromatic == true,
        aspect = (split_aspect(o.aspect, o.orientation)),
        orientation = select(2, split_aspect(o.aspect, o.orientation)),
        projection = PROJECTIONS[o.projection] and o.projection or "linear",
        lens = o.lens == "as-recorded" and "as-recorded" or "corrected",
        denoise = o.still == true,
        denoise_strong = o.denoise_strong == true,
        preset_horizon = preset_horizon(),
        -- The lens under water for a source that doesn't say (#317: the Library's renders take it too): auto by
        -- default, project too (default_underwater's rule; that's further down)
        underwater = (o.underwater == "yes" or o.underwater == "no") and o.underwater or "auto",
    }
end
local session = {}

-- The view, level, horizon, lens, edges, color fringes or frame changed: the warp's shader written again (gyroflow-gpu-table
-- keeps the clip loaded with every view and level: vs-gyroflow's Views)
local function settings_changed()
    if S.gpu and gpu_refresh then gpu_refresh() end
end
local function has_project(source)
    return source ~= nil and source.kind == "project"
end
local function view_label(v)
    if v == "wide" then return "on, wider view" end
    if v == "normal" then return has_project(S.current_source) and "on (Gyroflow project)" or "on" end
    return "off"
end
-- true or false: locked or not; nil: the project's (a clip with a Gyroflow project, which decides it)
local HORIZON = { yes = true, no = false }
-- The lens: corrected (and projected as frame.projection says), or as recorded; apart from the stabilization, so a clip
-- stabilized or not, and one with a lens profile and no motion data (kind "lens"), has it corrected or not
-- The temporal denoise's Stronger (the transform's denoise_strong): for a very noisy clip, on the GPU path
-- The lens under water with a flat port (yes), above water or behind a dome port (no), as the camera's telemetry says
-- (auto), or as the project says (project, a clip with one). Input: the source's own correction (the workspace
-- plugin's input), written there as it's changed; the app's default (underwater=) for sources without one. A clip's
-- camera that doesn't record it (source.water) leaves auto to the preset
local UNDERWATER_LABELS = { yes = "lens under water (flat port)", no = "lens above water", auto = "lens under water as the camera says",
                            project = "lens under water or not as the project says" }
-- Under water, auto or the project's, before Ctrl+u took the lens out of the water

-- The workspace plugin's bundle for the file playing, when one is saved for it (a moment's or clip preview's own,
-- else the source's own file's), the part stabilized on its own (trim: { in, out }, a moment's without a project),
-- and its switch (alone: its project's "only within the trim range"). nil: the session's plays
local function fixed()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") then return ws.view, ws.trim, ws.alone end
    return nil, nil, nil
end

-- The source's input (the workspace plugin's, from its own file): curve, rotation, underwater
local function input()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.input) == "table" then return ws.input end
    return {}
end

-- A bundle taken as what plays: true when it has the stabilization off
local function take(b)
    b = type(b) == "table" and b or session
    S.view = b.view == "wide" and "wide" or "normal"
    S.level = LEVEL_LABELS[b.level] and b.level or session.level
    local h = b.horizon_lock
    if h == "project" then h = S.DEFAULTS.preset_horizon end
    if h ~= "yes" and h ~= "no" then h = session.horizon_lock end
    S.horizon = h == "yes"
    if type(b.chromatic) == "boolean" then S.chromatic = b.chromatic else S.chromatic = session.chromatic end
    S.frame.aspect, S.frame.orientation = split_aspect(b.aspect, b.orientation)
    -- "project": a Gyroflow project's own (over one; linear without)
    S.frame.projection = (PROJECTIONS[b.projection] or b.projection == "project") and b.projection or "linear"
    -- A bundle from before the lens was a key of its own: off showed the lens as recorded too
    if b.lens == "corrected" or b.lens == "as-recorded" then S.lens_mode = b.lens
    elseif b ~= session and b.view == "off" then S.lens_mode = "as-recorded"
    else S.lens_mode = session.lens end
    if type(b.denoise_strong) == "boolean" then S.denoise_strong = b.denoise_strong else S.denoise_strong = session.denoise_strong end
    -- The temporal denoise on or off (Ctrl+p): the clip's (off where it harms, as on the water's surface); Performance
    -- & Quality's Off leaves it out whatever this says (still.lua)
    if type(b.denoise) == "boolean" then still.set(b.denoise) else still.set(session.denoise) end
    if dn_sync then dn_sync() end
    if (S.current_source and S.current_source.optical) then S.horizon = false end
    return b.view == "off"
end

-- A change by hand to what plays: the session's too when the session's plays (nothing saved for the file, no project)
local function sticks(k, v)
    if not fixed() then session[k] = v end
end

-- The output's lens over a Gyroflow project (#196): the bundle's (saved for the clip, or reverted to), as recorded or
-- corrected in a projection of its own; with none, the project's own ("project"), corrected
local function over_project(b)
    b = type(b) == "table" and b or {}
    S.lens_mode = b.lens == "as-recorded" and "as-recorded" or "corrected"
    S.frame.projection = (PROJECTIONS[b.projection] or b.projection == "project") and b.projection or "project"
end

-- The input's turn and lens a clip starts with, and the horizon: a project's own, or the source's
local function clip_settings(source)
    local i = input()
    local r = tonumber(i.rotation)
    S.frame.rotation = (r == 90 or r == 180 or r == 270) and r or 0
    if has_project(source) then
        S.horizon, S.underwater = nil, "project"
    elseif (source and source.optical) then
        -- The refraction the motion was measured with: the measured motion's own, as a project's
        S.horizon, S.underwater = false, "project"
    else
        S.underwater = (UNDERWATER_LABELS[i.underwater] and i.underwater ~= "project") and i.underwater or default_underwater
    end
    S.underwater_back = S.underwater ~= "no" and S.underwater or (has_project(source) and "project" or "auto")
end

-- A moment playing as a video of its own (the workspace plugin's: its part of the clip, from 0): where it starts in
-- the clip, which the warp adds to each frame's time to find its motion
local function time_offset()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.offset) == "number" then return ws.offset end
    return nil
end

-- A moment's end in its clip, nil: not a moment. The GPU path reads its part's motion data alone (gpu_table)
local function moment_end()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and ws.moment and type(ws.offset) == "number" and type(ws.length) == "number" then
        return ws.offset + ws.length
    end
    return nil
end

-- What the warp does with the lens and the motion (vs-gyroflow's LensMode, further down), nil: nothing
local filter_lens


-- The output projection the warp is given (vs-gyroflow's Frame: 0-4), nil for none of its own: over a Gyroflow
-- project, the one picked whatever it is (linear too: in place of a fisheye project's), "project" its own; without one,
-- linear is the preset's
local function lens_projection(source)
    local p = PROJECTIONS[S.frame.projection]
    if not p then return nil end
    if has_project(source) then return p end
    return S.frame.projection ~= "linear" and p or nil
end

-- The clip as it plays now, as gyroflow-gpu-table --player takes it (gpu_user_data; vs-gyroflow's PlayerClip): its
-- project or preset, the transform, the moment's part, the lens
local function player_args(source)
    local _, trim, alone = fixed()
    -- A project's "only within the trim range" as it's viewed: true or false, nil for the project's own
    local project_alone = nil
    if has_project(source) and alone ~= nil then project_alone = alone end
    return { project = source.project, preset = source.preset, interpolation = interpolation(source),
             wide = S.view == "wide" or nil, level = S.level, horizon_lock = S.horizon ~= nil and (S.horizon and 100 or 0) or nil, underwater = S.underwater, edges = S.edge_mode, edges_blur = edge_blur,
             time_offset = time_offset(), trim_start = trim and trim["in"], trim_end = trim and trim.out,
             alone = project_alone,
             rotation = S.frame.rotation ~= 0 and S.frame.rotation or nil,
             aspect = shown_aspect() ~= "original" and shown_aspect() or nil,
             projection = lens_projection(source),
             lens = filter_lens(source) or "stabilized", version = source.version }
end

-- Off for this clip (Ctrl+g)
-- The stabilization and the lens as the warp does them: stabilized with the lens corrected or as recorded, the lens
-- alone corrected (the stabilization off, or no motion data: kind "lens"), or nothing (off and as recorded: no warp)
filter_lens = function(source)
    source = source or S.current_source
    if not source then return nil end
    if source.kind ~= "lens" and not S.stab_off then return S.lens_mode == "as-recorded" and "recorded" or "stabilized" end
    if S.lens_mode == "corrected" then return "only" end
    return nil
end
-- The warp is on: it makes the frame (else mpv's own turn and crop do)
local function filtering()
    return S.current_source ~= nil and filter_lens() ~= nil
end
-- Ctrl+e on a clip without motion data: the project Gyroflow will save for it is waited for

-- For the right-click menu (mpv/menu.conf)
function publish()
    local state = {
        available = S.current_source ~= nil, kind = S.current_source and S.current_source.kind or "",
        project = has_project(S.current_source) and S.current_source.project or "",
        view = S.stab_off and "off" or S.view, level = S.level,
        -- The motion data to stabilize with (a project, or the clip's own), and the lens: corrected or as recorded, and
        -- what the warp does (stabilized, recorded, only: the lens alone; "" none)
        motion = S.current_source ~= nil and S.current_source.kind ~= "lens", lens_mode = S.lens_mode, filter = filter_lens() or "",
        horizon_lock = S.horizon == nil and "project" or (S.horizon and "yes" or "no"), edges = S.edge_mode, waiting = S.waiting,
        underwater = S.underwater, water = S.current_source and S.current_source.water or false,
        chromatic = S.chromatic, chromatic_available = gpu_missing() == nil, unstabilized = S.unstabilized or false,
        edges_blur = edge_blur,
        resolution = o.resolution,
        ask_when_slow = o.ask_when_slow, spherical = S.spherical, rig360 = S.rig360, aspect = S.frame.aspect, orientation = S.frame.orientation, frame_aspect = shown_aspect(),
        rotation = S.frame.rotation, projection = S.frame.projection,
        file_rotation = S.current_source and S.current_source.rotation or 0,
        chromatic_skip = o.chromatic_skip,
        temporal_denoise = o.temporal_denoise, denoise = still.switched(), denoise_strong = S.denoise_strong,
        -- The hand's sideways jerks: unavailable (off in gyroflow.conf, or not stabilized), measuring, none
        -- (not computed for the clip yet), on, off
        translation = (not o.translation or not (S.gpu and S.gpu.shader)) and "unavailable" or trans.job and "measuring"
            or S.gpu.translation and (trans.on and "on" or "off") or "none",
        translation_progress = trans.progress,
        -- The source's own ratio, upright (the file's turn and the input's): what the frame's "own" is
        source_aspect = source_aspect() or false,
        project_info = has_project(S.current_source) and S.current_source.info or false,
        -- Motion measured from the picture (gyroflow-probe's "measured": how, at, lens, in_water, readout_ms, frames,
        -- measured_frames), false for any other
        optical = S.current_source and S.current_source.optical or false,
        -- The session's bundle and the app's defaults (the workspace plugin fills and makes bundles with them)
        session = session, defaults = S.DEFAULTS,
        -- The lens as Gyroflow takes it: { camera, lens ("in the file", a profile's name, "" unknown) }
        lens = S.current_lens or false,
        -- The settings it plays with (gpu_settings): what an export of it takes
        settings = gpu_settings and gpu_settings(S.current_source) or false,
    }
    for k, v in pairs(state) do mp.set_property_native("user-data/gyroflow/" .. k, v) end
    -- What a render takes from this computer's settings, for Action Footage Library's recipes (library/src/renders.rs
    -- reads it; a batch writes it too): the defaults' bundle, the edge fill, how much temporal denoise, the preset
    -- used where a clip has no Gyroflow project. Written when it changes (what was written is kept beside the
    -- presets, a key ipairs doesn't see)
    if DATA and DATA ~= "" then
        local preset
        for _, p in ipairs(PRESETS) do if file_exists(p) then preset = p; break end end
        -- (the underwater colour's defaults with them: the dlogm plugin's uwcolor.conf)
        local defaults = {}
        for k, v in pairs(S.DEFAULTS) do defaults[k] = v end
        for k, v in pairs(mp.get_property_native("user-data/uwcolor/defaults") or {}) do defaults[k] = v end
        local text = utils.format_json({ v = 1, defaults = defaults, edges = o.edges, edges_blur = o.edges_blur,
                                         temporal_denoise = o.temporal_denoise, preset = preset })
        if text and text ~= PRESETS.defaults_written then
            local path = utils.join_path(DATA, "render-defaults.json")
            if not utils.file_info(DATA) then
                mp.command_native({ name = "subprocess", playback_only = false,
                                    args = platform.windows and { "cmd", "/d", "/c", "mkdir", DATA } or { "mkdir", "-p", DATA } })
            end
            local f = io.open(path .. ".tmp", "w")
            if f then
                f:write(text); f:close()
                os.remove(path)
                if os.rename(path .. ".tmp", path) then PRESETS.defaults_written = text end
            end
        end
    end
end

-- The transform as it plays, whole, as the app's defaults and the session's (the panel's Use as default); what a
-- Gyroflow project has stays the session's. With the edge fill and the clean still frame, which aren't a clip's
local function as_plays()
    local project = has_project(S.current_source)
    return {
        view = project and session.view or S.view,
        level = project and session.level or S.level,
        horizon_lock = (project or S.horizon == nil) and session.horizon_lock or (S.horizon and "yes" or "no"),
        chromatic = S.chromatic == true,
        aspect = project and session.aspect or S.frame.aspect,
        orientation = project and session.orientation or S.frame.orientation,
        projection = project and session.projection or S.frame.projection,
        lens = project and session.lens or S.lens_mode,
        denoise = still.switched(),
        denoise_strong = S.denoise_strong,
    }
end

function M.install(w)
    W = w
    S = W.S
    DATA, PRESETS, PROJECTIONS, dn_sync, o, trans = W.DATA, W.PRESETS, W.PROJECTIONS, W.dn_sync, W.o, W.trans
    S.edge_mode = EDGE_LABELS[o.edges] and o.edges or "sphere"
    edge_blur = o.edges_blur
    S.view = "normal"
    S.level = "medium"
    S.chromatic = o.chromatic
    S.DEFAULTS = defaults_bundle()
    for k, v in pairs(S.DEFAULTS) do session[k] = v end
    S.level = session.level
    S.lens_mode = "corrected"
    S.horizon = session.horizon_lock == "yes"
    S.denoise_strong = session.denoise_strong == true
    default_underwater = (UNDERWATER_LABELS[o.underwater] and o.underwater ~= "project") and o.underwater or "auto"
    S.underwater = default_underwater
    S.underwater_back = S.underwater ~= "no" and S.underwater or "auto"
    S.stab_off = false
    S.waiting = false
    W.EDGE_LABELS, W.EDGE_MODES, W.HORIZON, W.LEVELS, W.LEVEL_LABELS, W.UNDERWATER_LABELS = EDGE_LABELS, EDGE_MODES, HORIZON, LEVELS, LEVEL_LABELS, UNDERWATER_LABELS
    W.as_plays, W.clip_settings, W.defaults_bundle, W.edge_blur, W.filter_lens, W.filtering = as_plays, clip_settings, defaults_bundle, edge_blur, filter_lens, filtering
    W.fixed, W.has_project, W.moment_end, W.over_project, W.player_args, W.publish = fixed, has_project, moment_end, over_project, player_args, publish
    W.session, W.settings_changed, W.sticks, W.take, W.time_offset, W.view_label = session, settings_changed, sticks, take, time_offset, view_label
end

return M
