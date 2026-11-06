-- Moments (docs/moments-and-workspaces.md): parts of a clip kept as small MP4s in the project's _Moments/ folder,
-- `<name>.afplink.mp4`, each with the link to its original inside (link.lua reads it). This is the part with no mpv
-- in it, so that luajit tests it (tests/moments_test.lua): names, dates, which clip a moment is of, where its
-- original is, and the _Moments/ folder read.
--
--   local moments = require('moments').new(ws, fs)   -- ws: workspace.lua's; fs: is_dir, is_file, info, readdir,
--                                                    -- read_link (link.lua's read), parse_json
--   moments.scan(project, cache)       -> { list = { {file, name, link, raw}... }, dirs = { [dir] = mtime } }
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
-- Clip previews: _Clips/<the clip's path>.afplink.mp4
M.CLIPS = "_Clips"
-- What a project's footage is: videos, not the player's own folders
M.FOOTAGE = { mp4 = true, mov = true }
-- (and the photos' folder, whose Live Photos' .mov files aren't footage either)
local OWN_FOLDERS = { _moments = true, _clips = true, _exports = true, _stills = true, moments = true, clips = true, exports = true,
                      stabilized = true }

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

    -- Every .mp4 under <project>/_Moments/ (or `folder`), subfolders included, with its link (of `kind`: moment, or
    -- clip for _Clips/). cache[file] = { size, mtime, raw } keeps what was read of a file, read again only when its
    -- size or time changes. dirs: each folder's modification time, which a file added, removed or renamed in it
    -- changes. A link file renamed or moved is found all the same: by what's inside it
    function self.scan(project, cache, folder, kind)
        kind = kind or "moment"
        local root = ws.join(project, folder or M.FOLDER)
        local list, dirs = {}, {}
        local function walk(dir, depth)
            local info = fs.info(dir)
            if not info or not info.is_dir then return end
            dirs[dir] = info.mtime
            for _, name in ipairs(fs.readdir(dir, "files") or {}) do
                if name:lower():match("%.mp4$") and name:sub(1, 1) ~= "." then
                    local file = ws.join(dir, name)
                    local fi = fs.info(file)
                    if fi then
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
                        end
                    end
                end
            end
            if depth < 8 then
                for _, name in ipairs(fs.readdir(dir, "dirs") or {}) do
                    if name:sub(1, 1) ~= "." then walk(ws.join(dir, name), depth + 1) end
                end
            end
        end
        walk(root, 0)
        return { list = list, dirs = dirs }
    end

    -- The moments of a clip: its path in the project, and its size and creation time where the link has them, in
    -- the order of their in
    function self.of_clip(list, clip)
        local out = {}
        for _, m in ipairs(list) do
            local l = m.link
            if type(l.clip) == "string" and clip.relative and same(l.clip, clip.relative)
               and (l.size == nil or clip.size == nil or l.size == clip.size)
               and (l.created == nil or clip.created == nil or M.parse_time(l.created) == M.parse_time(clip.created))
               and type(l["in"]) == "number" and type(l.out) == "number" then
                out[#out + 1] = m
            end
        end
        table.sort(out, function(a, b)
            if a.link["in"] ~= b.link["in"] then return a.link["in"] < b.link["in"] end
            return a.name < b.name
        end)
        return out
    end

    -- The project a moment's file is in: the folder above the _Moments/ it's in (subfolders too), with the
    -- project's file (or the state folder of before) in it; an export's, `folder` Exports. nil: none (a moment copied out, to the Desktop say)
    function self.project_of(file, folder)
        folder = (folder or M.FOLDER):lower()
        local dir = ws.parent(file)
        while dir do
            local up = ws.parent(dir)
            if not up then return nil end
            if ws.name(dir):lower() == folder and (fs.is_file(ws.join(up, workspace_lib.PROJECT_FILE))
                                                   or fs.is_dir(ws.join(up, workspace_lib.LEGACY_DIR))) then return up end
            dir = up
        end
        return nil
    end

    -- Where the original is: the clip's path under the project, then under each of the roots it was made with.
    -- Its size must match where the link has it
    function self.original(link, project)
        if type(link) ~= "table" or type(link.clip) ~= "string" or link.clip == "" then return nil end
        local rel = windows and link.clip:gsub("/", "\\") or link.clip
        local places = {}
        if project then places[#places + 1] = project end
        if type(link.roots) == "table" then
            for _, r in ipairs(link.roots) do if type(r) == "string" and r ~= "" then places[#places + 1] = r end end
        end
        for _, place in ipairs(places) do
            local file = ws.join(place, rel)
            local info = fs.info(file)
            if info and info.is_file and (link.size == nil or info.size == link.size) then return file end
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

    -- The project's clips: its videos, but the player's own folders, hidden ones and link files. { file, relative }
    function self.footage(project)
        local out = {}
        local function walk(dir, rel, depth)
            for _, name in ipairs(fs.readdir(dir, "files") or {}) do
                local lower = name:lower()
                local ext = lower:match("%.([^.]+)$")
                if ext and M.FOOTAGE[ext] and name:sub(1, 1) ~= "." and lower:sub(-#M.EXT) ~= M.EXT then
                    out[#out + 1] = { file = ws.join(dir, name), relative = rel and (rel .. "/" .. name) or name }
                end
            end
            if depth >= 8 then return end
            for _, name in ipairs(fs.readdir(dir, "dirs") or {}) do
                if name:sub(1, 1) ~= "." and not (depth == 0 and OWN_FOLDERS[name:lower()]) then
                    walk(ws.join(dir, name), rel and (rel .. "/" .. name) or name, depth + 1)
                end
            end
        end
        walk(project, nil, 0)
        table.sort(out, function(a, b) return a.relative < b.relative end)
        return out
    end

    return self
end

return M
