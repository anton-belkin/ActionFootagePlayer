-- The telemetry a camera recorded, over time: every sample of the file's metadata track, and from each the values
-- the plugin shows. Two formats:
--   DJI    one protobuf message per frame, ProductMeta in DJI's dvtm_*.proto, read along a layout per model
--   GoPro  GPMF (github.com/gopro/gpmf-parser): about a second of KLV per sample, GPS, shutter, ISO, white balance
--
-- The track is the first with a `meta` handler, as dji_color.lua in the dlogm plugin reads it (GoPro's: the one
-- whose samples are `gpmd`); here all its samples are read, through the whole sample table (stts, stsc, stsz,
-- stco/co64). Plain Lua (5.1/LuaJIT: no string.unpack, no bit operations), so the plugin needs no binaries.
--
--   local t = require('telemetry').read(path, { "altitude_dm", "in_water" })
--   t.model                           "dvtm_ac206.proto", or "GoPro HERO6 Black"
--   telemetry.at(t.values.altitude_dm, time)   the value at that time (seconds), or nil
--
-- read() returns nil for files that aren't from a camera it reads, or that have none of the values asked for.

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

-- The metadata track: the first with a `meta` handler whose samples are GoPro's `gpmd`, else the first with a `meta`
-- handler. Its timescale and stbl
local function metadata_track(moov, first)
    local found = nil
    for typ, ta, tb in boxes(moov, first, #moov) do
        if typ == "trak" then
            local ma, mb = child(moov, ta, tb, "mdia")
            local ha = ma and child(moov, ma, mb, "hdlr")
            if ha and moov:sub(ha + 8, ha + 11) == "meta" then
                local da = child(moov, ma, mb, "mdhd")
                local timescale = da and u32(moov, da + (moov:byte(da) == 1 and 20 or 12))
                local ia, ib = child(moov, ma, mb, "minf")
                local sa, sb
                if ia then sa, sb = child(moov, ia, ib, "stbl") end
                if sa and timescale and timescale > 0 then
                    -- stsd: version and flags, entry count, then the first entry's size and format
                    local dd = child(moov, sa, sb, "stsd")
                    if dd and moov:sub(dd + 12, dd + 15) == "gpmd" then return timescale, sa, sb end
                    found = found or { timescale, sa, sb }
                elseif not found then
                    return nil
                end
            end
        end
    end
    if found then return found[1], found[2], found[3] end
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

-- GPMF: KLV items, each a four-character key, a type, the size of one sample and how many there are, the data
-- padded to 4 bytes. Big-endian. Type 0 nests more items; `?` is a struct whose types a TYPE item before it lists
local GPMF_SIZES = { b = 1, B = 1, c = 1, d = 8, f = 4, F = 4, j = 8, J = 8, l = 4, L = 4, q = 4, Q = 8, s = 2, S = 2 }

local function gpmf_items(s, first, last)
    local i = first
    return function()
        if i + 7 > last then return nil end
        local key, typ, size = s:sub(i, i + 3), s:sub(i + 4, i + 4), s:byte(i + 5)
        local count = s:byte(i + 6) * 256 + s:byte(i + 7)
        local a = i + 8
        local b = a + size * count - 1
        if b > last then return nil end
        i = a + math.ceil(size * count / 4) * 4
        return key, typ, size, count, a, b
    end
end

local function be_uint(s, i, n)
    local v = 0
    for k = i, i + n - 1 do v = v * 256 + s:byte(k) end
    return v
end

local function be_float32(s, i)
    local b1, b2, b3, b4 = s:byte(i, i + 3)
    local sign = b1 >= 128 and -1 or 1
    local exp = (b1 % 128) * 2 + math.floor(b2 / 128)
    local mant = (b2 % 128) * 65536 + b3 * 256 + b4
    if exp == 255 then return nil end
    if exp == 0 then return sign * mant * 2 ^ -149 end
    return sign * (1 + mant / 8388608) * 2 ^ (exp - 127)
end

local function be_float64(s, i)
    return float64(s:sub(i, i + 7):reverse())
end

-- One number of GPMF type t at s[i]
local function gpmf_number(s, i, t)
    local n = GPMF_SIZES[t]
    if t == "f" then return be_float32(s, i) end
    if t == "d" then return be_float64(s, i) end
    if t == "Q" then return be_uint(s, i, 4) - (s:byte(i) >= 128 and 4294967296 or 0) + be_uint(s, i + 4, 4) / 4294967296 end
    local v = be_uint(s, i, n)
    if (t == "b" or t == "s" or t == "l" or t == "q" or t == "j") and s:byte(i) >= 128 then v = v - 256 ^ n end
    if t == "q" then v = v / 65536 end
    return v
end

-- An item's samples as rows of numbers, each divided by its SCAL (one per column, or one for all). types: the
-- struct's types, for `?`
local function gpmf_rows(s, typ, size, count, a, scale, types)
    local columns = {}
    if typ == "?" then
        for c in (types or ""):gmatch(".") do columns[#columns + 1] = c end
    elseif GPMF_SIZES[typ] then
        for _ = 1, size / GPMF_SIZES[typ] do columns[#columns + 1] = typ end
    end
    local width = 0
    for _, c in ipairs(columns) do
        if not GPMF_SIZES[c] then return {} end
        width = width + GPMF_SIZES[c]
    end
    if width ~= size or width == 0 then return {} end
    local rows = {}
    for r = 0, count - 1 do
        local row, i = {}, a + r * size
        for k, c in ipairs(columns) do
            local v = gpmf_number(s, i, c)
            local sc = scale and (scale[k] or (#scale == 1 and scale[1])) or nil
            row[k] = (v and sc and sc ~= 0) and v / sc or v
            i = i + GPMF_SIZES[c]
        end
        rows[#rows + 1] = row
    end
    return rows
end

-- The values of one GPMF sample, recorded from t over dur seconds: push(name, time, value) for each, the samples of
-- an item spread evenly over the time. Returns the device's name (DVNM), when it has one
local function gpmf_sample(sample, t, dur, push)
    local model = nil
    local function spread(rows, f)
        for k, row in ipairs(rows) do f(t + dur * (k - 1) / #rows, row) end
    end
    local function position(time, fix, lat, lon, ms)
        push("latitude_deg", time, fix and lat or false)
        push("longitude_deg", time, fix and lon or false)
        push("speed_ms", time, fix and ms and math.floor(ms * 100 + 0.5) / 100 or false)
    end
    for key, typ, _, _, a, b in gpmf_items(sample, 1, #sample) do
        if key == "DEVC" and typ == "\0" then
            for k2, t2, sz2, n2, a2, b2 in gpmf_items(sample, a, b) do
                if k2 == "DVNM" and t2 == "c" then
                    model = model or sample:sub(a2, b2):gsub("%z+$", "")
                elseif k2 == "STRM" and t2 == "\0" then
                    local scale, types, fix = nil, nil, nil
                    for k3, t3, sz3, n3, a3 in gpmf_items(sample, a2, b2) do
                        if k3 == "SCAL" then
                            scale = {}
                            for _, row in ipairs(gpmf_rows(sample, t3, sz3, n3, a3)) do scale[#scale + 1] = row[1] end
                        elseif k3 == "TYPE" then
                            types = sample:sub(a3, a3 + sz3 * n3 - 1):gsub("%z+$", "")
                        elseif k3 == "GPSF" then
                            fix = (gpmf_rows(sample, t3, sz3, n3, a3)[1] or {})[1]
                        elseif k3 == "GPS5" then
                            -- latitude, longitude, altitude, 2D speed, 3D speed; the fix (0 none, 2 2D, 3 3D) before it
                            spread(gpmf_rows(sample, t3, sz3, n3, a3, scale), function(time, r)
                                position(time, (fix or 0) >= 2, r[1], r[2], r[4])
                            end)
                        elseif k3 == "GPS9" then
                            -- HERO11 and later: the same, then days, seconds, DOP and the fix, per row
                            spread(gpmf_rows(sample, t3, sz3, n3, a3, scale, types), function(time, r)
                                position(time, (r[9] or 0) >= 2, r[1], r[2], r[4])
                            end)
                        elseif k3 == "SHUT" then
                            spread(gpmf_rows(sample, t3, sz3, n3, a3, scale), function(time, r)
                                push("exposure_time", time, r[1] and r[1] > 0 and { 1, math.floor(1 / r[1] + 0.5) } or false)
                            end)
                        elseif k3 == "ISOE" or k3 == "WBAL" then
                            local name = k3 == "ISOE" and "iso" or "white_balance_k"
                            spread(gpmf_rows(sample, t3, sz3, n3, a3, scale), function(time, r)
                                push(name, time, r[1] and math.floor(r[1] + 0.5) or false)
                            end)
                        end
                    end
                end
            end
        end
    end
    return model
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

-- Chunks of the track read between the steps of reader(): a long clip from an SD card takes ~20 s (a read per frame,
-- spread over the file), and the script that reads stays free for mpv's events meanwhile, quitting among them
local CHUNKS_PER_STEP = 50
-- stdio's buffer, read whole on each seek: by default the file system's block, which is 1 MB on an SD card's exFAT,
-- so reading the samples read the whole file (7 GB: 75 s). A sample is a few hundred bytes
local BUFFER = 16384

local function read_all(f, names)
    local moov, first = read_moov(f)
    if not moov then return nil end
    local timescale, sa, sb = metadata_track(moov, first)
    if not timescale then return nil end
    local chunks, times, sizes = sample_table(moov, sa, sb, timescale)
    if not chunks or #chunks == 0 then return nil end
    moov = nil

    local out, wanted, model = nil, nil, nil
    -- GoPro: the names asked for, and what pushes a value of one
    local gpmf, push = nil, nil
    for c, chunk in ipairs(chunks) do
        if c % CHUNKS_PER_STEP == 0 then coroutine.yield() end
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
                if not model and not gpmf and sample:sub(1, 4) == "DEVC" then
                    gpmf, out = {}, { model = "GoPro", values = {} }
                    for _, name in ipairs(names) do gpmf[name] = true end
                    push = function(name, time, v)
                        if not gpmf[name] then return end
                        local series = out.values[name]
                        if not series then
                            series = { t = {}, v = {} }
                            out.values[name] = series
                        end
                        local n = #series.t
                        if n == 0 or not same(series.v[n], v) then
                            series.t[n + 1], series.v[n + 1] = time, v
                        end
                    end
                end
                if gpmf then
                    local dur = (times[k + 1] or t + 1) - t
                    local device = gpmf_sample(sample, t, dur, push)
                    if device and out.model == "GoPro" then out.model = device:find("^GoPro") and device or "GoPro " .. device end
                elseif not model then
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
                if wanted and not gpmf then
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
    if gpmf then
        -- A series that never had a value says nothing
        local any = false
        for name, series in pairs(out.values) do
            local has = false
            for _, v in ipairs(series.v) do has = has or v ~= false end
            if has then any = true else out.values[name] = nil end
        end
        if not any then return nil end
    end
    return out
end

-- { model, values = { <name> = { t = { times }, v = { values } } } }: each series only where the value changes
-- (false where it isn't recorded), for the names asked for that the model's layout has. Read in steps:
-- reader(path, names) returns step(), which reads a part and returns false while there's more, else true, the result
-- (or nil) and an error; and close(), to give it up
function M.reader(path, names)
    local f = io.open(path, "rb")
    if not f then return function() return true end, function() end end
    f:setvbuf("full", BUFFER)
    -- The coroutine's own error handling: Lua 5.1 can't yield inside a pcall
    local co = coroutine.create(function() return read_all(f, names) end)
    local finished, result, err = false, nil, nil
    local function close()
        if not finished then f:close() end
        finished = true
    end
    local function step()
        if finished then return true, result, err end
        local ok, value = coroutine.resume(co)
        if not ok then
            close()
            err = value
        elseif coroutine.status(co) == "dead" then
            close()
            result = value
        end
        return finished, result, err
    end
    return step, close
end

-- The same at once
function M.read(path, names)
    local step = M.reader(path, names)
    while true do
        local finished, result, err = step()
        if finished then return result, err end
    end
end

return M
