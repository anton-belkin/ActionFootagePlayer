-- The link a moment's, clip preview's or export's MP4 keeps to its original (docs/moments-and-workspaces.md, "The
-- link inside"): the text key `ch.actionfootageplayer.link`, JSON, read without mpv opening the file, so that the
-- player can go through a folder of them. Plain Lua (5.1/LuaJIT), as markers.lua in the highlights plugin;
-- tests/link_test.lua.
--
-- The keys are QuickTime's `mdta` keys: in `moov/meta`, where gyroflow-export writes them (vs-gyroflow/src/
-- mp4meta.rs, the writer) and Apple keeps its own, or in `moov/udta/meta`, where ffmpeg's `-movflags
-- use_metadata_tags` puts them (a file remuxed with -map_metadata 0). Only the boxes' headers are read on the way:
-- seeks over mdat, wherever the moov is.
--
--   local link = require 'link'
--   link.read(path)        -> the link's JSON text, nil for none
--   link.read_keys(path)   -> { [key] = value } of all the mdta keys, nil when it can't be read
--   link.created(path)     -> the file's own creation time (moov/mvhd), unix seconds, nil when it has none: what
--                             ffmpeg and mpv show as creation_time and links keep as `created`

local M = {}

M.KEY = "ch.actionfootageplayer.link"

-- What's read of a meta box at most: a link is some hundred bytes
local MAX_META = 256 * 1024
local BUFFER = 16384

local function u32(s, i)
    local a, b, c, d = s:byte(i, i + 3)
    if not d then return nil end
    return ((a * 256 + b) * 256 + c) * 256 + d
end
local function u64(s, i)
    local hi, lo = u32(s, i), u32(s, i + 4)
    return hi and lo and hi * 4294967296 + lo
end

-- The boxes from first to last (file offsets, last excluded), by their headers: an iterator of type, payload
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

-- The same over a string s[first..last] (1-based, inclusive)
local function sboxes(s, first, last)
    local pos = first
    return function()
        if pos + 7 > last then return nil end
        local size, typ = u32(s, pos), s:sub(pos + 4, pos + 7)
        if not size or size < 8 or pos + size - 1 > last then return nil end
        local start = pos
        pos = pos + size
        return typ, start + 8, start + size - 1
    end
end

-- A meta box's mdta keys, its body s: with version and flags before its boxes (ISO, ffmpeg) or without (Apple)
local function meta_keys(s, out)
    local first = s:sub(5, 8) == "hdlr" and 1 or 5
    local handler, names, ilst
    for typ, a, b in sboxes(s, first, #s) do
        if typ == "hdlr" then
            handler = s:sub(a + 8, a + 11)
        elseif typ == "keys" then
            names = {}
            local n, pos = u32(s, a + 4) or 0, a + 8
            for _ = 1, n do
                local size = u32(s, pos)
                if not size or size < 8 or pos + size - 1 > b then break end
                names[#names + 1] = s:sub(pos + 8, pos + size - 1)
                pos = pos + size
            end
        elseif typ == "ilst" then
            ilst = { a, b }
        end
    end
    if handler ~= "mdta" or not names or not ilst then return end
    for typ, a, b in sboxes(s, ilst[1], ilst[2]) do
        local name = names[u32(typ, 1) or 0]
        if name then
            for dt, da, db in sboxes(s, a, b) do
                -- UTF-8 text (type 1)
                if dt == "data" and db - da + 1 >= 8 and u32(s, da) == 1 and out[name] == nil then
                    out[name] = s:sub(da + 8, db)
                end
            end
        end
    end
end

local function read_meta(f, a, b, out)
    if b - a > MAX_META then return end
    f:seek("set", a)
    local s = f:read(b - a)
    if s and #s == b - a then meta_keys(s, out) end
end

function M.read_keys(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    f:setvbuf("full", BUFFER)
    local size = f:seek("end")
    local out = {}
    for typ, a, b in boxes(f, 0, size) do
        if typ == "moov" then
            -- moov/meta first: what gyroflow-export writes wins over a copy ffmpeg made
            local udta = {}
            for mt, ma, mb in boxes(f, a, b) do
                if mt == "meta" then read_meta(f, ma, mb, out)
                elseif mt == "udta" then udta[#udta + 1] = { ma, mb } end
            end
            for _, u in ipairs(udta) do
                for ut, ua, ub in boxes(f, u[1], u[2]) do
                    if ut == "meta" then read_meta(f, ua, ub, out) end
                end
            end
            break
        end
    end
    f:close()
    return out
end

-- Seconds from 1904-01-01 (QuickTime's epoch) to 1970-01-01
local EPOCH_1904 = 2082844800

function M.created(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    f:setvbuf("full", BUFFER)
    local size = f:seek("end")
    local out
    for typ, a, b in boxes(f, 0, size) do
        if typ == "moov" then
            for mt, ma in boxes(f, a, b) do
                if mt == "mvhd" then
                    f:seek("set", ma)
                    local h = f:read(12) or ""
                    local t = h:byte(1) == 1 and u64(h, 5) or u32(h, 5)
                    if t and t > 0 then out = t - EPOCH_1904 end
                    break
                end
            end
            break
        end
    end
    f:close()
    return out
end

function M.read(path)
    local keys = M.read_keys(path)
    return keys and keys[M.KEY]
end

return M
