-- Windows' 260-character limit for a path (MAX_PATH): mpv's Lua file calls (io.open, os.rename, os.remove, and
-- mp.utils' file_info and readdir) fail past it, "Invalid argument", unless the path says it's a long one: \\?\C:\…,
-- or \\?\UNC\server\share\… for a share, which Windows then takes as it is (absolute, backslashes only, no . or ..).
-- A trip's folders nested a few deep get there (a NAS's \\nas\share\Camera\ is 45 characters already), and then the
-- clip's marks aren't written, its preview isn't seen, its highlights, telemetry and look aren't read. The binaries
-- (Rust) don't need it.
--
-- install() puts the prefix on the long ones as they're called, in the Lua state of the plugin calling it (each mpv
-- script has its own); off Windows it does nothing. The other plugins find this file beside their own folder, as the
-- highlights plugin finds sidecar.lua. Plain Lua (tests/longpath_test.lua):
--   local longpath = require 'longpath'
--   longpath.extended(path[, limit]) -> the path as Windows takes it past MAX_PATH; unchanged when shorter than limit
--                                       (M.LIMIT) or not absolute
--   longpath.install(utils)       on Windows: io.open, os.rename, os.remove, utils.file_info and utils.readdir take it

local M = {}

-- From this many characters the prefix goes on: MAX_PATH is 260 with the terminating NUL, a folder made is 248 at
-- most (room for an 8.3 name), and a file written beside another gets ".tmp" after its name
M.LIMIT = 240

local PREFIX = "\\\\?\\"

-- `limit`: from how many characters (M.LIMIT when nil)
function M.extended(path, limit)
    if type(path) ~= "string" or #path < (limit or M.LIMIT) or path:sub(1, 4) == PREFIX then return path end
    local p = path:gsub("/", "\\")
    local unc = p:match("^\\\\[^\\?.]") ~= nil
    if not unc and not p:match("^%a:\\") then return path end
    local rest = (unc and p:sub(3) or p):gsub("\\\\+", "\\")
    -- Not resolved after the prefix
    for part in rest:gmatch("[^\\]+") do
        if part == "." or part == ".." then return path end
    end
    return unc and (PREFIX .. "UNC\\" .. rest) or (PREFIX .. rest)
end

function M.install(utils, windows)
    if windows == nil then windows = package.config:sub(1, 1) == "\\" end
    if not windows or M.installed then return end
    M.installed = true
    local x = M.extended
    local open, rename, remove = io.open, os.rename, os.remove
    io.open = function(path, ...) return open(x(path), ...) end
    os.rename = function(from, to) return rename(x(from), x(to)) end
    os.remove = function(path) return remove(x(path)) end
    if utils then
        local file_info, readdir = utils.file_info, utils.readdir
        utils.file_info = function(path, ...) return file_info(x(path), ...) end
        -- mpv asks each entry's kind by "<folder>/<name>": past the limit that fails even in a short folder, and a "/"
        -- after the prefix always does. So the names come unfiltered, and their kinds are asked here
        utils.readdir = function(path, filter)
            filter = filter or "normal"
            local folder = x(path, 0)
            if folder == path or filter == "all" then return readdir(folder, filter) end
            local names, err = readdir(folder, "all")
            if not names then return names, err end
            local sep = path:match("[/\\]$") and "" or "\\"
            local out = {}
            for _, name in ipairs(names) do
                local info = name ~= "." and name ~= ".." and utils.file_info(path .. sep .. name)
                if info and ((filter ~= "dirs" and info.is_file) or (filter ~= "files" and info.is_dir)) then
                    out[#out + 1] = name
                end
            end
            return out
        end
    end
end

return M
