-- The Protune color a GoPro recorded a video in: PTCL ("NATURAL", "FLAT", "LOG", "GPLOG2", ...) in the global
-- settings GoPro writes as GPMF into moov/udta/GPMF (github.com/gopro/gpmf-parser, "DVID 1, Global Settings").
-- Read on a HERO12 Black clip (NATURAL); GPLOG2, MISSION 1's GP-Log2 by gpmf-parser's list, not seen on a clip yet.
--
--   require('gopro_color').read(path)  ->  "GPLOG2", "LOG", "NATURAL", ..., or nil when it can't tell

local mp4 = require 'dji_color'

local M = {}

-- The data of the first KLV item with the key in s[first..last], looking into nested ones (type 0). An item: the key
-- (4 bytes), its type (1), the size of one sample (1) and how many (2, big-endian), then the data, padded to 4 bytes
local function klv(s, first, last, wanted)
    local pos = first
    while pos + 7 <= last do
        local key, typ = s:sub(pos, pos + 3), s:byte(pos + 4)
        local n = s:byte(pos + 5) * (s:byte(pos + 6) * 256 + s:byte(pos + 7))
        local a = pos + 8
        if a + n - 1 > last then return nil end
        if key == wanted then return s:sub(a, a + n - 1) end
        if typ == 0 then
            local v = klv(s, a, a + n - 1, wanted)
            if v then return v end
        end
        pos = a + math.ceil(n / 4) * 4
    end
    return nil
end

local function protune_color(f)
    local moov, first = mp4.read_moov(f)
    if not moov then return nil end
    local ua, ub = mp4.child(moov, first, #moov, "udta")
    local ga, gb
    if ua then ga, gb = mp4.child(moov, ua, ub, "GPMF") end
    if not ga then return nil end
    local v = klv(moov, ga, gb, "PTCL")
    v = v and v:gsub("%z.*$", ""):match("^%s*(.-)%s*$")
    return v ~= "" and v or nil
end

function M.read(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local ok, color = pcall(protune_color, f)
    f:close()
    return ok and color or nil
end

return M
