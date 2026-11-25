-- The lists the player, gyroflow-export and the Library share (registry.json beside this file: the curves, an export's
-- settings, the transform's keys, the project's own folders), read once on the first require. Plain Lua, no mpv: the
-- JSON is read here rather than by mp.utils.parse_json, so tests with a stand-in mpv read the same file.
--
--   local reg = require 'registry'
--   reg.curves, reg.export, reg.settings, reg.own_folders, reg.recycle_bins   registry.json's, as they are
--   reg.curve(id)            -> that curve's entry, nil for none
--   reg.keys(group)          -> the settings' keys of a group (bundle, uw, input, opts), in order
--   reg.setting(key)         -> that setting's entry, nil for none

local function here()
    local src = debug.getinfo(1, "S").source
    if src:sub(1, 1) ~= "@" then return "" end
    return src:sub(2):match("^(.*[/\\])") or ""
end

-- A JSON reader: objects, arrays, strings (with escapes, \u as UTF-8), numbers, true, false, null (nil)
local function decode(text, where)
    local i = 1
    local function fail(what) error(("%s: %s at character %d"):format(where, what, i), 0) end
    local function space() i = text:find("[^ \t\r\n]", i) or #text + 1 end
    local value
    local function str()
        local out = {}
        i = i + 1
        while true do
            local j = text:find('["\\]', i)
            if not j then fail("unclosed string") end
            out[#out + 1] = text:sub(i, j - 1)
            if text:sub(j, j) == '"' then i = j + 1; break end
            local e = text:sub(j + 1, j + 1)
            local plain = { ['"'] = '"', ["\\"] = "\\", ["/"] = "/", b = "\b", f = "\f", n = "\n", r = "\r", t = "\t" }
            if plain[e] then
                out[#out + 1] = plain[e]; i = j + 2
            elseif e == "u" then
                local c = tonumber(text:sub(j + 2, j + 5), 16) or fail("bad \\u")
                if c < 0x80 then out[#out + 1] = string.char(c)
                elseif c < 0x800 then out[#out + 1] = string.char(0xC0 + math.floor(c / 64), 0x80 + c % 64)
                else out[#out + 1] = string.char(0xE0 + math.floor(c / 4096), 0x80 + math.floor(c / 64) % 64, 0x80 + c % 64) end
                i = j + 6
            else
                fail("bad escape")
            end
        end
        return table.concat(out)
    end
    value = function()
        space()
        local c = text:sub(i, i)
        if c == "{" then
            local t = {}
            i = i + 1; space()
            if text:sub(i, i) == "}" then i = i + 1; return t end
            while true do
                space()
                if text:sub(i, i) ~= '"' then fail("a key expected") end
                local k = str()
                space()
                if text:sub(i, i) ~= ":" then fail("':' expected") end
                i = i + 1
                t[k] = value()
                space()
                local d = text:sub(i, i); i = i + 1
                if d == "}" then return t end
                if d ~= "," then fail("',' or '}' expected") end
            end
        elseif c == "[" then
            local t = {}
            i = i + 1; space()
            if text:sub(i, i) == "]" then i = i + 1; return t end
            while true do
                t[#t + 1] = value()
                space()
                local d = text:sub(i, i); i = i + 1
                if d == "]" then return t end
                if d ~= "," then fail("',' or ']' expected") end
            end
        elseif c == '"' then
            return str()
        elseif text:sub(i, i + 3) == "true" then i = i + 4; return true
        elseif text:sub(i, i + 4) == "false" then i = i + 5; return false
        elseif text:sub(i, i + 3) == "null" then i = i + 4; return nil
        end
        local n = text:match("^-?%d+%.?%d*[eE]?[-+]?%d*", i)
        if not n or n == "" then fail("a value expected") end
        i = i + #n
        return tonumber(n) or fail("bad number " .. n)
    end
    local out = value()
    space()
    if i <= #text then fail("text after the end") end
    return out
end

local path = here() .. "registry.json"
local f = io.open(path, "rb")
if not f then error("can't read " .. path, 0) end
local M = decode(f:read("*a"), path)
f:close()
M.decode = decode

local curve_by_id, setting_by_key = {}, {}
for _, c in ipairs(M.curves) do curve_by_id[c.id] = c end
for _, s in ipairs(M.settings) do setting_by_key[s.key] = s end
function M.curve(id) return curve_by_id[id] end
function M.setting(key) return setting_by_key[key] end
function M.keys(group)
    local out = {}
    for _, s in ipairs(M.settings) do if s.group == group then out[#out + 1] = s.key end end
    return out
end

return M
