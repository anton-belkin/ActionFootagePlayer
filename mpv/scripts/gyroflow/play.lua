-- play: a clip opening in the gyroflow plugin (main.lua wires it): what it's stabilized with set up as it loads (the
-- card while it waits), the warp put on, the first frame's card, the lens read once it plays, a photo link's frame
local mp = require 'mp'
local utils = require 'mp.utils'
local crop = require 'crop'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, GPU_HOLD_SHADER, GPU_TABLE, KIND_LABELS, PROBE, dn_sync, gpu_blocked, o
-- The other parts' functions, looked up in W as they're called
local function add_sphere(...) return W.add_sphere(...) end
local function clip_path(...) return W.clip_path(...) end
local function clip_settings(...) return W.clip_settings(...) end
local function file_exists(...) return W.file_exists(...) end
local function filter_lens(...) return W.filter_lens(...) end
local function filtering(...) return W.filtering(...) end
local function find_project(...) return W.find_project(...) end
local function find_source(...) return W.find_source(...) end
local function find_water(...) return W.find_water(...) end
local function fixed(...) return W.fixed(...) end
local function gpu_apply(...) return W.gpu_apply(...) end
local function gpu_camera(...) return W.gpu_camera(...) end
local function gpu_eligible(...) return W.gpu_eligible(...) end
local function gpu_missing(...) return W.gpu_missing(...) end
local function gpu_refresh(...) return W.gpu_refresh(...) end
local function gpu_set_shader(...) return W.gpu_set_shader(...) end
local function gpu_settings(...) return W.gpu_settings(...) end
local function gpu_table(...) return W.gpu_table(...) end
local function gpu_unhold(...) return W.gpu_unhold(...) end
local function gpu_user_data(...) return W.gpu_user_data(...) end
local function has_project(...) return W.has_project(...) end
local function mpv_frame(...) return W.mpv_frame(...) end
local function output_shape(...) return W.output_shape(...) end
local function over_project(...) return W.over_project(...) end
local function play_unstabilized(...) return W.play_unstabilized(...) end
local function publish(...) return W.publish(...) end
local function read_json(...) return W.read_json(...) end
local function take(...) return W.take(...) end
local function unable_text(...) return W.unable_text(...) end
-- Set further down, or as it installs
local play_again

-- A photo link playing (the workspace plugin's photo, the live-photo plugin's still): its own frame, as Share renders
-- it: Save frame's camera (crop.view) and its crop, in shares of the frame. Crop mode's crop goes into the camera's zoom
-- (crop.fold: cut out of the whole frame, the picture trembled), what's left of it, and any other, as mpv's video-crop
-- of the picture shown
local function apply()
    local ws = mp.get_property_native("user-data/workspace")
    local ph = type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.photo) == "table" and ws.photo or nil
    local cam = ph and type(ph.camera) == "table" and ph.camera or nil
    local c = ph and type(ph.crop) == "table" and ph.crop or nil
    local folded = crop.fold(cam, c)
    if folded then cam, c = folded.camera, folded.rest end
    crop.view(cam)
    local o = mp.get_property_native("video-out-params")
    if c and type(o) == "table" and o.w and o.h then
        mp.set_property("file-local-options/video-crop", string.format("%dx%d+%d+%d",
            math.floor(c.w * o.w + 0.5), math.floor(c.h * o.h + 0.5), math.floor(c.x * o.w + 0.5), math.floor(c.y * o.h + 0.5)))
    end
    return c ~= nil and not (type(o) == "table" and o.w)
end
-- Its crop once the picture's size is known (after the filters)
function M.photo_loaded()
    if apply() then
        local function once(_, o)
            if type(o) == "table" and o.w then mp.unobserve_property(once); apply() end
        end
        mp.observe_property("video-out-params", "native", once)
    end
end

-- A clip played again from where it was (play_again)
local resume = nil

-- What add_filter would play the clip with, when render_args asks for it instead

local function add_filter(source, done)
    done = done or function() end
    S.current_source = source
    -- The bundle saved for it, else the session's; then the input's turn and lens. With a Gyroflow project, what it
    -- has is its own: the horizon and frame. Not the view: saved for the clip it plays so (on, wider: the project's
    -- zoom left out, off: the project not used), with none saved the project as it is; nor the output's lens (as
    -- recorded, or a projection in place of the project's), with none saved the project's
    local saved = fixed()
    local off = take(saved)
    clip_settings(source)
    if has_project(source) then
        if type(saved) ~= "table" or not saved.view then S.view, off = "normal", false end
        S.horizon = nil
        over_project(saved)
        S.frame.aspect, S.frame.orientation, S.frame.rotation = "original", "as-is", 0
        -- What it has, for the panel: its horizon lock, output size and projection
        local j = read_json(source.project) or {}
        local st, out = type(j.stabilization) == "table" and j.stabilization or {}, type(j.output) == "table" and j.output or {}
        source.info = { horizon = tonumber(st.horizon_lock_amount) or 0, width = tonumber(out.output_width), height = tonumber(out.output_height),
                        projection = type(st.output_projection) == "string" and st.output_projection or nil }
    end
    -- Nothing stabilizes it (no motion data: its lens alone could be corrected): as recorded unless saved for it so
    -- (Anton, 2026-11-11: such a clip is the camera's own picture, a stabilized one's often; a corrected lens is a choice)
    if source.kind == "lens" and not (type(saved) == "table" and saved.lens == "corrected") then S.lens_mode = "as-recorded" end
    S.stab_off = off

    -- render_args (afp-stream, tools/stream): what it would play with, written down instead of played
    if o.render_args ~= "" then
        -- (user_data: the table's request; settings: what it's made of, the clean-up and the look's too)
        S.render_dump = { stabilize = filter_lens(source) ~= nil, kind = source.kind, user_data = gpu_user_data(source), settings = gpu_settings(source) }
        return done()
    end

    -- This mpv can't, or the warp couldn't do this clip before: as recorded, a card saying why (mpv's own turn and crop).
    -- No card for a clip without motion data that plays as recorded anyway
    if not gpu_eligible(source) then
        gpu_unhold()
        if source.kind ~= "lens" or filter_lens(source) ~= nil then
            play_unstabilized(gpu_missing() or unable_text(gpu_blocked[mp.get_property("path") or ""]))
        end
        S.current_source = nil
        return done()
    end
    if o.hwdec then mp.set_property("file-local-options/hwdec", "auto") end
    mp.set_property("file-local-options/video-rotate", "no")
    -- Off with the lens as recorded: nothing in the list until it's turned on (gpu_refresh)
    if filter_lens(source) == nil then
        gpu_unhold()
        S.gpu = { shader = nil }
        return done()
    end
    -- Opening: mpv opens the clip while the table is written, paused on black; the window in the shape it will
    -- have (the project's output, or the frame's), corrected if the table says otherwise
    local opening = mp.get_property_native("time-pos") == nil
    if opening then
        gpu_set_shader(S.gpu and S.gpu.shader, GPU_HOLD_SHADER)
        S.gpu = { shader = GPU_HOLD_SHADER, hold = true }
        local w, h = output_shape(source)
        if w and h and math.abs(w / h - source.width / source.height) > 0.005 then
            mp.set_property("file-local-options/video-aspect-override", string.format("%d:%d", w, h))
        end
        if not mp.get_property_native("pause") then
            S.gpu_held = true
            mp.set_property_native("pause", true)
        end
    end
    S.gpu_pending = true
    gpu_table(source, function(reply)
        local again = S.gpu_pending == "again"
        S.gpu_pending = false
        if reply.shader and S.current_source == source then
            gpu_apply(reply)
            gpu_unhold()
            mp.msg.info(string.format("stabilizing on the GPU with %s%s (%s%s), its table in %d ms", source.project, KIND_LABELS[source.kind],
                reply.mode or "", reply.chromatic and ", color fringes taken out" or "", reply.ms or 0))
            if again then gpu_refresh() end
            if not opening then done() end
            return
        end
        if S.current_source ~= source then return end
        -- As recorded from here on (and when it's loaded again): the hold or the old warp out, mpv's own turn
        gpu_blocked[mp.get_property("path") or ""] = reply.unsupported or reply.error or "?"
        gpu_set_shader(S.gpu and S.gpu.shader, nil)
        S.gpu, S.current_source = nil, nil
        mp.set_property("file-local-options/video-aspect-override", "no")
        dn_sync()
        mpv_frame()
        play_unstabilized(unable_text(reply.unsupported or reply.error))
        publish()
        gpu_unhold()
        if not opening then done() end
    end)
    if opening then done() end
end

-- The card shown while a clip's loading waits for this plugin: first the check of what it can be stabilized with
-- (gyroflow-probe: from a NAS a second or more), then, for a clip that has motion data, the reading of it for the
-- warp. One that has none (a drone's, a phone's) is only checked
local loading = {
    phase = "checking", shown = false,
    cards = {
        checking = { title = "Checking the clip for motion data", detail = "Whether it can be stabilized" },
        reading = { title = "Reading the clip's motion", detail = "For the stabilization" },
    },
}
function loading.card()
    loading.shown = true
    local card = loading.cards[loading.phase]
    mp.notify({ id = "gyroflow:reading", kind = "working", title = card.title, detail = card.detail })
end
function loading.to(phase)
    loading.phase = phase
    if loading.shown then loading.card() end
end

-- The stabilization for the clip, to done() once it's decided
local function setup(done)
    S.current_source, S.stab_off, S.spherical, S.rig360, S.unstabilized = nil, false, false, false, nil
    S.render_dump = nil
    if S.gpu then gpu_set_shader(S.gpu.shader, nil) end
    S.gpu = nil
    dn_sync()
    -- Crop mode's camera stays with the clip it was turned on
    if (mp.get_property_native("glsl-shader-opts") or {}).gyroflow_cam_w then gpu_camera(nil) end
    if (mp.get_property_native("glsl-shader-opts") or {}).gyroflow_stamp360 then
        local opts = mp.get_property_native("glsl-shader-opts")
        opts.gyroflow_stamp360 = nil
        mp.set_property_native("glsl-shader-opts", opts)
    end
    mp.set_property_native("user-data/gyroflow/gpu", false)
    mp.set_property_native("user-data/gyroflow/headings360", false)
    -- What's saved for it, else the session's; the source's input (a clip without motion data shows the frame and turn
    -- too, by mpv's own crop and rotation)
    S.frame = { aspect = "original", orientation = "as-is", rotation = 0, projection = "linear" }
    S.stab_off = take(fixed())
    clip_settings(nil)
    if resume and resume.path == mp.get_property("path") and resume.pos then mp.set_property("file-local-options/start", tostring(resume.pos)) end
    resume = nil
    local path = clip_path()
    if not file_exists(GPU_TABLE) then
        if find_project(path) then mp.msg.warn("gyroflow-gpu-table missing at " .. GPU_TABLE) end
        return done()
    end
    find_source(path, function(source)
        if source and source.kind == "rig360" then
            add_sphere(source)
            return done()
        end
        if not source then return done() end
        if source.kind ~= "lens" then loading.to("reading") end
        add_filter(source, function()
            done()
            if source.kind == "project" then find_water(path, source) end
        end)
    end)
end

-- Loading waits for the probe, with the script free meanwhile: quitting doesn't wait for it
function M.on_load(hook)
    hook:defer()
    loading.phase, loading.shown = "checking", false
    local slow = mp.add_timeout(0.7, loading.card)
    setup(function()
        if loading.shown then mp.notify({ id = "gyroflow:reading", kind = "hide" }) end
        slow:kill()
        loading.shown = false
        -- Held for a clip before this one that didn't get to play on the GPU path
        if not (S.gpu and S.gpu.hold) then gpu_unhold() end
        publish()
        hook:cont()
    end)
end

-- The clip loaded again from where it plays: to stabilize it with a project that wasn't there when it loaded
play_again = function()
    resume = { path = mp.get_property("path"), pos = mp.get_property_number("time-pos") }
    mp.commandv("playlist-play-index", "current")
end

-- What it does until the first frame shows: the clip's motion is read and its warp written (seconds from a NAS),
-- and the window, there at once (the Windows app's force-window=immediate), stays black meanwhile. Past a second
-- without a frame, a card says the first one is being prepared, until it shows. (What it plays with isn't said: the
-- panel shows it)
local first_frame = { timer = nil, observer = nil, waiting = false }
local function first_frame_done()
    if first_frame.timer then first_frame.timer:kill() end
    if first_frame.observer then mp.unobserve_property(first_frame.observer) end
    local was = first_frame.waiting
    first_frame.timer, first_frame.observer, first_frame.waiting = nil, nil, false
    return was
end
local function first_frame_message(label)
    first_frame_done()
    if mp.get_property_native("video-frame-info") ~= nil then return end
    first_frame.observer = function(_, info)
        if info == nil then return end
        if first_frame_done() then
            mp.msg.verbose("the first frame shows")
            mp.notify({ id = "gyroflow:first-frame", kind = "hide" })
        end
    end
    mp.observe_property("video-frame-info", "native", first_frame.observer)
    first_frame.timer = mp.add_timeout(1, function()
        first_frame.timer = nil
        first_frame.waiting = true
        mp.msg.verbose("no frame yet: the first one is being prepared")
        mp.notify({ id = "gyroflow:first-frame", kind = "working", title = "Preparing the first frame", detail = "The stabilization is being set up" })
        -- Never for good (as mpv's text was: two minutes)
        first_frame.timer = mp.add_timeout(120, function()
            first_frame.timer = nil
            mp.notify({ id = "gyroflow:first-frame", kind = "hide" })
        end)
    end)
end

function M.first_frame_end()
    if first_frame_done() then mp.notify({ id = "gyroflow:first-frame", kind = "hide" }) end
end

function M.lens_loaded()
    -- The lens, for the Source tab: read after it starts, so the start doesn't wait for it
    S.current_lens = nil
    local lens_for = clip_path()
    if file_exists(PROBE) and lens_for and not lens_for:find("://", 1, true) then
        mp.command_native_async({ name = "subprocess", args = { PROBE, "--lens", lens_for }, capture_stdout = true, playback_only = true }, function(_, r)
            local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
            if type(info) == "table" and clip_path() == lens_for then
                -- With lens types (the curated list): the type given by hand, the one recorded, and the camera's own
                -- number for its mode, for "Unknown (mode 6)" while the list doesn't name it
                local label_of = {}
                for _, t in ipairs(type(info.types) == "table" and info.types or {}) do
                    if type(t) == "table" and t.id then label_of[t.id] = t.label or t.id end
                end
                local typ = type(info.type) == "string" and info.type or type(info.recorded) == "string" and info.recorded or nil
                -- Where it comes from, the clip's mode as the player's own lens database keeps it, and the lenses to
                -- choose for that mode (lens_pick::candidates)
                S.current_lens = { camera = info.camera or "", lens = info.lens or "", mode = tonumber(info.mode),
                                 type_label = typ and (label_of[typ] or typ) or nil, from = info.from or "", mode_key = info.mode_key,
                                 candidates = type(info.candidates) == "table" and info.candidates or {} }
                publish()
            end
        end)
    end
    local label
    if S.current_source and S.current_source.kind == "lens" then
        if filtering() then label = "Gyroflow: no motion data, the lens corrected" end
    elseif S.current_source and filtering() then
        label = "Gyroflow: " .. (S.stab_off and "the lens corrected, not stabilized" or "stabilized") .. KIND_LABELS[S.current_source.kind] .. (has_project(S.current_source) and "" or ", " .. S.level)
                .. ((S.current_source.in_camera and not S.stab_off) and (", on top of " .. S.current_source.in_camera) or "")
                .. (S.view == "wide" and ", wider view" or "")
    end
    if label then first_frame_message(label) end
end

function M.install(w)
    W = w
    S = W.S
    GPU_HOLD_SHADER, GPU_TABLE, KIND_LABELS, PROBE, dn_sync, gpu_blocked, o = W.GPU_HOLD_SHADER, W.GPU_TABLE, W.KIND_LABELS, W.PROBE, W.dn_sync, W.gpu_blocked, W.o
    S.render_dump = nil
    W.play_again = play_again
end

return M
