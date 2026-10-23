-- The telemetry a DJI camera recorded, over time: every sample of the file's metadata track (one protobuf
-- message per frame, ProductMeta in DJI's dvtm_*.proto), and from each the values the plugin shows.
--
-- The track is the one dji_color.lua in the dlogm plugin reads (the first with a `meta` handler); here all its
-- samples are read, through the whole sample table (stts, stsc, stsz, stco/co64). Plain Lua (5.1/LuaJIT: no
-- string.unpack, no bit operations), so the plugin needs no binaries.
--
--   local t = require('telemetry').read(path, { "altitude_dm", "in_water" })
--   t.model                           "dvtm_ac206.proto"
--   telemetry.at(t.values.altitude_dm, time)   the value at that time (seconds), or nil
--
-- read() returns nil for files that aren't from a DJI camera with a known layout, or that have none of the
-- values asked for.

local M = {}

-- Where each model's schema puts the values, as paths of field numbers from ProductMeta (what
-- tools/dji_meta_dump.py prints): 3 is FrameMeta, 3.2 its camera part. A value missing from a message that is
-- there is its default (proto3): 0, false.
--   uint, int32, float, double: the field;  rational: a packed repeated int32 [numerator, denominator]
local LAYOUTS = {
    -- Osmo Action 6. Its schema isn't public: found on real clips with tools/dji_meta_dump.py. 3.2 numbers its
    -- fields like Osmo 360's camera part, with the barometer at 13 and the aperture at 15. No clip had a GPS fix
    -- (gps_status 1, invalid: no remote or Mimo connected), so the position and velocity are where Osmo 360's
    -- schema has them, under the same 3.4 with the same gps_status: not checked against a recording
    ["dvtm_ac206.proto"] = {
        in_water = { "3.2.13.1", "uint" },
        altitude_dm = { "3.2.13.2", "int32" },
        pressure_hpa = { "3.2.13.3", "uint" },
        underwater_confidence = { "3.2.8.1", "uint" },
        iso = { "3.2.3.1", "float" },
        exposure_time = { "3.2.4.1", "rational" },
        white_balance_k = { "3.2.6.1", "uint" },
        f_number = { "3.2.15.1", "rational" },
        gps_status = { "3.4.2.3", "uint" },
        gps_unit = { "3.4.2.1.1", "uint" },
        latitude = { "3.4.2.1.2", "double" },
        longitude = { "3.4.2.1.3", "double" },
        gps_altitude_mm = { "3.4.2.2", "int32" },
        velocity_x = { "3.4.3.1", "float" },
        velocity_y = { "3.4.3.2", "float" },
        velocity_z = { "3.4.3.3", "float" },
    },
    -- Osmo 360, from its schema (dvtm_oq101.proto in telemetry-parser); not checked against a recording. Without
    -- the underwater confidence: its scale (0-256 on Action 6) isn't known here
    ["dvtm_oq101.proto"] = {
        iso = { "3.2.3.1", "float" },
        exposure_time = { "3.2.4.1", "rational" },
        white_balance_k = { "3.2.6.1", "uint" },
        sensor_temperature_c = { "3.2.16.1", "float" },
        gps_status = { "3.4.2.3", "uint" },
        gps_unit = { "3.4.2.1.1", "uint" },
        latitude = { "3.4.2.1.2", "double" },
        longitude = { "3.4.2.1.3", "double" },
        gps_altitude_mm = { "3.4.2.2", "int32" },
        velocity_x = { "3.4.3.1", "float" },
        velocity_y = { "3.4.3.2", "float" },
        velocity_z = { "3.4.3.3", "float" },
        ebike_speed_kmh = { "3.5.12.1", "float" },
        heart_rate_bpm = { "3.5.13.1", "uint" },
    },
}

-- moov is read whole; a camera's is a few MB at most
local MAX_MOOV = 64 * 1024 * 1024
local MAX_SAMPLE = 4 * 1024 * 1024
local MAX_CHUNK = 64 * 1024 * 1024

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

-- A full box's entry count and where its entries start
local function entries(s, a)
    return u32(s, a + 4), a + 8
end

-- The track's chunks: { offset, first sample, sample count }, and the samples' times (seconds) and sizes
local function sample_table(s, sa, sb, timescale)
    local a = child(s, sa, sb, "stsz")
    if not a then return nil end
    local fixed, count = u32(s, a + 4), u32(s, a + 8)
    if not fixed or not count then return nil end
    local sizes = {}
    for k = 1, count do
        sizes[k] = fixed ~= 0 and fixed or u32(s, a + 12 + (k - 1) * 4)
        if not sizes[k] then return nil end
    end

    local offsets = {}
    a = child(s, sa, sb, "stco")
    if a then
        local n, p = entries(s, a)
        for k = 1, n or 0 do offsets[k] = u32(s, p + (k - 1) * 4) end
    else
        a = child(s, sa, sb, "co64")
        if not a then return nil end
        local n, p = entries(s, a)
        for k = 1, n or 0 do offsets[k] = u64(s, p + (k - 1) * 8) end
    end

    local times = {}
    a = child(s, sa, sb, "stts")
    if not a then return nil end
    local n, p = entries(s, a)
    local t = 0
    for k = 1, n or 0 do
        local c, d = u32(s, p), u32(s, p + 4)
        if not c or not d then return nil end
        for _ = 1, c do
            times[#times + 1] = t / timescale
            t = t + d
        end
        p = p + 8
    end

    -- stsc: runs of chunks with the same number of samples, each from its first chunk to the next run's
    a = child(s, sa, sb, "stsc")
    if not a then return nil end
    local runs = {}
    n, p = entries(s, a)
    for k = 1, n or 0 do
        runs[k] = { first = u32(s, p), per_chunk = u32(s, p + 4) }
        if not runs[k].first or not runs[k].per_chunk then return nil end
        p = p + 12
    end
    local chunks, i = {}, 1
    for k, run in ipairs(runs) do
        local last = runs[k + 1] and runs[k + 1].first - 1 or #offsets
        for c = run.first, last do
            if i > count or not offsets[c] then break end
            local m = math.min(run.per_chunk, count - i + 1)
            chunks[#chunks + 1] = { offset = offsets[c], first = i, count = m }
            i = i + m
        end
    end
    return chunks, times, sizes
end

-- The first track with a `meta` handler: its timescale and stbl
local function metadata_track(moov, first)
    for typ, ta, tb in boxes(moov, first, #moov) do
        if typ == "trak" then
            local ma, mb = child(moov, ta, tb, "mdia")
            local ha = ma and child(moov, ma, mb, "hdlr")
            if ha and moov:sub(ha + 8, ha + 11) == "meta" then
                local da = child(moov, ma, mb, "mdhd")
                if not da then return nil end
                local timescale = u32(moov, da + (moov:byte(da) == 1 and 20 or 12))
                local ia, ib = child(moov, ma, mb, "minf")
                local sa, sb
                if ia then sa, sb = child(moov, ia, ib, "stbl") end
                if not sa or not timescale or timescale == 0 then return nil end
                return timescale, sa, sb
            end
        end
    end
    return nil
end

-- Protobuf, read along known paths. A varint's value is exact up to 2^53; low32 is its low 32 bits, exact always
-- (a negative int32 is written as a 10-byte varint, which a double can't hold)
local function varint(s, i)
    local v, low, mul, n = 0, 0, 1, 0
    while i <= #s do
        local b = s:byte(i)
        i = i + 1
        n = n + 1
        v = v + (b % 128) * mul
        if n == 5 then low = v % 4294967296 end
        if b < 128 then
            if n < 5 then low = v end
            return v, i, low
        end
        mul = mul * 128
        if n >= 10 then return nil end
    end
    return nil
end

-- Every field of a message: field number, wire type, value (a number for varints, the bytes otherwise), low32
local function fields(msg)
    local i = 1
    return function()
        if not msg or i > #msg then return nil end
        local key, value, low
        key, i = varint(msg, i)
        if not key then i = #msg + 1; return nil end
        local wire, len = key % 8, nil
        if wire == 0 then
            value, i, low = varint(msg, i)
            if not value then i = #msg + 1; return nil end
        elseif wire == 1 or wire == 5 then
            len = wire == 1 and 8 or 4
        elseif wire == 2 then
            len, i = varint(msg, i)
            if not len then i = #msg + 1; return nil end
        else
            i = #msg + 1
            return nil
        end
        if len then
            if i + len - 1 > #msg then i = #msg + 1; return nil end
            value, i = msg:sub(i, i + len - 1), i + len
        end
        return math.floor(key / 8), wire, value, low
    end
end

-- The first occurrence of a field: value, wire type, low32
local function field(msg, wanted)
    for f, wire, value, low in fields(msg) do
        if f == wanted then return value, wire, low end
    end
    return nil
end

-- Little-endian IEEE 754
local function float32(s)
    local b1, b2, b3, b4 = s:byte(1, 4)
    local sign = b4 >= 128 and -1 or 1
    local exp = (b4 % 128) * 2 + math.floor(b3 / 128)
    local mant = (b3 % 128) * 65536 + b2 * 256 + b1
    if exp == 255 then return nil end
    if exp == 0 then return sign * mant * 2 ^ -149 end
    return sign * (1 + mant / 8388608) * 2 ^ (exp - 127)
end
local function float64(s)
    local b = { s:byte(1, 8) }
    local sign = b[8] >= 128 and -1 or 1
    local exp = (b[8] % 128) * 16 + math.floor(b[7] / 16)
    local mant = 0
    for k = 7, 1, -1 do mant = mant * 256 + (k == 7 and b[7] % 16 or b[k]) end
    if exp == 2047 then return nil end
    if exp == 0 then return sign * mant * 2 ^ -1074 end
    return sign * (1 + mant / 2 ^ 52) * 2 ^ (exp - 1023)
end

local DECODE = {
    uint = function(v, wire)
        if v == nil then return 0 end
        return wire == 0 and v or nil
    end,
    int32 = function(v, wire, low)
        if v == nil then return 0 end
        if wire ~= 0 then return nil end
        return low >= 2147483648 and low - 4294967296 or low
    end,
    float = function(v, wire)
        if v == nil then return 0 end
        return wire == 5 and float32(v) or nil
    end,
    double = function(v, wire)
        if v == nil then return 0 end
        return wire == 1 and float64(v) or nil
    end,
    rational = function(v, wire)
        if wire ~= 2 then return nil end
        local num, i = varint(v, 1)
        local den = num and varint(v, i)
        if not den or den == 0 then return nil end
        return { num, den }
    end,
}

local function split_path(path)
    local out = {}
    for n in path:gmatch("%d+") do out[#out + 1] = tonumber(n) end
    return out
end

-- A value from a sample: nil when a message on its path isn't there
local function extract(sample, path, kind, cache)
    local msg, key = sample, ""
    for k = 1, #path - 1 do
        key = key .. "." .. path[k]
        local sub = cache[key]
        if sub == nil then
            local v, wire = field(msg, path[k])
            sub = (wire == 2 and v) or false
            cache[key] = sub
        end
        if not sub then return nil end
        msg = sub
    end
    local v, wire, low = field(msg, path[#path])
    return DECODE[kind](v, wire, low)
end

local function same(a, b)
    if type(a) == "table" and type(b) == "table" then return a[1] == b[1] and a[2] == b[2] end
    return a == b
end

-- The value of a series at time t: the last one recorded at or before t
function M.at(series, t)
    if not series or not t then return nil end
    local times = series.t
    local lo, hi = 1, #times
    if hi == 0 or t < times[1] then return nil end
    while lo < hi do
        local mid = math.floor((lo + hi + 1) / 2)
        if times[mid] <= t then lo = mid else hi = mid - 1 end
    end
    local v = series.v[lo]
    if v == false then return nil end
    return v
end

-- The ProductMeta's model (clip_meta.clip_meta_header.proto_file_name), from the first sample that has it
local function model_of(sample)
    local clip = field(sample, 1)
    local header = type(clip) == "string" and field(clip, 1)
    local name = type(header) == "string" and field(header, 1)
    return type(name) == "string" and name or nil
end

function M.layout(model)
    return LAYOUTS[model]
end

-- { model, values = { <name> = { t = { times }, v = { values } } } }: each series only where the value changes
-- (false where it isn't recorded), for the names asked for that the model's layout has
function M.read(path, names)
    local f = io.open(path, "rb")
    if not f then return nil end
    local ok, result = pcall(function()
        local moov, first = read_moov(f)
        if not moov then return nil end
        local timescale, sa, sb = metadata_track(moov, first)
        if not timescale then return nil end
        local chunks, times, sizes = sample_table(moov, sa, sb, timescale)
        if not chunks or #chunks == 0 then return nil end
        moov = nil

        local out, wanted, model = nil, nil, nil
        for _, chunk in ipairs(chunks) do
            local total = 0
            for k = chunk.first, chunk.first + chunk.count - 1 do total = total + sizes[k] end
            if total > MAX_CHUNK then return nil end
            f:seek("set", chunk.offset)
            local data = f:read(total)
            if not data or #data ~= total then break end
            local pos = 1
            for k = chunk.first, chunk.first + chunk.count - 1 do
                local size = sizes[k]
                local t = times[k]
                if size <= MAX_SAMPLE and size > 4 and t then
                    local sample = data:sub(pos, pos + size - 1)
                    if not model then
                        model = model_of(sample)
                        if model then
                            local layout = LAYOUTS[model]
                            if not layout then return nil end
                            wanted, out = {}, { model = model, values = {} }
                            for _, name in ipairs(names) do
                                if layout[name] then
                                    wanted[#wanted + 1] = { name = name, path = split_path(layout[name][1]), kind = layout[name][2] }
                                    out.values[name] = { t = {}, v = {} }
                                end
                            end
                            if #wanted == 0 then return nil end
                        end
                    end
                    if wanted then
                        local cache = {}
                        for _, w in ipairs(wanted) do
                            local v = extract(sample, w.path, w.kind, cache)
                            if v == nil then v = false end
                            local s = out.values[w.name]
                            local n = #s.t
                            if n == 0 or not same(s.v[n], v) then
                                s.t[n + 1], s.v[n + 1] = t, v
                            end
                        end
                    end
                end
                pos = pos + size
            end
        end
        return out
    end)
    f:close()
    if not ok then return nil, result end
    return result
end

return M
