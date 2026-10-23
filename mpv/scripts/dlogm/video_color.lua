-- The transfer and primaries an MP4/MOV says its video is in: the colr box (nclx, or QuickTime's nclc) of the first
-- video track's sample entry, with ITU-T H.273's codes, as ffmpeg writes it for a file tagged HDR (the player's own
-- exports among them) and phones and cameras do. Read before the file plays, so an HDR file is shown as one from
-- its first frame, and never taken for log footage whatever metadata it carries.
--
--   require('video_color').read(path)  ->  "hlg" or "pq" (nil: SDR, or it can't tell), and the primaries:
--                                          "bt.2020", "display-p3", "bt.709" or nil; as mpv's target-trc and
--                                          target-prim take them

local mp4 = require 'dji_color'

local M = {}

local TRANSFERS = { [16] = "pq", [18] = "hlg" }
local PRIMARIES = { [1] = "bt.709", [9] = "bt.2020", [12] = "display-p3" }
-- A visual sample entry's own fields, before its boxes
local VISUAL_SAMPLE_ENTRY = 78

local function u16(s, i)
    local a, b = s:byte(i, i + 1)
    return b and a * 256 + b
end

local function colr(f)
    local moov, first = mp4.read_moov(f)
    if not moov then return nil end
    for typ, ta, tb in mp4.boxes(moov, first, #moov) do
        if typ == "trak" then
            local ma, mb = mp4.child(moov, ta, tb, "mdia")
            local ha = ma and mp4.child(moov, ma, mb, "hdlr")
            if ha and moov:sub(ha + 8, ha + 11) == "vide" then
                local ia, ib = mp4.child(moov, ma, mb, "minf")
                local sa, sb, da, db
                if ia then sa, sb = mp4.child(moov, ia, ib, "stbl") end
                if sa then da, db = mp4.child(moov, sa, sb, "stsd") end
                if not da then return nil end
                -- stsd: version and flags, the entry count, then the entries; the first one's boxes
                for _, ea, eb in mp4.boxes(moov, da + 8, db) do
                    for t, ca in mp4.boxes(moov, ea + VISUAL_SAMPLE_ENTRY, eb) do
                        local kind = moov:sub(ca, ca + 3)
                        if t == "colr" and (kind == "nclx" or kind == "nclc") then
                            return u16(moov, ca + 6), u16(moov, ca + 4)
                        end
                    end
                    return nil
                end
                return nil
            end
        end
    end
    return nil
end

function M.read(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local ok, transfer, primaries = pcall(colr, f)
    f:close()
    if not ok or not transfer then return nil end
    return TRANSFERS[transfer], PRIMARIES[primaries]
end

return M
