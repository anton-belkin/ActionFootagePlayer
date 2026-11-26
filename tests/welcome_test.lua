-- The welcome card (mpv/scripts/side-panel/welcome.lua): shown when the player sits idle with no video, gone as a file
-- starts; its steps, the sample's, and what its buttons do. Over a stand-in for mpv. From the repository's folder:
-- luajit tests/welcome_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path

local tmp = os.tmpname()
os.remove(tmp)
os.execute("mkdir -p '" .. tmp .. "/app/plugins/side-panel' '" .. tmp .. "/home'")

local props = {
    platform = "darwin", ["osd-dimensions"] = { w = 1280, h = 800 }, ["display-hidpi-scale"] = 1, border = true, fullscreen = false,
    ["video-margin-ratio-right"] = 0, ["video-margin-ratio-bottom"] = 0, ["idle-active"] = false,
}
local observers, events, messages, bindings, sent, run = {}, {}, {}, {}, {}, {}
local timeouts = {}
local drawn = ""
local mp = {}
function mp.get_property_native(name, def) local v = props[name]; if v == nil then return def end; return v end
function mp.get_property(name, def) local v = props[name]; if v == nil or type(v) == "table" then return def end; return tostring(v) end
function mp.get_property_number(name, def) return tonumber(props[name]) or def end
function mp.observe_property(name, _, fn) observers[name] = observers[name] or {}; table.insert(observers[name], fn) end
function mp.register_event(name, fn) events[name] = fn end
function mp.register_script_message(name, fn) messages[name] = fn end
function mp.add_forced_key_binding(key, name, fn) bindings[name] = { key = key, fn = fn } end
function mp.remove_key_binding(name) bindings[name] = nil end
function mp.create_osd_overlay() return { update = function(self) drawn = self.data end, remove = function() drawn = "" end } end
function mp.add_timeout(_, fn)
    local t = { fn = fn, on = true }
    t.kill = function() t.on = false end
    timeouts[#timeouts + 1] = t
    return t
end
function mp.get_script_directory() return tmp .. "/app/plugins/side-panel" end
function mp.commandv(...) sent[#sent + 1] = { ... } end
function mp.command_native(c) run[#run + 1] = c.args; if c.args[1] == "mkdir" then os.execute("mkdir -p '" .. c.args[3] .. "'") end end
function mp.osd_message() end
mp.msg = { info = function() end, warn = function() end, error = function() end }
local utils = {
    join_path = function(a, b) return (a:gsub("/$", "")) .. "/" .. b end,
}
package.preload.mp = function() return mp end
package.preload["mp.utils"] = function() return utils end

local failures = 0
local function check(name, ok)
    if not ok then failures = failures + 1; print("FAIL " .. name) end
end
local function has(text) return drawn:find(text, 1, true) ~= nil end
local function set(name, v)
    props[name] = v
    for _, fn in ipairs(observers[name] or {}) do fn(name, v) end
end
local function fire()
    local due = timeouts
    timeouts = {}
    for _, t in ipairs(due) do if t.on then t.fn() end end
end

local real_getenv = os.getenv
os.getenv = function(k) if k == "HOME" then return tmp .. "/home" end return real_getenv(k) end

local welcome = require 'welcome'

-- Idle: shown after a moment, not at once (a video opened from Finder comes a moment after the app starts)
check("not shown at start", not welcome.shown() and drawn == "")
set("idle-active", true)
check("not shown at once", not welcome.shown())
fire()
check("shown after a moment", welcome.shown() and has("Welcome to Action Footage Player"))
check("the steps", has("Open a clip from your camera") and has("Before your next dive") and has("Tell us how it went"))
check("Finder named on a Mac", has("Finder"))
check("no sample step without a sample", not has("sample dive") and #welcome.steps() == 3)

-- A file before the moment's up: never shown
welcome.hide()
set("idle-active", false)
set("idle-active", true)
events["start-file"]()
fire()
check("a file starting first: not shown", not welcome.shown())

-- A file starting while it shows: gone
welcome.show()
check("shown", welcome.shown())
events["start-file"]()
check("gone as a file starts", not welcome.shown() and drawn == "")

-- With the sample beside the plugins: its step, and the button copies it to Movies and shows it in Finder
os.execute("mkdir -p '" .. tmp .. "/app/sample'")
local f = io.open(tmp .. "/app/sample/" .. welcome.SAMPLE, "wb")
f:write(string.rep("x", 5 * 1024 * 1024 + 7))
f:close()
welcome.show()
check("sample step", has("No clip at hand? Try the sample dive") and has("Show the sample") and #welcome.steps() == 4)
local function sample_detail()
    for _, st in ipairs(welcome.steps()) do if st.button and st.button.id == "sample" then return st.detail end end
end
check("no compare line without its key", not sample_detail():find("as recorded.", 1, true))
welcome.COMPARE_KEY = "C"
check("compare line with its key", sample_detail():find("Hold C while it plays to see it as recorded.", 1, true) ~= nil)
check("sample shown", welcome.show_sample())
local copied = tmp .. "/home/Movies/Action Footage Player/" .. welcome.SAMPLE
local g = io.open(copied, "rb")
check("copied whole", g and g:seek("end") == 5 * 1024 * 1024 + 7)
if g then g:close() end
local last = run[#run]
check("shown in Finder", last[1] == "open" and last[2] == "-R" and last[3] == copied)

-- The buttons: feedback goes to whoever handles afp-feedback; How opens the divers' page at the camera settings
for _, st in ipairs(welcome.steps()) do
    if st.button and st.button.id == "feedback" then st.button.cmd() end
    if st.button and st.button.id == "how" then st.button.cmd() end
end
check("feedback message", sent[#sent] and sent[#sent][1] == "script-message" and sent[#sent][2] == "afp-feedback")
local opened = false
for _, a in ipairs(run) do if a[1] == "open" and a[2] == "https://actionfootageplayer.ch/divers.html#setup" then opened = true end end
check("How opens the divers' page", opened)

-- Windows: Explorer named, the sample to Videos
props.platform = "windows"
package.loaded.welcome = nil
check("Windows videos folder", (function()
    os.getenv = function(k) if k == "USERPROFILE" then return "C:\\Users\\a" end return real_getenv(k) end
    local w = require 'welcome'
    return w.videos_dir() == "C:\\Users\\a\\Videos\\Action Footage Player"
end)())

os.execute("rm -rf '" .. tmp .. "'")
if failures > 0 then print(failures .. " failed"); os.exit(1) end
print("welcome_test: ok")
