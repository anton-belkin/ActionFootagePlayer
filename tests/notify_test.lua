-- The cards and the name dialog (mpv/scripts/side-panel/notify.lua) and what the plugins load to reach them
-- (notify_client.lua), over a stand-in for mpv. From the repository's folder: luajit tests/notify_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path

local props = {
    platform = "darwin", ["osd-dimensions"] = { w = 1280, h = 800 }, ["display-hidpi-scale"] = 1, border = true, fullscreen = false,
    ["video-margin-ratio-right"] = 0, ["video-margin-ratio-bottom"] = 0, ["user-data/side_panel/loaded"] = true,
}
local messages, bindings, sent, shown = {}, {}, {}, {}
local now = 100
local drawn = ""
local mp = {}
function mp.get_property_native(name, def) local v = props[name]; if v == nil then return def end; return v end
function mp.get_property(name, def) local v = props[name]; if v == nil or type(v) == "table" then return def end; return tostring(v) end
function mp.get_property_number(name, def) return tonumber(props[name]) or def end
function mp.observe_property() end
function mp.register_script_message(name, fn) messages[name] = fn end
function mp.add_forced_key_binding(key, name, fn) bindings[name] = { key = key, fn = fn } end
function mp.remove_key_binding(name) bindings[name] = nil end
function mp.create_osd_overlay() return { update = function(self) drawn = self.data end, remove = function() drawn = "" end } end
function mp.add_periodic_timer()
    local on = true
    return { kill = function() on = false end, resume = function() on = true end, is_enabled = function() return on end }
end
function mp.get_time() return now end
function mp.commandv(...) sent[#sent + 1] = { ... } end
function mp.command_native() error("no") end
function mp.get_script_name() return "workspace" end
function mp.osd_message(text, seconds) shown[#shown + 1] = { text, seconds } end
mp.msg = { info = function() end, warn = function() end }
-- JSON enough for these tests: what's sent is kept as the table it was
local last_json
local utils = {
    format_json = function(t) last_json = t; return "<json>" end,
    parse_json = function(s) return s == "<json>" and last_json or nil end,
}
package.preload.mp = function() return mp end
package.preload["mp.utils"] = function() return utils end

local failures = 0
local function check(name, ok)
    if not ok then failures = failures + 1; print("FAIL " .. name) end
end
local function has(text) return drawn:find(text, 1, true) ~= nil end

local notify = require 'notify'
local client = require 'notify_client'

-- A working card stays, with the time so far; done replaces it and goes after a few seconds
notify.notify({ id = "m1", kind = "working", title = "Saving moment “Wake 1”", detail = "Making its picture" })
check("working card drawn", has("Saving moment “Wake 1”") and has("Making its picture"))
now = now + 75
notify.render()
check("time so far", has("1:15"))
check("still there later", #notify.state().cards == 1)
notify.notify({ id = "m1", kind = "done", title = "Saved moment “Wake 1”" })
check("one card for the id", #notify.state().cards == 1 and notify.state().cards[1].kind == "done")
now = now + 3.5
notify.render()
check("done goes", #notify.state().cards == 0 and drawn == "")

-- A share: the percent; a working card isn't wiped by a note
notify.notify({ id = "e", kind = "working", title = "Exporting A.MP4", share = 0.42 })
notify.notify({ id = "note", kind = "info", title = "Horizon: level", seconds = 1.5 })
check("percent", has("42 %"))
check("note below it", #notify.state().cards == 2 and has("Horizon: level"))
now = now + 2
notify.render()
check("note gone, export stays", #notify.state().cards == 1 and has("Exporting A.MP4"))

-- An error stays until Esc
notify.notify({ id = "x", kind = "error", title = "Couldn't save “Wake 1”", detail = "the NAS didn't answer" })
now = now + 600
notify.render()
check("error stays", has("Couldn't save") and has("Click or Esc closes it"))
check("Esc bound", bindings["notify-esc"] ~= nil)
bindings["notify-esc"].fn()
check("Esc closed it", not has("Couldn't save") and bindings["notify-esc"] == nil)
notify.hide("e")
check("all gone", #notify.state().cards == 0)

-- Quiet notes: none while the panel is open
notify.set_panel_open(true)
notify.notify({ id = "q", kind = "info", title = "Telemetry: on", quiet = true })
check("quiet not drawn with the panel open", not has("Telemetry: on"))
notify.set_panel_open(false)
check("drawn with it closed", has("Telemetry: on"))
notify.hide("q")

-- The cards sit over the video, beside the panel: in the middle of what's left of the window
props["video-margin-ratio-right"] = 340 / 1280
local a = notify.area(1280, 800)
check("area beside the panel", math.abs(a.x1 - 940) < 0.01 and a.y0 == 28)

-- The client: osd_message goes to the cards; errors by their words
check("error words", client.is_error("Couldn't write the marks: x") and client.is_error("Export failed: y") and not client.is_error("Renamed to A"))
sent = {}
mp.osd_message("Renamed to A", 2)
local s = sent[1]
check("routed to side_panel", s and s[1] == "script-message-to" and s[2] == "side_panel" and s[3] == "notify")
local t = last_json
check("a quiet note", t.kind == "info" and t.title == "Renamed to A" and t.quiet == true)
mp.osd_message("Mark the part first\nl at its start", 4)
check("longer one not quiet, its lines", last_json.quiet == false and last_json.title == "Mark the part first" and last_json.detail == "l at its start")
mp.osd_message("Couldn't save the transform: disk full", 4)
check("an error card", last_json.kind == "error")
sent = {}
mp.osd_message("", 0)
check("an empty message: nothing", #sent == 0)
-- Without the side panel: mpv's own text
props["user-data/side_panel/loaded"] = false
shown = {}
mp.osd_message("Renamed to A", 2)
mp.notify({ kind = "working", title = "Exporting", share = 0.5 })
check("plain text without the panel", shown[1] and shown[1][1] == "Renamed to A" and shown[2] and shown[2][1] == "Exporting 50 %")
check("no dialog without the panel", mp.ask({ title = "x" }, function() end) == false)
props["user-data/side_panel/loaded"] = true

-- The dialog: asked through the client, typed in, answered
local answered
check("asked", mp.ask({ title = "Save moment 0:01–0:04", suggestion = "Wake 1", names = { "Boat", "Fall" }, ok = "Save" },
                     function(text, cancelled) answered = { text, cancelled } end))
messages["ask"](utils.format_json(last_json))
check("dialog open", notify.dialog_open() and has("Save moment 0:01–0:04") and has("Wake 1") and has("Boat"))
check("keys taken", bindings["notify-dlg-ENTER"] ~= nil and bindings["notify-dlg-text"] ~= nil)
local function key(name) bindings["notify-dlg-" .. name].fn() end
local function typed(text) bindings["notify-dlg-text"].fn({ event = "down", key_text = text }) end
typed("J"); typed("ü"); typed("m"); typed("p")
check("typed", notify.state().dialog.value == "Jümp")
key("BS"); key("LEFT"); key("BS")
check("back over a two-byte letter", notify.state().dialog.value == "Jm" and notify.state().dialog.cursor == 2)
key("UP")
check("↑: the newest earlier name", notify.state().dialog.value == "Fall")
key("UP")
check("↑ again", notify.state().dialog.value == "Boat")
key("DOWN"); key("DOWN")
check("↓ back to what was typed", notify.state().dialog.value == "Jm")
key("ENTER")
-- The answer goes back to the asker as a script message, which the client hands to the callback
local reply = sent[#sent]
check("answer sent", reply[1] == "script-message-to" and reply[2] == "workspace" and reply[3] == "notify-answer")
messages["notify-answer"](reply[4])
check("answered", answered and answered[1] == "Jm" and answered[2] == false)
check("dialog closed, keys given back", not notify.dialog_open() and bindings["notify-dlg-ENTER"] == nil and bindings["notify-dlg-text"] == nil)

-- Nothing typed: "" (the asker takes its suggestion); Esc: cancelled
mp.ask({ title = "Name", suggestion = "Wake 2" }, function(text, cancelled) answered = { text, cancelled } end)
messages["ask"](utils.format_json(last_json))
key("ENTER")
messages["notify-answer"](sent[#sent][4])
check("empty answer", answered[1] == "" and answered[2] == false)
mp.ask({ title = "Name", suggestion = "Wake 2" }, function(text, cancelled) answered = { text, cancelled } end)
messages["ask"](utils.format_json(last_json))
typed("x")
key("ESC")
messages["notify-answer"](sent[#sent][4])
check("Esc cancels", answered[2] == true)
-- Tab puts the suggestion in to change it
mp.ask({ title = "Name", suggestion = "Wake 2" }, function(text, cancelled) answered = { text, cancelled } end)
messages["ask"](utils.format_json(last_json))
key("TAB"); typed("b")
check("Tab: the suggestion to edit", notify.state().dialog.value == "Wake 2b")
key("ESC")

if failures > 0 then
    print(failures .. " failed")
    os.exit(1)
end
print("notify: all passed")
