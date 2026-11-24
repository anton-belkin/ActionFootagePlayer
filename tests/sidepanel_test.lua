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
package.preload["mp.utils"] = function() return { parse_json = function() return nil end, format_json = function() return "{}" end } end
package.preload["mp.options"] = function() return { read_options = function(o) o.shown, o.open, o.tab = true, true, "output" end } end

local ui = require 'ui'
local tl = require 'timeline'
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
    expect("…the top part showing", find("collapse") ~= nil and find("look") ~= nil, true)
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

-- Drop-downs (#196): a single choice of more than three is a field with the value chosen; a click opens its list over
-- the panel, a pick runs it and closes the list; Esc, a click outside or the field again close it; Up, Down and Enter
-- work in it. Two or three choices are buttons in a row again, the chosen one lit (as before #196)
local stab = { available = true, kind = "default", motion = true, view = "normal", level = "medium", horizon_lock = "yes",
               lens_mode = "corrected", projection = "linear", filter = "stabilized", edges = "sphere", aspect = "original",
               orientation = "as-is", source_aspect = "4:3", frame_aspect = "4:3", exporting = false, last_export = "" }
mp.set_property_native("user-data/gyroflow", stab)
messages.tab("transform")
-- Three choices: buttons, each its own place, no field and no list
expect("level: buttons, no field", find("level") == nil and find("level:weak") ~= nil and find("level:strong") ~= nil, true)
expect("…the chosen one has no place (it's lit)", find("level:medium"), nil)
n = #ran
click("level:strong")
expect("a button runs its command", last_ran(), "script-message-to gyroflow level strong")
expect("…one", #ran, n + 1)
expect("…and opens no list", bindings["popover-esc"], nil)
expect("two choices: buttons (horizon)", find("horizon:no") ~= nil and find("horizon") == nil, true)
expect("three: buttons (the view)", find("view:wide") ~= nil and find("view:off") ~= nil, true)
-- Five: a drop-down
expect("aspect: one field, no buttons", find("ratio") ~= nil and find("ratio:2") == nil and find("ratio:16:9") == nil, true)
click("ratio")
expect("open: its rows", find("ratio:16:9") ~= nil and find("ratio:1:1") ~= nil, true)
expect("Esc bound while it's open", bindings["popover-esc"] ~= nil, true)
expect("Up, Down and Enter too", bindings["popover-up"] ~= nil and bindings["popover-down"] ~= nil and bindings["popover-enter"] ~= nil, true)
n = #ran
click("ratio:1:1")
expect("a pick runs its command", last_ran(), "script-message-to gyroflow aspect 1:1")
expect("…one", #ran, n + 1)
expect("…and closes the list", find("ratio:1:1"), nil)
expect("…and lets the keys go", bindings["popover-down"] == nil and bindings["popover-esc"] == nil, true)
-- A click outside: closed, nothing run
click("ratio")
n = #ran
click_at(100, 300)
expect("a click outside closes it", find("ratio:16:9"), nil)
expect("…and runs nothing", #ran, n)
-- The field again
click("ratio")
click("ratio")
expect("the field again closes it", find("ratio:16:9"), nil)
-- Esc
click("ratio")
bindings["popover-esc"].fn()
expect("Esc closes it", find("ratio:16:9"), nil)
-- Another drop-down's field while one is open: the click only closes the open one
click("ratio")
n = #ran
click("lens")
expect("one list at a time", find("lens:stereographic") == nil and find("ratio:16:9") == nil, true)
expect("…nothing run", #ran, n)
-- The keys: from the one chosen (its own), down and Enter
click("ratio")
bindings["popover-down"].fn()
bindings["popover-enter"].fn()
expect("Down, Enter: the next one", last_ran(), "script-message-to gyroflow aspect 16:9")
expect("Enter closes it", find("ratio:16:9") == nil and bindings["popover-enter"] == nil, true)
click("ratio")
bindings["popover-up"].fn()
bindings["popover-up"].fn()
bindings["popover-enter"].fn()
expect("Up past the top stays on the first", last_ran(), "script-message-to gyroflow aspect original")
-- Enter on the one chosen picks it again (no harm), and closes
click("ratio")
bindings["popover-kp-enter"].fn()
expect("Enter alone: the one chosen", last_ran(), "script-message-to gyroflow aspect original")
-- Greyed rows are skipped by the keys and can't be clicked
mp.set_property_native("user-data/view360", { available = true, active = true, stamped = true, heading = "camera", projection = "auto",
                                              modes = { travel = { available = false, why = "no GPS" }, north = { available = false } },
                                              recording = { can = true, parts = 0, spans = {} } })
messages.tab("view360")
click("view360-heading")
expect("a greyed row has no place", find("view360-heading:travel"), nil)
expect("…the others have", find("view360-heading:world") ~= nil, true)
bindings["popover-up"].fn()
bindings["popover-enter"].fn()
expect("Up from the first one it can be: stays", last_ran(), "script-message-to view360 view360-heading camera")
click("view360-heading")
bindings["popover-down"].fn()
bindings["popover-enter"].fn()
expect("Down skips north (greyed)", last_ran(), "script-message-to view360 view360-heading world")
-- A switch stays a switch
mp.set_property_native("user-data/view360", { available = false })
messages.tab("transform")
expect("on/off stays a switch", find("still") ~= nil and find("still:1") == nil, true)

-- The output's lens for a clip with its own Gyroflow project (#196): a drop-down there too, the project's own first,
-- the projections and as recorded; a pick goes to the gyroflow plugin as for any clip
local proj = {}
for k, v in pairs(stab) do proj[k] = v end
proj.kind, proj.project, proj.horizon_lock, proj.projection = "project", "/footage/DJI_0001.gyroflow", "project", "project"
proj.project_info = { horizon = 0, projection = "Equidistant" }
mp.set_property_native("user-data/gyroflow", proj)
messages.tab("transform")
expect("project: the lens can be picked", find("lens") ~= nil, true)
click("lens")
expect("project: its own listed", find("lens:project") ~= nil, true)
expect("…a projection", find("lens:stereographic") ~= nil and find("lens:linear") ~= nil, true)
expect("…and as recorded", find("lens:as-recorded") ~= nil, true)
local ids = {}
for _, h in ipairs(panel.hits) do if h.popover and h.id:find("^lens:") then ids[#ids + 1] = h.id end end
expect("project's own first", ids[1], "lens:project")
click("lens:stereographic")
expect("project: a projection picked", last_ran(), "script-message-to gyroflow lens stereographic")
click("lens")
click("lens:as-recorded")
expect("project: as recorded", last_ran(), "script-message-to gyroflow lens as-recorded")
-- Without a project: no "project's own" row
mp.set_property_native("user-data/gyroflow", stab)
click("lens")
expect("no project: no project's own", find("lens:project"), nil)
bindings["popover-esc"].fn()
-- Culling and tags (the workspace plugin's tags.lua, docs/tags-and-culling.md): Accept / Reject, the stars, the
-- original's tags (a click takes one off), the recent ones it hasn't (a click puts one on), Tag… (K); a moment's are
-- the moment's; the clip preview button isn't culling any more
mp.set_property_native("user-data/workspace", { path = "/footage/DJI_0001.MP4", clip = "/footage/DJI_0001.MP4", project = "/footage",
    relative = "DJI_0001.MP4", moments = {}, clip_preview = "made",
    tags = { kind = "original", pick = "accept", list = { "Turtle", "for family" }, rating = 2, recent = { "turtle", "Reef", "Dolphin" }, stores = { "finder" } } })
messages.tab("project")
expect("pick: both buttons", find("pick:accept") ~= nil and find("pick:reject") ~= nil, true)
click("pick:reject")
expect("pick: Reject clicked", last_ran(), "script-message-to workspace tag-pick reject")
click("pick:accept")
expect("pick: Accept clicked (on: the plugin takes it off)", last_ran(), "script-message-to workspace tag-pick accept")
click("not-interesting")
expect("the album button: takes the clip out of its album", last_ran(), "script-binding workspace/not-interesting")
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
    tags = { kind = "moment", list = {}, recent = {}, stores = {} } })
expect("tags: a moment's, none yet", find("tag:Turtle") == nil and find("tag-ask") ~= nil and find("star:1") ~= nil, true)
mp.set_property_native("user-data/workspace", { path = "https://x/y.mp4", clip = "https://x/y.mp4" })
expect("tags: none for a stream", find("tag-ask"), nil)
messages.tab("output")

-- The photo button and the export's settings (docs/export-settings.md): the Output tab's Export has its size, encoding
-- and quality, and a button saving a photo of the frame as Alt+s does; Save frame has the photo's kind, size and quality
-- and the same button, framing or not. The choices are the gyroflow plugin's (exportsettings.lua), published with them
local es = dofile("mpv/scripts/gyroflow/exportsettings.lua")
local function gyro(extra)
    local g = { available = true, exporting = false, last_export = "", live_photo = "no", export_settings = {
        video = { size = "original", encode = "fast", quality = "displayed" }, photo = { size = "original", quality = "displayed" }, choices = es.CHOICES } }
    for k, v in pairs(extra or {}) do g[k] = v end
    return g
end
mp.set_property_native("user-data/view360", { available = false })
mp.set_property_native("user-data/workspace", nil)
mp.set_property_native("user-data/gyroflow", gyro())
messages.tab("output")
expect("Output: the photo button", find("photo") ~= nil, true)
click("photo")
expect("…saves a photo as Alt+s does", last_ran(), "script-message-to gyroflow photo")
-- Size: four, a drop-down; encoding (two) and quality (three): buttons
expect("size: a drop-down", find("video-size") ~= nil and find("video-size:1080") == nil, true)
click("video-size")
click("video-size:1080")
expect("size picked", last_ran(), "script-message-to gyroflow export-setting video size 1080")
expect("encoding: buttons", find("video-encode:quality") ~= nil and find("video-encode") == nil, true)
expect("…the chosen one lit (no place)", find("video-encode:fast"), nil)
click("video-encode:quality")
expect("encoding picked", last_ran(), "script-message-to gyroflow export-setting video encode quality")
expect("quality: buttons", find("video-quality:quickest") ~= nil and find("video-quality:highest") ~= nil, true)
click("video-quality:highest")
expect("quality picked", last_ran(), "script-message-to gyroflow export-setting video quality highest")
-- The plugin keeps it and says so: drawn as chosen
local g = gyro()
g.export_settings.video.quality = "highest"
mp.set_property_native("user-data/gyroflow", g)
expect("highest shown chosen", find("video-quality:highest") == nil and find("video-quality:displayed") ~= nil, true)
-- A 360 export: its own settings, these greyed
mp.set_property_native("user-data/view360", { available = true, stamped = true, active = true, heading = "camera",
                                              recording = { can = true, parts = 0, spans = {} } })
messages.tab("output")
click("export:more")
click("export-kind:360")
expect("360: no size", find("video-size"), nil)
expect("360: no quality buttons", find("video-quality:quickest"), nil)
mp.set_property_native("user-data/view360", { available = false })
-- An older gyroflow plugin (no settings published): no rows, the export as it was
mp.set_property_native("user-data/gyroflow", { available = true, exporting = false, last_export = "" })
messages.tab("output")
expect("no settings published: no rows", find("video-size") == nil and find("video-encode:quality") == nil, true)
expect("…the export button still", find("export") ~= nil, true)
-- Save frame, not framing: the photo block anyway
mp.set_property_native("user-data/gyroflow", gyro({ crop = { active = false } }))
messages.tab("crop")
expect("Save frame: the photo button without framing", find("crop-photo") ~= nil, true)
click("crop-photo")
expect("…saves a photo as Alt+s does", last_ran(), "script-message-to gyroflow photo")
expect("Save frame: the kind as buttons (three)", find("crop-live:live") ~= nil and find("crop-live:motion") ~= nil, true)
expect("Save frame: the photo's size", find("photo-size") ~= nil, true)
expect("Save frame: no encoding for a photo", find("photo-encode:quality"), nil)
click("photo-quality:quickest")
expect("photo quality picked", last_ran(), "script-message-to gyroflow export-setting photo quality quickest")
-- Framing: the same, Enter on its button
mp.set_property_native("user-data/gyroflow", gyro({ crop = { active = true, aspect = "original", flat = { zoom = 0 } } }))
expect("framing: the photo button", find("crop-photo") ~= nil, true)
expect("framing: the size", find("photo-size") ~= nil, true)
-- Buttons for short lists, drop-downs for long ones: what fits a button decides too
expect("rule: 3 short labels are buttons", ui.as_buttons({ { label = "Off" }, { label = "Light" }, { label = "Full" } }, 260, 1), true)
expect("rule: 2 are buttons", ui.as_buttons({ { label = "Level" }, { label = "Free" } }, 260, 1), true)
expect("rule: 4 are a drop-down", ui.as_buttons({ { label = "A" }, { label = "B" }, { label = "C" }, { label = "D" } }, 260, 1), false)
expect("rule: 1 is a drop-down", ui.as_buttons({ { label = "A" } }, 260, 1), false)
expect("rule: labels too long for a third of the row: a drop-down",
       ui.as_buttons({ { label = "Trail by direction" }, { label = "Frame's edge spread out" }, { label = "Background color" } }, 260, 1), false)
expect("rule: a value off the list (1.5×): a drop-down", ui.as_buttons({ { label = "1×" }, { label = "2×" } }, 260, 1, { text = "1.5×" }), false)
mp.set_property_native("user-data/gyroflow", { available = true, edges = "sphere", fps_limit = 0, resolution = "auto", temporal_denoise = "full" })
messages.tab("quality")
expect("Quality: the frame rate as buttons", find("fps:30") ~= nil, true)
expect("Quality: the edges stay a drop-down (long labels)", find("edges") ~= nil and find("edges:smooth") == nil, true)
expect("Quality: the size (five) a drop-down", find("size") ~= nil, true)

-- The Playlist tab (the rail's button, F8) with the clip playing near the end of a long list: opening it scrolls the
-- clip into the middle, past the list's end, and the panel was drawn twice and given up (render's `draw() or draw()`
-- both nil): an error in render, the panel and the bar left unpainted, black (Anton). Now drawn until it settles
local list = {}
for i = 1, 60 do list[i] = { filename = ("/footage/DJI_%04d.MP4"):format(i), current = i == 59 or nil, playing = i == 59 or nil } end
mp.set_property_native("video-exts", { "mp4" })
mp.set_property_native("playlist", list)
mp.set_property_native("playlist-count", 60)
mp.set_property_native("playlist-pos", 58)
mp.set_property_native("osd-dimensions", { w = 1280, h = 700 })
for _, open_it in ipairs({ function() messages.tab("playlist") end, function() bindings.playlist.fn() end,
                           function() messages.tab("output"); click("rail:playlist") end }) do
    local ok, err = pcall(open_it)
    expect("the playlist opens near the list's end without an error" .. (ok and "" or (": " .. tostring(err))), ok, true)
    expect("…and the panel is drawn (its rail)", find("rail:playlist") ~= nil, true)
    expect("…the clip playing in view", find("pl:58") ~= nil, true)
    events["file-loaded"]()
end
-- An error in a part's drawing doesn't take the panel down: the part says so, the rail and the bar are still drawn
local list_rows = tl.playlist_rows
tl.playlist_rows = function() error("boom") end
local ok = pcall(function() messages.tab("playlist") end)
expect("a part that fails: no error out of the panel", ok, true)
expect("…the rail still drawn", find("rail:playlist") ~= nil, true)
tl.playlist_rows = list_rows
mp.set_property_native("osd-dimensions", { w = 1280, h = 1800 })
-- Underwater colour: its own tab, its switches and strength part of the clip's transform (the dot: what's saved)
mp.set_property_native("user-data/workspace", { path = "/footage/DJI_0001.MP4", clip = "/footage/DJI_0001.MP4", view_target = "source",
    view_status = "changed", view_from = "source", saved = { uw = false, uw_highlights = false, uw_strength = 1 } })
mp.set_property_native("user-data/uwcolor", { uw = true, uw_highlights = false, uw_strength = 0.6, active = true, ok = true,
    session = {}, defaults = {}, stats = { under_water = 1, water = 0.48, water_hue = 263, water_hue_after = 263, water_a = -0.012,
    water_a_after = -0.043, leak = 0.5, red = 0.09, blue = 0.18, blue_water = 0, lift = 0.94, median = 0.12,
    applied = { red = 0.09, blue = 0.18, leak = 0.5, lift = 0.94 }, ms = 300 } })
messages.tab("water")
expect("underwater: the rail has its button", find("rail:water") ~= nil, true)
expect("underwater: its switches and strength", find("uw-on") ~= nil and find("uw-hl") ~= nil and find("uw-strength") ~= nil, true)
expect("underwater: the transform's Save there too", find("view-save") ~= nil and find("view-revert") ~= nil, true)
click("uw-on")
expect("underwater: the switch", last_ran(), "script-message-to dlogm uw-set uw toggle")
click("uw-hl")
expect("underwater: brighter highlights", last_ran(), "script-message-to dlogm uw-set uw_highlights toggle")
click("view-save")
expect("underwater: Save saves the transform", last_ran(), "script-message-to workspace view-save")
click("view-default")
expect("underwater: Use as default is the transform's", last_ran(), "script-message-to gyroflow remember")
mp.set_property_native("user-data/uwcolor", { uw = false, uw_highlights = false, uw_strength = 1, active = false, ok = true, stats = {} })
expect("underwater: off, highlights can't be switched", (function() local before = #ran; click("uw-hl"); return #ran == before end)(), true)
messages.tab("output")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
