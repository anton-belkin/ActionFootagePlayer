-- The 360 view's geometry (mpv/scripts/view360/view.lua, the same as view360.glsl's), the gestures crop mode and the
-- view share (mpv/scripts/gyroflow/gestures.lua), and what the view360 script does with a 360° clip and with any
-- other, over a fake mpv. From the repository's folder: luajit tests/view360_test.lua (or lua 5.1 and later)

package.path = "mpv/scripts/view360/?.lua;mpv/scripts/gyroflow/?.lua;" .. package.path

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function near(what, got, want, eps)
    checks = checks + 1
    if type(got) ~= "number" or math.abs(got - want) > (eps or 1e-6) then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- A fake mpv, enough for the scripts: properties, bindings, script messages, hooks, events, observers
local function fake_mp()
    local f = { props = {}, bindings = {}, messages = {}, hooks = {}, events = {}, observers = {}, osd = nil, commands = {} }
    f.props["osd-dimensions"] = { w = 1600, h = 900 }
    f.props["display-hidpi-scale"] = 2
    f.props["window-dragging"] = true
    f.props["keepaspect-window"] = "yes"
    f.props["video-aspect-override"] = "-1"
    f.props["vo"] = "gpu-next"
    f.props["video-timing-offset"] = "0.050000"
    f.props["display-fps"] = 120
    -- The app's mpv: the option's there
    f.props["macos-gesture-messages"] = false
    local function changed(name)
        for _, o in ipairs(f.observers) do
            if o.name == name or name:sub(1, #o.name + 1) == o.name .. "/" then o.fn(o.name, f.props[o.name]) end
        end
    end
    local mp = {
        msg = { warn = function() end, info = function() end, verbose = function() end, debug = function() end, error = function() end },
        get_script_directory = function() return "mpv/scripts/view360" end,
        get_property_native = function(name) return f.props[name] end,
        get_property = function(name) local v = f.props[name]; return v ~= nil and tostring(v) or nil end,
        get_property_number = function(name) return tonumber(f.props[name]) end,
        set_property_native = function(name, v)
            if f.props[name] == nil and name == "macos-gesture-messages-missing" then return false end
            f.props[name] = v; changed(name); return true
        end,
        commandv = function(...) table.insert(f.commands, { ... }) end,
        add_hook = function(name, prio, fn) table.insert(f.hooks, { name = name, prio = prio, fn = fn }) end,
        register_event = function(name, fn) f.events[name] = f.events[name] or {}; table.insert(f.events[name], fn) end,
        observe_property = function(name, _, fn) table.insert(f.observers, { name = name, fn = fn }) end,
        add_key_binding = function(key, name, fn) f.bindings[name] = { key = key, fn = fn, forced = false } end,
        add_forced_key_binding = function(key, name, fn) f.bindings[name] = { key = key, fn = fn, forced = true } end,
        remove_key_binding = function(name) f.bindings[name] = nil end,
        register_script_message = function(name, fn) f.messages[name] = fn end,
        osd_message = function(text) f.osd = text end,
        -- Run at once, or (f.defer) when f.flush() says the messages waiting are handled
        add_timeout = function(_, fn)
            if f.defer then table.insert(f.timers, fn) else fn() end
        end,
    }
    f.timers = {}
    f.flush = function()
        local t = f.timers
        f.timers = {}
        for _, fn in ipairs(t) do fn() end
    end
    mp.set_property = function(name, v) f.props[name] = v; changed(name) end
    f.mp = mp
    f.set = function(name, v) f.props[name] = v; changed(name) end
    f.message = function(name, ...) return f.messages[name](...) end
    f.event = function(name) for _, fn in ipairs(f.events[name] or {}) do fn() end end
    f.forced = function()
        local n = 0
        for _, b in pairs(f.bindings) do if b.forced then n = n + 1 end end
        return n
    end
    return f
end

-- view.lua needs no mpv
local view = require 'view'

-- Where things are: forward, right, up (y is down)
do
    local v = view.default()
    local w = view.ray(v, 0, 0)
    near("ahead x", w[1], 0); near("ahead y", w[2], 0); near("ahead z", w[3], 1)
    v.yaw = 90
    w = view.ray(v, 0, 0)
    near("yaw 90 looks right", w[1], 1)
    v.yaw, v.pitch = 0, 30
    w = view.ray(v, 0, 0)
    near("pitch 30 looks up", w[2], -0.5)
    -- The equirect: ahead in the middle, right of it to the right, up at the top: not mirrored
    local u, t = view.equirect({ 0, 0, 1 })
    near("equirect ahead u", u, 0.5); near("equirect ahead v", t, 0.5)
    u, t = view.equirect({ 1, 0, 0 })
    near("equirect right u", u, 0.75); near("equirect right v", t, 0.5)
    u, t = view.equirect({ -1, 0, 0 })
    near("equirect left u", u, 0.25)
    u, t = view.equirect({ 0, -1, 0 })
    near("equirect up v", t, 0)
    u, t = view.equirect({ 0, 1, 0 })
    near("equirect down v", t, 1)
    -- A point right of the screen's middle is right of the equirect's
    u = view.equirect(view.ray(view.default(), 0.5, 0))
    expect("screen right is sphere right", u > 0.5, true)
    -- and one below it below
    _, t = view.equirect(view.ray(view.default(), 0, 0.5))
    expect("screen down is sphere down", t > 0.5, true)
    -- Roll positive turns the picture counterclockwise: a point on the horizon to the right shows higher
    v = view.default()
    v.roll = 10
    local x, y = view.place(v, { math.sin(math.rad(30)), 0, math.cos(math.rad(30)) })
    expect("roll ccw: right side up", y < 0, true)
    expect("roll ccw: still right", x > 0, true)
end

-- The FOV spans a 16:9 picture's width at the window's height, in every projection
for _, projection in ipairs({ "auto", "rectilinear", "stereographic" }) do
    for _, fov in ipairs({ 60, 100, 140 }) do
        if fov <= view.FOV[projection].max then
            local v = view.default(projection)
            v.fov = fov
            local w = view.ray(v, 16 / 9, 0)
            near(projection .. " fov " .. fov, math.deg(math.acos(w[3])), fov / 2, 1e-6)
        end
    end
end
expect("auto is rectilinear up to 100", view.distance({ projection = "auto", fov = 100 }), 0)
expect("auto is stereographic from 160", view.distance({ projection = "auto", fov = 160 }), 1)
expect("auto in between", view.distance({ projection = "auto", fov = 130 }) > 0 and view.distance({ projection = "auto", fov = 130 }) < 1, true)
expect("fov clamped", view.clamp_fov("rectilinear", 170), 150)
expect("fov clamped low", view.clamp_fov("planet", 10), 60)
-- The little planet looks straight down, the way ahead at the top of the screen
do
    local v = view.default("planet")
    local w = view.ray(v, 0, 0)
    near("planet looks down", w[2], 1)
    local x, y = view.place(v, { 0, 0, 1 })
    expect("planet: ahead is above", y and y < 0, true)
    near("planet: ahead centered", x, 0)
end

-- Round trips: place(ray(p)) = p
for _, projection in ipairs({ "auto", "rectilinear", "stereographic", "planet" }) do
    local v = view.default(projection)
    v.yaw, v.pitch, v.roll = 37, -20, 12
    if projection == "planet" then v.pitch = 15 end
    for _, p in ipairs({ { 0, 0 }, { 0.5, 0.2 }, { -1.2, 0.7 }, { 1.7, -0.9 } }) do
        local x, y = view.place(v, view.ray(v, p[1], p[2]))
        near(projection .. " round trip x " .. p[1], x, p[1], 1e-9)
        near(projection .. " round trip y " .. p[2], y, p[2], 1e-9)
    end
end

-- The little planet: a drag around the middle spins it, the point under the mouse staying under it (looking straight
-- down); towards the middle, the point comes as far in as the mouse
do
    local v = view.default("planet")
    v.yaw = 20
    local x0, y0 = 0.6, -0.3
    local a = math.atan2(y0, x0) + math.rad(25)
    local r = math.sqrt(x0 * x0 + y0 * y0)
    local x1, y1 = r * math.cos(a), r * math.sin(a)
    local w = view.ray(v, x0, y0)
    local n = view.copy(v)
    n.yaw, n.pitch = view.pan(v, x0, y0, x1, y1)
    local x, y = view.place(n, w)
    near("planet spin keeps x", x, x1, 1e-6)
    near("planet spin keeps y", y, y1, 1e-6)
    n = view.copy(v)
    n.yaw, n.pitch = view.pan(v, 0, -0.8, 0, -0.6)
    x, y = view.place(n, view.ray(v, 0, -0.8))
    near("planet tilt: as far from the middle", math.sqrt(x * x + y * y), 0.6, 1e-6)
    near("planet tilt: no spin", n.yaw, 20, 1e-9)
end

-- A drag: what was under the mouse stays under it, in every projection but the little planet, in the middle and off it
for _, projection in ipairs({ "auto", "rectilinear", "stereographic" }) do
    for _, case in ipairs({ { 0, 0, 0.1, 0.05 }, { 0.8, -0.4, 0.9, -0.35 }, { -1.0, 0.6, -1.05, 0.5 } }) do
        local v = view.default(projection)
        v.yaw, v.pitch = 20, -10
        local w = view.ray(v, case[1], case[2])
        local n = view.copy(v)
        n.yaw, n.pitch = view.pan(v, case[1], case[2], case[3], case[4])
        local x, y = view.place(n, w)
        near(projection .. " drag keeps x", x, case[3], 1e-6)
        near(projection .. " drag keeps y", y, case[4], 1e-6)
    end
end
-- Dragging right looks left; dragging down looks up
do
    local v = view.default()
    local yaw, pitch = view.pan(v, 0, 0, 0.1, 0)
    expect("drag right: yaw left", yaw < 0, true)
    near("drag right: pitch kept", pitch, 0, 1e-9)
    yaw, pitch = view.pan(v, 0, 0, 0, 0.1)
    expect("drag down: pitch up", pitch > 0, true)
    -- Not past the pole
    v.pitch = 89
    yaw, pitch = view.pan(v, 0, 0, 0, 0.5)
    expect("pitch held below the pole", pitch <= view.PITCH_MAX, true)
end
expect("wrap 190", view.wrap(190), -170)
expect("wrap -190", view.wrap(-190), 170)
do
    local o = view.shader_opts({ yaw = 1.5, pitch = -2, roll = 0, fov = 100, projection = "planet" }, 1, 2)
    expect("opts projection", o.view360_projection, "3")
    expect("opts yaw", o.view360_yaw, "1.5000")
    expect("opts stereo", o.view360_stereo, "1")
    expect("opts heading", o.view360_heading, "2")
end

-- The gestures (crop mode's and the view's), over a fake mpv
do
    local f = fake_mp()
    package.loaded['mp'] = f.mp
    package.loaded['gestures'] = nil
    local gestures = require 'gestures'
    local got = {}
    local g = gestures.new({
        prefix = "test",
        pan = function(dx, dy, x, y) table.insert(got, { "pan", dx, dy, x, y }) end,
        zoom = function(k) table.insert(got, { "zoom", k }) end,
        roll = function(d) table.insert(got, { "roll", d }) end,
        reset = function() table.insert(got, { "reset" }) end,
        released = function() table.insert(got, { "released" }) end,
        width = function() return 1000 end,
    })
    expect("off: no bindings", f.forced(), 0)
    g:set(true)
    expect("on: bindings", f.forced() >= 10, true)
    expect("on: window not dragged", f.props["window-dragging"], false)
    expect("on: gesture messages", f.props["macos-gesture-messages"], true)
    -- A drag
    f.props["mouse-pos"] = { x = 100, y = 100 }
    f.bindings["test-key-drag"].fn({ event = "down" })
    f.set("mouse-pos", { x = 110, y = 95 })
    expect("drag pans", got[#got][1], "pan")
    expect("drag dx", got[#got][2], 10)
    expect("drag dy", got[#got][3], -5)
    expect("drag at the mouse", got[#got][4], 110)
    f.bindings["test-key-drag"].fn({ event = "up" })
    expect("released", got[#got][1], "released")
    -- Alt+drag to the right: clockwise (negative), 90° over the width
    f.bindings["test-key-roll"].fn({ event = "down" })
    f.set("mouse-pos", { x = 210, y = 95 })
    expect("alt-drag rolls", got[#got][1], "roll")
    near("alt-drag right is clockwise", got[#got][2], -9)
    f.bindings["test-key-roll"].fn({ event = "up" })
    -- The trackpad's messages: a scroll pans by its points at the display's scale, the coasting is left out, a smooth
    -- wheel zooms
    local n = #got
    f.message("gesture", "scroll", "3", "-2", "changed")
    expect("scroll pans", got[#got][1], "pan")
    expect("scroll dx in pixels", got[#got][2], 6)
    expect("scroll dy in pixels", got[#got][3], -4)
    f.message("gesture", "scroll", "3", "-2", "momentum")
    expect("momentum ignored", #got, n + 1)
    f.message("gesture", "scroll", "0", "10", "none")
    expect("smooth wheel zooms", got[#got][1], "zoom")
    near("smooth wheel: one click", got[#got][2], gestures.WHEEL_ZOOM)
    f.message("gesture", "magnify", "0.2", "changed")
    near("pinch zooms", got[#got][2], 1.2)
    f.message("gesture", "rotate", "5", "changed")
    near("rotation rolls", got[#got][2], 5)
    -- With the messages, the wheel zooms
    f.bindings["test-key-wheel-up"].fn({ event = "down", scale = 1 })
    expect("wheel up zooms in", got[#got][1] == "zoom" and got[#got][2] > 1, true)
    f.bindings["test-key-reset-dbl"].fn()
    expect("double click resets", got[#got][1], "reset")
    -- Paused: nothing
    n = #got
    g:pause(true)
    f.message("gesture", "scroll", "3", "-2", "changed")
    f.bindings["test-key-wheel-up"].fn({ event = "down", scale = 1 })
    f.bindings["test-key-reset"].fn()
    expect("paused: left alone", #got, n)
    -- Another set of gestures gave the messages back as it left: taken again
    f.props["macos-gesture-messages"] = false
    g:pause(false)
    expect("unpaused: messages again", f.props["macos-gesture-messages"], true)
    g:set(false)
    expect("off: bindings gone", f.forced(), 0)
    expect("off: window dragged again", f.props["window-dragging"], true)
    expect("off: messages given back", f.props["macos-gesture-messages"], false)
    f.message("gesture", "scroll", "3", "-2", "changed")
    expect("off: no more", #got, n)
end

-- Off macOS (no gesture messages): a wheel's whole clicks zoom, a touchpad's fractions pan, Ctrl+wheel zooms
do
    local f = fake_mp()
    f.props["macos-gesture-messages"] = nil
    local set = f.mp.set_property_native
    f.mp.set_property_native = function(name, v)
        if name == "macos-gesture-messages" then return false end
        return set(name, v)
    end
    package.loaded['mp'] = f.mp
    package.loaded['gestures'] = nil
    local gestures = require 'gestures'
    local got = {}
    local g = gestures.new({
        prefix = "t2", pan = function(dx, dy) table.insert(got, { "pan", dx, dy }) end,
        zoom = function(k) table.insert(got, { "zoom", k }) end, roll = function() end, reset = function() end,
    })
    g:set(true)
    f.bindings["t2-key-wheel-down"].fn({ event = "down", scale = 1 })
    expect("click zooms", got[#got][1], "zoom")
    expect("down zooms out", got[#got][2] < 1, true)
    f.bindings["t2-key-wheel-down"].fn({ event = "down", scale = 0.25 })
    expect("fraction pans", got[#got][1], "pan")
    near("fraction pans by the scroll unit", got[#got][3], -0.25 * gestures.SCROLL_PX)
    f.bindings["t2-key-pinch-in"].fn({ event = "down", scale = 0.25 })
    expect("ctrl+wheel zooms", got[#got][1], "zoom")
    g:set(false)
end

-- The view360 script: a 360° clip turns the view on, any other clip is left as it is
local function load_view360()
    local f = fake_mp()
    package.loaded['mp'] = f.mp
    package.loaded['gestures'] = nil
    package.loaded['view'] = nil
    local chunk = assert(loadfile("mpv/scripts/view360/main.lua"))
    chunk()
    local hook
    for _, h in ipairs(f.hooks) do if h.name == "on_load" then hook = h end end
    f.load = function(spherical)
        f.props["user-data/gyroflow/spherical"] = spherical
        hook.fn({})
        f.event("file-loaded")
    end
    return f, hook
end

do
    local f, hook = load_view360()
    expect("hooked after the gyroflow plugin's probe (50)", hook.prio > 50, true)
    expect("shader in glsl-shaders", f.commands[1] and f.commands[1][4] or nil, "mpv/scripts/view360/view360.glsl")
    -- An ordinary clip: nothing of the view
    f.load(false)
    local opts = f.props["glsl-shader-opts"] or {}
    expect("ordinary: shader off", opts.view360_on, "0")
    expect("ordinary: aspect untouched", f.props["file-local-options/video-aspect-override"], nil)
    expect("ordinary: no forced bindings", f.forced(), 0)
    expect("ordinary: window still dragged", f.props["window-dragging"], true)
    expect("ordinary: not available", f.props["user-data/view360"].available, false)
    f.event("end-file")

    -- A stitched 360° clip, with V1's initial view
    f.props["glsl-shader-opts"] = { denoise_strength = "2" }
    f.load({ projection = "equirect", stereo = "mono", source = "xmp", initial_view = { heading = 30, pitch = 5, roll = 0 } })
    opts = f.props["glsl-shader-opts"]
    expect("360: shader on", opts.view360_on, "1")
    expect("360: others' options kept", opts.denoise_strength, "2")
    expect("360: the window's aspect", f.props["file-local-options/video-aspect-override"], "1600:900")
    expect("360: the window free", f.props["file-local-options/keepaspect-window"], "no")
    expect("360: gestures taken", f.forced() >= 10, true)
    expect("360: a frame drawn a refresh ahead", f.props["file-local-options/video-timing-offset"], "0.0123")
    expect("360: not the app's mpv, no display sync option", f.props["file-local-options/macos-display-sync"], nil)
    expect("360: initial heading", opts.view360_yaw, "30.0000")
    local st = f.props["user-data/view360"]
    expect("360: available", st.available, true)
    expect("360: active", st.active, true)
    -- The side panel: the area narrower, the aspect follows
    f.set("video-margin-ratio-right", 0.25)
    expect("360: the aspect beside the panel", f.props["file-local-options/video-aspect-override"], "1200:900")
    f.set("video-margin-ratio-right", 0)
    -- The wheel: closer
    local fov = tonumber(f.props["glsl-shader-opts"].view360_fov)
    f.bindings["view360-key-wheel-up"].fn({ event = "down", scale = 1 })
    expect("wheel narrows the fov", tonumber(f.props["glsl-shader-opts"].view360_fov) < fov, true)
    -- A drag to the right looks left
    f.props["mouse-pos"] = { x = 800, y = 450 }
    f.bindings["view360-key-drag"].fn({ event = "down" })
    f.set("mouse-pos", { x = 900, y = 450 })
    expect("drag right looks left", tonumber(f.props["glsl-shader-opts"].view360_yaw) < 30, true)
    f.bindings["view360-key-drag"].fn({ event = "up" })
    -- Back ahead
    f.bindings["view360-key-reset-dbl"].fn()
    expect("reset: the initial heading", f.props["glsl-shader-opts"].view360_yaw, "30.0000")
    expect("reset: the fov", f.props["glsl-shader-opts"].view360_fov, "100.0000")
    -- The panel's messages
    f.message("view360-projection", "planet")
    expect("planet", f.props["glsl-shader-opts"].view360_projection, "3")
    expect("planet's fov", f.props["glsl-shader-opts"].view360_fov, "270.0000")
    f.message("view360-projection", "next")
    expect("next after planet: auto", f.props["glsl-shader-opts"].view360_projection, "0")
    f.message("view360-set", "fov", "500")
    expect("fov held in range", f.props["glsl-shader-opts"].view360_fov, "300.0000")
    f.message("view360-heading", "camera")
    expect("heading", f.props["glsl-shader-opts"].view360_heading, "1")
    -- The app's mpv: each message carries its time and number; the view says which it has applied, so the VO turns
    -- it by the later ones itself; messages waiting together are applied once (setting the options waits for the VO)
    f.defer = true
    local sets = 0
    local set = f.mp.set_property_native
    f.mp.set_property_native = function(name, v)
        if name == "glsl-shader-opts" then sets = sets + 1 end
        return set(name, v)
    end
    local before = f.props["glsl-shader-opts"].view360_yaw
    f.message("gesture", "scroll", "10", "0", "changed", "100.250000", "41")
    f.message("gesture", "scroll", "10", "0", "changed", "100.258000", "42")
    f.message("gesture", "scroll", "10", "0", "momentum", "100.266000", "43")
    expect("waiting messages: not applied yet", sets, 0)
    f.flush()
    expect("waiting messages: applied once", sets, 1)
    expect("applied: turned", f.props["glsl-shader-opts"].view360_yaw ~= before, true)
    expect("applied: the last number, the coasting's too", f.props["glsl-shader-opts"].afp_input_seq, "43")
    expect("applied: the last taken one's time", f.props["glsl-shader-opts"].afp_input_time, "100.258000")
    f.mp.set_property_native = set
    f.defer = false
    -- Crop mode on top: the view's gestures pause (crop's take the mouse), and come back
    f.set("user-data/gyroflow/crop", { active = true })
    expect("under crop mode: the VO doesn't turn the view", f.props["glsl-shader-opts"].afp_input_seq, nil)
    local yaw = f.props["glsl-shader-opts"].view360_yaw
    f.message("gesture", "scroll", "30", "0", "changed")
    expect("under crop mode: left alone", f.props["glsl-shader-opts"].view360_yaw, yaw)
    f.props["macos-gesture-messages"] = false
    f.set("user-data/gyroflow/crop", { active = false })
    expect("crop mode left: messages again", f.props["macos-gesture-messages"], true)
    f.message("gesture", "scroll", "30", "0", "changed")
    expect("crop mode left: turns again", f.props["glsl-shader-opts"].view360_yaw ~= yaw, true)
    -- Off by hand: the sphere flat, keepaspect back
    f.bindings["toggle"].fn()
    expect("toggled off: shader off", f.props["glsl-shader-opts"].view360_on, "0")
    expect("toggled off: aspect back", f.props["file-local-options/video-aspect-override"], "-1")
    expect("toggled off: window aspect back", f.props["file-local-options/keepaspect-window"], "yes")
    expect("toggled off: timing back", f.props["file-local-options/video-timing-offset"], "0.050000")
    expect("toggled off: gestures given back", f.forced(), 0)
    expect("toggled off: window dragged", f.props["window-dragging"], true)
    f.bindings["toggle"].fn()
    expect("toggled on again", f.props["glsl-shader-opts"].view360_on, "1")
    -- The next file: everything back
    f.event("end-file")
    expect("end: shader off", f.props["glsl-shader-opts"].view360_on, "0")
    expect("end: gestures given back", f.forced(), 0)
    expect("end: window dragged", f.props["window-dragging"], true)
    expect("end: not available", f.props["user-data/view360"].available, false)
end

do
    -- A cubemap, and a stereo layout it doesn't know: said why, not shown
    local f = load_view360()
    f.load({ projection = "cubemap", stereo = "mono" })
    expect("cubemap: not shown", f.props["glsl-shader-opts"].view360_on, "0")
    expect("cubemap: says why", type(f.props["user-data/view360"].note), "string")
    f.event("end-file")
    f.load({ projection = "equirect", stereo = "other" })
    expect("odd stereo: not shown", f.props["glsl-shader-opts"].view360_on, "0")
    f.event("end-file")
    f.load({ projection = "equirect", stereo = "top-bottom" })
    expect("top-bottom: shown, the left eye", f.props["glsl-shader-opts"].view360_stereo, "1")
end

do
    -- No gpu video output: no shader, so no view
    local f = fake_mp()
    f.props["vo"] = "image"
    package.loaded['mp'] = f.mp
    package.loaded['gestures'] = nil
    package.loaded['view'] = nil
    assert(loadfile("mpv/scripts/view360/main.lua"))()
    f.props["user-data/gyroflow/spherical"] = { projection = "equirect", stereo = "mono" }
    for _, h in ipairs(f.hooks) do h.fn({}) end
    expect("vo=image: not shown", f.props["user-data/view360"].active, false)
    expect("vo=image: says why", type(f.props["user-data/view360"].note), "string")
    expect("vo=image: no gestures", f.forced(), 0)
end

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
