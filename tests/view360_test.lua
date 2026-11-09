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

-- The recording (mpv/scripts/view360/recording.lua): takes from synthetic streams of frames at 25 fps
local recording = require 'recording'
local FPS = 25
-- A take from a to b seconds, the head given by fn(t) -> yaw, pitch, roll, fov, projection
local function take(r, follows, a, b, fn)
    local ok, why = r:start(follows)
    if not ok then return nil, why end
    local n = math.floor((b - a) * FPS + 0.5)
    for i = 0, n do
        local t = a + i / FPS
        r:add(t, fn(t))
    end
    return r:stop()
end
local function still(yaw, fov) return function() return yaw, 0, 0, fov or 100, "auto" end end
do
    -- A still head is two samples; a steady turn too; a wobble keeps what it needs to stay within the tolerance
    local r = recording.new()
    local p = take(r, "travel", 10, 20, still(30))
    expect("still: two samples", #p.samples, 2)
    expect("still: from", p.from, 10)
    near("still: to", p.to, 20, 1e-9)
    r:clear()
    p = take(r, "travel", 0, 10, function(t) return 5 * t, -t, 0, 100, "auto" end)
    expect("steady turn: two samples", #p.samples, 2)
    near("steady turn: between", r:head(3.3).yaw, 16.5, 1e-9)
    r:clear()
    local function wobble(t) return 20 * math.sin(t), 5 * math.cos(2 * t), 0, 100 * math.exp(0.2 * math.sin(t / 3)), "auto" end
    p = take(r, "travel", 0, 20, wobble)
    expect("wobble: thinned", #p.samples < 20 * FPS / 2, true)
    expect("wobble: not to nothing", #p.samples > 10, true)
    local worst = 0
    for i = 0, 20 * FPS do
        local t = i / FPS
        local h = r:head(t)
        local y, pi, _, f = wobble(t)
        worst = math.max(worst, math.abs(h.yaw - y), math.abs(h.pitch - pi), math.abs(100 * math.log(h.fov) - 100 * math.log(f)))
    end
    expect("wobble: every frame within the tolerance", worst <= recording.TOLERANCE + 1e-9, true)
    -- The yaw across ±180°: unwrapped, played back the short way
    r:clear()
    p = take(r, "camera", 0, 2, function(t) return ((170 + 10 * t) + 180) % 360 - 180, 0, 0, 100, "auto" end)
    near("wrap: unwrapped", p.samples[#p.samples].yaw, 190, 1e-6)
    near("wrap: the short way", r:head(1).yaw, 180, 1e-6)
end
do
    -- Punch-in: 20-30 over 12-47 keeps 12-20 and 30-47 as they were
    local r = recording.new()
    take(r, "travel", 12, 47, function(t) return t, 0, 0, 100, "auto" end)
    take(r, "travel", 20, 30, still(-90, 60))
    expect("punch-in: three parts", #r.parts, 3)
    near("punch-in: before kept", r:head(15).yaw, 15, 1e-6)
    near("punch-in: up to the cut", r:head(19.96).yaw, 19.96, 1e-6)
    near("punch-in: the take", r:head(25).yaw, -90, 1e-9)
    near("punch-in: the take's fov", r:head(25).fov, 60, 1e-9)
    near("punch-in: its first frame", r:head(20).yaw, -90, 1e-9)
    near("punch-in: its last frame", r:head(30).yaw, -90, 1e-9)
    near("punch-in: after kept", r:head(30.04).yaw, 30.04, 1e-6)
    near("punch-in: after kept, later", r:head(40).yaw, 40, 1e-6)
    expect("punch-in: one span", #r:spans(), 1)
    near("punch-in: the span", r:spans()[1].to, 47, 1e-9)
    -- Holding: before the first part, between parts (the end of the one before), after the last
    take(r, "travel", 60, 70, still(45))
    near("hold before", r:head(3).yaw, 12, 1e-9)
    near("hold between", r:head(55).yaw, 47, 1e-6)
    near("hold after", r:head(90).yaw, 45, 1e-9)
    expect("two spans", #r:spans(), 2)
    -- Undo, one take at a time: what each replaced comes back
    expect("undo: the last take", r:undo(), true)
    near("undo: after the last part again", r:head(90).yaw, 47, 1e-6)
    expect("undo: the punch-in", r:undo(), true)
    expect("undo: one part again", #r.parts, 1)
    near("undo: as it was", r:head(25).yaw, 25, 1e-6)
    expect("undo: the first", r:undo(), true)
    expect("undo: nothing", r:empty(), true)
    expect("undo: follows nothing", r.follows, nil)
    expect("undo: no more", r:undo(), false)
    -- A take that covers a whole part replaces it; one inside a part's start only cuts the start
    take(r, "travel", 10, 20, still(1))
    take(r, "travel", 5, 25, still(2))
    expect("covered: one part", #r.parts, 1)
    near("covered: the new", r:head(15).yaw, 2, 1e-9)
    take(r, "travel", 0, 10, still(3))
    expect("start cut: two parts", #r.parts, 2)
    near("start cut: the rest", r:head(20).yaw, 2, 1e-9)
    -- At most 20 takes back
    r:clear()
    for i = 1, 25 do take(r, "travel", i, i + 0.5, still(i)) end
    expect("undo limited", r:undoable(), recording.UNDO)
end
do
    -- Follows: the first take's; another's refused, saying which the recording has
    local r = recording.new()
    take(r, "travel", 0, 1, still(0))
    local p, why = take(r, "camera", 2, 3, still(0))
    expect("other follows refused", p, nil)
    expect("says the recording's", why, "travel")
    expect("same follows taken", take(r, "travel", 2, 3, still(0)) ~= nil, true)
    r:clear()
    expect("cleared: any follows", take(r, "camera", 2, 3, still(0)) ~= nil, true)
    expect("now camera", r.follows, "camera")
    -- A seek back during a take: add says so
    r:start("camera")
    r:add(5, 0, 0, 0, 100, "auto")
    expect("back in time", r:add(4, 0, 0, 0, 100, "auto"), false)
    -- Too short to keep
    r:stop()
    r:start("camera")
    r:add(5, 0, 0, 0, 100, "auto")
    expect("one frame: nothing", r:stop(), nil)
end
do
    -- The projection: a change is a cut at the later sample, kept through thinning; in the file only where it changes
    local r = recording.new()
    take(r, "world", 0, 4, function(t) if t < 2 then return 0, 0, 0, 100, "auto" else return 0, 0, 0, 270, "planet" end end)
    expect("projection: four samples", #r.parts[1].samples, 4)
    expect("projection: before", r:head(1.99).projection, "auto")
    near("projection: held to the change", r:head(1.99).fov, 100, 1e-9)
    expect("projection: on it", r:head(2).projection, "planet")
    local t = r:to_table()
    expect("file: follows", t.follows, "world")
    expect("file: first sample has it", t.parts[1].samples[1][6], "auto")
    expect("file: no repeat", t.parts[1].samples[2][6], nil)
    expect("file: the change", t.parts[1].samples[3][6], "planet")
    expect("file: frame", t.frame, "16:9")
    -- Read back: the same; keys it doesn't know kept; bad samples left out
    t.tidy = { turns = "medium" }
    table.insert(t.parts[1].samples, { 1, 0, 0, 0, 100 })
    table.insert(t.parts, { samples = { { 9, 0, 0, 0, 100 } } })
    local r2 = recording.new():load(t)
    expect("read: one part", #r2.parts, 1)
    expect("read: samples", #r2.parts[1].samples, 4)
    expect("read: projection filled in", r2.parts[1].samples[2].projection, "auto")
    expect("read: planet after", r2:head(3).projection, "planet")
    expect("read: tidy kept", r2:to_table().tidy.turns, "medium")
    expect("read: nonsense", recording.new():load({ follows = "sideways", parts = {} }):empty(), true)
    expect("empty: no table", recording.new():to_table(), nil)
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

do
    -- A 360 camera's own file (an Insta360 .insv with GPS): stamped, along the camera until the clip's been read; then along
    -- the way, which it has; Alt+h through the modes it has; north it hasn't, refused saying why
    local f = load_view360()
    f.props["user-data/gyroflow/rig360"] = { brand = "Insta360", model = "X5", lenses = 2, tracks = 2 }
    f.load(false)
    local function mode() return f.props["glsl-shader-opts"].view360_heading end
    expect("rig: stamped", f.props["user-data/view360"].stamped, true)
    expect("rig: along the camera first", mode(), "1")
    expect("rig: modes not known yet", f.props["user-data/view360"].modes, false)
    f.props["time-pos"] = 2.4
    f.set("user-data/gyroflow/headings360", {
        travel = { available = true, confidence = 1, sigma_deg = 1.6 },
        north = { available = false, why = "an Insta360 has no compass" },
        world = { held_by = "gps" },
        seconds = { { 0, 0 }, { 0.5, 0 }, { 0.9, 0 } },
    })
    expect("read: along the way", mode(), "2")
    expect("read: travel there", f.props["user-data/view360"].modes.travel.available, true)
    expect("read: north not, and why", f.props["user-data/view360"].modes.north.why, "an Insta360 has no compass")
    expect("read: the world held by GPS", f.props["user-data/view360"].modes.world.held_by, "gps")
    near("read: sure now (third second)", f.props["user-data/view360"].heading_confidence, 0.9)
    f.set("time-pos", 1.2)
    near("later: sure then", f.props["user-data/view360"].heading_confidence, 0.5)
    f.bindings["heading"].fn()
    expect("Alt+h: along the camera", mode(), "1")
    f.bindings["heading"].fn()
    expect("Alt+h: north skipped, the world", mode(), "0")
    f.bindings["heading"].fn()
    expect("Alt+h: back to along the way", mode(), "2")
    f.message("view360-heading", "north")
    expect("north refused", mode(), "2")
    expect("north refused: says why", (f.osd or ""):find("compass", 1, true) ~= nil, true)
    f.message("view360-heading", "camera")
    -- Chosen by the viewer: a later reading doesn't take it back
    f.set("user-data/gyroflow/headings360", { travel = { available = true }, north = { available = true }, seconds = {} })
    expect("chosen stays", mode(), "1")
    f.message("view360-heading", "north")
    expect("north now there", mode(), "3")
    f.event("end-file")
    f.props["user-data/gyroflow/rig360"] = nil
end

do
    -- Recording in the view360 script: a take (Alt+r, paused: it plays), ended by pausing and kept in the clip's file,
    -- played back; a gesture takes the view back; Follows locked; a take under another refused; a punch-in ended by a
    -- seek; undo; clear
    local function serialize(v)
        if type(v) == "table" then
            local parts = {}
            for k, x in pairs(v) do
                parts[#parts + 1] = "[" .. (type(k) == "string" and string.format("%q", k) or tostring(k)) .. "]=" .. serialize(x)
            end
            return "{" .. table.concat(parts, ",") .. "}"
        end
        return type(v) == "string" and string.format("%q", v) or tostring(v)
    end
    local function parse(text)
        local fn = (loadstring or load)("return " .. text)
        local ok, v = pcall(fn or function() end)
        return ok and v or nil
    end
    package.loaded['mp.utils'] = { parse_json = parse, format_json = serialize }
    package.loaded['sidecar'] = nil
    package.loaded['recording'] = nil
    local dir = os.tmpname()
    os.remove(dir)
    os.execute("mkdir -p '" .. dir .. "'")
    local clip = dir .. "/VID_20250416_143628_00_005.insv"
    local file = dir .. "/VID_20250416_143628_00_005.afp.json"
    local function on_disk()
        local fh = io.open(file, "r")
        if not fh then return nil end
        local text = fh:read("*a")
        fh:close()
        return parse(text)
    end

    local f = load_view360()
    f.props["user-data/gyroflow/rig360"] = { brand = "Insta360", model = "X5", lenses = 2, tracks = 2 }
    f.props["path"] = clip
    f.props["pause"] = true
    f.props["time-pos"] = 10
    f.load(false)
    local function st() return f.props["user-data/view360"].recording end
    local function yaw() return tonumber(f.props["glsl-shader-opts"].view360_yaw) end
    expect("rec: can", st().can, true)
    expect("rec: nothing yet", st().parts, 0)
    f.bindings["record"].fn()
    expect("rec: paused, so it plays", f.props["pause"], false)
    expect("rec: taking", st().taking, true)
    -- Follows locked during a take
    f.message("view360-heading", "world")
    expect("rec: follows locked", f.props["glsl-shader-opts"].view360_heading, "1")
    for i = 1, 100 do
        f.message("view360-set", "yaw", tostring(i * 0.5))
        f.set("time-pos", 10 + i / 25)
    end
    expect("rec: seconds", st().seconds, 4)
    f.set("pause", true)
    expect("rec: a pause ends the take", st().taking, false)
    expect("rec: one part", st().parts, 1)
    expect("rec: plays by itself", st().playing, true)
    local doc = on_disk()
    expect("rec: in the clip's file", doc and doc.view360.recording.follows, "camera")
    expect("rec: thinned in the file", doc and #doc.view360.recording.parts[1].samples, 2)
    -- Played back: the head from the recording at each frame shown
    f.set("time-pos", 12)
    near("rec: played back", yaw(), 25, 1e-3)
    f.set("time-pos", 3)
    near("rec: held before", yaw(), 0, 1e-3)
    f.set("time-pos", 11)
    near("rec: between", yaw(), 12.5, 1e-3)
    -- Follows locked while it plays
    f.message("view360-heading", "world")
    expect("rec: follows locked while playing", f.props["glsl-shader-opts"].view360_heading, "1")
    -- A gesture: the view is yours, from there
    f.bindings["view360-key-wheel-up"].fn({ event = "down", scale = 1 })
    expect("rec: a gesture pauses it", st().playing, false)
    expect("rec: says so", f.osd, "Your view: the recording is paused")
    f.set("time-pos", 12)
    near("rec: no longer followed", yaw(), 12.5, 1e-3)
    -- A take under another Follows: refused, saying which
    f.message("view360-heading", "world")
    f.bindings["record"].fn()
    expect("rec: other follows refused", st().taking, false)
    expect("rec: says which", (f.osd or ""):find("follows the camera", 1, true) ~= nil, true)
    f.message("view360-heading", "camera")
    -- A punch-in, ended by a seek
    f.set("time-pos", 11)
    f.bindings["record"].fn()
    f.message("view360-set", "yaw", "-90")
    for i = 1, 25 do f.set("time-pos", 11 + i / 25) end
    f.event("seek")
    expect("rec: a seek ends it", st().taking, false)
    expect("rec: punched in", st().parts, 3)
    expect("rec: one span", #st().spans, 1)
    f.set("time-pos", 11.5)
    near("rec: the punch-in plays", yaw(), -90, 1e-3)
    f.message("view360-undo")
    expect("rec: undone", st().parts, 1)
    f.set("time-pos", 11.48)
    near("rec: as before", yaw(), 37 * 0.5, 1e-3)
    expect("rec: undone in the file", #on_disk().view360.recording.parts, 1)
    f.message("view360-frame", "9:16")
    expect("rec: frame", on_disk().view360.recording.frame, "9:16")
    f.bindings["play-recording"].fn()
    expect("rec: Alt+R toggles", st().playing, false)
    f.message("view360-clear")
    expect("rec: cleared", st().parts, 0)
    expect("rec: file gone with it", on_disk(), nil)
    -- Leaving the view ends a take
    f.bindings["record"].fn()
    f.set("time-pos", 11.52)
    f.set("time-pos", 11.56)
    f.bindings["toggle"].fn()
    expect("rec: leaving the view ends it", st().taking, false)
    expect("rec: and keeps it", st().parts, 1)
    f.event("end-file")
    os.remove(file)
    os.execute("rmdir '" .. dir .. "'")
    f.props["user-data/gyroflow/rig360"] = nil
end

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
