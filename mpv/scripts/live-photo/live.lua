-- live.lua: what a photo is, read from its files (no mpv here: tests/live_photo_test.lua runs it with plain luajit).
--
-- A Live Photo (Apple): a HEIC or JPEG and a .mov of the same name beside it. When both carry Apple's content
-- identifier (the MOV in its QuickTime keys, the photo in its maker note) they must be the same one. The still is the
-- MOV's frame at its still-image-time (a timed-metadata track: when its one sample is), about 1.5 s in.
--
-- A Motion Photo (Android, and the player's own export): a JPEG (or HEIC) with an MP4 appended, its XMP saying so:
-- GCamera:MotionPhoto with a Container:Directory whose last item (Semantic MotionPhoto) has the video's Length counted
-- from the file's end, or the older GCamera:MicroVideo with MicroVideoOffset. The still is the video's frame at
-- GCamera:MotionPhotoPresentationTimestampUs (MicroVideoPresentationTimestampUs).
--
-- When it was taken: the photo's Exif DateTimeOriginal + SubSecTimeOriginal (+ OffsetTimeOriginal), as the camera's
-- clock reads. The live part then runs from (taken - still time) for the video's length: what chains photos.

local M = {}

local HEAD = 512 * 1024 -- the photo's bytes read for its Exif and XMP (an iPhone HEIC's are within ~25 KB)

local function u16(s, i, le)
    local a, b = s:byte(i, i + 1)
    if not b then return nil end
    if le then return a + b * 256 end
    return a * 256 + b
end
local function u32(s, i, le)
    local a, b, c, d = s:byte(i, i + 3)
    if not d then return nil end
    if le then return a + b * 256 + c * 65536 + d * 16777216 end
    return ((a * 256 + b) * 256 + c) * 256 + d
end
local function u64(s, i)
    local hi, lo = u32(s, i), u32(s, i + 4)
    if not lo then return nil end
    return hi * 4294967296 + lo
end
M.u32 = u32

local function read_head(path, n)
    local f = io.open(path, "rb")
    if not f then return nil end
    local s = f:read(n or HEAD)
    local size = f:seek("end")
    f:close()
    return s or "", size
end

local function days_from_civil(y, m, d)
    if m <= 2 then y = y - 1 end
    local era = math.floor(y / 400)
    local yoe = y - era * 400
    local mp = m > 2 and m - 3 or m + 9
    local doy = math.floor((153 * mp + 2) / 5) + d - 1
    local doe = yoe * 365 + math.floor(yoe / 4) - math.floor(yoe / 100) + doy
    return era * 146097 + doe - 719468
end

-- "YYYY:MM:DD HH:MM:SS" (+ "585" sub-seconds, "+01:00") -> seconds since 1970 (UTC when the zone is known, else the
-- camera's own clock), and whether the zone was known
function M.exif_seconds(dt, subsec, offset)
    if type(dt) ~= "string" then return nil end
    local y, mo, d, h, mi, s = dt:match("^(%d%d%d%d)[:%-](%d%d)[:%-](%d%d)[ T](%d%d):(%d%d):(%d%d)")
    if not y then return nil end
    local t = days_from_civil(tonumber(y), tonumber(mo), tonumber(d)) * 86400 + tonumber(h) * 3600 + tonumber(mi) * 60 + tonumber(s)
    local digits = type(subsec) == "string" and subsec:match("^%s*(%d+)") or nil
    if digits then t = t + tonumber(digits) / 10 ^ #digits end
    local sign, oh, om = (type(offset) == "string" and offset or ""):match("^([%+%-])(%d%d):?(%d%d)")
    if sign then
        local o = tonumber(oh) * 3600 + tonumber(om) * 60
        t = sign == "+" and t - o or t + o
        return t, true
    end
    return t, false
end

-- The Exif's TIFF in `s` (a JPEG's APP1, a HEIC's Exif item): its DateTimeOriginal, SubSecTimeOriginal,
-- OffsetTimeOriginal. Looked for as a TIFF header whose first IFD is at 8 (what every camera writes)
function M.exif_taken(s)
    local at, le = s:find("MM\0\42\0\0\0\8", 1, true), false
    local at2 = s:find("II\42\0\8\0\0\0", 1, true)
    if at2 and (not at or at2 < at) then at, le = at2, true end
    if not at then return nil end
    local base = at - 1 -- offsets in the TIFF count from its first byte
    local function ifd(off)
        local p = base + off + 1
        local n = u16(s, p, le)
        if not n or n > 500 then return {} end
        local e = {}
        for k = 0, n - 1 do
            local q = p + 2 + k * 12
            local tag, typ, cnt = u16(s, q, le), u16(s, q + 2, le), u32(s, q + 4, le)
            if not cnt then break end
            if typ == 2 then
                local val = cnt <= 4 and s:sub(q + 8, q + 7 + cnt) or s:sub(base + u32(s, q + 8, le) + 1, base + u32(s, q + 8, le) + cnt)
                e[tag] = (val:gsub("%z.*$", ""))
            elseif typ == 4 or typ == 13 then
                e[tag] = u32(s, q + 8, le)
            end
        end
        return e
    end
    local ifd0 = ifd(8)
    local exif = ifd0[0x8769] and ifd(ifd0[0x8769]) or {}
    local dt = exif[0x9003] or exif[0x9004] or ifd0[0x0132]
    if not dt then return nil end
    return { datetime = dt, subsec = exif[0x9291], offset = exif[0x9011] }
end

-- A HEIC's Exif item, where its meta box's iloc puts it: { offset, length } in the file, or nil. (A big iPhone HEIC
-- has it past the first 512 KB)
function M.heic_exif_at(s)
    if s:sub(5, 8) ~= "ftyp" then return nil end
    local ftyp_len = u32(s, 1)
    if not ftyp_len or s:sub(ftyp_len + 5, ftyp_len + 8) ~= "meta" then return nil end
    local meta_len = u32(s, ftyp_len + 1)
    local meta = s:sub(ftyp_len + 13, ftyp_len + meta_len) -- after its version/flags
    if #meta < meta_len - 12 then return nil end
    local id
    local function kids(body, from)
        local out, i = {}, from or 1
        while i + 7 <= #body do
            local len = u32(body, i)
            if not len or len < 8 then break end
            out[#out + 1] = { type = body:sub(i + 4, i + 7), body = body:sub(i + 8, i + len - 1) }
            i = i + len
        end
        return out
    end
    local boxes = kids(meta)
    for _, b in ipairs(boxes) do
        if b.type == "iinf" then
            local v = b.body:byte(1)
            for _, e in ipairs(kids(b.body, v == 0 and 7 or 9)) do
                if e.type == "infe" then
                    local ev = e.body:byte(1)
                    local item, typ
                    if ev == 2 then item, typ = u16(e.body, 5), e.body:sub(9, 12)
                    elseif ev == 3 then item, typ = u32(e.body, 5), e.body:sub(11, 14) end
                    if typ == "Exif" then id = item end
                end
            end
        end
    end
    if not id then return nil end
    for _, b in ipairs(boxes) do
        if b.type == "iloc" then
            local d = b.body
            local v = d:byte(1)
            local sizes, sizes2 = d:byte(5), d:byte(6)
            local off_size, len_size = math.floor(sizes / 16), sizes % 16
            local base_size, index_size = math.floor(sizes2 / 16), (v == 1 or v == 2) and sizes2 % 16 or 0
            local function num(p, n)
                if n == 0 then return 0, p end
                if n == 4 then return u32(d, p), p + 4 end
                if n == 8 then return u64(d, p), p + 8 end
                return nil, p
            end
            local p, count = 7, nil
            if v < 2 then count, p = u16(d, p), p + 2 else count, p = u32(d, p), p + 4 end
            for _ = 1, count or 0 do
                local item
                if v < 2 then item, p = u16(d, p), p + 2 else item, p = u32(d, p), p + 4 end
                local method = 0
                if v == 1 or v == 2 then method, p = u16(d, p) % 16, p + 2 end
                p = p + 2 -- data reference index
                local base; base, p = num(p, base_size)
                local extents = u16(d, p); p = p + 2
                local first
                for _ = 1, extents or 0 do
                    if index_size > 0 then local _; _, p = num(p, index_size) end
                    local o, l
                    o, p = num(p, off_size)
                    l, p = num(p, len_size)
                    first = first or { offset = (base or 0) + (o or 0), length = l }
                end
                if item == id and method == 0 and first then return first end
            end
        end
    end
    return nil
end

-- Apple's content identifier written in the photo (a UUID in its maker note): the first one found in the head
function M.photo_identifiers(s)
    local ids = {}
    for id in s:gmatch("%x%x%x%x%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x") do ids[id:upper()] = true end
    return ids
end

-- An attribute of the XMP, as an attribute (Name="v") or an element (<Name>v</Name>)
local function xmp_value(x, name)
    local v = x:match(name .. '%s*=%s*"([^"]*)"') or x:match("<" .. name .. ">([^<]*)</" .. name .. ">")
    return v
end

-- The XMP's Motion Photo: { offset (from the file's start), length, still_t (seconds into the video, nil: unknown) }
function M.motion_photo(s, size)
    local x = s:match("<x:xmpmeta.-</x:xmpmeta>")
    if not x then return nil end
    local motion = xmp_value(x, "GCamera:MotionPhoto")
    local micro = xmp_value(x, "GCamera:MicroVideo")
    local length
    if motion == "1" then
        -- The Container's items in order: the video is the one whose Semantic is MotionPhoto, the file's last
        for item in x:gmatch("<Container:Item(.-)/?>") do
            if (xmp_value(item, "Item:Semantic") or "") == "MotionPhoto" then
                length = tonumber(xmp_value(item, "Item:Length"))
            end
        end
        local us = tonumber(xmp_value(x, "GCamera:MotionPhotoPresentationTimestampUs"))
        if length and length > 0 and length < size then
            return { offset = size - length, length = length, still_t = (us and us >= 0) and us / 1e6 or nil }
        end
    end
    if micro == "1" then
        length = tonumber(xmp_value(x, "GCamera:MicroVideoOffset"))
        local us = tonumber(xmp_value(x, "GCamera:MicroVideoPresentationTimestampUs"))
        if length and length > 0 and length < size then
            return { offset = size - length, length = length, still_t = (us and us >= 0) and us / 1e6 or nil }
        end
    end
    return nil
end

-- ---------- MP4 / QuickTime ----------

-- The top-level boxes of the file from `from` (a Motion Photo's video starts inside the JPEG): the moov box's bytes
local function read_moov(path, from, upto)
    local f = io.open(path, "rb")
    if not f then return nil end
    local size = f:seek("end")
    upto = upto or size
    local pos = from or 0
    while pos + 8 <= upto do
        f:seek("set", pos)
        local h = f:read(16)
        if not h or #h < 8 then break end
        local len, typ = u32(h, 1), h:sub(5, 8)
        local hdr = 8
        if len == 1 then len, hdr = u64(h, 9), 16 elseif len == 0 then len = upto - pos end
        if not len or len < hdr then break end
        if typ == "moov" then
            if len > 64 * 1024 * 1024 then break end
            f:seek("set", pos + hdr)
            local body = f:read(len - hdr)
            f:close()
            return body
        end
        pos = pos + len
    end
    f:close()
    return nil
end

-- The child boxes of a box's body: { type, body } in order
local function children(s, i, j)
    local out = {}
    i, j = i or 1, j or #s
    while i + 7 <= j do
        local len, typ = u32(s, i), s:sub(i + 4, i + 7)
        local hdr = 8
        if len == 1 then len, hdr = u64(s, i + 8), 16 elseif len == 0 then len = j - i + 1 end
        if not len or len < hdr or i + len - 1 > j then break end
        out[#out + 1] = { type = typ, body = s:sub(i + hdr, i + len - 1) }
        i = i + len
    end
    return out
end
local function child(s, typ)
    for _, c in ipairs(children(s)) do if c.type == typ then return c.body end end
    return nil
end
local function path_of(s, ...)
    for _, t in ipairs({ ... }) do
        if not s then return nil end
        s = child(s, t)
    end
    return s
end

-- mvhd/mdhd: (timescale, duration)
local function header_times(b)
    if not b then return nil end
    if b:byte(1) == 1 then return u32(b, 21), u64(b, 25) end
    return u32(b, 13), u32(b, 17)
end

-- A track's first sample's presentation time in movie seconds: the empty edits before it (and the first edit's media
-- time, by the media's clock)
local function track_start(trak, movie_scale)
    local elst = path_of(trak, "edts", "elst")
    local mscale = header_times(path_of(trak, "mdia", "mdhd"))
    if not elst then return 0 end
    local v, n = elst:byte(1), u32(elst, 5)
    local t, p = 0, 9
    for _ = 1, n or 0 do
        local dur, mtime
        if v == 1 then dur, mtime = u64(elst, p), u64(elst, p + 8); p = p + 20
        else dur, mtime = u32(elst, p), u32(elst, p + 4); p = p + 12
            if mtime == 0xFFFFFFFF then mtime = -1 end end
        if mtime == -1 then
            t = t + dur / movie_scale
        else
            return t - (mscale and mscale > 0 and mtime / mscale or 0)
        end
    end
    return t
end

-- The mebx (timed metadata) keys of a track's sample description
local function mebx_keys(trak)
    local stsd = path_of(trak, "mdia", "minf", "stbl", "stsd")
    if not stsd then return {} end
    local keys = {}
    -- stsd: version/flags, count, then entries; a mebx entry: 8 reserved/index bytes, then boxes (keys)
    for _, e in ipairs(children(stsd, 9)) do
        if e.type == "mebx" then
            local k = child(e.body:sub(9), "keys")
            for _, kb in ipairs(k and children(k) or {}) do
                local keyd = child(kb.body, "keyd")
                if keyd then keys[#keys + 1] = keyd:sub(5) end
            end
        end
    end
    return keys
end

-- moov/meta's QuickTime keys and values (strings): { ["com.apple.quicktime.content.identifier"] = "…" }
local function quicktime_keys(moov)
    local meta = child(moov, "meta")
    if not meta then return {} end
    -- QuickTime's meta has no version/flags; an ISO one does (its first child would then not be a box)
    if not (meta:sub(5, 8) == "hdlr" or meta:sub(5, 8) == "keys") then meta = meta:sub(5) end
    local keys, out = {}, {}
    local k = child(meta, "keys")
    if k then
        local i, n = 9, u32(k, 5) or 0
        for idx = 1, n do
            local len = u32(k, i)
            if not len or len < 8 then break end
            keys[idx] = k:sub(i + 8, i + len - 1)
            i = i + len
        end
    end
    local ilst = child(meta, "ilst")
    for _, item in ipairs(ilst and children(ilst) or {}) do
        local idx = u32(item.type, 1)
        local data = child(item.body, "data")
        if keys[idx] and data then out[keys[idx]] = data:sub(9) end
    end
    return out
end

-- An MP4 or MOV (from `from`, up to `upto`): { duration, still_t (from a still-image-time track), id, has_video }
function M.movie(path, from, upto)
    local moov = read_moov(path, from, upto)
    if not moov then return nil end
    local scale, dur = header_times(child(moov, "mvhd"))
    if not scale or scale == 0 then return nil end
    local info = { duration = dur / scale }
    for _, c in ipairs(children(moov)) do
        if c.type == "trak" then
            local hdlr = path_of(c.body, "mdia", "hdlr")
            local kind = hdlr and hdlr:sub(9, 12)
            if kind == "vide" then info.has_video = true end
            if kind == "meta" then
                for _, key in ipairs(mebx_keys(c.body)) do
                    if key == "com.apple.quicktime.still-image-time" then info.still_t = track_start(c.body, scale) end
                end
            end
        end
    end
    local qt = quicktime_keys(moov)
    local id = qt["com.apple.quicktime.content.identifier"]
    if id and #id > 0 then info.id = id:upper() end
    return info
end

-- ---------- a photo ----------

local PHOTO = { heic = true, heif = true, jpg = true, jpeg = true, png = true, avif = true, webp = true, tif = true, tiff = true, dng = true }
-- (Not a photo's link, <name>.afplink.jpg: the workspace plugin opens its clip at the photo's frame instead)
function M.is_photo(path)
    local ext = type(path) == "string" and path:match("%.([%w]+)$")
    return ext ~= nil and PHOTO[ext:lower()] == true and not path:find("://", 1, true)
           and not path:lower():match("%.afplink%.jpe?g$")
end

-- The .mov beside a photo (same name; .mov or .MOV), or nil
function M.paired_mov(path, exists)
    local stem = path:match("^(.*)%.[^./\\]+$")
    if not stem then return nil end
    for _, ext in ipairs({ ".mov", ".MOV", ".Mov" }) do
        if exists(stem .. ext) then return stem .. ext end
    end
    return nil
end

local function default_exists(p)
    local f = io.open(p, "rb")
    if f then f:close() return true end
    return false
end

-- What a photo is: nil (not a photo), or { live = false } for a plain photo, or
-- { live = true, kind = "apple"|"motion", url (the video for mpv), video (the file), duration, still_t,
--   taken (seconds), zoned, start (taken - still_t) }
function M.describe(path, exists)
    if not M.is_photo(path) then return nil end
    exists = exists or default_exists
    local head, size = read_head(path)
    if not head then return nil end
    local info = { live = false }
    local t
    -- A HEIC: its Exif item where the iloc says (the maker note with Apple's identifier is in it too)
    local at = M.heic_exif_at(head)
    if at and at.length and at.length < 1024 * 1024 then
        local f = io.open(path, "rb")
        if f then
            f:seek("set", at.offset)
            local exif = f:read(at.length) or ""
            f:close()
            t = M.exif_taken(exif)
            if at.offset + at.length > #head then head = head .. exif end
        end
    end
    t = t or M.exif_taken(head)
    if t then info.taken, info.zoned = M.exif_seconds(t.datetime, t.subsec, t.offset) end
    local mp = M.motion_photo(head, size)
    if mp then
        local mv = M.movie(path, mp.offset, size)
        if mv and mv.has_video then
            info.live, info.kind = true, "motion"
            info.video = path
            info.url = string.format("slice://%d@%s", mp.offset, path)
            info.duration = mv.duration
            info.still_t = mp.still_t or mv.duration / 2
        end
    end
    if not info.live then
        local mov = M.paired_mov(path, exists)
        local mv = mov and M.movie(mov)
        if mv and mv.has_video then
            local ids = M.photo_identifiers(head)
            -- Both say which Live Photo they are: they must agree. One that doesn't say: paired by its name
            local agree = not mv.id or next(ids) == nil or ids[mv.id]
            if agree then
                info.live, info.kind = true, "apple"
                info.video, info.url = mov, mov
                info.duration = mv.duration
                info.still_t = mv.still_t or mv.duration / 2
                info.id = mv.id
            end
        end
    end
    if info.live and info.taken then info.start = info.taken - info.still_t end
    return info
end

-- ---------- chains ----------

-- How much further a step must take the picture to be worth it (s): a photo whose live part ends sooner after the
-- current one's end is passed over
M.MIN_MORE = 0.1

-- The next photo a live part carries on into: among `others` (described photos), the live one taken next after `cur`
-- (by the photos' own times) whose live part has begun before `cur`'s ends and goes on past it. Nil when none. Both
-- must say when they were taken on the same kind of clock (both with a zone, or both without)
function M.next_in_chain(cur, others)
    if not (cur and cur.live and cur.start) then return nil end
    local stop = cur.start + cur.duration
    local best
    for _, o in ipairs(others) do
        if o ~= cur and o.live and o.start and o.zoned == cur.zoned and o.taken > cur.taken
           and o.start < stop and o.start + o.duration > stop + M.MIN_MORE then
            if not best or o.taken < best.taken then best = o end
        end
    end
    return best
end

-- The whole run from `cur`: { cur, next, next's next, … } (each taken once)
function M.chain(cur, others)
    local run, seen = { cur }, { [cur] = true }
    local at = cur
    while true do
        local n = M.next_in_chain(at, others)
        if not n or seen[n] then break end
        run[#run + 1], seen[n] = n, true
        at = n
    end
    return run
end

-- Where a step goes on: the moment `from`'s video ends, in `to`'s video
function M.carry_on_at(from, to)
    return from.start + from.duration - to.start
end

return M
