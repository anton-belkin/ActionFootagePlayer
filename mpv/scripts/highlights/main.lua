-- highlights: the marks of a clip: the moments the camera marked while it recorded (GoPro's HiLight button, DJI's
-- highlights from the quick switch button or the remote), and your own (M). As marks on the seek bar, with a key to
-- jump to the next one and back. An mpv plugin: this folder goes into ~~/scripts/.
--
-- The camera's are read from the file when it's loaded (markers.lua, plain Lua: a few small reads). Cameras: GoPro
-- HERO5 and later, DJI Osmo Action 4, 5 Pro and 6. While you've changed none, those are the marks. The first change
-- (M) writes <clip>.marks.json beside the clip (docs/moments-and-workspaces.md, "Marks"), with the camera's copied in
-- and flagged ("camera": "dji"), and from then on that file is the marks: a camera mark removed is gone from it.
-- Delete the file, and the camera's are the marks again. A marks file for a clip of another size (GoPro's GX010123
-- names come back on every card) is renamed .marks.json.old rather than applied. The workspace plugin says where the
-- file is (user-data/workspace/marks_file). Marks need no project: the file is beside the clip.
--
-- Keys (rebind with `<key> script-binding highlights/<name>` in input.conf):
--   Ctrl+]  next   the next mark
--   Ctrl+[  prev   the one before; a couple of seconds after landing on one, the one before that
--   M       mark   a mark here; on one (within half a second), remove it
-- A clip without marks: the next or previous chapter, if it has chapters. As chapters, the marks are also what mpv's
-- own chapter keys go to (PGUP next, PGDWN previous), at the mark itself.
--
-- Script messages (`script-message-to highlights <message>`, what the right-click menu runs):
--   go <n>     the n-th mark
--   mark       as M
-- The state is in user-data/highlights/ for the menu: camera (GoPro, DJI: the clip's own), times (seconds, in order),
-- marks ({ t, camera (dji, gopro: one of the camera's), name }, in order), edited (yes: from the marks file). A moment
-- playing as a video of its own (the workspace plugin's offset and length): only the marks inside it, in its time;
-- offset: where it starts in the clip.
--
-- Options (~~/script-opts/highlights.conf):
--   before=2       seconds before each of the camera's marks a jump lands: its button is pressed once the moment has
--                  happened. Your own marks (M) are where you put them: a jump lands on them
--   chapters=yes   the marks as chapters, which mpv's seek bar shows (next to the file's own, if any)

local mp = require 'mp'
local utils = require 'mp.utils'
local options = require 'mp.options'
local markers = require 'markers'

-- The clip playing: mpv's path, or the original when the file playing is a moment (the workspace plugin's
-- user-data/workspace/clip, docs/moments-and-workspaces.md)
local function clip_path()
    local path = mp.get_property("path")
    local ws = mp.get_property_native("user-data/workspace")
    if path and type(ws) == "table" and ws.path == path and ws.redirected and ws.clip then return ws.clip end
    return path
end

-- The clip's fingerprint, when the workspace plugin has read it (its hash): kept in the marks file, to find the clip
-- again once it's moved or renamed
local function clip_hash()
    local ws = mp.get_property_native("user-data/workspace")
    return type(ws) == "table" and ws.path == mp.get_property("path") and ws.hash or nil
end

local o = { before = 2, chapters = true }
options.read_options(o, "highlights")

local PROBED_EXTENSIONS = { mp4 = true, mov = true, lrv = true }
-- prev within this long after a mark's landing goes on to the one before
local AGAIN = 2
-- M within this long of a mark removes it
local NEAR = 0.5
local FORMAT = "actionfootageplayer-marks"
local CAMERA_NOTES = { dji = "DJI quick switch", gopro = "GoPro HiLight" }

-- The clip's: { path, camera, marks = { {t, camera, name} }, file (the marks file), doc (what it holds, unknown
-- fields kept) }
local found = nil
-- The mark last jumped to: a jump lands `before` seconds ahead of a camera's mark, and until it's passed that's the
-- mark "here"
local jumped = nil
-- The chapters this put in chapter-list
local ours = {}
local applying = false

local function times()
    local out = {}
    for i, m in ipairs(found and found.marks or {}) do out[i] = m.t end
    return out
end

local function publish()
    if not found or #found.marks == 0 then
        mp.set_property_native("user-data/highlights", found and found.doc and { marks = {}, times = {}, edited = true } or {})
        return
    end
    mp.set_property_native("user-data/highlights", { camera = found.camera, times = times(), marks = found.marks, edited = found.doc ~= nil,
                                                    jumped = jumped, before = o.before, offset = found.offset })
end

local function clock(s)
    s = math.floor(s)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

-- How far ahead of a mark a jump lands: the camera's are marked once the moment has happened, yours where you want them
local function lead(m)
    return m.camera and o.before or 0
end

local function landing(i)
    return math.max(0, found.marks[i].t - lead(found.marks[i]))
end

local function title(i, m)
    if m.name and m.name ~= "" then return m.name end
    return (m.camera and "Highlight " or "Mark ") .. i
end

local function go(i)
    jumped = i
    publish()
    mp.commandv("seek", landing(i), "absolute")
    mp.osd_message(string.format("%s, %d of %d  %s", title(i, found.marks[i]), i, #found.marks, clock(found.marks[i].t)), 1.5)
end

-- The marks as chapters, beside the others there (the file's own, the workspace plugin's moments). FFmpeg makes
-- untitled chapters of a GoPro HERO5/6's HMMT highlights: those are ours, by time
local function same_chapter(a, b) return a.title == b.title and math.abs(a.time - b.time) < 0.001 end

local function apply_chapters()
    if not o.chapters then return end
    local list = {}
    for i, m in ipairs(found and found.marks or {}) do list[#list + 1] = { title = title(i, m), time = m.t } end
    local chapters = {}
    for _, c in ipairs(mp.get_property_native("chapter-list") or {}) do
        local mine = (c.title or "") == "" and found and found.camera_times and found.camera_times[string.format("%.2f", c.time)]
        for _, x in ipairs(ours) do if same_chapter(c, x) then mine = true end end
        if not mine then chapters[#chapters + 1] = { title = c.title, time = c.time } end
    end
    for _, c in ipairs(list) do chapters[#chapters + 1] = c end
    table.sort(chapters, function(a, b) return a.time < b.time end)
    ours = list
    applying = true
    mp.set_property_native("chapter-list", chapters)
    applying = false
end

mp.observe_property("chapter-list", "native", function(_, chapters)
    if applying or #ours == 0 then return end
    for _, x in ipairs(ours) do
        local there = false
        for _, c in ipairs(chapters or {}) do if same_chapter(c, x) then there = true end end
        if not there then return apply_chapters() end
    end
end)

-- Where the clip's marks file is: the workspace plugin's, else beside the clip
local function marks_file(path)
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and ws.clip == path and ws.marks_file then return ws.marks_file end
    if type(ws) == "table" and ws.path == path and ws.marks_file then return ws.marks_file end
    return (path:match("^(.*)%.[^./\\]+$") or path) .. ".marks.json"
end

local function read_doc(file, size)
    local f = io.open(file, "r")
    if not f then return nil end
    local doc = utils.parse_json(f:read("*a") or "")
    f:close()
    if type(doc) ~= "table" or doc.format ~= FORMAT then
        mp.msg.warn(file .. ": not a marks file, left alone")
        return nil
    end
    -- Another clip's (a reused name): moved aside, not applied
    if type(doc.clip) == "table" and doc.clip.size and size and doc.clip.size ~= size then
        os.remove(file .. ".old")
        os.rename(file, file .. ".old")
        mp.osd_message("Marks of another clip with this name: kept as " .. (file:match("([^/\\]+)$") or file) .. ".old", 4)
        return nil
    end
    local marks = {}
    for _, m in ipairs(type(doc.marks) == "table" and doc.marks or {}) do
        if type(m) == "table" and type(m.t) == "number" then marks[#marks + 1] = m end
    end
    table.sort(marks, function(a, b) return a.t < b.t end)
    doc.marks = marks
    return doc
end

-- A moment playing as a video of its own (the workspace plugin's): where its part starts in the clip, and how long
-- it is; 0 and nil for a clip
local function window()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.offset) == "number" then return ws.offset, ws.length end
    return 0, nil
end

-- The marks that show: inside the part, in its time
local function visible(all, off, len)
    local out = {}
    for _, m in ipairs(all) do
        local t = m.t - off
        if t >= -0.001 and (not len or t <= len + 0.001) then out[#out + 1] = { t = math.max(0, t), camera = m.camera, name = m.name } end
    end
    return out
end

local function load()
    found = nil
    local path = clip_path()
    if not path or path:find("://", 1, true) then return end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBED_EXTENSIONS[ext] then return end
    local info = utils.file_info(path)
    local camera, marks, camera_times = nil, {}, {}
    local ok, m = pcall(markers.read, path)
    if not ok then mp.msg.warn("couldn't read the highlights: " .. tostring(m)) end
    if ok and m then
        -- Past the end: not this file's (a recording split into several files)
        local duration = window() == 0 and mp.get_property_number("duration") or nil
        for _, t in ipairs(m.times) do
            if not duration or t <= duration then
                marks[#marks + 1] = { t = t, camera = m.camera:lower() }
                camera_times[string.format("%.2f", t)] = true
            end
        end
        if #marks > 0 then camera = m.camera end
    end
    local file = marks_file(path)
    local doc = read_doc(file, info and info.size)
    local off, len = window()
    local all = doc and doc.marks or marks
    -- all: in the clip's time; marks: what shows (a moment playing: those inside it, in its time)
    found = { path = path, camera = camera, all = all, marks = visible(all, off, len), offset = off, length = len, file = file,
              doc = doc, camera_times = camera_times, size = info and info.size }
    mp.msg.verbose(#found.marks .. " marks" .. (doc and (" from " .. file) or (camera and (" from the " .. camera) or "")))
    if #found.marks > 0 or #ours > 0 then apply_chapters() end
end

-- The marks file written whole: next to it, then renamed over it
local function write(doc)
    local json = utils.format_json(doc)
    if not json then return false, "couldn't make the file" end
    -- An empty list is an array
    json = json:gsub('"marks":{}', '"marks":[]')
    local tmp = found.file .. ".tmp"
    local f, err = io.open(tmp, "w")
    if not f then return false, err end
    f:write(json, "\n")
    f:close()
    os.remove(found.file)
    local ok, rerr = os.rename(tmp, found.file)
    if not ok then os.remove(tmp); return false, rerr end
    return true
end

-- M: a mark here, or the one here removed. The first change copies the camera's into the file
local function mark()
    if not found then
        local path = clip_path()
        if not path or path:find("://", 1, true) then return mp.osd_message("Marks are for a clip on this computer or the NAS", 2) end
        return mp.osd_message("Marks: this clip is still loading", 2)
    end
    local pos = mp.get_property_number("time-pos")
    if not pos then return end
    -- In the clip's time
    pos = pos + found.offset
    local doc = found.doc
    if not doc then
        local copy = {}
        for _, m in ipairs(found.all) do copy[#copy + 1] = { t = math.floor(m.t * 1000 + 0.5) / 1000, camera = m.camera } end
        local ws = mp.get_property_native("user-data/workspace")
        doc = {
            format = FORMAT, version = 1,
            -- hash: the clip's fingerprint, to find it again once it's moved (the workspace plugin's)
            clip = { file = found.path:match("([^/\\]+)$"), size = found.size,
                     hash = clip_hash(),
                     duration = found.offset == 0 and mp.get_property_number("duration") or nil,
                     created = mp.get_property("metadata/by-key/creation_time") or (type(ws) == "table" and ws.created) or nil },
            marks = copy,
        }
    end
    -- The mark here: within half a second, or the one just jumped to, from its landing to just after it
    local near, best = nil, NEAR
    for i, m in ipairs(doc.marks) do
        if math.abs(m.t - pos) <= best then near, best = i, math.abs(m.t - pos) end
    end
    local j = jumped and found.marks[jumped]
    local jt = j and j.t + found.offset
    if not near and jt and pos >= jt - lead(j) - 0.25 and pos <= jt + NEAR then
        for i, m in ipairs(doc.marks) do if math.abs(m.t - jt) < 0.002 then near = i end end
    end
    jumped = nil
    local message
    if near then
        local m = table.remove(doc.marks, near)
        message = (m.name or (m.camera and (CAMERA_NOTES[m.camera] or "The camera's mark") or "Mark")) .. " at " .. clock(m.t) .. " removed"
    else
        local t = math.floor(pos * 1000 + 0.5) / 1000
        doc.marks[#doc.marks + 1] = { t = t }
        table.sort(doc.marks, function(a, b) return a.t < b.t end)
        message = "Mark at " .. clock(t)
    end
    -- A file written before fingerprints: the clip's too, once it's known
    if type(doc.clip) == "table" and doc.clip.hash == nil then doc.clip.hash = clip_hash() end
    local ok, err = write(doc)
    if not ok then return mp.osd_message("Couldn't write the marks: " .. tostring(err), 4) end
    found.doc, found.all, found.marks = doc, doc.marks, visible(doc.marks, found.offset, found.length)
    mp.osd_message(message .. " (" .. (#doc.marks == 1 and "1 mark" or (#doc.marks .. " marks")) .. ")", 2)
    apply_chapters()
    publish()
end

mp.register_event("file-loaded", function()
    load()
    publish()
end)

mp.register_event("end-file", function()
    found = nil
    jumped = nil
    ours = {}
    publish()
end)

-- A clip without marks: its chapters, as mpv's chapter keys do; or a word that there are none
local function no_marks(step)
    if (mp.get_property_number("chapters") or 0) > 0 then
        mp.commandv("add", "chapter", tostring(step))
    else
        mp.osd_message("No marks in this clip: M adds one", 1.5)
    end
end

mp.add_key_binding("Ctrl+]", "next", function()
    if not found or #found.marks == 0 then return no_marks(1) end
    local pos = mp.get_property_number("time-pos", 0)
    for i = 1, #found.marks do
        if landing(i) > pos + 0.5 then return go(i) end
    end
    mp.osd_message("No more marks", 1.5)
end)

mp.add_key_binding("Ctrl+[", "prev", function()
    if not found or #found.marks == 0 then return no_marks(-1) end
    local pos = mp.get_property_number("time-pos", 0)
    for i = #found.marks, 1, -1 do
        if landing(i) < pos - AGAIN then return go(i) end
    end
    mp.osd_message("No mark before this", 1.5)
end)

mp.add_key_binding("M", "mark", mark)
mp.register_script_message("mark", mark)

mp.register_script_message("go", function(n)
    n = tonumber(n)
    if found and n and found.marks[n] then go(n) end
end)
