-- Tags and the rating (docs/moments-and-workspaces.md, "Rating and tags"; docs/library-management.md §7): words on a
-- clip, a moment or an export, and 1 to 5 stars, kept with the item in the library, the same place the Library keeps
-- them (library/src/tags.rs is this file's twin):
--   a clip        its own file, <clip>.afp.json beside it (sidecar.lua): `tags` (a list), `rating` (1–5)
--   a moment, an export, a clip preview's own   its link (the MP4 key, written in place by afp-cut --set-key):
--                 `tags`, `rating`. A clip preview playing tags its clip: the clip is what it shows
-- A tag is a name, compared without case ("Turtle" is "turtle"); the first spelling stays.
--
-- macOS Finder tags are written beside, so that Finder shows them and Spotlight finds them (a local disk; Spotlight
-- doesn't index a network volume, Finder still shows them): the xattr com.apple.metadata:_kMDItemUserTags, a binary
-- property list of strings ("Name", or "Name\n6" with Finder's colour). The library's own are the truth; Finder's
-- follow them, and a Finder tag set by hand that the library doesn't have is left alone. Which files get them is this
-- computer's choice (the Library's Project settings, On this Mac: `finder_tags` in its projects/<id>.json):
--   own   (default) the project's own files: moments, exports, clip previews (with their clip's tags), photos
--   all   the originals too: an attribute, never the video's bytes, but it's written on the footage
--   off   none
-- Windows and Linux: the library's own only.
--
-- Plain Lua, no mpv (tests/tags_test.lua).

local M = {}

M.FINDER_XATTR = "com.apple.metadata:_kMDItemUserTags"
M.RECENT_MAX = 12
M.MAX_RATING = 5

-- A tag as typed: trimmed, spaces folded; nil when nothing's left
function M.clean(name)
    if type(name) ~= "string" then return nil end
    name = name:gsub("[%c]", " "):gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "")
    return name ~= "" and name or nil
end

local function same(a, b) return a:lower() == b:lower() end

-- The tag in the list (its spelling there), or nil
function M.find(list, name)
    for i, t in ipairs(list or {}) do if type(t) == "string" and same(t, name) then return i, t end end
    return nil
end

-- A list from what a file holds (anything that isn't a list of strings: none)
function M.list(v)
    local out = {}
    if type(v) ~= "table" then return out end
    for _, t in ipairs(v) do
        t = M.clean(t)
        if t and not M.find(out, t) then out[#out + 1] = t end
    end
    return out
end

-- The list with `add` added (at the end, unless there in any case) and `remove` taken out: a new list
function M.changed(list, add, remove)
    local out = M.list(list)
    for _, a in ipairs(add or {}) do
        a = M.clean(a)
        if a and not M.find(out, a) then out[#out + 1] = a end
    end
    for _, r in ipairs(remove or {}) do
        local i = M.clean(r) and M.find(out, M.clean(r))
        if i then table.remove(out, i) end
    end
    return out
end

-- On if it's off, off if it's on: the new list and whether it's on now
function M.toggle(list, name)
    name = M.clean(name)
    if not name then return M.list(list), false end
    if M.find(list, name) then return M.changed(list, nil, { name }), false end
    return M.changed(list, { name }), true
end

-- `from` renamed `to` where it is (merged when `to` is there already: one of them left, where `to` was); the new list
-- and whether it changed. A change of case only renames too
function M.rename(list, from, to)
    from, to = M.clean(from), M.clean(to)
    local out = M.list(list)
    if not from or not to then return out, false end
    local i = M.find(out, from)
    if not i then return out, false end
    local j = M.find(out, to)
    if j and j ~= i then
        table.remove(out, i)
    elseif out[i] == to then
        return out, false
    else
        out[i] = to
    end
    return out, true
end

-- A rating as kept: 1–5, else nil (none)
function M.rating(v)
    v = tonumber(v)
    if not v then return nil end
    v = math.floor(v + 0.5)
    if v < 1 then return nil end
    return math.min(v, M.MAX_RATING)
end

-- What was typed at the tag prompt (K): tags and stars, separated by commas.
--   dolphin        on if it's off, off if it's on
--   +dolphin       on;  -dolphin  off
--   *** or *3      three stars;  *0 (or a lone *): no stars
-- -> { toggle = {...}, add = {...}, remove = {...}, rating = n | 0 (cleared) | nil (unchanged) }
function M.parse(text)
    local r = { toggle = {}, add = {}, remove = {} }
    for part in ((text or "") .. ","):gmatch("([^,]*),") do
        local p = part:gsub("^%s+", ""):gsub("%s+$", "")
        local rest = p:gsub("★", ""):gsub("%*", "")
        local stars = p ~= "" and p ~= "*" and rest == ""
        local n = p:match("^%*%s*(%d)$") or p:match("^(%d)%s*%*$")
        if n then
            r.rating = math.min(tonumber(n), M.MAX_RATING)
        elseif p == "*" then
            r.rating = 0
        elseif stars then
            local count = 0
            for _ in p:gmatch("[%*]") do count = count + 1 end
            for _ in p:gmatch("★") do count = count + 1 end
            r.rating = math.min(count, M.MAX_RATING)
        elseif p:sub(1, 1) == "+" and M.clean(p:sub(2)) then
            r.add[#r.add + 1] = M.clean(p:sub(2))
        elseif p:sub(1, 1) == "-" and M.clean(p:sub(2)) then
            r.remove[#r.remove + 1] = M.clean(p:sub(2))
        elseif M.clean(p) then
            r.toggle[#r.toggle + 1] = M.clean(p)
        end
    end
    return r
end

-- A tags-and-rating doc (a sidecar, a link) changed by what was parsed: true when anything changed. `doc.tags` is
-- left out when empty, `doc.rating` when none
function M.apply(doc, op)
    local before = table.concat(M.list(doc.tags), "\n") .. "|" .. tostring(M.rating(doc.rating))
    local list = M.list(doc.tags)
    for _, t in ipairs(op.toggle or {}) do list = M.toggle(list, t) end
    list = M.changed(list, op.add, op.remove)
    if op.rename then list = M.rename(list, op.rename[1], op.rename[2]) end
    doc.tags = #list > 0 and list or nil
    if op.rating ~= nil then doc.rating = M.rating(op.rating) else doc.rating = M.rating(doc.rating) end
    return before ~= table.concat(list, "\n") .. "|" .. tostring(doc.rating)
end

-- The tags used last (most recent first): `used` put at the front, at most RECENT_MAX
function M.recent(list, used)
    local out = {}
    for _, t in ipairs(used or {}) do t = M.clean(t); if t and not M.find(out, t) then out[#out + 1] = t end end
    for _, t in ipairs(M.list(list)) do if not M.find(out, t) then out[#out + 1] = t end end
    while #out > M.RECENT_MAX do table.remove(out) end
    return out
end

-- ---------- Finder tags: a binary property list of strings ----------

-- UTF-8 -> code points; nil when it isn't UTF-8
local function code_points(s)
    local out, i = {}, 1
    while i <= #s do
        local c = s:byte(i)
        local n, cp
        if c < 0x80 then n, cp = 1, c
        elseif c >= 0xC2 and c < 0xE0 then n, cp = 2, c - 0xC0
        elseif c >= 0xE0 and c < 0xF0 then n, cp = 3, c - 0xE0
        elseif c >= 0xF0 and c < 0xF5 then n, cp = 4, c - 0xF0
        else return nil end
        for k = 1, n - 1 do
            local d = s:byte(i + k)
            if not d or d < 0x80 or d > 0xBF then return nil end
            cp = cp * 64 + (d - 0x80)
        end
        out[#out + 1] = cp
        i = i + n
    end
    return out
end

local function utf8_char(cp)
    if cp < 0x80 then return string.char(cp) end
    if cp < 0x800 then return string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64) end
    if cp < 0x10000 then return string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
    return string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
end

-- A big-endian unsigned integer of n bytes
local function be(v, n)
    local b = {}
    for k = n, 1, -1 do b[k] = string.char(v % 256); v = math.floor(v / 256) end
    return table.concat(b)
end

local function bytes_for(v)
    if v < 256 then return 1 elseif v < 65536 then return 2 elseif v < 4294967296 then return 4 end
    return 8
end

-- An object's marker with its count (counts of 15 or more follow as an integer object)
local function marker(kind, count)
    if count < 15 then return string.char(kind * 16 + count) end
    local n = bytes_for(count)
    local p = ({ [1] = 0, [2] = 1, [4] = 2, [8] = 3 })[n]
    return string.char(kind * 16 + 15) .. string.char(0x10 + p) .. be(count, n)
end

local function encode_string(s)
    local cps = code_points(s) or {}
    local ascii = true
    for _, cp in ipairs(cps) do if cp > 127 then ascii = false end end
    if ascii then return marker(5, #s) .. s end
    local units = {}
    for _, cp in ipairs(cps) do
        if cp >= 0x10000 then
            cp = cp - 0x10000
            units[#units + 1] = be(0xD800 + math.floor(cp / 1024), 2)
            units[#units + 1] = be(0xDC00 + cp % 1024, 2)
        else
            units[#units + 1] = be(cp, 2)
        end
    end
    return marker(6, #units) .. table.concat(units)
end

-- Finder's tags (strings: "Name" or "Name\n<colour>") as the xattr's bytes
function M.finder_plist(list)
    local n = #list + 1
    local ref = n < 256 and 1 or 2
    local objects = {}
    local refs = {}
    for i = 1, #list do refs[#refs + 1] = be(i, ref) end
    objects[1] = marker(10, #list) .. table.concat(refs)
    for i, s in ipairs(list) do objects[i + 1] = encode_string(s) end
    local out, offsets, pos = { "bplist00" }, {}, 8
    for i, o in ipairs(objects) do offsets[i] = pos; out[#out + 1] = o; pos = pos + #o end
    local osize = bytes_for(pos)
    for _, off in ipairs(offsets) do out[#out + 1] = be(off, osize) end
    out[#out + 1] = string.rep("\0", 6) .. string.char(osize, ref) .. be(n, 8) .. be(0, 8) .. be(pos, 8)
    return table.concat(out)
end

local function read_be(s, i, n)
    local v = 0
    for k = 0, n - 1 do v = v * 256 + (s:byte(i + k) or 0) end
    return v
end

-- The xattr's bytes as Finder's strings; nil when it isn't a property list of strings this reads
function M.finder_parse(data)
    if type(data) ~= "string" or #data < 40 or data:sub(1, 8) ~= "bplist00" then return nil end
    local t = #data - 31
    local osize, ref = data:byte(t + 6), data:byte(t + 7)
    local count, top, table_at = read_be(data, t + 8, 8), read_be(data, t + 16, 8), read_be(data, t + 24, 8)
    local function offset(i) return read_be(data, table_at + 1 + i * osize, osize) end
    local function head(at)
        local m = data:byte(at + 1)
        if not m then return nil end
        local kind, c, start = math.floor(m / 16), m % 16, at + 2
        if c == 15 then
            local im = data:byte(at + 2)
            local n = 2 ^ (im % 16)
            c = read_be(data, at + 3, n)
            start = at + 3 + n
        end
        return kind, c, start
    end
    if top >= count then return nil end
    local kind, c, start = head(offset(top))
    if kind ~= 10 then return nil end
    local out = {}
    for k = 0, c - 1 do
        local o = read_be(data, start + k * ref, ref)
        if o >= count then return nil end
        local sk, sc, ss = head(offset(o))
        if sk == 5 then
            out[#out + 1] = data:sub(ss, ss + sc - 1)
        elseif sk == 6 then
            local chars, k2 = {}, 0
            while k2 < sc do
                local u = read_be(data, ss + k2 * 2, 2)
                if u >= 0xD800 and u < 0xDC00 and k2 + 1 < sc then
                    local l = read_be(data, ss + (k2 + 1) * 2, 2)
                    u = 0x10000 + (u - 0xD800) * 1024 + (l - 0xDC00)
                    k2 = k2 + 1
                end
                chars[#chars + 1] = utf8_char(u)
                k2 = k2 + 1
            end
            out[#out + 1] = table.concat(chars)
        else
            return nil
        end
    end
    return out
end

-- The bytes as hex (xattr -wx), and back (xattr -px prints them so, spaces and lines between)
function M.hex(s) return (s:gsub(".", function(c) return string.format("%02X", c:byte()) end)) end
function M.unhex(h)
    h = (h or ""):gsub("%s", "")
    if #h % 2 == 1 or h:find("[^%x]") then return nil end
    return (h:gsub("%x%x", function(x) return string.char(tonumber(x, 16)) end))
end

-- A Finder tag's name ("Name\n6" -> "Name")
local function finder_name(s) return (s:match("^(.-)\n%d+$") or s) end

-- Finder's tags made to follow the library's: `removed` taken off, `list` all there (a colour kept for one there
-- already), the others (set by hand in Finder) left. nil when nothing changes
function M.finder_follow(finder, list, removed)
    local out, changed = {}, false
    for _, f in ipairs(finder or {}) do
        local name = finder_name(f)
        if M.find(removed or {}, name) and not M.find(list, name) then changed = true else out[#out + 1] = f end
    end
    for _, t in ipairs(list or {}) do
        local there = false
        for _, f in ipairs(out) do if same(finder_name(f), t) then there = true end end
        if not there then out[#out + 1] = t; changed = true end
    end
    return changed and out or nil
end

return M
