-- What differs between macOS, Linux and Windows for the gyroflow plugin: the binaries' names, the temp folder,
-- file names in paths, writing the options. Loaded by main.lua and export.lua.

local mp = require 'mp'
local utils = require 'mp.utils'

local M = {}

-- mpv's platform property (0.36 and later), else from the path separator and macOS's own folders
local name = mp.get_property("platform")
if not name or name == "" then
    if package.config:sub(1, 1) == "\\" then
        name = "windows"
    elseif utils.file_info("/System/Library/CoreServices") then
        name = "darwin"
    else
        name = "linux"
    end
end
M.windows = name == "windows"
M.macos = name == "darwin"

-- The VapourSynth plugin as cargo names it: libvs_gyroflow.dylib, libvs_gyroflow.so, vs_gyroflow.dll
function M.library(base)
    if M.windows then return base .. ".dll" end
    return "lib" .. base .. (M.macos and ".dylib" or ".so")
end

function M.exe(base)
    return M.windows and (base .. ".exe") or base
end

function M.temp_dir()
    local dir = os.getenv(M.windows and "TEMP" or "TMPDIR")
    if dir and dir ~= "" then return dir end
    return M.windows and (os.getenv("TMP") or ".") or "/tmp"
end

-- The last part of a path, with either separator on Windows
function M.file_name(path)
    return path:match(M.windows and "([^/\\]+)$" or "([^/]+)$") or path
end

-- Sets options in ~~/script-opts/<name>.conf, the rest of the file as it was. `values`: { { key, value }, ... }.
-- False when it can't be written
function M.save_options(name, values)
    local path = mp.command_native({ "expand-path", "~~/script-opts/" .. name .. ".conf" })
    local new, done, lines = {}, {}, {}
    for _, kv in ipairs(values) do new[kv[1]] = kv[2] end
    local f = io.open(path, "r")
    if f then
        for line in f:lines() do
            local key = line:match("^%s*([%w_%-]+)%s*=")
            if not (key and new[key] ~= nil) then
                table.insert(lines, line)
            elseif not done[key] then
                table.insert(lines, key .. "=" .. new[key])
                done[key] = true
            end
        end
        f:close()
    end
    for _, kv in ipairs(values) do
        if not done[kv[1]] then table.insert(lines, kv[1] .. "=" .. kv[2]) end
    end
    local dir = path:match(M.windows and "^(.*)[/\\]" or "^(.*)/")
    if dir and not utils.file_info(dir) then
        mp.command_native({ name = "subprocess", playback_only = false,
                            args = M.windows and { "cmd", "/d", "/c", "mkdir", dir } or { "mkdir", "-p", dir } })
    end
    f = io.open(path, "w")
    if not f then return false end
    f:write(table.concat(lines, "\n") .. "\n")
    f:close()
    return true
end

return M
