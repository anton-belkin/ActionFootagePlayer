-- What differs between macOS, Linux and Windows for the gyroflow plugin: the binaries' names, the temp folder,
-- file names in paths. Loaded by main.lua and export.lua.

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

return M
