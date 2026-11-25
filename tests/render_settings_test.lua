-- What a render is made with (transform.render_settings), case by case from tests/render_settings.json: the same cases
-- Action Footage Library's renders::settings is checked against (cargo test in library/), so the recipe the Library
-- hashes and what the player renders can't part. From the repository's folder: luajit tests/render_settings_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local t = require 'transform'

-- A small JSON reader: enough for the cases (objects, arrays, strings without escapes but \", numbers, true, false, null)
local NULL = {}
local function parse(s)
    local i = 1
    local function ws() i = s:find("[^ \t\r\n]", i) or #s + 1 end
    local value
    local function str()
        local j, out = i + 1, {}
        while true do
            local c = s:sub(j, j)
            if c == '"' then i = j + 1; return table.concat(out) end
            if c == "\\" then j = j + 1; c = s:sub(j, j) end
            out[#out + 1] = c
            j = j + 1
        end
    end
    function value()
        ws()
        local c = s:sub(i, i)
        if c == "{" then
            local o = {}
            i = i + 1; ws()
            if s:sub(i, i) == "}" then i = i + 1; return o end
            while true do
                ws(); local k = str(); ws(); i = i + 1
                local v = value()
                if v ~= NULL then o[k] = v end
                ws(); c = s:sub(i, i); i = i + 1
                if c == "}" then return o end
            end
        elseif c == "[" then
            local a = {}
            i = i + 1; ws()
            if s:sub(i, i) == "]" then i = i + 1; return a end
            while true do
                a[#a + 1] = value()
                ws(); c = s:sub(i, i); i = i + 1
                if c == "]" then return a end
            end
        elseif c == '"' then return str()
        elseif s:sub(i, i + 3) == "true" then i = i + 4; return true
        elseif s:sub(i, i + 4) == "false" then i = i + 5; return false
        elseif s:sub(i, i + 3) == "null" then i = i + 4; return NULL
        else
            local n = s:match("^-?[%d%.eE+-]+", i)
            i = i + #n
            return tonumber(n)
        end
    end
    return value()
end

local f = assert(io.open("tests/render_settings.json", "r"))
local doc = parse(f:read("*a"))
f:close()

local function show(v)
    if type(v) ~= "table" then return type(v) == "string" and string.format("%q", v) or tostring(v) end
    local keys = {}
    for k in pairs(v) do keys[#keys + 1] = k end
    table.sort(keys)
    local parts = {}
    for _, k in ipairs(keys) do parts[#parts + 1] = k .. "=" .. show(v[k]) end
    return "{" .. table.concat(parts, ",") .. "}"
end

local failures, checks = 0, 0
local print_all = os.getenv("PRINT")
for _, c in ipairs(doc.cases) do
    local own = c.own ~= NULL and c.own or nil
    local d = c.doc ~= NULL and c.doc or nil
    local _, rendered = t.render_settings(d, own, c.defaults or doc.defaults)
    checks = checks + 1
    if print_all then print(c.name .. ": " .. show(rendered)) end
    if c.want and show(rendered) ~= show(c.want) then
        failures = failures + 1
        print(("FAIL %s:\n  got  %s\n  want %s"):format(c.name, show(rendered), show(c.want)))
    elseif not c.want then
        failures = failures + 1
        print("FAIL " .. c.name .. ": no `want`")
    end
end
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
