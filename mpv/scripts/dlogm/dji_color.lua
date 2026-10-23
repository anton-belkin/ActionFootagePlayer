-- The color profile a DJI camera recorded a video in, from the camera's own metadata: ColorMode
-- (dvtm_library.proto) in the stream metadata of the first sample of the file's metadata track. DJI tags D-Log M
-- video as BT.709 and names Normal-color clips *_D.MP4 too, so this is the only reliable way to know.
--
-- The same as color_profile() in vs-gyroflow/src/probe.rs, in plain Lua so this plugin needs no binaries: the
-- first track with a `meta` handler (as telemetry-parser picks it), its first sample of more than 4 bytes, and
-- from that just the protobuf fields on the way to ColorMode.
--
--   require('dji_color').read(path)  ->  "dlogm", "normal", "hlg", ..., or nil when it can't tell

local M = {}

-- Where each model's schema puts ColorMode in StreamMeta. Other models use field 4 for other things (FOVType in
-- dvtm_wa530), so only the ones checked against their .proto are read
local STREAM_COLOR_MODE_FIELD = {
    ["dvtm_ac206.proto"] = 4, -- Osmo Action 6
    ["dvtm_oq101.proto"] = 4, -- Osmo 360
}
local COLOR_MODES = { [0] = "normal", [1] = "d-cinelike", [2] = "dlog", [9] = "hlg", [19] = "dlogm", [22] = "dlog2" }

-- moov is read whole; a camera's is a few MB at most
local MAX_MOOV = 64 * 1024 * 1024
local MAX_SAMPLE = 4 * 1024 * 1024

-- Big-endian integers (Lua 5.1/LuaJIT: no string.unpack; doubles hold them exactly up to 2^53)
local function u32(s, i)
    local a, b, c, d = s:byte(i, i + 3)
    if not d then return nil end
    return ((a * 256 + b) * 256 + c) * 256 + d
end
local function u64(s, i)
    local hi, lo = u32(s, i), u32(s, i + 4)
    return hi and lo and hi * 4294967296 + lo
end

-- The boxes in s[first..last]: an iterator of type, payload start, payload end
local function boxes(s, first, last)
    local pos = first
    return function()
        if pos + 7 > last then return nil end
        local size, typ, header = u32(s, pos), s:sub(pos + 4, pos + 7), 8
        if size == 1 then size, header = u64(s, pos + 8), 16
        elseif size == 0 then size = last - pos + 1 end
        if not size or size < header or pos + size - 1 > last then return nil end
        local start = pos
        pos = pos + size
        return typ, start + header, start + size - 1
    end
end

local function child(s, first, last, wanted)
    for typ, a, b in boxes(s, first, last) do
        if typ == wanted then return a, b end
    end
    return nil
end

-- The moov box, read from the file (top-level boxes are walked with seeks, mdat isn't read), and where its payload
-- starts in it
local function read_moov(f)
    local file_size = f:seek("end")
    local pos = 0
    while pos + 8 <= file_size do
        f:seek("set", pos)
        local head = f:read(16)
        if not head or #head < 8 then return nil end
        local size, typ, header = u32(head, 1), head:sub(5, 8), 8
        if size == 1 then size, header = u64(head, 9), 16
        elseif size == 0 then size = file_size - pos end
        if not size or size < header then return nil end
        if typ == "moov" then
            if size > MAX_MOOV then return nil end
            f:seek("set", pos)
            local moov = f:read(size)
            if not moov or #moov ~= size then return nil end
            return moov, header + 1
        end
        pos = pos + size
    end
    return nil
end

-- Offset and size of the first sample of more than 4 bytes in the track's first chunk
local function first_sample(s, stbl_a, stbl_b)
    local offset
    local a = child(s, stbl_a, stbl_b, "stco")
    if a then offset = u32(s, a + 8)
    else
        a = child(s, stbl_a, stbl_b, "co64")
        if not a then return nil end
        offset = u64(s, a + 8)
    end
    if not offset or (u32(s, a + 4) or 0) < 1 then return nil end

    local per_chunk = 1
    a = child(s, stbl_a, stbl_b, "stsc")
    if a and (u32(s, a + 4) or 0) >= 1 then per_chunk = u32(s, a + 12) or 1 end

    a = child(s, stbl_a, stbl_b, "stsz")
    if not a then return nil end
    local fixed, count = u32(s, a + 4), u32(s, a + 8)
    if not fixed or not count then return nil end
    for k = 1, math.min(per_chunk, count) do
        local size = fixed ~= 0 and fixed or u32(s, a + 12 + (k - 1) * 4)
        if not size then return nil end
        if size > 4 then return offset, size end
        offset = offset + size
    end
    return nil
end

-- The first sample of the first metadata track
local function metadata_sample(f)
    local moov, first = read_moov(f)
    if not moov then return nil end
    for typ, ta, tb in boxes(moov, first, #moov) do
        if typ == "trak" then
            local ma, mb = child(moov, ta, tb, "mdia")
            local ha = ma and child(moov, ma, mb, "hdlr")
            if ha and moov:sub(ha + 8, ha + 11) == "meta" then
                local ia, ib = child(moov, ma, mb, "minf")
                local sa, sb
                if ia then sa, sb = child(moov, ia, ib, "stbl") end
                if not sa then return nil end
                local offset, size = first_sample(moov, sa, sb)
                if not offset or size > MAX_SAMPLE then return nil end
                f:seek("set", offset)
                return f:read(size)
            end
        end
    end
    return nil
end

-- Minimal protobuf reading: the first occurrence of a field in a message, as a number (varint) or a string
local function varint(s, i)
    local v, mul = 0, 1
    while i <= #s do
        local b = s:byte(i)
        i = i + 1
        v = v + (b % 128) * mul
        if b < 128 then return v, i end
        mul = mul * 128
        if mul > 2 ^ 63 then return nil end
    end
    return nil
end

local function pb_field(msg, wanted)
    if not msg then return nil end
    local i = 1
    while i <= #msg do
        local key
        key, i = varint(msg, i)
        if not key then return nil end
        local wire, value = key % 8, nil
        if wire == 0 then
            value, i = varint(msg, i)
            if not value then return nil end
        elseif wire == 1 or wire == 5 then
            local len = wire == 1 and 8 or 4
            if i + len - 1 > #msg then return nil end
            value, i = msg:sub(i, i + len - 1), i + len
        elseif wire == 2 then
            local len
            len, i = varint(msg, i)
            if not len or i + len - 1 > #msg then return nil end
            value, i = msg:sub(i, i + len - 1), i + len
        else
            return nil
        end
        if math.floor(key / 8) == wanted then return value end
    end
    return nil
end
local function pb_bytes(msg, field)
    local v = pb_field(msg, field)
    return type(v) == "string" and v or nil
end

local function color_mode(sample)
    -- ProductMeta { clip_meta = 1 { clip_meta_header = 1 { proto_file_name = 1 } }, stream_meta = 2 }
    local proto = pb_bytes(pb_bytes(pb_bytes(sample, 1), 1), 1)
    local field = proto and STREAM_COLOR_MODE_FIELD[proto]
    if not field then return nil end
    local mode = pb_bytes(pb_bytes(sample, 2), field)
    if not mode then return nil end
    -- A ColorMode message without its field is the default (0) in proto3
    local v = pb_field(mode, 1)
    return type(v) == "number" and v or 0
end

function M.read(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local ok, sample = pcall(metadata_sample, f)
    f:close()
    if not ok or not sample then return nil end
    local mode = color_mode(sample)
    if not mode then return nil end
    return COLOR_MODES[mode] or "other"
end

-- The MP4 reading, for gopro_color.lua
M.read_moov, M.boxes, M.child = read_moov, boxes, child

return M
