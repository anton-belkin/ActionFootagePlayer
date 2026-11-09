-- The navigation bar's reckoning, apart from its drawing (bar.lua) so it can be tested without mpv
-- (tests/timeline_test.lua): times as text, the buttons that fit a width, where a time is on the track and back,
-- what's on the track (the marks, the moments, the A–B part, the file's own chapters), the mark the mouse is near,
-- and the playlist's rows. Plain Lua (5.1/LuaJIT).

local M = {}

-- A time as text: "1:23.45" (hundredths: precise enough to land on a frame at 60 fps and tell it), "1:02:03.45"
-- past an hour; places=0 leaves out the fraction ("1:23")
function M.clock(t, places)
    if not t or t ~= t then return "–:––" end
    places = places or 2
    local neg = t < 0
    if neg then t = -t end
    local scale = 10 ^ places
    -- Rounded once, at the precision shown, so 59.999 reads 1:00.00 and not 0:60.00
    local total = math.floor(t * scale + 0.5)
    local whole = math.floor(total / scale)
    local frac = total - whole * scale
    local h, m, s = math.floor(whole / 3600), math.floor(whole % 3600 / 60), whole % 60
    local out = h > 0 and string.format("%d:%02d:%02d", h, m, s) or string.format("%d:%02d", m, s)
    if places > 0 then out = out .. string.format(".%0" .. places .. "d", frac) end
    return (neg and "-" or "") .. out
end

-- The buttons that fit: items { id, w (pixels, its gap included), keep (higher stays longer) }, in their order. The
-- least kept go first, the later of equals first; the order of what's left is kept
function M.fit(items, width)
    local shown, total = {}, 0
    for i, it in ipairs(items) do shown[i] = true; total = total + it.w end
    while total > width do
        local worst = nil
        for i, it in ipairs(items) do
            if shown[i] and (not worst or it.keep <= items[worst].keep) then worst = i end
        end
        if not worst or items[worst].keep >= math.huge then break end
        shown[worst] = false
        total = total - items[worst].w
    end
    local out = {}
    for i, it in ipairs(items) do if shown[i] then out[#out + 1] = it end end
    return out, total
end

-- The track from x0 to x1 for a clip of duration seconds: a time's place, and the time at a place (within the clip)
function M.x_of(t, x0, x1, duration)
    if not duration or duration <= 0 then return x0 end
    return x0 + (x1 - x0) * math.min(1, math.max(0, t / duration))
end

function M.t_of(x, x0, x1, duration)
    if not duration or duration <= 0 or x1 <= x0 then return 0 end
    return duration * math.min(1, math.max(0, (x - x0) / (x1 - x0)))
end

-- What's on the track, from the plugins' state:
--   highlights   user-data/highlights: marks { t, camera, name } in the playing file's time
--   workspace    user-data/workspace: moments { name, in, out } in the clip's time; offset (a moment playing as a video
--                of its own: its marks are already in its time, and the clip's moments aren't on its track)
--   chapters     chapter-list: what's left after the marks and the moments (which are chapters too) is the file's own
-- Returns { marks = { t, kind ("camera", "own"), title }, moments = { a, b, title }, chapters = { t, title } }
local function near(a, b) return math.abs(a - b) < 0.01 end

function M.track_items(highlights, workspace, chapters)
    local out = { marks = {}, moments = {}, chapters = {} }
    local taken = {}
    if type(highlights) == "table" and type(highlights.marks) == "table" then
        for i, m in ipairs(highlights.marks) do
            if type(m) == "table" and type(m.t) == "number" then
                local title = (m.name and m.name ~= "") and m.name or ((m.camera and "Highlight " or "Mark ") .. i)
                out.marks[#out.marks + 1] = { t = m.t, kind = m.camera and "camera" or "own", title = title }
                taken[#taken + 1] = m.t
            end
        end
    end
    if type(workspace) == "table" and type(workspace.moments) == "table" and not workspace.offset then
        for _, m in ipairs(workspace.moments) do
            if type(m) == "table" and type(m["in"]) == "number" and type(m.out) == "number" then
                out.moments[#out.moments + 1] = { a = m["in"], b = m.out, title = m.name or "Moment" }
                taken[#taken + 1] = m["in"]
                taken[#taken + 1] = m.out
            end
        end
    end
    for _, c in ipairs(type(chapters) == "table" and chapters or {}) do
        if type(c) == "table" and type(c.time) == "number" then
            local ours = false
            for _, t in ipairs(taken) do if near(t, c.time) then ours = true; break end end
            if not ours then out.chapters[#out.chapters + 1] = { t = c.time, title = c.title } end
        end
    end
    table.sort(out.marks, function(a, b) return a.t < b.t end)
    return out
end

-- The point the mouse at x is nearest to, within reach pixels: the marks first, then the moments' ends, the A–B
-- part's ends, the chapters. Returns { t, title } or nil
function M.snap(x, items, ab_a, ab_b, x0, x1, duration, reach)
    local best, best_d = nil, reach
    local function try(t, title)
        if type(t) ~= "number" then return end
        local d = math.abs(M.x_of(t, x0, x1, duration) - x)
        if d <= best_d then best, best_d = { t = t, title = title }, d end
    end
    -- The later kinds win only when nearer: the marks are tried last, with <=
    for _, c in ipairs(items.chapters) do try(c.t, c.title) end
    try(ab_a, "Part start")
    try(ab_b, "Part end")
    for _, m in ipairs(items.moments) do try(m.a, m.title); try(m.b, m.title .. " end") end
    for _, m in ipairs(items.marks) do try(m.t, m.title) end
    return best
end

-- The A–B button's state, from ab-loop-a and ab-loop-b ("no" or a number): "none", "a" (its start set), "ab"
function M.ab_state(a, b)
    a, b = tonumber(a), tonumber(b)
    if a and b then return "ab" end
    if a or b then return "a" end
    return "none"
end

-- The playlist's rows: { index (0-based, for playlist-play-index), name (the file's name, without its extension),
-- current }, from mpv's playlist property. exts (a set, lowercase: mpv's video-exts): only the videos, not the folders
-- and other files mpv put in it beside them (the one playing is always there)
function M.playlist_rows(playlist, exts)
    local rows = {}
    for i, e in ipairs(type(playlist) == "table" and playlist or {}) do
        local file = e.filename or ""
        local base = file:match("([^/\\]+)[/\\]*$") or file
        local stem, ext = base:match("^(.+)%.([^.]+)$")
        local current = e.current == true or e.playing == true
        if current or not exts or (ext and exts[ext:lower()]) then
            local name = (e.title and e.title ~= "") and e.title or stem or base
            rows[#rows + 1] = { index = i - 1, name = name, current = current }
        end
    end
    return rows
end

-- A set of the extensions in a list ("mp4", ...), lowercase
function M.set_of(list)
    local set = {}
    for _, x in ipairs(type(list) == "table" and list or {}) do set[tostring(x):lower()] = true end
    return set
end

-- A text cut in its middle to n characters (UTF-8), with an ellipsis: the ends of a camera's file names are what tell
-- them apart (DJI_2026102215…0068_D)
function M.cut_middle(text, n)
    local chars = {}
    for ch in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do chars[#chars + 1] = ch end
    if #chars <= n then return text end
    local tail = math.floor((n - 1) / 2)
    local head = n - 1 - tail
    return table.concat(chars, "", 1, head) .. "…" .. table.concat(chars, "", #chars - tail + 1, #chars)
end

-- The folder the playlist's files are in, when they're all in one: its path, else nil
function M.common_folder(playlist)
    local dir = nil
    for _, e in ipairs(type(playlist) == "table" and playlist or {}) do
        local d = (e.filename or ""):match("^(.*)[/\\][^/\\]+$")
        if not d then return nil end
        if dir and d ~= dir then return nil end
        dir = d
    end
    return dir
end

return M
