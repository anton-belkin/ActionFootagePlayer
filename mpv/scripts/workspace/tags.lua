-- Tags and culling (docs/tags-and-culling.md): on an original, a moment or a still (never a clip preview or an
-- export), one model of three parts, kept with the item in the library (library/src/tags.rs is this file's twin):
--   pick     "accept" or "reject" (one or the other), absent: neither          P, X, U
--   rating   1–5 stars, absent: none                                           1–5, 0
--   tags     custom words, compared without case, the first spelling kept      K
-- The library's own store is the truth: an original's own file (<clip>.afp.json, sidecar.lua), a moment's link (the
-- MP4 key, written in place by afp-cut --set-key). Stores the library enables besides (Project.afproject's
-- `tag_stores`: "finder" for now) are synced both ways, three-way: the library now, the store now, and what they
-- last agreed on (`synced.<store>` in the item's own record; absent: empty). Custom tags: the changes of both sides
-- (union of additions and of removals); pick and rating: the side that changed, both: the last writer.
--
-- macOS Finder tags: the xattr com.apple.metadata:_kMDItemUserTags, a binary property list of strings ("Name", or
-- "Name\n6" with Finder's colour). Accept is "Accept" (green), Reject "Reject" (red), a rating "★★★"; the rest are the
-- custom tags. A Finder tag's colour is kept.
--
-- Plain Lua, no mpv (tests/tags_test.lua).

local M = {}

M.FINDER_XATTR = "com.apple.metadata:_kMDItemUserTags"
M.RECENT_MAX = 12
M.MAX_RATING = 5
M.ACCEPT, M.REJECT = "Accept", "Reject"
-- Finder's colours: 2 green, 6 red
M.FINDER_ACCEPT, M.FINDER_REJECT = "Accept\n2", "Reject\n6"

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

-- A pick as kept: "accept", "reject", else nil (neither)
function M.pick(v)
    if type(v) ~= "string" then return nil end
    v = v:lower()
    return (v == "accept" or v == "reject") and v or nil
end

-- An item's model from its record (a sidecar, a link): { pick, rating, tags }
function M.model(doc)
    doc = type(doc) == "table" and doc or {}
    return { pick = M.pick(doc.pick), rating = M.rating(doc.rating), tags = M.list(doc.tags) }
end

local function key(m) return tostring(m.pick) .. "|" .. tostring(m.rating) .. "|" .. table.concat(m.tags, "\n") end
function M.same(a, b) return key(a) == key(b) end

-- The model written into a record: keys out when empty
function M.put(doc, m)
    doc.pick, doc.rating = m.pick, m.rating
    doc.tags = #m.tags > 0 and m.tags or nil
end

-- A record changed by `op` (parse's, or { pick = "accept"|"reject"|"none", rating = n (0: none), rename = {a, b} }):
-- true when anything changed
function M.apply(doc, op)
    local before = M.model(doc)
    local m = M.model(doc)
    for _, t in ipairs(op.toggle or {}) do m.tags = M.toggle(m.tags, t) end
    m.tags = M.changed(m.tags, op.add, op.remove)
    if op.rename then m.tags = M.rename(m.tags, op.rename[1], op.rename[2]) end
    if op.rating ~= nil then m.rating = M.rating(op.rating) end
    if op.pick ~= nil then m.pick = M.pick(op.pick) end
    M.put(doc, m)
    return not M.same(before, m)
end

-- ---------- Finder's tags as the model ----------

-- A Finder tag's name ("Name\n6" -> "Name")
local function finder_name(s) return (s:match("^(.-)\n%d+$") or s) end
M.finder_name = finder_name

-- Finder's strings read as the model: Accept / Reject (both: Reject), a tag of only stars the rating (several: the
-- most), the rest custom
function M.from_finder(list)
    local m = { tags = {} }
    for _, f in ipairs(list or {}) do
        local n = M.clean(finder_name(f))
        if n then
            local rest, stars = n:gsub("★", "")
            if n:lower() == "reject" then m.pick = "reject"
            elseif n:lower() == "accept" then if m.pick ~= "reject" then m.pick = "accept" end
            elseif rest == "" and stars > 0 then m.rating = math.max(m.rating or 0, math.min(stars, M.MAX_RATING))
            elseif not M.find(m.tags, n) then m.tags[#m.tags + 1] = n end
        end
    end
    return m
end

-- The model as Finder's strings, the ones there (`now`) kept as they are (their colour, their order) where they say
-- the same, the new ones after them
function M.to_finder(m, now)
    local want = {}
    if m.pick == "accept" then want[#want + 1] = { M.ACCEPT, M.FINDER_ACCEPT } end
    if m.pick == "reject" then want[#want + 1] = { M.REJECT, M.FINDER_REJECT } end
    if m.rating then want[#want + 1] = { string.rep("★", m.rating), string.rep("★", m.rating) } end
    for _, t in ipairs(m.tags) do want[#want + 1] = { t, t } end
    local out, used = {}, {}
    for _, f in ipairs(now or {}) do
        for i, w in ipairs(want) do
            if not used[i] and same(finder_name(f), w[1]) then used[i] = true; out[#out + 1] = f; break end
        end
    end
    for i, w in ipairs(want) do if not used[i] then out[#out + 1] = w[2] end end
    return out
end

-- ---------- Sync: three-way ----------

-- The library's model and a store's, with what they last agreed on (`base`; nil: nothing): the model both get.
-- store_newer: the store's change is later than the library's (pick and rating changed on both sides)
function M.merge(base, lib, store, store_newer)
    base = base or { tags = {} }
    local out = { tags = M.list(lib.tags) }
    -- Tags: what the store added or removed since, onto the library's
    for _, t in ipairs(store.tags) do if not M.find(base.tags, t) then out.tags = M.changed(out.tags, { t }) end end
    for _, t in ipairs(base.tags) do if not M.find(store.tags, t) then out.tags = M.changed(out.tags, nil, { t }) end end
    for _, k in ipairs({ "pick", "rating" }) do
        local lib_changed, store_changed = lib[k] ~= base[k], store[k] ~= base[k]
        if store_changed and (not lib_changed or store_newer) then out[k] = store[k] else out[k] = lib[k] end
    end
    return out
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

return M
