-- The project's own file, `<project>/Project.afproject` (docs/moments-and-workspaces.md): what marks a folder as a
-- project, and what the player and Action Footage Library keep for it. A double click on it opens the project in Action
-- Footage Library. JSON:
--
--   format        "actionfootage-project"; version 1
--   settings      the project's settings: clips_flat (clip previews all in _Clips/ itself), moments_alone (new
--                 moments of the marks stabilized on their own)
--   clips_made    a line per clip a clip preview was made for (moments.lua's made_line: its path and size), so that
--                 one deleted (not interesting) isn't made again
--   clips_failed  the last clip previews that couldn't be made: { <the clip's path>, <why> }
--   stills_made   a line per mark a photo was made of (its clip's path, size and time), so that one deleted isn't
--                 made again
--   moment_names  the names typed for moments, the last one last (the name dialog suggests the next)
--
-- Since 0.11 (docs/library-management.md §8, "The project is its files") the player writes none of these lists: what
-- was made is found by the link inside each file, settings are chosen per run or kept on this computer, names are the
-- moments'. Older files are read as they are; empty lists aren't written.
--
-- Each change reads the file again, changes it and writes it whole through a temporary file renamed over it, keeping
-- what this doesn't know: the player and Action Footage Library write it both.
--
-- Up to 0.8 it was a folder, `_ActionFootagePlayer/`, with a file for each (clips-made.txt, clips-failed.txt,
-- stills-made.txt, settings.json, moment-names.txt): read when there's no file, and moved into it by migrate().
--
-- Plain Lua, no mpv (tests/projectfile_test.lua):
--   local pf = require('projectfile').new({ parse_json, format_json, join, is_dir, is_file, rmdir })
--   pf.path(root)          <root>/Project.afproject
--   pf.read(root)          -> doc (the old folder's when there's no file; an empty one when neither)
--   pf.update(root, fn)    fn(doc) changes the doc read fresh: ok, err (writes the file, which makes the project)
--   pf.migrate(root)       the old folder moved into the file, the folder removed once empty: true when it moved

local M = {}

M.FILE = "Project.afproject"
M.FORMAT = "actionfootage-project"
-- The project's version (afp-catalog's project::VERSION, which says what each means): a newer one isn't written by
-- this player, whose writes would lose what it doesn't know; an older one is raised to it
M.VERSION = 3
M.LEGACY_DIR = "_ActionFootagePlayer"
-- The old folder's files, and what each became
local LEGACY = {
    { file = "clips-made.txt", key = "clips_made", kind = "lines" },
    { file = "clips-failed.txt", key = "clips_failed", kind = "pairs" },
    { file = "stills-made.txt", key = "stills_made", kind = "lines" },
    { file = "moment-names.txt", key = "moment_names", kind = "lines" },
    { file = "settings.json", key = "settings", kind = "json" },
}

function M.new(env)
    local p = {}
    local join = env.join

    function p.path(root) return join(root, M.FILE) end
    function p.legacy_dir(root) return join(root, M.LEGACY_DIR) end

    local function read_text(file)
        local f = io.open(file, "rb")
        if not f then return nil end
        local text = f:read("*a") or ""
        f:close()
        return text
    end

    local function lines(text)
        local out = {}
        for line in (text or ""):gmatch("[^\r\n]+") do
            if line:match("%S") then out[#out + 1] = line end
        end
        return out
    end

    local function blank()
        return { format = M.FORMAT, version = 1, settings = {}, clips_made = {}, clips_failed = {}, stills_made = {},
                 moment_names = {} }
    end

    -- Every list and the settings present (a file written by hand, or by an older version, may lack some)
    local function complete(doc)
        local b = blank()
        for k, v in pairs(b) do
            if type(doc[k]) ~= type(v) then doc[k] = v end
        end
        -- Never lowered (raised to this player's when written)
        doc.format, doc.version = M.FORMAT, math.max(tonumber(doc.version) or 1, 1)
        return doc
    end

    -- The old folder's files as a doc, nil when there's no old folder
    function p.read_legacy(root)
        local dir = p.legacy_dir(root)
        if not env.is_dir(dir) then return nil end
        local doc = blank()
        for _, l in ipairs(LEGACY) do
            local text = read_text(join(dir, l.file))
            if text then
                if l.kind == "lines" then
                    doc[l.key] = lines(text)
                elseif l.kind == "pairs" then
                    for _, line in ipairs(lines(text)) do
                        local rel, why = line:match("^([^\t]*)\t(.*)$")
                        doc[l.key][#doc[l.key] + 1] = { rel or line, why or "" }
                    end
                else
                    local t = env.parse_json(text)
                    if type(t) == "table" then doc[l.key] = t end
                end
            end
        end
        return doc
    end

    -- The doc in the file, nil when there's none (or it isn't ours)
    local function read_file(root)
        local text = read_text(p.path(root))
        if not text then return nil end
        local doc = env.parse_json(text)
        if type(doc) ~= "table" or (doc.format ~= nil and doc.format ~= M.FORMAT) then return nil end
        return complete(doc)
    end

    function p.read(root)
        return read_file(root) or p.read_legacy(root) or blank()
    end

    -- What's written: since 0.11 (docs/library-management.md §8) the file keeps only where the footage is; the lists of
    -- before are kept while they have something (the Library takes what it needs from them), empty ones left out
    local OLD_LISTS = { "settings", "clips_made", "clips_failed", "stills_made", "moment_names" }
    local function written(doc)
        local out = {}
        for k, v in pairs(doc) do out[k] = v end
        for _, k in ipairs(OLD_LISTS) do
            if type(out[k]) == "table" and next(out[k]) == nil then out[k] = nil end
        end
        return out
    end

    local function write(root, doc)
        doc = written(doc)
        local file = p.path(root)
        local tmp = file .. ".tmp"
        local f, err = io.open(tmp, "wb")
        if not f then return false, err end
        f:write(env.format_json(doc) or "{}", "\n")
        f:close()
        -- rename over an existing file fails on Windows
        local ok = os.rename(tmp, file)
        if not ok then
            os.remove(file)
            ok, err = os.rename(tmp, file)
        end
        if not ok then os.remove(tmp) end
        return ok and true or false, err
    end

    function p.update(root, fn)
        -- A file of that name that isn't ours is left alone
        if env.is_file(p.path(root)) and not read_file(root) then return false, M.FILE .. " isn't Action Footage's" end
        -- One of a newer version left as it is: update the player to work with it
        local text = read_text(p.path(root))
        local now = text and env.parse_json(text)
        if type(now) == "table" and (tonumber(now.version) or 1) > M.VERSION then
            return false, M.FILE .. " is of a newer version (" .. tostring(now.version) .. ") than this player knows (" .. M.VERSION .. "): update it"
        end
        local doc = p.read(root)
        fn(doc)
        doc = complete(doc)
        doc.version = math.max(doc.version, M.VERSION)
        return write(root, doc)
    end

    function p.migrate(root)
        local dir = p.legacy_dir(root)
        if not env.is_dir(dir) then return false end
        -- A file already there (made by a newer player beside an older one) is the one kept; what only the folder
        -- has is added to it
        local doc = read_file(root)
        local old = p.read_legacy(root)
        if doc then
            for _, key in ipairs({ "clips_made", "stills_made", "moment_names" }) do
                local seen = {}
                for _, v in ipairs(doc[key]) do seen[v] = true end
                for _, v in ipairs(old[key]) do if not seen[v] then doc[key][#doc[key] + 1] = v end end
            end
            for k, v in pairs(old.settings) do if doc.settings[k] == nil then doc.settings[k] = v end end
        else
            doc = old
        end
        if not write(root, doc) or not read_file(root) then return false end
        -- Only the files moved over are removed: anything else keeps the folder
        for _, l in ipairs(LEGACY) do os.remove(join(dir, l.file)) end
        if env.rmdir then env.rmdir(dir) end
        return true
    end

    return p
end

return M
