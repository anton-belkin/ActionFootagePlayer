-- The side panel itself (mpv/scripts/side-panel/main.lua) over a stand-in for mpv: the properties the plugins publish
-- are set, the panel draws, and the mouse clicks where it drew. For now: the Output tab's one Export button and its
-- list (docs/360-reframe.md, "One Export button in the Output tab").
-- From the repository's folder: luajit tests/sidepanel_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path

-- The stand-in: properties, observers, bindings, script messages; what's run is logged
local props = {
    platform = "darwin", ["osd-dimensions"] = { w = 1280, h = 1800 }, ["display-hidpi-scale"] = 1, border = true,
    fullscreen = false, path = "/footage/DJI_0001.MP4", filename = "DJI_0001.MP4", pause = true, duration = 60,
    ["time-pos"] = 3, ["playlist-count"] = 1, ["playlist-pos"] = 0, ["input-bindings"] = {}, osc = false,
}
local observers, events, messages, bindings, ran = {}, {}, {}, {}, {}
local now = 0
local mp = {}
function mp.get_property_native(name, def)
    local v = props[name]
    if v == nil then return def end
    return v
end
function mp.get_property(name, def)
    local v = props[name]
    if v == nil then return def end
    if type(v) == "table" then return def end
    return tostring(v)
end
function mp.get_property_number(name, def) return tonumber(props[name]) or def end
function mp.set_property_native(name, v)
    props[name] = v
    for _, o in ipairs(observers) do
        if o.name == name or name:sub(1, #o.name + 1) == o.name .. "/" then o.fn(o.name, props[o.name]) end
    end
end
mp.set_property_number = mp.set_property_native
function mp.observe_property(name, _, fn) table.insert(observers, { name = name, fn = fn }) end
function mp.register_event(name, fn) events[name] = fn end
function mp.register_script_message(name, fn) messages[name] = fn end
function mp.add_key_binding(key, name, fn) bindings[name] = { key = key, fn = fn } end
function mp.add_forced_key_binding(key, name, fn) bindings[name] = { key = key, fn = fn, forced = true } end
function mp.remove_key_binding(name) bindings[name] = nil end
function mp.create_osd_overlay() return { update = function() end, remove = function() end } end
function mp.add_periodic_timer()
    local on = true
    return { kill = function() on = false end, resume = function() on = true end, is_enabled = function() return on end }
end
function mp.add_timeout() return { kill = function() end } end
function mp.get_time() return now end
function mp.command(c) table.insert(ran, c) end
function mp.commandv(...) table.insert(ran, table.concat({ ... }, " ")) end
function mp.command_native() return nil end
function mp.command_native_async() end
function mp.get_script_name() return "side_panel" end
mp.msg = { info = function() end, warn = function() end, error = function() end, debug = function() end, verbose = function() end }
package.preload.mp = function() return mp end
package.preload["mp.options"] = function() return { read_options = function(o) o.shown, o.open, o.tab = true, true, "output" end } end

local ui = require 'ui'
local new = ui.new

dofile("mpv/scripts/side-panel/main.lua")

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- The panel's drawing with its places (the bar draws into its own)
local panel
local function redraw()
    events["file-loaded"]()
end
local function find(id)
    for _, h in ipairs(panel and panel.hits or {}) do if h.id == id then return h end end
end
-- Drawn: the Ui whose places include the rail's (the panel, not the bar)
ui.new = function(s)
    local u = new(s)
    local hit = u.hit
    u.hit = function(self, x, y, w, h, id, cmd)
        if type(id) == "string" and id:find("^rail:") then panel = self end
        return hit(self, x, y, w, h, id, cmd)
    end
    return u
end
local function move(x, y)
    mp.set_property_native("mouse-pos", { x = x, y = y, hover = true })
end
local function click_at(x, y)
    move(x, y)
    local b = bindings.click
    if not b then return false end
    b.fn({ event = "down" })
    b.fn({ event = "up" })
    return true
end
local function click(id)
    local h = find(id)
    if not h then return false end
    return click_at((h.x0 + h.x1) / 2, (h.y0 + h.y1) / 2)
end
local function last_ran() return ran[#ran] end

-- A log clip with an SDR look, not a 360 one
mp.set_property_native("user-data/dlogm", { log = true, curve = "dlogm", recorded = "dlogm", hdr = "", look = "hdr",
                                            looks = { "hdr", "maker" }, sdr_look = "maker", maker = "DJI", screen = "sdr" })
mp.set_property_native("user-data/gyroflow", { available = true, exporting = false, last_export = "/footage/stabilized/x.mp4" })
mp.set_property_native("user-data/view360", { available = false })
redraw()
expect("the Export button", find("export") ~= nil, true)
expect("its ▾ part", find("export:more") ~= nil, true)
expect("no In SDR button any more", find("export-sdr"), nil)
expect("Show the last export still there", find("last") ~= nil, true)
click("export")
expect("the main part exports as it plays", last_ran(), "script-message-to gyroflow export hdr")

-- ▾ opens the list over the panel; Esc is the list's
click("export:more")
expect("the list open: its rows", find("export-kind:hdr") ~= nil and find("export-kind:sdr") ~= nil, true)
expect("no 360 rows for a flat clip", find("export-kind:360"), nil)
expect("Esc bound while it's open", bindings["popover-esc"] ~= nil and bindings["popover-esc"].key, "ESC")
-- A pick: the button makes that kind, the list closes
click("export-kind:sdr")
expect("a pick closes the list", find("export-kind:sdr"), nil)
expect("Esc let go", bindings["popover-esc"], nil)
local n = #ran
click("export")
expect("the main part makes the kind picked", last_ran(), "script-message-to gyroflow export sdr")
expect("one command for one click", #ran, n + 1)

-- A click off the list closes it and does nothing else
click("export:more")
local mark = find("mark-a")
n = #ran
click_at((mark.x0 + mark.x1) / 2, (mark.y0 + mark.y1) / 2)
expect("a click outside closes it", find("export-kind:hdr"), nil)
expect("…and runs nothing", #ran, n)
-- The ▾ again while open: closes it
click("export:more")
click("export:more")
expect("▾ again closes it", find("export-kind:hdr"), nil)
-- Esc
click("export:more")
bindings["popover-esc"].fn()
expect("Esc closes it", find("export-kind:hdr"), nil)
-- A click on the video while it's open: the mouse is still the panel's, and the click only closes it
click("export:more")
move(100, 300)
expect("over the video, clicks are the panel's while it's open", bindings.click ~= nil, true)
n = #ran
click_at(100, 300)
expect("a click on the video closes it", find("export-kind:hdr"), nil)
expect("…and runs nothing", #ran, n)
move(100, 310)
expect("closed: the video's clicks are mpv's again", bindings.click, nil)
-- The wheel closes it
click("export:more")
local h = find("export")
move(h.x0 + 5, h.y0 + 5)
bindings["wheel-down"].fn({ event = "down" })
expect("scrolling closes it", find("export-kind:hdr"), nil)
-- Another tab closes it
messages.tab("output")
click("export:more")
messages.tab("transform")
expect("another tab closes it", bindings["popover-esc"], nil)
messages.tab("output")
expect("back on Output: closed", find("export-kind:hdr"), nil)
expect("…the kind picked kept", find("export") ~= nil, true)
click("export")
expect("still SDR", last_ran(), "script-message-to gyroflow export sdr")

-- A 360 clip: its own choice, 360° and flat listed, flat greyed until there's a recording
mp.set_property_native("user-data/view360", { available = true, stamped = true, active = true, heading = "camera",
                                              recording = { can = true, parts = 0, spans = {} } })
messages.tab("output")
click("export")
expect("a 360 clip starts as it plays (the choice is per kind of clip)", last_ran(), "script-message-to gyroflow export hdr")
click("export:more")
expect("360° listed", find("export-kind:360") ~= nil, true)
expect("flat greyed: no place", find("export-kind:flat"), nil)
n = #ran
local list = find("export-kind")
click_at(list.x0 + 20, list.y1 - 10)
expect("a click on the greyed row: the list stays", find("export-kind:360") ~= nil, true)
expect("…and nothing runs", #ran, n)
click("export-kind:360")
click("export")
expect("360°", last_ran(), "script-message-to gyroflow export 360")
-- Recorded: flat can be picked
mp.set_property_native("user-data/view360", { available = true, stamped = true, active = true, heading = "camera",
                                              recording = { can = true, parts = 2, spans = { { 1, 4 }, { 6, 9 } } } })
messages.tab("output")
click("export:more")
click("export-kind:flat")
click("export")
expect("flat", last_ran(), "script-message-to gyroflow export flat")
-- Recording now: the main part waits
mp.set_property_native("user-data/view360", { available = true, stamped = true, active = true, heading = "camera",
                                              recording = { can = true, parts = 2, taking = true, spans = { { 1, 4 } } } })
messages.tab("output")
expect("recording: the main part can't be clicked", find("export"), nil)
expect("recording: the list still opens", find("export:more") ~= nil, true)
-- Back to a flat clip: its own choice, SDR
mp.set_property_native("user-data/view360", { available = false })
messages.tab("output")
click("export")
expect("a flat clip again: its own choice", last_ran(), "script-message-to gyroflow export sdr")

-- Exporting: Cancel in its place, and the list closes
click("export:more")
mp.set_property_native("user-data/gyroflow", { available = true, exporting = true, last_export = "" })
expect("exporting closes the list", bindings["popover-esc"], nil)
click("export")
expect("Cancel export", last_ran(), "script-message-to gyroflow cancel-export")
-- The panel hidden (Tab) closes it
mp.set_property_native("user-data/gyroflow", { available = true, exporting = false, last_export = "" })
click("export:more")
bindings.toggle.fn()
expect("hiding the panel closes it", bindings["popover-esc"], nil)
bindings.toggle.fn()
-- The popover message (for IPC tests)
messages.popover("export")
expect("popover export opens it", find("export-kind:hdr") ~= nil, true)
messages.popover("")
expect("popover with nothing closes it", find("export-kind:hdr"), nil)

-- The scroll bar: a short window, the bar held and dragged down scrolls the panel, let go it stays
mp.set_property_native("osd-dimensions", { w = 1280, h = 420 })
messages.tab("output")
local bar_hit = find("scrollbar")
expect("a short window: a scroll bar", bar_hit ~= nil, true)
if bar_hit then
    expect("the Export button below what shows", find("export"), nil)
    move(bar_hit.x0 + 3, bar_hit.y0 + 5)
    bindings.click.fn({ event = "down" })
    mp.set_property_native("mouse-pos", { x = bar_hit.x0 + 3, y = bar_hit.y0 + 120, hover = true })
    local moved = find("export")
    expect("dragged down: the panel scrolls to it", moved ~= nil, true)
    bindings.click.fn({ event = "up" })
    local after = find("scrollbar")
    -- Dragged back up past its top: at the top again
    move(after.x0 + 3, after.y1 - 5)
    bindings.click.fn({ event = "down" })
    mp.set_property_native("mouse-pos", { x = after.x0 + 3, y = -500, hover = true })
    bindings.click.fn({ event = "up" })
    expect("dragged up past the top: back at the top", find("export"), nil)
    expect("…the top part showing", find("collapse") ~= nil and find("look:maker") ~= nil, true)
end
mp.set_property_native("osd-dimensions", { w = 1280, h = 1800 })

-- Subtitles: a switch when the clip has some, which runs the dji-telemetry plugin's v; the camera's text (a DJI
-- drone's) is said to be the camera's data, also in Telemetry in place of "No telemetry"
messages.tab("output")
expect("no subtitles: no switch", find("subtitles"), nil)
mp.set_property_native("user-data/dji_telemetry", { available = false, model = "", visible = true,
                                                    subtitles = { count = 1, camera = true, shown = false, camera_shown = false } })
expect("the camera's text: a switch in Output", find("subtitles") ~= nil, true)
click("subtitles")
expect("the switch runs the plugin's key", last_ran(), "script-binding dji_telemetry/subtitles")
messages.tab("telemetry")
expect("…and in Telemetry, with no telemetry", find("subtitles") ~= nil, true)
mp.set_property_native("user-data/dji_telemetry", { available = false, model = "", visible = true,
                                                    subtitles = { count = 0, camera = false, shown = false } })
expect("no subtitles: none in Telemetry", find("subtitles"), nil)
messages.tab("output")

-- Tags (the workspace plugin's tags.lua): the stars, the clip's tags (a click takes one off), the recent ones it hasn't
-- (a click puts one on), Tag… (K); a moment's are the moment's
mp.set_property_native("user-data/workspace", { path = "/footage/DJI_0001.MP4", clip = "/footage/DJI_0001.MP4", project = "/footage",
    relative = "DJI_0001.MP4", moments = {},
    tags = { kind = "clip", list = { "Turtle", "for family" }, rating = 2, recent = { "turtle", "Reef", "Dolphin" }, finder = "own" } })
messages.tab("project")
expect("tags: its chips", find("tag:Turtle") ~= nil and find("tag:for family") ~= nil, true)
expect("tags: recent ones it hasn't", find("recent:Reef") ~= nil and find("recent:Dolphin") ~= nil, true)
expect("tags: one it has isn't offered again", find("recent:turtle"), nil)
click("tag:for family")
expect("tags: a chip clicked takes it off", last_ran(), "script-message-to workspace tag-toggle for family")
click("recent:Reef")
expect("tags: a recent one clicked puts it on", last_ran(), "script-message-to workspace tag-toggle Reef")
click("star:4")
expect("tags: a star clicked rates", last_ran(), "script-message-to workspace tag-rate 4")
click("tag-ask")
expect("tags: Tag… asks", last_ran(), "script-message-to workspace tag-ask")
mp.set_property_native("user-data/workspace", { path = "/footage/_Moments/Turtle.afplink.mp4", clip = "/footage/DJI_0001.MP4", project = "/footage",
    moment = { file = "/footage/_Moments/Turtle.afplink.mp4", name = "Turtle", ["in"] = 1, out = 4 }, offset = 1, moments = {},
    tags = { kind = "moment", list = {}, recent = {}, finder = "off" } })
expect("tags: a moment's, none yet", find("tag:Turtle") == nil and find("tag-ask") ~= nil and find("star:1") ~= nil, true)
mp.set_property_native("user-data/workspace", { path = "https://x/y.mp4", clip = "https://x/y.mp4" })
expect("tags: none for a stream", find("tag-ask"), nil)
messages.tab("output")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
