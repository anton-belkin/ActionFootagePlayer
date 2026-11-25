-- denoise: the temporal denoise on the GPU path kept in step (Ctrl+p; warp.glsl's dn_ passes), and frame steps from
-- the frame on screen (dnstep.lua), for the gyroflow plugin. main.lua wires it: start() registers its keys and events
local mp = require 'mp'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, DENOISE_FRAMES, o
-- The other parts' functions, looked up in W as they're called
local function gpu_render_scale(...) return W.gpu_render_scale(...) end
-- What the other parts call: dn_sync() keeps the denoise's delay in step (shown: the frame on screen before a new
-- shader), dn_sync.shown(), dn_sync.new_ring(); there once start() has run
local dn_call
local dn_sync = setmetatable({}, { __call = function(_, ...) if dn_call then return dn_call(...) end end })

-- The denoise on the GPU path (Ctrl+p; warp.glsl's dn_ passes) averages each frame with DN_AFTER frames after it and as
-- many before: the picture shown is DN_AFTER frames behind the newest decoded, so the audio is delayed as much
-- (audio-delay, by what this adds to it), and after a seek the first frame stays until the frames after it are there.
-- In crop mode none after it (gyroflow_dn_after 0): its camera turns the newest frame as it's dragged. Where that
-- changes, or the shader is new, the shader's ring of frames starts again from the newest frame: paused, the frame on
-- screen is sought to exactly, so the picture stays where it was. user-data/gyroflow/gpu_delay: { delay, restart, show }
-- for what's on screen (the seek bar, marks, the part, export's photo: their shown_time()).
--
-- Paused (docs/design/seek-and-scrub.md, dnstep.lua): frame steps (`,` `.`, Option+arrows, the bar's buttons and wheel: this
-- plugin's frame-back / frame-on) go from the frame on screen; a step back shows the frame before from the ring at
-- once (gyroflow_dn_show), a step on plays the frames after it first. Where the frame on screen hasn't all its
-- neighbours (after a seek, a step back past the ring, a clip opened paused), the ring is filled again around it, the
-- frame held on screen meanwhile: after a seek at once (its frame is on screen as decoded first), after steps once
-- they stop for still_delay. Playing on goes on from the frame on screen.
-- (start: its keys and events registered, where main.lua wires it)
function M.start()
    local DN_AFTER = 5
    -- After a seek of someone else's, the fill waits this long for the next one (a drag over the bar)
    local DN_FILL_AFTER_SEEK = 0.15
    local dnstep = require 'dnstep'
    local dn = { after = DN_AFTER, epoch = 0, delay = 0, restart = nil, show = nil }
    -- Paused work under way: { kind = "frames" (frame-step N, until_ the newest frame it ends on), "fill-seek" (the
    -- fill's seek, frames to play after it, until_) }; own_seek: the time a seek of ours goes to, until it's there (a
    -- seek landing elsewhere was someone else's: theirs came last); steps waiting behind it
    local dn_busy, dn_own_seek, dn_queue, dn_fill_timer = nil, nil, 0, nil
    local function dn_publish()
        mp.set_property_native("user-data/gyroflow/gpu_delay", { delay = dn.delay, restart = dn.restart or -1, show = dn.show or -1 })
    end
    local function dn_state()
        local fps = S.current_source and tonumber(S.current_source.fps)
        local t = mp.get_property_number("time-pos")
        if not t or not fps or fps <= 0 then return nil end
        return { t = t, F = 1 / fps, after = dn.after, radius = DENOISE_FRAMES[o.temporal_denoise] or DN_AFTER,
                 restart = dn.restart or t, show = dn.show, duration = mp.get_property_number("duration") }
    end
    local function dn_shown()
        local t = mp.get_property_number("time-pos")
        if not t or dn.delay <= 0 then return t end
        local st = dn_state()
        return st and dnstep.shown(st) or t
    end
    -- The frame asked of the shader, paused (nil: its own, newest - after)
    local function dn_set_show(x)
        dn.show = x
        local opts = mp.get_property_native("glsl-shader-opts") or {}
        local v = x and string.format("%.6f", x) or "-1"
        if opts.gyroflow_dn_show ~= v then
            opts.gyroflow_dn_show = v
            mp.set_property_native("glsl-shader-opts", opts)
        end
        dn_publish()
    end
    -- Frame steps and the fill: paused, with the denoise's frames after the shown one (not crop mode)
    local function dn_stepping() return dn.delay > 0 and dn.after > 0 and S.gpu ~= nil and S.gpu.shader ~= nil end
    local function dn_seek(t)
        dn_own_seek = t
        mp.commandv("seek", string.format("%.6f", t), "absolute+exact")
    end
    local function dn_cancel()
        if dn_fill_timer then dn_fill_timer:kill() end
        dn_fill_timer, dn_busy, dn_queue = nil, nil, 0
    end
    -- The ring filled again around the frame on screen, when it hasn't all its neighbours
    local function dn_fill()
        dn_fill_timer = nil
        -- Not over a seek under way (someone else's: its frame is filled once it's there)
        if dn_busy or dn_own_seek or not dn_stepping() or not mp.get_property_native("pause") or mp.get_property_native("seeking") then return end
        local st = dn_state()
        if not st then return end
        local x = dnstep.shown(st)
        if dnstep.complete(st, x) then return end
        local r = dnstep.refill(st, x)
        mp.msg.verbose(string.format("denoise: the ring filled again around %.3f s (from %.3f s, %d frames)", x, r.from, r.frames))
        dn_set_show(x)
        dn_busy = { kind = "fill-seek", frames = r.frames, until_ = r.until_ }
        dn_seek(r.from)
    end
    local function dn_schedule_fill(delay)
        if dn_fill_timer then dn_fill_timer:kill() end
        dn_fill_timer = nil
        if not dn_stepping() or not mp.get_property_native("pause") then return end
        dn_fill_timer = mp.add_timeout(delay, dn_fill)
    end
    -- Plays n frames (mpv's frame-step, sound off), up to the frame at until_
    local function dn_play_frames(n, until_)
        dn_busy = { kind = "frames", until_ = until_, started = false }
        mp.commandv("frame-step", tostring(n), "mute")
    end
    local dn_step
    -- A step done (or the fill): the next one waiting, else the fill once they stop
    local function dn_done()
        dn_busy = nil
        if dn_queue ~= 0 then
            local dir = dn_queue > 0 and 1 or -1
            dn_queue = dn_queue - dir
            return dn_step(dir)
        end
        dn_schedule_fill(o.still_delay)
    end
    dn_step = function(dir)
        if not dn_stepping() then return mp.command(dir > 0 and "frame-step" or "frame-back-step") end
        if not mp.get_property_native("pause") then
            -- Playing: it pauses on the frame on screen, and steps from there
            dn_cancel()
            mp.set_property_native("pause", true)
        end
        if dn_busy then dn_queue = dn_queue + dir; return end
        if dn_fill_timer then dn_fill_timer:kill(); dn_fill_timer = nil end
        local st = dn_state()
        local a = st and dnstep.step(st, dir)
        if not a then return end
        dn_set_show(a.show)
        if a.seek then
            dn_busy = { kind = "seek" }
            dn_seek(a.seek)
        elseif a.frames > 0 then
            dn_play_frames(a.frames, st.t + a.frames * st.F)
        else
            dn_done()
        end
    end
    mp.add_key_binding(".", "frame-on", function() dn_step(1) end, { repeatable = true })
    mp.add_key_binding(",", "frame-back", function() dn_step(-1) end, { repeatable = true })

    -- The ring started again by its epoch (a shader file used before, which mpv may give back with its ring as it was)
    local function dn_new_ring()
        dn.epoch = dn.epoch + 1
        local opts = mp.get_property_native("glsl-shader-opts") or {}
        opts.gyroflow_dn_after, opts.gyroflow_dn_epoch = tostring(dn.after), tostring(dn.epoch)
        mp.set_property_native("glsl-shader-opts", opts)
    end
    -- shown: the frame on screen before a new shader (gpu_apply), nil otherwise
    dn_sync.shown, dn_sync.new_ring = dn_shown, dn_new_ring
    dn_call = function(shown)
        local crop_on = (mp.get_property_native("user-data/gyroflow/crop") or {}).active == true
        local frames = DENOISE_FRAMES[o.temporal_denoise] or DN_AFTER
        local after = crop_on and 0 or math.min(DN_AFTER, frames)
        -- The frames on each side (Full or Light) and the grain kept: the shader's own parameters, nothing rebuilt
        local want = { gyroflow_dn_radius = string.format("%d", frames), gyroflow_dn_keep = string.format("%.3f", o.still_grain),
                       gyroflow_dn_strong = S.denoise_strong and "1" or "0" }
        local opts = mp.get_property_native("glsl-shader-opts") or {}
        local differ = false
        for k, v in pairs(want) do if opts[k] ~= v then opts[k], differ = v, true end end
        if differ then mp.set_property_native("glsl-shader-opts", opts) end
        local fps = S.current_source and tonumber(S.current_source.fps)
        local on = S.gpu ~= nil and S.gpu.shader ~= nil and S.gpu.denoise and fps and fps > 0
        local before = shown or dn_shown()
        local restart = shown ~= nil
        if after ~= dn.after then
            dn.after, dn.epoch = after, dn.epoch + 1
            local opts = mp.get_property_native("glsl-shader-opts") or {}
            opts.gyroflow_dn_after, opts.gyroflow_dn_epoch = tostring(after), tostring(dn.epoch)
            mp.set_property_native("glsl-shader-opts", opts)
            restart = true
        end
        local delay = on and after / fps or 0
        if delay ~= dn.delay then
            mp.set_property_number("audio-delay", (mp.get_property_number("audio-delay") or 0) - dn.delay + delay)
            dn.delay = delay
        end
        if restart then
            -- The ring is empty again: what was asked of it goes, the frame on screen is sought to (and filled again)
            dn_cancel()
            dn_set_show(nil)
            local t = mp.get_property_number("time-pos")
            if before and t and mp.get_property_native("pause") and fps and math.abs(t - before) > 0.25 / fps then
                mp.commandv("seek", string.format("%.6f", before), "absolute+exact")
                t = before
            end
            dn.restart = t
        end
        dn_publish()
    end
    mp.register_event("seek", function()
        -- Ours: told apart once it's there (playback-restart)
        if dn_own_seek then return end
        -- A seek of someone else's (the bar, a mark, the keys): what was under way stops; the frame asked of the shader
        -- goes once the new one is there (until then the shader keeps the old one on screen, no flash of another).
        -- Frames being played (frame-step N, which unpauses) stop too: mpv keeps the steps not played through a seek
        -- and adds the next ones to them, so a drag over the bar while paused went on playing them once it stopped
        local stepping = dn_busy and dn_busy.kind == "frames"
        dn_cancel()
        if stepping then mp.set_property_native("pause", true) end
        dn.clear_show = true
    end)
    mp.register_event("playback-restart", function()
        dn.restart = mp.get_property_number("time-pos")
        if dn_own_seek then
            local fps = S.current_source and tonumber(S.current_source.fps) or 60
            local ours = dn.restart and math.abs(dn.restart - dn_own_seek) < 0.5 / fps
            dn_own_seek = nil
            if not ours then
                -- Someone else's seek came after ours: theirs it is
                dn_cancel()
                dn.clear_show = true
            end
        end
        if dn.clear_show then
            dn.clear_show = false
            dn_set_show(nil)
            -- Paused there: filled as soon as the seeks stop (its frame is on screen as decoded; a drag over the bar
            -- seeks on as the mouse moves, and each seek puts the fill off)
            dn_schedule_fill(DN_FILL_AFTER_SEEK)
            return dn_publish()
        end
        dn_publish()
        local b = dn_busy
        if b and b.kind == "fill-seek" then
            -- The fill's seek: the frames up to the frame's last neighbour played, the frame held on screen
            if b.frames > 0 then dn_play_frames(b.frames, b.until_) else dn_busy = nil end
        elseif b and b.kind == "seek" then
            dn_done()
        elseif not b then
            dn_schedule_fill(o.still_delay)
        end
    end)
    mp.observe_property("time-pos", "number", function(_, t)
        local b = dn_busy
        if b and b.kind == "frames" and t and S.current_source and t >= b.until_ - 0.5 / tonumber(S.current_source.fps)
           and mp.get_property_native("pause") then
            dn_done()
        end
    end)
    mp.observe_property("pause", "bool", function(_, paused)
        local b = dn_busy
        if b and b.kind == "frames" then
            if not paused then b.started = true return end
            -- Paused again: the steps done (time-pos says so), or cut short. Cut short by Space (mpv's cycle: the
            -- frame-step had it playing), which on a paused picture means play: it plays on, from the frame on screen
            local t = mp.get_property_number("time-pos")
            local fps = S.current_source and tonumber(S.current_source.fps)
            if b.started and t and fps and t >= b.until_ - 0.5 / fps then return dn_done() end
            if b.started then
                dn_busy = nil
                mp.set_property_native("pause", false)
            end
            return
        end
        if paused then
            -- Paused while playing: its frame has its neighbours, unless it played only a moment since a seek
            if not b then dn_schedule_fill(o.still_delay) end
            return
        end
        -- Playing on: from the frame on screen (a seek to it, when the shader would otherwise go on from another)
        local st = dn.show and dn_state()
        local from = st and dnstep.play_from(st)
        dn_cancel()
        if from then
            dn.clear_show = true
            mp.commandv("seek", string.format("%.6f", from), "absolute+exact")
        elseif dn.show then
            dn_set_show(nil)
        end
    end)
    mp.register_event("end-file", function()
        dn_cancel()
        dn_own_seek, dn.clear_show = nil, false
        if dn.show then dn_set_show(nil) end
    end)
    mp.observe_property("user-data/gyroflow/crop", "native", function() dn_sync() end)
    -- A new size of the picture starts the ring again too
    mp.observe_property("osd-dimensions", "native", function()
        gpu_render_scale()
        if dn.delay > 0 then dn_sync(dn_shown()) end
    end)
end

function M.install(w)
    W = w
    S = W.S
    DENOISE_FRAMES, o = W.DENOISE_FRAMES, W.o
    W.dn_sync = dn_sync
end

return M
