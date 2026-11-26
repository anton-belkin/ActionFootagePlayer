-- The as-recorded comparison (mpv/scripts/gyroflow/compare.lua, h and afp-compare) over a fake mpv: held, it's shown
-- until the key is let go; tapped, until the next press; the message toggles, or sets on/off. Each switch is one change
-- of glsl-shader-opts with both parameters (gyroflow_gpu_on 0 skips the warp's passes, afp_recorded 1 the looks'
-- branch), and back the keys come out so the shaders' defaults hold; the user's other parameters stay. Where the warp
-- turned or reshaped the frame, mpv gets the file's geometry meanwhile and the warp's back after. A 360 camera's file
-- isn't compared; the file ending takes it back; the card says it. And the shaders: every look template declares
-- afp_recorded and branches on it. From the repository's folder: luajit tests/compare_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- A fake mpv: properties, the clock, bindings, messages, events, and every change of glsl-shader-opts counted
local f = { props = {}, now = 100, bindings = {}, messages = {}, events = {}, opt_sets = 0, cards = {} }
local function copy(t) local c = {} for k, v in pairs(t or {}) do c[k] = v end return c end
f.props["glsl-shader-opts"] = { gyroflow_render_scale = "0.5000", uw_dr = "0.2" }
f.props["input-bindings"] = { { key = "h", cmd = "script-binding gyroflow/compare" } }
local mp = {
    msg = { info = function() end, warn = function() end, verbose = function() end },
    get_time = function() return f.now end,
    get_property = function(n) local v = f.props[n]; if v == nil then return nil end return tostring(v) end,
    get_property_native = function(n) local v = f.props[n]; if type(v) == "table" then return copy(v) end return v end,
    set_property = function(n, v) f.props[n] = v end,
    set_property_native = function(n, v)
        if n == "glsl-shader-opts" then f.opt_sets = f.opt_sets + 1; v = copy(v) end
        f.props[n] = v
    end,
    add_key_binding = function(key, name, fn, o) f.bindings[name] = { key = key, fn = fn, complex = o and o.complex } end,
    register_script_message = function(name, fn) f.messages[name] = fn end,
    register_event = function(name, fn) f.events[name] = fn end,
}
mp.notify = function(t) f.cards[#f.cards + 1] = t end
package.preload['mp'] = function() return mp end

local compare = require 'compare'
local S = { gpu = { shader = "/tmp/warp.glsl", output = { 3840, 2160 }, denoise = true }, current_source = { rotation = 0 } }
compare.setup({ S = S })

local function opts() return f.props["glsl-shader-opts"] end
local function shown() return f.props["user-data/gyroflow/compare"] end
local function last_card() return f.cards[#f.cards] end
local key = f.bindings.compare
expect("bound to h", key and key.key, "h")
expect("the key sees down and up", key and key.complex, true)
expect("the message", type(f.messages["afp-compare"]), "function")
expect("the binding isn't named like the message", f.bindings["afp-compare"], nil)
expect("published off at start", shown(), false)

-- Held: on while down, off when let go
local before = f.opt_sets
key.fn({ event = "down" })
expect("held: shown", shown(), true)
expect("held: warp skipped", opts().gyroflow_gpu_on, "0")
expect("held: looks as recorded", opts().afp_recorded, "1")
expect("held: one change of the opts", f.opt_sets - before, 1)
expect("held: others kept", opts().gyroflow_render_scale, "0.5000")
expect("held: the card", last_card().title, "As recorded")
expect("held: the card says what's left out", last_card().detail:match("^[^\n]*"), "Without the stabilization and the denoise")
key.fn({ event = "repeat" })
f.now = f.now + 1.2
key.fn({ event = "up" })
expect("let go: back", shown(), false)
expect("let go: gpu_on key out", opts().gyroflow_gpu_on, nil)
expect("let go: afp_recorded key out", opts().afp_recorded, nil)
expect("let go: others kept", opts().uw_dr, "0.2")
expect("let go: the card goes", last_card().kind, "hide")

-- Tapped: stays on after its up, the next press ends it and its up does nothing
key.fn({ event = "down" })
f.now = f.now + 0.1
key.fn({ event = "up" })
expect("tapped: still shown", shown(), true)
expect("tapped: the card says how back", last_card().detail:match("\n(.*)$"), "H again: as it plays here")
f.now = f.now + 3
key.fn({ event = "down" })
expect("tapped again: back", shown(), false)
f.now = f.now + 0.1
key.fn({ event = "up" })
expect("its up: still back", shown(), false)

-- A key without up events (mpv's "press"): a toggle
key.fn({ event = "press" })
expect("press: on", shown(), true)
key.fn({ event = "press" })
expect("press again: off", shown(), false)

-- The message: toggle by default, on/off as said, the input's time for the latency log
f.messages["afp-compare"]()
expect("message: on", shown(), true)
f.messages["afp-compare"]("toggle")
expect("message toggle: off", shown(), false)
f.messages["afp-compare"]("on", "12345.5")
expect("message on: on", shown(), true)
expect("message on: input time", opts().afp_input_time, "12345.500000")
f.messages["afp-compare"]("on")
expect("on again: still on", shown(), true)
f.messages["afp-compare"]("off")
expect("message off: off", shown(), false)

-- A photo, a moment's picture: export.lua turns it off first
compare.set(true)
compare.off()
expect("off(): off", shown(), false)

-- The warp's geometry: the file's own turn and an output of another shape are mpv's meanwhile, the warp's after
S.current_source.rotation = 90
f.props["file-local-options/video-rotate"] = "no"
f.props["file-local-options/video-aspect-override"] = "9:16"
compare.set(true)
expect("turned: the file's turn", f.props["file-local-options/video-rotate"], "90")
expect("reshaped: the file's shape", f.props["file-local-options/video-aspect-override"], "no")
compare.set(false)
expect("back: the warp's turn", f.props["file-local-options/video-rotate"], "no")
expect("back: the warp's shape", f.props["file-local-options/video-aspect-override"], "9:16")
S.current_source.rotation = 0
f.props["file-local-options/video-aspect-override"] = "no"
f.props["file-local-options/video-rotate"] = nil
compare.set(true)
expect("as the file: nothing touched", f.props["file-local-options/video-rotate"], nil)
compare.set(false)

-- The file ends: back, without restoring file-local geometry (mpv does)
S.current_source.rotation = 180
compare.set(true)
f.props["file-local-options/video-rotate"] = "sentinel"
f.events["end-file"]()
expect("end-file: off", shown(), false)
expect("end-file: keys out", opts().gyroflow_gpu_on, nil)
expect("end-file: file-local left to mpv", f.props["file-local-options/video-rotate"], "sentinel")
S.current_source.rotation = 0

-- A 360 camera's own file: refused with a card
S.rig360 = { brand = "Insta360" }
expect("360: refused", compare.set(true), false)
expect("360: not shown", shown(), false)
expect("360: the card", last_card().title, "No as-recorded view for a 360 camera's file")
S.rig360 = false

-- A log clip in its look, not stabilized: the card says the colour
S.gpu = nil
f.props["user-data/dlogm"] = { log = true, curve = "dlogm", look = "hdr" }
compare.set(true)
expect("log alone: the card", last_card().detail:match("^[^\n]*"), "Without the colour")
compare.set(false)

-- The shaders. Every look template declares afp_recorded (DYNAMIC, default 0, no comment after its value) and
-- branches on it in its pass
local function read(path) local h = assert(io.open(path, "rb")); local s = h:read("*a"); h:close(); return s end
for _, path in ipairs({ "shaders/log_hlg.glsl.in", "shaders/dlogm_hlg.glsl.in", "shaders/dlogm_lut3d.glsl.in", "shaders/underwater_hlg.glsl" }) do
    local s = read(path)
    expect(path .. ": declares afp_recorded", s:find("//!PARAM afp_recorded\n//!TYPE DYNAMIC int\n//!MINIMUM 0\n//!MAXIMUM 1\n0\n\n//!", 1, true) ~= nil, true)
    local pass = s:match("//!HOOK MAIN.*$") or ""
    expect(path .. ": branches on it in its pass", pass:find("if (afp_recorded != 0)", 1, true) ~= nil, true)
end
print(("%d checks, %d failures"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
