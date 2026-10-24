-- highlights: the moments the camera marked while it recorded (GoPro's HiLight button, DJI's highlights from the quick
-- switch button or the remote), as marks on the seek bar, with a key to jump to the next one and back. An mpv plugin:
-- this folder goes into ~~/scripts/.
--
-- They're read from the file when it's loaded (markers.lua, plain Lua: a few small reads). Cameras: GoPro HERO5 and
-- later, DJI Osmo Action 4, 5 Pro and 6. Any other file, or one without highlights: nothing happens.
--
-- Keys (rebind with `<key> script-binding highlights/<name>` in input.conf):
--   Ctrl+]  next   the next highlight
--   Ctrl+[  prev   the one before; a couple of seconds after landing on one, the one before that
-- A clip without highlights: the next or previous chapter, if it has chapters. As chapters, the highlights are also
-- what mpv's own chapter keys go to (PGUP next, PGDWN previous), at the mark itself.
--
-- Script messages (`script-message-to highlights <message>`, what the right-click menu runs):
--   go <n>     the n-th highlight
-- The state is in user-data/highlights/ for the menu: camera (GoPro, DJI), times (seconds, in order).
--
-- Options (~~/script-opts/highlights.conf):
--   before=2       seconds before each highlight a jump lands: the button is pressed once the moment has happened
--   chapters=yes   the highlights as chapters, which mpv's seek bar shows as marks (next to the file's own, if any)

local mp = require 'mp'
local options = require 'mp.options'
local markers = require 'markers'

local o = { before = 2, chapters = true }
options.read_options(o, "highlights")

local PROBED_EXTENSIONS = { mp4 = true, mov = true, lrv = true }
-- prev within this long after a highlight's landing goes on to the one before
local AGAIN = 2

local found = nil

local function publish()
    mp.set_property_native("user-data/highlights", found and { camera = found.camera, times = found.times } or {})
end

local function clock(s)
    s = math.floor(s)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

local function landing(i)
    return math.max(0, found.times[i] - o.before)
end

local function go(i)
    mp.commandv("seek", landing(i), "absolute")
    mp.osd_message(string.format("Highlight %d of %d  %s", i, #found.times, clock(found.times[i])), 1.5)
end

local function load()
    found = nil
    local path = mp.get_property("path")
    if not path or path:find("://", 1, true) then return end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBED_EXTENSIONS[ext] then return end
    local ok, m = pcall(markers.read, path)
    if not ok then
        mp.msg.warn("couldn't read the highlights: " .. tostring(m))
        return
    end
    if not m then return end
    -- Past the end: not this file's (a recording split into several files)
    local duration = mp.get_property_number("duration")
    local times = {}
    for _, t in ipairs(m.times) do
        if not duration or t <= duration then times[#times + 1] = t end
    end
    if #times == 0 then return end
    found = { camera = m.camera, times = times }
    mp.msg.verbose(m.camera .. ": " .. #times .. " highlights")

    if o.chapters then
        -- FFmpeg already makes chapters of HMMT (GoPro HERO5, HERO6), untitled: those get the title
        local chapters = mp.get_property_native("chapter-list") or {}
        for i, t in ipairs(times) do
            local same = nil
            for _, c in ipairs(chapters) do
                if math.abs(c.time - t) < 0.05 then same = c end
            end
            if not same then
                chapters[#chapters + 1] = { title = "Highlight " .. i, time = t }
            elseif (same.title or "") == "" then
                same.title = "Highlight " .. i
            end
        end
        table.sort(chapters, function(a, b) return a.time < b.time end)
        mp.set_property_native("chapter-list", chapters)
    end
end

mp.register_event("file-loaded", function()
    load()
    publish()
end)

mp.register_event("end-file", function()
    found = nil
    publish()
end)

-- A clip without highlights: its chapters, as mpv's chapter keys do; or a word that there are none
local function no_highlights(step)
    if (mp.get_property_number("chapters") or 0) > 0 then
        mp.commandv("add", "chapter", tostring(step))
    else
        mp.osd_message("No highlights in this clip", 1.5)
    end
end

mp.add_key_binding("Ctrl+]", "next", function()
    if not found then return no_highlights(1) end
    local pos = mp.get_property_number("time-pos", 0)
    for i = 1, #found.times do
        if landing(i) > pos + 0.5 then return go(i) end
    end
    mp.osd_message("No more highlights", 1.5)
end)

mp.add_key_binding("Ctrl+[", "prev", function()
    if not found then return no_highlights(-1) end
    local pos = mp.get_property_number("time-pos", 0)
    for i = #found.times, 1, -1 do
        if landing(i) < pos - AGAIN then return go(i) end
    end
    mp.osd_message("No highlight before this", 1.5)
end)

mp.register_script_message("go", function(n)
    n = tonumber(n)
    if found and n and found.times[n] then go(n) end
end)
