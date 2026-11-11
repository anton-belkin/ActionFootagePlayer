-- The Output tab's one Export button: its kinds, the list's rows, the label (mpv/scripts/side-panel/exportkind.lua),
-- and the split button and the list as the panel draws them (ui.lua, over a stand-in for mpv).
-- From the repository's folder: luajit tests/exportkind_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path
package.preload.mp = function()
    return { get_property = function() return nil end, command_native = function() return nil end }
end
local ek = require 'exportkind'
local ui = require 'ui'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function kinds(rows)
    local t = {}
    for _, r in ipairs(rows) do t[#t + 1] = r.kind .. (r.disabled and "(off)" or "") end
    return table.concat(t, ",")
end
local function clock(s) return string.format("%d:%02d", math.floor(s / 60), math.floor(s) % 60) end

-- What a file made as it plays is
local LOG = { log = true, curve = "dlogm", hdr = "", look = "hdr", sdr_look = "maker" }
expect("log in HDR", ek.plays_as(LOG), "HDR")
expect("log in the maker's SDR", ek.plays_as({ log = true, hdr = "", look = "maker", sdr_look = "maker" }), "SDR")
expect("log in NBCU", ek.plays_as({ log = true, hdr = "", look = "nbcu", sdr_look = "nbcu" }), "SDR")
expect("log as recorded", ek.plays_as({ log = true, hdr = "", look = "off" }), "log")
expect("tone mapped on screen is HDR in the file", ek.plays_as({ log = true, hdr = "", look = "tonemap" }), "HDR")
expect("an SDR clip", ek.plays_as({ log = false, hdr = "", look = "" }), "SDR")
expect("an HLG clip", ek.plays_as({ log = false, hdr = "hlg", look = "hdr" }), "HDR")
local _, long = ek.plays_as({ log = true, hdr = "", look = "dv" })
expect("the dv look, in the list", long, "HDR + Dolby Vision")
_, long = ek.plays_as({ log = false, hdr = "pq", look = "dv", dv_own = true })
expect("a file's own Dolby Vision isn't carried", long, "HDR")
expect("no dlogm", ek.plays_as(nil), "as it plays")

-- The rows
local FLAT_CLIP = { available = false }
local SPHERE = { available = true, stamped = true, recording = { parts = 0 } }
local RECORDED = { available = true, stamped = true, recording = { parts = 2 } }
local STITCHED = { available = true, stamped = false, recording = { parts = 0 } }
expect("a log clip", kinds(ek.rows(LOG, FLAT_CLIP)), "hdr,sdr,youtube")
expect("no view360 state", kinds(ek.rows(LOG, nil)), "hdr,sdr,youtube")
expect("no SDR look", kinds(ek.rows({ log = false, hdr = "", look = "" }, nil)), "hdr")
expect("playing in SDR already: no second SDR", kinds(ek.rows({ log = true, hdr = "", look = "maker", sdr_look = "maker" }, nil)), "hdr,youtube")
expect("an HDR file with NBCU", kinds(ek.rows({ log = false, hdr = "hlg", look = "hdr", sdr_look = "nbcu" }, nil)), "hdr,sdr,youtube")
expect("a 360 clip, nothing recorded", kinds(ek.rows(LOG, SPHERE)), "hdr,sdr,youtube,360,flat(off)")
expect("a 360 clip recorded", kinds(ek.rows(LOG, RECORDED)), "hdr,sdr,youtube,360,flat")
expect("a stitched 360 clip: neither", kinds(ek.rows(LOG, STITCHED)), "hdr,sdr,youtube")
local rows = ek.rows(LOG, SPHERE)
expect("as it plays, named", rows[1].label, "As it plays (HDR)")
expect("flat greyed, saying why", rows[5].label, "Flat: record a view first")
expect("flat recorded", ek.rows(LOG, RECORDED)[5].label, "Flat, the recording")
expect("360 named", rows[4].label, "360° video")
expect("YouTube, D-Log M in DJI's look", rows[3].label, "For YouTube (HDR, its SDR by DJI's LUT)")
expect("YouTube, an HLG file", ek.rows({ log = false, hdr = "hlg", look = "hdr", sdr_look = "nbcu" }, nil)[3].label, "For YouTube (HDR, its SDR by NBCU)")
expect("YouTube, D-Log M in NBCU's look", (ek.youtube_sdr({ log = true, curve = "dlogm", hdr = "", sdr_look = "nbcu" })), "nbcu")
expect("YouTube, another curve: NBCU", (ek.youtube_sdr({ log = true, curve = "gplog", hdr = "", sdr_look = "maker" })), "nbcu")
expect("YouTube, an SDR clip: none", ek.youtube_sdr({ log = false, hdr = "" }), nil)

-- The kind made: the one wanted while it's there, else as it plays
expect("nothing chosen", ek.chosen(ek.rows(LOG, nil), nil), "hdr")
expect("SDR chosen", ek.chosen(ek.rows(LOG, nil), "sdr"), "sdr")
expect("SDR gone (playing in SDR)", ek.chosen(ek.rows({ log = true, hdr = "", look = "nbcu", sdr_look = "nbcu" }, nil), "sdr"), "hdr")
expect("flat without a recording", ek.chosen(ek.rows(LOG, SPHERE), "flat"), "hdr")
expect("flat with one", ek.chosen(ek.rows(LOG, RECORDED), "flat"), "flat")
expect("YouTube chosen", ek.chosen(ek.rows(LOG, nil), "youtube"), "youtube")
expect("YouTube gone (an SDR clip)", ek.chosen(ek.rows({ log = false, hdr = "", look = "" }, nil), "youtube"), "hdr")
expect("360 on a flat clip", ek.chosen(ek.rows(LOG, FLAT_CLIP), "360"), "hdr")
expect("remembered per kind of clip: 360", ek.clip_kind(RECORDED), "360")
expect("remembered per kind of clip: flat", ek.clip_kind(FLAT_CLIP), "flat")
expect("a stitched clip is a flat one here", ek.clip_kind(STITCHED), "flat")

-- Busy: a take recording
local TAKING = { available = true, stamped = true, recording = { parts = 1, taking = true } }
expect("flat while recording", ek.busy("flat", TAKING) ~= nil, true)
expect("360 while recording", ek.busy("360", TAKING) ~= nil, true)
expect("HDR while recording", ek.busy("hdr", TAKING), nil)
expect("flat, not recording", ek.busy("flat", RECORDED), nil)

-- The label
expect("the clip, HDR", ek.label("hdr", LOG, nil, nil, clock), "Export the clip · HDR")
expect("the clip, SDR", ek.label("sdr", LOG, nil, nil, clock), "Export the clip · SDR")
expect("as it plays in SDR", ek.label("hdr", { log = true, hdr = "", look = "maker" }, nil, nil, clock), "Export the clip · SDR")
expect("the clip, 360", ek.label("360", LOG, nil, nil, clock), "Export the clip · 360°")
expect("the clip, for YouTube", ek.label("youtube", LOG, nil, nil, clock), "Export the clip · YouTube")
expect("a part, flat", ek.label("flat", LOG, 12, 47, clock), "Export the part 0:12–0:47 · flat")
expect("only a start marked", ek.label("flat", LOG, 12, nil, clock), "Export the clip · flat")
expect("an end before the start", ek.label("hdr", LOG, 47, 12, clock), "Export the clip · HDR")

-- The split button: two places, the main one gone while it's disabled
local function hit(u, id)
    for _, h in ipairs(u.hits) do if h.id == id then return h end end
end
local function hit_at(u, x, y)
    for i = #u.hits, 1, -1 do
        local h = u.hits[i]
        if x >= h.x0 and x < h.x1 and y >= h.y0 and y < h.y1 then return h end
    end
end
local u = ui.new(2)
local opened = false
local y = u:split_button(20, 520, 100, "export", "Export the clip · HDR", "script-message-to gyroflow export hdr",
                         function() opened = true end)
expect("split: next y", y, 100 + 28 * 2 + 6 * 2)
local main, more = hit(u, "export"), hit(u, "export:more")
expect("split: main part", main and main.cmd, "script-message-to gyroflow export hdr")
expect("split: main part's right edge", main and main.x1, 20 + 520 - ui.SPLIT_ARROW * 2)
expect("split: ▾ part's left edge", more and more.x0, main and main.x1)
expect("split: ▾ part's right edge", more and more.x1, 540)
if more then more.cmd() end
expect("split: ▾ opens", opened, true)
u = ui.new(2)
u:split_button(20, 520, 100, "export", "Export the clip · flat", "x", function() end, { disabled = true })
expect("split disabled: no main part", hit(u, "export"), nil)
expect("split disabled: ▾ still opens the list", hit(u, "export:more") ~= nil, true)

-- The list: only noted until popover() draws it, under the button, its right edge at the button's
u = ui.new(2)
expect("no list: nothing drawn", u:popover(0, 1000), nil)
local picked = nil
local list = ek.rows(LOG, SPHERE)
for _, r in ipairs(list) do local k = r.kind; r.cmd = function() picked = k end end
u:dropdown(20, 540, 100, 156, "export-kind", list, "sdr")
local events_before = #u.events
local box = u:popover(0, 1000)
expect("list: drawn", box ~= nil, true)
expect("list: drawn when popover() is called", #u.events > events_before, true)
expect("list: under the button", box[2], 156 + 4 * 2)
expect("list: right edge at the button's", box[3], 540)
expect("list: within the button's width", box[1] >= 20, true)
expect("list: a row each", box[4] - box[2], #list * ui.ROW * 2 + 2 * 4 * 2)
expect("list: drawn once", u:popover(0, 1000), nil)
local sdr_row = hit(u, "export-kind:sdr")
expect("list: a row's place", sdr_row ~= nil and sdr_row.popover, true)
expect("list: a greyed row has no place", hit(u, "export-kind:flat"), nil)
local flat_y = box[2] + 4 * 2 + 4.5 * ui.ROW * 2
local under = hit_at(u, 300, flat_y)
expect("list: a click on a greyed row stays on the list", under and under.id, "export-kind")
expect("list: the list's own place is the popover's", under and under.popover, true)
local first = hit_at(u, 300, box[2] + 4 * 2 + 0.5 * ui.ROW * 2)
expect("list: the rows are found first", first and first.id, "export-kind:hdr")
if sdr_row then sdr_row.cmd() end
expect("list: a pick runs its row", picked, "sdr")
local check = 0
for _, e in ipairs(u.events) do if e:find("\\1c&H" .. ui.C.accent .. "&", 1, true) then check = check + 1 end end
expect("list: the chosen row checked (two strokes)", check, 2)

-- No room below: above the button
u = ui.new(2)
u:dropdown(20, 540, 900, 956, "export-kind", list, "hdr")
box = u:popover(0, 1000)
expect("list: above when there's no room below", box[4], 900 - 4 * 2)
-- No room either way: below, as far as it goes
u = ui.new(2)
u:dropdown(20, 540, 40, 96, "export-kind", list, "hdr")
box = u:popover(0, 120)
expect("list: below when it fits neither way", box[2], 96 + 4 * 2)
-- Drawn over what's scrolled: not cut by the scrolled area's clip
u = ui.new(2)
u.clip = { 0, 0, 600, 150 }
u:dropdown(20, 540, 100, 156, "export-kind", list, "hdr")
u:popover(0, 1000)
expect("list: not clipped", hit(u, "export-kind:hdr") ~= nil and hit(u, "export-kind:hdr").y1 > 150, true)
expect("list: the clip kept for what follows", u.clip and u.clip[4], 150)

-- Its button scrolled out of what shows: not drawn
u = ui.new(2)
u:dropdown(20, 540, 100, 156, "export-kind", list, "hdr")
expect("list: button scrolled out below", u:popover(0, 1000, { 200, 900 }), nil)
u:dropdown(20, 540, 100, 156, "export-kind", list, "hdr")
expect("list: button scrolled out above", u:popover(0, 1000, { 0, 90 }), nil)
u:dropdown(20, 540, 100, 156, "export-kind", list, "hdr")
expect("list: button partly seen", u:popover(0, 1000, { 120, 900 }) ~= nil, true)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
