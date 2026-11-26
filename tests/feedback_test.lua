-- Send feedback… (footage-menu's afp-feedback message): the site's form opens with the player's version, the system
-- and, with a clip open, its camera, log curve and kind of motion data; never the clip's name or path, and with no clip
-- open just the version and the system. Over a stand-in for mpv. From the repository's folder: luajit tests/feedback_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path

local props, messages, ran = {}, {}, {}
local mp = {}
function mp.get_property_native(name, def) local v = props[name]; if v == nil then return def end; return v end
function mp.get_property(name, def)
    local v = props[name]
    if v == nil or type(v) == "table" then return def end
    return tostring(v)
end
function mp.get_property_number(name, def) return tonumber(props[name]) or def end
function mp.set_property_native(name, v) props[name] = v end
function mp.set_property(name, v) props[name] = v; return true end
function mp.observe_property() end
function mp.register_event() end
function mp.register_script_message(name, fn) messages[name] = fn end
function mp.add_key_binding() end
function mp.add_forced_key_binding() end
function mp.remove_key_binding() end
function mp.add_timeout() return { kill = function() end } end
function mp.add_periodic_timer() return { kill = function() end, resume = function() end } end
function mp.get_time() return 0 end
function mp.command() end
function mp.commandv() end
function mp.osd_message() end
function mp.get_script_name() return "footage_menu" end
function mp.get_script_directory() return "mpv/scripts/footage-menu" end
-- sw_vers, run for the macOS version
function mp.command_native(c)
    if c.args and c.args[1] == "sw_vers" then return { status = 0, stdout = "15.6.1\n" } end
end
function mp.command_native_async(c) table.insert(ran, c.args) end
mp.notify = function() end
mp.msg = { info = function() end, warn = function() end, error = function() end, debug = function() end, verbose = function() end }
package.preload.mp = function() return mp end
package.preload["mp.utils"] = function()
    return { parse_json = function() return nil end, format_json = function() return "{}" end,
             join_path = function(a, b) return a .. "/" .. b end, split_path = function(p) return p end }
end
local version = "0.15.0"
package.preload["mp.options"] = function() return { read_options = function(o) o.version = version end } end

local failures, checks = 0, 0
local function expect(what, ok, detail)
    checks = checks + 1
    if not ok then failures = failures + 1; print("FAIL " .. what .. (detail and (": " .. detail) or "")) end
end

local function load(platform)
    props = { platform = platform, ["mpv-version"] = "mpv v0.42.0", ["input-bindings"] = {} }
    messages = {}
    dofile("mpv/scripts/footage-menu/main.lua")
end

-- The URL the message opened, and its fields
local function send()
    ran = {}
    messages["afp-feedback"]()
    local args = ran[#ran]
    local url = args and args[#args] or ""
    local fields = {}
    for k, v in (url:match("%?(.*)$") or ""):gmatch("([^&=]+)=([^&]*)") do
        fields[k] = v:gsub("%%(%x%x)", function(h) return string.char(tonumber(h, 16)) end)
    end
    return url, fields, args
end

-- No clip open: the version and the system only
load("darwin")
expect("the message is registered", messages["afp-feedback"] ~= nil)
local url, f, args = send()
expect("opens the site's form", url:find("^https://actionfootageplayer%.ch/feedback%?") ~= nil, url)
expect("with open on a Mac", args and args[1] == "open", args and args[1])
expect("the version", f.v == "0.15.0", f.v)
expect("the system with its version", f.os == "macOS 15.6.1", f.os)
expect("no camera without a clip", f.camera == nil, f.camera)
expect("no curve without a clip", f.curve == nil and f.motion == nil)

-- A clip open: its camera, curve and motion data, and nothing of its name or path
props.path = "/Volumes/Dives/Thistlegorm wreck/DJI_20261101_0042_D.MP4"
props.filename = "DJI_20261101_0042_D.MP4"
props["user-data/dji_telemetry"] = { model = "dvtm_ac206.proto", available = true }
props["user-data/dlogm"] = { log = true, curve = "dlogm", recorded = "dlogm", hdr = "" }
props["user-data/gyroflow"] = { available = true, kind = "default", lens = { camera = "DJI Osmo Action 6" } }
url, f = send()
expect("the camera, by DJI's telemetry", f.camera == "DJI Osmo Action 6", f.camera)
expect("the log curve", f.curve == "D-Log M", f.curve)
expect("the motion data", f.motion == "gyro", f.motion)
for _, bit in ipairs({ "Thistlegorm", "DJI_2026", "0042", "Volumes", "MP4" }) do
    expect("never the clip's name or path (" .. bit .. ")", not url:find(bit, 1, true), url)
end

-- A GoPro: the camera as Gyroflow names it; not log, no motion data
props["user-data/dji_telemetry"] = { model = "", available = false }
props["user-data/dlogm"] = { log = false, curve = "", recorded = "", hdr = "" }
props["user-data/gyroflow"] = { available = false, lens = { camera = " GoPro HERO12 Black" } }
url, f = send()
expect("the camera, by Gyroflow's lens", f.camera == "GoPro HERO12 Black", f.camera)
expect("not log", f.curve == "not log", f.curve)
expect("no motion data", f.motion == "none", f.motion)

-- Windows, the plugins in someone's own mpv
version = ""
load("windows")
url, f, args = send()
expect("opens with Windows' URL handler", args and args[1] == "rundll32", args and args[1])
expect("Windows", f.os == "Windows", f.os)
expect("the plugins' mpv", f.v == "plugins, mpv v0.42.0", f.v)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
