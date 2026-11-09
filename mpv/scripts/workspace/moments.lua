-- Moments (docs/moments-and-workspaces.md): parts of a clip kept as small MP4s in the project's _Moments/ folder,
-- `<name>.afplink.mp4`, each with the link to its original inside (link.lua reads it). This is the part with no mpv
-- in it, so that luajit tests it (tests/moments_test.lua): names, dates, which clip a moment is of, where its
-- original is, and the _Moments/ folder read.
--
--   local moments = require('moments').new(ws, fs)   -- ws: workspace.lua's; fs: is_dir, is_file, info, readdir,
--                                                    -- read_link (link.lua's read), parse_json
--   moments.scan(project, cache[, folder, kind]) -> { list = { {file, name, link, raw}... }, dirs = { [dir] = mtime } }:
--                                         the link files of a kind anywhere in the project's folder
--   moments.of_clip(list, clip)        -> the moments of a clip ({relative, size, created})
--   moments.original(link, project)    -> the original's path, nil when it isn't found
--   moments.safe_name(name), moments.next_name(last, taken), moments.unique_file(dir, name)
--   moments_lib.loose_parts(marks, taken, duration, around) -> the parts for marks in no moment

local workspace_lib = require('workspace')

local M = {}

-- The project's folders start with _: sorted by name, they come first (Anton, 2026-11-05)
M.FOLDER = "_Moments"
-- The double extension says what the file is, and it still opens as an MP4
M.EXT = ".afplink.mp4"
-- Exports, videos of their own: _Exports/<name>[ (SDR)].mov
M.EXPORTS = "_Exports"
-- Clip previews: _Clips/<the clip's path>.afplink.mp4 (made before 0.11)
M.CLIPS = "_Clips"
-- Where what's made goes since 0.11 (docs/library-management.md §8): Generated/<date> <time> <what>/, a folder per run,
-- from where it's moved anywhere in the project folder; it's found there by the link inside
M.GENERATED = "Generated"
-- What a project's footage is: videos, not the player's own folders
M.FOOTAGE = { mp4 = true, mov = true }
-- (and the photos' folder, whose Live Photos' .mov files aren't footage either)
local OWN_FOLDERS = { _moments = true, _clips = true, _exports = true, _stills = true, _renders = true, _cache = true, moments = true,
                      clips = true, exports = true, stabilized = true, generated = true }
M.OWN_FOLDERS = OWN_FOLDERS

-- A new folder for what a run makes: <project>/Generated/<date> <time> <what> (`day`: <date> <what>, one a day), by
-- this computer's clock. `t` os.time()
function M.generated_rel(what, t, day)
    local stamp = os.date(day and "%Y-%m-%d" or "%Y-%m-%d %H-%M", t)
    return M.GENERATED .. "/" .. stamp .. " " .. what
end

-- A clip preview's path in _Clips/, from the clip's in the project ("Day 1/A.MP4" -> "Day 1/A.afplink.mp4"), without
-- a camera's own folders (DCIM and the one in it). `flat`: all in _Clips/ itself ("A.afplink.mp4")
function M.clip_preview_relative(relative, flat)
    if flat then
        local name = relative:match("([^/]+)$") or relative
        return (name:gsub("%.[^.]*$", "")) .. M.EXT
    end
    local parts = {}
    for p in relative:gmatch("[^/]+") do parts[#parts + 1] = p end
    local out, i = {}, 1
    while i <= #parts do
        if parts[i]:upper() == "DCIM" and i < #parts - 1 then i = i + 2
        else out[#out + 1] = parts[i]; i = i + 1 end
    end
    out[#out] = out[#out]:gsub("%.[^.]*$", "") .. M.EXT
    return table.concat(out, "/")
end

-- When a clip was recorded, in seconds (local time): a DJI file's name says it (DJI_20261022152841_...), else its
-- file's time
function M.recorded(c)
    local y, mo, d, h, mi, se = (c.relative or ""):match("DJI_(%d%d%d%d)(%d%d)(%d%d)(%d%d)(%d%d)(%d%d)[^/]*$")
    if y then return os.time({ year = tonumber(y), month = tonumber(mo), day = tonumber(d), hour = tonumber(h), min = tonumber(mi), sec = tonumber(se) }) end
    return c.mtime
end

local MONTHS = { "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December" }
-- A trip's folder: its year and month first (they sort), then its days: "2026-10 October 20-26",
-- "2026-09 September 28 - October 3", "2026-12 December 28 - January 2, 2027", "2026-10 October 20"
function M.trip_name(t0, t1)
    local a, b = os.date("*t", t0), os.date("*t", t1)
    local head = string.format("%04d-%02d %s %d", a.year, a.month, MONTHS[a.month], a.day)
    if a.year == b.year and a.month == b.month then
        return a.day == b.day and head or (head .. "-" .. b.day)
    end
    return head .. " - " .. MONTHS[b.month] .. " " .. b.day .. (a.year ~= b.year and (", " .. b.year) or "")
end

-- The clips in trips: one trip while the next clip is at most `gap` seconds (2 days) after the last. Sets each clip's
-- `trip` to its trip's folder name; a clip with no time gets none
function M.trips(clips, gap)
    gap = gap or 2 * 86400
    local timed = {}
    for _, c in ipairs(clips) do
        c.recorded = M.recorded(c)
        if c.recorded then timed[#timed + 1] = c end
    end
    table.sort(timed, function(x, y) return x.recorded < y.recorded end)
    local i = 1
    while i <= #timed do
        local j = i
        while j < #timed and timed[j + 1].recorded - timed[j].recorded <= gap do j = j + 1 end
        local name = M.trip_name(timed[i].recorded, timed[j].recorded)
        for k = i, j do timed[k].trip = name; timed[k].trip_first = timed[i] end
        i = j + 1
    end
end

-- The project file's clips_made: a line per clip a preview was made for, its path in the project and its size
function M.parse_made(text)
    local made = {}
    for line in (text or ""):gmatch("[^\r\n]+") do
        local rel, size = line:match("^(.-)\t(%d+)$")
        if rel then made[rel .. "\t" .. size] = true end
    end
    return made
end

function M.made_line(relative, size)
    return relative .. "\t" .. tostring(size)
end

-- Characters a file name can't have on Windows or macOS, and Windows' reserved names
local RESERVED = { CON = true, PRN = true, AUX = true, NUL = true }
for i = 1, 9 do RESERVED["COM" .. i] = true; RESERVED["LPT" .. i] = true end
local MAX_NAME_BYTES = 200

-- Days since 1970-01-01, and back (Howard Hinnant's algorithms)
local function days_from_civil(y, m, d)
    if m <= 2 then y = y - 1 end
    local era = math.floor(y / 400)
    local yoe = y - era * 400
    local mp = m > 2 and m - 3 or m + 9
    local doy = math.floor((153 * mp + 2) / 5) + d - 1
    return era * 146097 + yoe * 365 + math.floor(yoe / 4) - math.floor(yoe / 100) + doy - 719468
end

local function civil_from_days(z)
    z = z + 719468
    local era = math.floor(z / 146097)
    local doe = z - era * 146097
    local yoe = math.floor((doe - math.floor(doe / 1460) + math.floor(doe / 36524) - math.floor(doe / 146096)) / 365)
    local doy = doe - (365 * yoe + math.floor(yoe / 4) - math.floor(yoe / 100))
    local mp = math.floor((5 * doy + 2) / 153)
    local d = doy - math.floor((153 * mp + 2) / 5) + 1
    local m = mp < 10 and mp + 3 or mp - 9
    return yoe + era * 400 + (m <= 2 and 1 or 0), m, d
end
M.days_from_civil = days_from_civil
M.civil_from_days = civil_from_days

-- An ISO 8601 time: seconds since 1970 (UTC), and the zone's offset in minutes (nil when it has none: then it's
-- taken as UTC, as ffmpeg's creation_time is)
function M.parse_time(s)
    if type(s) ~= "string" then return nil end
    local y, mo, d, h, mi, sec, rest = s:match("^(%d%d%d%d)%-(%d%d)%-(%d%d)[T ](%d%d):(%d%d):(%d%d%.?%d*)(.*)$")
    if not y then return nil end
    local offset
    if rest == "Z" or rest == "z" then
        offset = 0
    elseif rest ~= "" then
        local sign, oh, om = rest:match("^([%+%-])(%d%d):?(%d%d)$")
        if not sign then return nil end
        offset = (tonumber(oh) * 60 + tonumber(om)) * (sign == "-" and -1 or 1)
    end
    local t = days_from_civil(tonumber(y), tonumber(mo), tonumber(d)) * 86400 + tonumber(h) * 3600 + tonumber(mi) * 60
        + tonumber(sec)
    return t - (offset or 0) * 60, offset
end

-- The local time of `t` (seconds since 1970) in a zone `offset` minutes from UTC: y, m, d, h, min, s (s with its
-- fraction)
local function fields(t, offset)
    local l = t + offset * 60
    local days = math.floor(l / 86400)
    local s = l - days * 86400
    local y, m, d = civil_from_days(days)
    return y, m, d, math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60
end

-- For gyroflow-export --dates: 2026-10-22T15:28:42.250+02:00
function M.format_time(t, offset)
    local y, m, d, h, mi, s = fields(t, offset)
    local a = math.abs(offset)
    return string.format("%04d-%02d-%02dT%02d:%02d:%06.3f%s%02d:%02d", y, m, d, h, mi, math.floor(s * 1000 + 0.5) / 1000,
                         offset < 0 and "-" or "+", math.floor(a / 60), a % 60)
end

-- A moment's name by its time: 2026-10-22 15.28.52 (colons aren't allowed in file names on Windows)
function M.time_name(t, offset)
    local y, m, d, h, mi, s = fields(t, offset)
    return string.format("%04d-%02d-%02d %02d.%02d.%02d", y, m, d, h, mi, math.floor(s))
end

-- The zone a clip was recorded in, in minutes from UTC: a DJI clip's name is the camera's local time
-- (DJI_20261024094541_0093_D.MP4) and its creation_time UTC, so their difference is it, to the quarter hour.
-- nil for other names
function M.zone_of_name(name, created)
    local y, mo, d, h, mi, s = name:match("^DJI_(%d%d%d%d)(%d%d)(%d%d)(%d%d)(%d%d)(%d%d)")
    if not y or not created then return nil end
    local localtime = days_from_civil(tonumber(y), tonumber(mo), tonumber(d)) * 86400 + tonumber(h) * 3600 + tonumber(mi) * 60
        + tonumber(s)
    local offset = math.floor((localtime - created) / 900 + 0.5) * 15
    if math.abs(offset) > 14 * 60 then return nil end
    return offset
end

-- The computer's zone at time t, in minutes from UTC (its summer time included)
function M.local_zone(t)
    local l = os.date("*t", math.floor(t))
    if not l then return 0 end
    local as_utc = days_from_civil(l.year, l.month, l.day) * 86400 + l.hour * 3600 + l.min * 60 + l.sec
    return math.floor((as_utc - math.floor(t)) / 60 + 0.5)
end

-- The name a file can have on every system, nil when nothing's left of it
function M.safe_name(name)
    if type(name) ~= "string" then return nil end
    name = name:gsub('[/\\:%*%?"<>|%c]', "-")
    name = name:gsub("^[%s%.]+", ""):gsub("[%s%.]+$", "")
    if #name > MAX_NAME_BYTES then
        local cut = MAX_NAME_BYTES
        -- Not inside a UTF-8 character: back to the byte that starts one
        while cut > 0 and name:byte(cut + 1) and name:byte(cut + 1) >= 0x80 and name:byte(cut + 1) < 0xC0 do cut = cut - 1 end
        name = name:sub(1, cut):gsub("[%s%.]+$", "")
    end
    if name == "" then return nil end
    if RESERVED[(name:match("^[^%.]*") or ""):upper()] then name = name .. "-" end
    return name
end

-- A moment file's name as the player shows it: its file name without .afplink.mp4 (or .mp4, renamed in Finder)
function M.display_name(file)
    local name = file:match("([^/\\]+)$") or file
    local lower = name:lower()
    if lower:sub(-#M.EXT) == M.EXT then return name:sub(1, -#M.EXT - 1) end
    return (name:gsub("%.[^.]*$", ""))
end

-- The name suggested after `last`: the next free number ("Turtle" -> "Turtle 2", "Turtle 2" -> "Turtle 3").
-- `taken`: the names there are, lower case, as keys
function M.next_name(last, taken)
    local base, n = last:match("^(.-) (%d+)$")
    if base and base ~= "" then n = tonumber(n) else base, n = last, 1 end
    repeat n = n + 1 until not taken[(base .. " " .. n):lower()]
    return base .. " " .. n
end

-- Seconds to the millisecond, as the link keeps them
function M.ms(t)
    return math.floor(t * 1000 + 0.5) / 1000
end

-- The time of a moment (its original's creation time plus its in), seconds since 1970, nil when unknown
function M.moment_time(link)
    local created = link and M.parse_time(link.created)
    if not created or type(link["in"]) ~= "number" then return nil end
    return created + link["in"]
end

-- The parts for a clip's marks that are in none of its moments: `around` seconds on each side of each, inside the
-- clip (0 to `duration`, when known), those that overlap joined into one. marks: times; taken: { {in, out} } of its
-- moments. Each part: { a, b, marks = { t... } }, in order
function M.loose_parts(marks, taken, duration, around)
    local loose = {}
    for _, t in ipairs(marks) do
        local inside = false
        for _, r in ipairs(taken) do
            if t >= r[1] - 0.001 and t <= r[2] + 0.001 then inside = true end
        end
        if not inside then loose[#loose + 1] = t end
    end
    table.sort(loose)
    local parts = {}
    for _, t in ipairs(loose) do
        local a, b = math.max(0, t - around), t + around
        if duration then b = math.min(duration, b) end
        local last = parts[#parts]
        if last and a <= last.b then
            last.b = math.max(last.b, b)
            last.marks[#last.marks + 1] = t
        elseif b > a then
            parts[#parts + 1] = { a = a, b = b, marks = { t } }
        end
    end
    return parts
end

function M.new(ws, fs)
    local self = {}
    local windows = fs.windows

    local function same(a, b)
        if windows then return a:lower() == b:lower() end
        return a == b
    end

    -- dir/<name>.afplink.mp4 (or <name><suffix>), or "<name> 2" and on when it's taken: the file and its name
    function self.unique_file(dir, name, suffix)
        suffix = suffix or M.EXT
        local file, n, try = nil, 1, name
        while true do
            file = ws.join(dir, try .. suffix)
            if not fs.is_file(file) and not fs.is_dir(file) then return file, try end
            n = n + 1
            try = name .. " " .. n
        end
    end

    -- A folder's files ({ name, info }) and subfolders (names) from one listing, but hidden ones: each entry's kind
    -- asked once (fs.info), not by a listing of the files and another of the folders. On a NAS each is a round trip,
    -- and through fs.info each is a moment the player's own script can let mpv have a turn (a quit, the next clip)
    local function entries(dir)
        local files, subdirs = {}, {}
        for _, name in ipairs(fs.readdir(dir, "all") or {}) do
            if name:sub(1, 1) ~= "." then
                local info = fs.info(ws.join(dir, name))
                if info and info.is_file then files[#files + 1] = { name = name, info = info }
                elseif info and info.is_dir then subdirs[#subdirs + 1] = name end
            end
        end
        return files, subdirs
    end

    -- Every link file of `kind` (moment, or clip for clip previews, export) in the project's folder, wherever it was
    -- put (docs/library-management.md §8): found by the link inside. In the project's own folders (_Moments, _Clips,
    -- _Exports, Generated…) and outside its footage every .mp4 and .mov is read; among the footage (a project next to
    -- it), only <name>.afplink.mp4, so as not to read every clip. cache[file] = { size, mtime, raw } keeps what was
    -- read of a file, read again only when its size or time changes. dirs: the folders to watch (modification time,
    -- which a file added, removed or renamed in it changes): the project's top, its own folders and every folder a
    -- link file of this kind is in. `folder` (a path in the project) keeps it to that folder
    function self.scan(project, cache, folder, kind)
        kind = kind or "moment"
        local list, dirs = {}, {}
        -- The footage's folders inside the project's (a project next to its footage)
        local footage_dirs = {}
        local locs = fs.locations_of and fs.locations_of(project) or (ws.locations and ws.locations(project)) or {}
        for _, l in ipairs(locs) do
            if l.dir and (same(l.dir, project) or ws.relative(l.dir, project)) then footage_dirs[#footage_dirs + 1] = l.dir end
        end
        local function among_footage(dir)
            for _, d in ipairs(footage_dirs) do if same(dir, d) or ws.relative(dir, d) then return true end end
            return false
        end
        local function walk(dir, depth, own)
            local info = fs.info(dir)
            if not info or not info.is_dir then return end
            local files, subdirs = entries(dir)
            local any = false
            local loose = own or not among_footage(dir)
            for _, e in ipairs(files) do
                local lower = e.name:lower()
                if lower:sub(-#M.EXT) == M.EXT or (loose and (lower:match("%.mp4$") or lower:match("%.mov$"))) then
                    local file, fi = ws.join(dir, e.name), e.info
                    local c = cache and cache[file]
                    local raw
                    if c and c.size == fi.size and c.mtime == fi.mtime then
                        raw = c.raw
                    else
                        raw = fs.read_link(file) or false
                        if cache then cache[file] = { size = fi.size, mtime = fi.mtime, raw = raw } end
                    end
                    local link = raw and fs.parse_json(raw)
                    if type(link) == "table" and link.v == 1 and link.kind == kind then
                        list[#list + 1] = { file = file, name = M.display_name(file), link = link, raw = raw }
                        any = true
                    end
                end
            end
            if any or depth == 0 or own then dirs[dir] = info.mtime end
            if depth < 8 then
                for _, name in ipairs(subdirs) do
                    -- The NAS's recycle bin isn't the project's
                    if name:sub(1, 1) ~= "#" then walk(ws.join(dir, name), depth + 1, own or (depth == 0 and OWN_FOLDERS[name:lower()] == true)) end
                end
            end
        end
        if folder then
            walk(ws.join(project, windows and folder:gsub("/", "\\") or folder), 1, true)
        else
            walk(project, 0, false)
        end
        return { list = list, dirs = dirs }
    end

    -- The project's photos (.heic, .jpg) with a link to their clip, wherever they are in its folder: { file, link }.
    -- Read through fs.read_photo_link; cache as scan's. Only the project's own folders and what isn't footage are looked
    -- at (a project next to its footage has no photos among it)
    function self.photos(project, cache)
        local out = {}
        local footage_dirs = {}
        for _, l in ipairs(fs.locations_of and fs.locations_of(project) or {}) do
            if l.dir and (same(l.dir, project) or ws.relative(l.dir, project)) then footage_dirs[#footage_dirs + 1] = l.dir end
        end
        local function among_footage(dir)
            for _, d in ipairs(footage_dirs) do if same(dir, d) or ws.relative(dir, d) then return true end end
            return false
        end
        local function walk(dir, depth, own)
            local files, subdirs = entries(dir)
            if own or not among_footage(dir) then
                for _, e in ipairs(files) do
                    local lower = e.name:lower()
                    if lower:match("%.heic$") or lower:match("%.jpe?g$") then
                        local file, fi = ws.join(dir, e.name), e.info
                        local key = "photo\t" .. file
                        local c = cache and cache[key]
                        local raw
                        if c and c.size == fi.size and c.mtime == fi.mtime then raw = c.raw
                        else
                            raw = fs.read_photo_link and fs.read_photo_link(file) or false
                            if cache then cache[key] = { size = fi.size, mtime = fi.mtime, raw = raw } end
                        end
                        local l = raw and fs.parse_json(raw)
                        if type(l) == "table" and type(l.clip) == "string" then out[#out + 1] = { file = file, link = l } end
                    end
                end
            end
            if depth < 8 then
                for _, name in ipairs(subdirs) do
                    if name:sub(1, 1) ~= "#" then walk(ws.join(dir, name), depth + 1, own or (depth == 0 and OWN_FOLDERS[name:lower()] == true)) end
                end
            end
        end
        walk(project, 0, false)
        return out
    end

    -- Whether a link names this clip ({ relative, size, created, hash }), by what the clip is (docs/library-
    -- management.md §8): the same fingerprint where both have one; else the same size and creation time; a link
    -- with neither (older ones), its path and size
    function self.names_clip(l, clip)
        if type(l.hash) == "string" and type(clip.hash) == "string" then return l.hash == clip.hash end
        if l.size ~= nil and clip.size ~= nil and l.size ~= clip.size then return false end
        local function secs(x) if type(x) == "number" then return x end return (M.parse_time(x)) end
        local a, b = secs(l.created), secs(clip.created)
        if a and b and l.size ~= nil and clip.size ~= nil then return math.floor(a) == math.floor(b) end
        if a and b and math.floor(a) ~= math.floor(b) then return false end
        return type(l.clip) == "string" and clip.relative ~= nil and same(l.clip, clip.relative)
    end

    -- The moments of a clip (names_clip), in the order of their in
    function self.of_clip(list, clip)
        local out = {}
        for _, m in ipairs(list) do
            local l = m.link
            if self.names_clip(l, clip) and type(l["in"]) == "number" and type(l.out) == "number" then
                out[#out + 1] = m
            end
        end
        table.sort(out, function(a, b)
            if a.link["in"] ~= b.link["in"] then return a.link["in"] < b.link["in"] end
            return a.name < b.name
        end)
        return out
    end

    -- The project a moment's, clip preview's or export's file is in: the nearest folder above it with the project's
    -- file (or the state folder of before), wherever in it the file was put. nil: none (copied out, to the Desktop)
    function self.project_of(file)
        local dir = ws.parent(file)
        while dir do
            if fs.is_file(ws.join(dir, workspace_lib.PROJECT_FILE)) or fs.is_dir(ws.join(dir, workspace_lib.LEGACY_DIR)) then return dir end
            if ws.is_volume_root and ws.is_volume_root(dir) then return nil end
            dir = ws.parent(dir)
        end
        return nil
    end

    -- The project's footage (self.footage), kept a minute: a search for each of a run's moments lists it once
    local footage_memo = {}
    local function footage_of(project)
        local m = footage_memo[project]
        local now = os.time()
        if not m or now - m.t > 60 then m = { t = now, list = self.footage(project) }; footage_memo[project] = m end
        return m.list
    end
    function self.forget_footage() footage_memo = {} end

    -- What identifies a link's clip, for this computer's memory of where it was found: its fingerprint, else its
    -- size and creation time
    local function identity(link)
        if type(link.hash) == "string" and link.hash ~= "" then return link.hash end
        return tostring(link.size) .. "|" .. tostring(M.parse_time(link.created))
    end

    local function last_part(p) return (p:match("([^/\\]+)$") or p) end

    -- The clip a link names (docs/library-management.md §8): found by what the clip is, its path in the tree only a
    -- hint of where it was last seen.
    --   1. the hint: the file at link.clip in the project's tree, through its locations in order, of the link's size;
    --   2. where this computer last found it (fs.found_get, by the clip's identity), if it's still there;
    --   3. the project's footage searched, the files of the link's size only (sources never change): with the
    --      link's fingerprint, the one whose fingerprint (fs.fingerprint) is it, those of its name first; without,
    --      the one match by its creation time (fs.created; with "I never rename my footage", fs.never_rename, by
    --      name and creation time first), else by its name: never by size alone, which would be a guess;
    --   4. a moment copied out of its project: its path under each of the roots it was made with.
    -- -> the file, its path in the tree now, its location, and how it was found ("hint", "cache", "search", "roots")
    function self.original(link, project)
        if type(link) ~= "table" or type(link.clip) ~= "string" or link.clip == "" then return nil end
        local locs = project and ws.files_at and (fs.locations_of and fs.locations_of(project) or ws.locations(project)) or nil
        local function sized(file)
            local info = fs.info(file)
            return info and info.is_file and (link.size == nil or info.size == link.size)
        end
        if locs then
            for _, c in ipairs(ws.files_at(link.clip, locs)) do
                if sized(c.file) then return c.file, link.clip, c.location, "hint" end
            end
            local key = identity(link)
            local seen = fs.found_get and fs.found_get(project, key)
            if seen and seen ~= link.clip then
                for _, c in ipairs(ws.files_at(seen, locs)) do
                    if sized(c.file) then return c.file, seen, c.location, "cache" end
                end
            end
            if type(link.size) == "number" then
                local name = last_part(link.clip)
                local cands = {}
                for _, f in ipairs(footage_of(project)) do
                    if f.size == link.size then cands[#cands + 1] = f end
                end
                local function found(f)
                    if fs.found_set then fs.found_set(project, key, f.relative) end
                    return f.file, f.relative, f.location, "search"
                end
                if #cands > 0 then
                    if type(link.hash) == "string" and link.hash ~= "" and fs.fingerprint then
                        table.sort(cands, function(a, b)
                            local na, nb = same(last_part(a.relative), name), same(last_part(b.relative), name)
                            if na ~= nb then return na end
                            return a.relative < b.relative
                        end)
                        for _, f in ipairs(cands) do
                            if fs.fingerprint(f.file) == link.hash then return found(f) end
                        end
                    else
                        local want = M.parse_time(link.created)
                        local function created(f)
                            if f.created == nil then f.created = fs.created and fs.created(f.file) or false end
                            return f.created
                        end
                        -- Anton's steps (checks.rs's find_clip): name, size and creation time; size and creation
                        -- time (not with "I never rename my footage"); name and size. Never size alone
                        local tests = {}
                        local function at(f) return created(f) and math.floor(created(f)) == math.floor(want) end
                        if want then tests[#tests + 1] = { function(f) return same(last_part(f.relative), name) and at(f) end, true } end
                        if want and not (fs.never_rename and fs.never_rename(project)) then tests[#tests + 1] = { at } end
                        tests[#tests + 1] = { function(f) return same(last_part(f.relative), name) end }
                        for _, t in ipairs(tests) do
                            local hit = {}
                            for _, f in ipairs(cands) do if t[1](f) then hit[#hit + 1] = f end end
                            if #hit == 1 then return found(hit[1]) end
                            -- Several of that name and time go on to the next step; several of a looser one: no find
                            if #hit > 1 and not t[2] then break end
                        end
                    end
                end
            end
        end
        local rel = windows and link.clip:gsub("/", "\\") or link.clip
        local places = {}
        if project then places[#places + 1] = project end
        if type(link.roots) == "table" then
            for _, r in ipairs(link.roots) do if type(r) == "string" and r ~= "" then places[#places + 1] = r end end
        end
        for _, place in ipairs(places) do
            local file = ws.join(place, rel)
            if sized(file) then return file, nil, nil, "roots" end
        end
        return nil
    end

    -- The moments in the order they happened (their original's creation time plus in), else by name
    function self.by_time(list)
        local out = {}
        for i, m in ipairs(list) do out[i] = m end
        table.sort(out, function(a, b)
            local ta, tb = M.moment_time(a.link), M.moment_time(b.link)
            if ta and tb and ta ~= tb then return ta < tb end
            if (ta ~= nil) ~= (tb ~= nil) then return ta ~= nil end
            return a.file < b.file
        end)
        return out
    end

    -- The project's clips: its videos, but the player's own folders, hidden ones and link files. { file, relative,
    -- size, mtime, loc (its location's name), loc_rel (its path in that location) }
    function self.footage(project)
        local out = {}
        local seen = {}
        local loc, at, li, cur = nil, "", 0, nil
        local function walk(dir, rel, depth)
            local files, subdirs = entries(dir)
            for _, e in ipairs(files) do
                local name = e.name
                local lower = name:lower()
                local ext = lower:match("%.([^.]+)$")
                if ext and M.FOOTAGE[ext] and lower:sub(-#M.EXT) ~= M.EXT then
                    local r = rel and (rel .. "/" .. name) or name
                    if not seen[r] then
                        seen[r] = true
                        local in_loc = (at ~= "" and r:sub(1, #at + 1) == at .. "/") and r:sub(#at + 2) or r
                        out[#out + 1] = { file = ws.join(dir, name), relative = r, size = e.info.size, mtime = e.info.mtime, loc = loc, li = li, loc_rel = in_loc, location = cur }
                    end
                end
            end
            if depth >= 8 then return end
            for _, name in ipairs(subdirs) do
                -- A project's own folders at the top of a location aren't footage (this project's, or the one there before)
                if not (depth == 0 and OWN_FOLDERS[name:lower()]) then
                    walk(ws.join(dir, name), rel and (rel .. "/" .. name) or name, depth + 1)
                end
            end
        end
        -- Every location of the project, in its order (a path the first one has wins), each at its place in the tree
        local locs = ws.locations and (fs.locations_of and fs.locations_of(project) or ws.locations(project)) or { { dir = project, at = "" } }
        for i, l in ipairs(locs) do
            li, cur = i, l
            if l.dir then
                loc, at = (l.name and l.name ~= "") and l.name or (l.dir:match("([^/\\]+)[/\\]*$") or "Location"), l.at or ""
                walk(l.dir, l.at ~= "" and l.at or nil, 0)
            end
        end
        table.sort(out, function(a, b) return a.relative < b.relative end)
        return out
    end

    return self
end

return M
