-- The moments a camera marked in a clip while it recorded (a button, the remote, a voice command), as times in seconds.
-- Where each camera keeps them in the MP4:
--   GoPro HERO5, HERO6    moov/udta/HMMT: their count, then each one's time (ms), 32-bit big-endian (as VLC reads
--                         them; 100 at most)
--   GoPro HERO7 and later moov/udta/GPMF, GPMF as in the metadata track: the DEVC whose DVID is HLMT ("Highlights"),
--                         its HLMT item a struct per highlight, laid out by the TYPE before it, the time (ms) first
--                         (gopro-telemetry's mp4header sample, from a HERO7 Black: Time, in, out, location, type
--                         MANL, confidence, score)
--   DJI Osmo Action 4, Osmo Action 5 Pro, Osmo Action 6
--                         a free box at the top level: "data", then a box hglg (32-bit big-endian size, its type, 5
--                         bytes), then 5 bytes per highlight: one byte, and its time (ms), 32-bit little-endian
--                         (ExifTool's HighlightMarkers, worked out on an Action 4; the camera takes 190 at most)
--
-- Only the boxes' headers are read on the way (seeks: mdat isn't read), then the few that have the highlights. Plain
-- Lua (5.1/LuaJIT), as telemetry.lua in the dji-telemetry plugin.
--
--   local m = require('markers').read(path)
--   m.camera   "GoPro" or "DJI"
--   m.times    { 12.3, 45.6 }: in order, each once
--
-- read() returns nil for a file without any.

local M = {}

-- What's read of a box: HMMT is 404 bytes, the udta GPMF some KB, hglg 13 + 5 per highlight
local MAX_READ = 1024 * 1024
local HGLG_HEAD = 17
-- stdio's buffer, read whole on each seek: by default the file system's block, which is 1 MB on an SD card's exFAT
local BUFFER = 16384

-- Big-endian integers (doubles hold them exactly up to 2^53)
local function u32(s, i)
    local a, b, c, d = s:byte(i, i + 3)
    if not d then return nil end
    return ((a * 256 + b) * 256 + c) * 256 + d
end
local function u64(s, i)
    local hi, lo = u32(s, i), u32(s, i + 4)
    return hi and lo and hi * 4294967296 + lo
end
local function le32(s, i)
    local a, b, c, d = s:byte(i, i + 3)
    if not d then return nil end
    return ((d * 256 + c) * 256 + b) * 256 + a
end
local function i32(s, i)
    local v = u32(s, i)
    return v and (v >= 2147483648 and v - 4294967296 or v)
end

-- The boxes of the file from first to last (offsets, last excluded), walked with seeks: an iterator of type, payload
-- start, payload end
local function boxes(f, first, last)
    local pos = first
    return function()
        if pos + 8 > last then return nil end
        f:seek("set", pos)
        local head = f:read(16)
        if not head or #head < 8 then return nil end
        local size, typ, header = u32(head, 1), head:sub(5, 8), 8
        if size == 1 then size, header = u64(head, 9), 16
        elseif size == 0 then size = last - pos end
        if not size or size < header or pos + size > last then return nil end
        local start = pos
        pos = pos + size
        return typ, start + header, start + size
    end
end

local function read_at(f, a, n)
    f:seek("set", a)
    return f:read(n) or ""
end

-- HMMT: GoPro HERO5 and HERO6 (a HERO4's is all zeros)
local function hmmt(s, times)
    local n = u32(s, 1)
    if not n or n > 100 then return end
    for k = 1, n do
        local ms = u32(s, 1 + 4 * k)
        if not ms then return end
        if ms > 0 then times[#times + 1] = ms / 1000 end
    end
end

-- GPMF: KLV items, each a FourCC, a type (0: nested items), the size of a value and how many there are, then the
-- values, padded to 4 bytes. An iterator over s[first..last] of key, type, size, count, where the values start
local function klv(s, first, last)
    local pos = first
    return function()
        if pos + 7 > last then return nil end
        local key, typ, size = s:sub(pos, pos + 3), s:sub(pos + 4, pos + 4), s:byte(pos + 5)
        local count = s:byte(pos + 6) * 256 + s:byte(pos + 7)
        local start, len = pos + 8, size * count
        if start + len - 1 > last then return nil end
        pos = start + math.ceil(len / 4) * 4
        return key, typ, size, count, start
    end
end

-- The HLMT stream of a DEVC: TYPE says the struct's fields; the time is the first, a 32-bit integer (ms), divided
-- by the first SCAL
local function hlmt(s, first, last, times)
    local layout, scale = nil, 1
    for key, typ, size, count, at in klv(s, first, last) do
        if key == "TYPE" and typ == "c" then
            layout = s:sub(at, at + size * count - 1)
        elseif key == "SCAL" and size == 4 and (typ == "l" or typ == "L") then
            local v = typ == "l" and i32(s, at) or u32(s, at)
            if v and v ~= 0 then scale = v end
        elseif key == "HLMT" and size >= 4 then
            local first_type = layout and layout:sub(1, 1)
            if first_type ~= "L" and first_type ~= "l" then return end
            for k = 0, count - 1 do
                local p = at + k * size
                local v = first_type == "l" and i32(s, p) or u32(s, p)
                if v and v > 0 then times[#times + 1] = v / scale / 1000 end
            end
        end
    end
end

-- The udta GPMF: GoPro HERO7 and later
local function gpmf(s, times)
    for key, typ, size, count, at in klv(s, 1, #s) do
        if key == "DEVC" and typ == "\0" then
            local last = at + size * count - 1
            local highlights = false
            for k, t, ks, kc, a in klv(s, at, last) do
                if k == "DVID" and t == "F" and s:sub(a, a + 3) == "HLMT" then highlights = true end
                if k == "STRM" and t == "\0" and highlights then hlmt(s, a, a + ks * kc - 1, times) end
            end
        end
    end
end

-- A free box that's DJI's highlights
local function hglg(f, a, b, times)
    if b - a < HGLG_HEAD then return false end
    local head = read_at(f, a, HGLG_HEAD)
    if head:sub(1, 4) ~= "data" or head:sub(9, 12) ~= "hglg" then return false end
    local len = u32(head, 5)
    if not len or len < 13 or a + 4 + len > b or len > MAX_READ then return false end
    local s = read_at(f, a, len + 4)
    for k = 0, math.floor((len - 13) / 5) - 1 do
        local ms = le32(s, HGLG_HEAD + 5 * k + 2)
        if not ms then break end
        times[#times + 1] = ms / 1000
    end
    return true
end

function M.read(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    f:setvbuf("full", BUFFER)
    local size = f:seek("end")
    local gopro, dji = {}, {}
    local found_dji = false
    for typ, a, b in boxes(f, 0, size) do
        if typ == "free" or typ == "skip" then
            found_dji = hglg(f, a, b, dji) or found_dji
        elseif typ == "moov" then
            for mt, ma, mb in boxes(f, a, b) do
                if mt == "udta" then
                    for ut, ua, ub in boxes(f, ma, mb) do
                        if ut == "HMMT" and ub - ua <= MAX_READ then
                            hmmt(read_at(f, ua, ub - ua), gopro)
                        elseif ut == "GPMF" and ub - ua <= MAX_READ then
                            gpmf(read_at(f, ua, ub - ua), gopro)
                        elseif ut == "free" or ut == "skip" then
                            found_dji = hglg(f, ua, ub, dji) or found_dji
                        end
                    end
                end
            end
        end
    end
    f:close()

    local camera, times = nil, nil
    if #gopro > 0 then camera, times = "GoPro", gopro
    elseif found_dji and #dji > 0 then camera, times = "DJI", dji
    else return nil end
    -- In order, each once (HERO6 may have them in both HMMT and GPMF)
    table.sort(times)
    local out = {}
    for _, t in ipairs(times) do
        if #out == 0 or t - out[#out] > 0.05 then out[#out + 1] = t end
    end
    return { camera = camera, times = out }
end

return M
