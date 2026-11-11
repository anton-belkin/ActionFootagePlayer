-- The clip's own file beside it, `<clip>.afp.json` (docs/moments-and-workspaces.md, "The clip's file"): its marks
-- (the highlights plugin's) and the view it's saved to play in (the workspace plugin's: log curve, stabilization,
-- lens, aspect and rotation). Each part is written on its own: the file is read again, the part replaced, the rest
-- (and fields this doesn't know) kept, and it's written whole through a temporary file renamed over it.
--
-- Before view settings it was `<clip>.marks.json`, marks only: such a file is read when there's no .afp.json, and the
-- first write moves it over (written as .afp.json, the old one removed).
--
-- `marks` absent: the camera's marks are the clip's (nothing changed yet); present, even empty: they're these.
-- `view` absent: the clip plays as the defaults say.
-- `view360` (the view360 plugin's): a 360° clip's recorded view, `view360.recording` (docs/360-reframe.md).
-- `tags` (a list of names) and `rating` (1–5 stars): the clip's, set in the player (K) or the Library (tags.lua).
--
-- Plain Lua, no mpv (tests/sidecar_test.lua):
--   local sidecar = require('sidecar').new({ parse_json, format_json, windows, own_path })
--   sidecar.path(clip)               where it's written: <clip without its extension>.afp.json; with own_path, where
--                                    that says (the clip's own file over the project's locations), nil when there are
--                                    two (then nothing is written: update fails, read gives the first)
--   sidecar.old_path(clip)           <clip>.marks.json
--   sidecar.read(clip)               -> doc, file (nil: none, or not one of ours)
--   sidecar.update(clip, fn)         fn(doc) changes the doc read fresh (a new one when there's none): ok, err
--   env.note(file), when given, is told of each file written or removed (the project's catalog notes it)

local M = {}

M.FORMAT = "actionfootageplayer-clip"
-- What the file said it was before it held view settings
M.OLD_FORMAT = "actionfootageplayer-marks"
M.EXT = ".afp.json"
M.OLD_EXT = ".marks.json"

function M.new(env)
    local s = {}
    local windows = env.windows

    local function base(clip)
        return clip:match(windows and "^(.*)%.[^./\\]+$" or "^(.*)%.[^./]+$") or clip
    end
    -- The clip's own file: own_path(clip) -> the file, or nil and the files when there are two or more
    local function own(clip)
        if env.own_path then
            local p, list = env.own_path(clip)
            if p or list then return p, list end
        end
        return base(clip) .. M.EXT
    end
    function s.path(clip) return (own(clip)) end
    function s.old_path(clip)
        local p, list = own(clip)
        p = p or (list and list[1])
        return p and (p:gsub("%.afp%.json$", "") .. M.OLD_EXT) or base(clip) .. M.OLD_EXT
    end

    local function read_file(file)
        local f = io.open(file, "r")
        if not f then return nil end
        local text = f:read("*a") or ""
        f:close()
        return text
    end

    -- The doc in `file`: nil when there's none, false when it's something else
    local function parse(file)
        local text = read_file(file)
        if not text then return nil end
        local doc = env.parse_json(text)
        if type(doc) ~= "table" or (doc.format ~= M.FORMAT and doc.format ~= M.OLD_FORMAT) then return false end
        return doc
    end

    -- The clip's doc and the file it's in: the .afp.json, else the .marks.json from before
    function s.read(clip)
        local p, list = own(clip)
        for _, file in ipairs({ p or list[1], s.old_path(clip) }) do
            local doc = parse(file)
            if doc then return doc, file end
            if doc == false then return nil, nil, file .. ": not one of the player's files, left alone" end
        end
        return nil, nil
    end

    local function write(file, doc)
        local json = env.format_json(doc)
        if not json then return false, "couldn't make the file" end
        -- An empty list is an array
        json = json:gsub('"marks":{}', '"marks":[]')
        local tmp = file .. ".tmp"
        local f, err = io.open(tmp, "w")
        if not f then return false, err end
        f:write(json, "\n")
        f:close()
        os.remove(file)
        local ok, rerr = os.rename(tmp, file)
        if not ok then os.remove(tmp); return false, rerr end
        return true
    end

    -- The doc read again, changed by fn, written as .afp.json; a .marks.json it came from removed. fn returning false:
    -- nothing written. A doc left with no marks, no view, no source (the input: curve, turn, lens under water), no
    -- 360 recording, no location (the Library's, docs/geolocation.md), no tags and no rating (tags.lua): the file removed
    function s.update(clip, fn)
        local p, list = own(clip)
        if not p then
            return false, "this clip has " .. #list .. " files of its own (" .. table.concat(list, ", ") .. "): keep one, nothing is saved until then"
        end
        local doc, from, problem = s.read(clip)
        if problem then return false, problem end
        doc = doc or { format = M.FORMAT, version = 1 }
        if fn(doc) == false then return true end
        doc.format = M.FORMAT
        local file = p
        if doc.marks == nil and doc.view == nil and doc.source == nil and doc.view360 == nil and doc.location == nil
            and (type(doc.tags) ~= "table" or next(doc.tags) == nil) and doc.rating == nil then
            if from then os.remove(from); if env.note then env.note(from) end end
            return true
        end
        local ok, err = write(file, doc)
        if not ok then return false, err end
        if from and from ~= file then os.remove(from); if env.note then env.note(from) end end
        if env.note then env.note(file) end
        return true
    end

    return s
end

return M
