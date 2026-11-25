-- trash: files to the Trash (or aside, where a drive has none), for the workspace plugin's parts. main.lua wires it
local mp = require 'mp'

local M = {}
-- Set by install (main.lua's W)
local W, fs, ws, windows, macos, catalog
local function mkdir(...) return W.mkdir(...) end

-- Windows has a Recycle Bin on local fixed drives only: what's sent there from a network share or a removable drive
-- (a card) is deleted for good, without a word. Asked of the drive once a session
local recycle_bins = {}
local function has_recycle_bin(file)
    if not windows then return true end
    if file:match("^[/\\][/\\]") then return false end
    local root = file:match("^(%a):")
    if not root then return false end
    root = root:upper()
    if recycle_bins[root] == nil then
        local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stdout = true,
            args = { "powershell", "-NoProfile", "-Command", "([IO.DriveInfo]'" .. root .. ":\\').DriveType" } })
        recycle_bins[root] = r and r.status == 0 and (r.stdout or ""):match("Fixed") ~= nil
    end
    return recycle_bins[root]
end

-- Where a file goes instead, on the same drive (a rename, instant): the project's .Trash\ at the file's path in the
-- project, else .Trash\ beside it (a dot folder: never taken for footage)
local TRASH_DIR = ".Trash"
local function trash_folder(file)
    local project = ws.project_root(file)
    local rel = project and ws.relative(file, project)
    if rel then
        return ws.parent(ws.join(ws.join(project, TRASH_DIR), (rel:gsub("/", "\\"))))
    end
    return ws.join(ws.parent(file), TRASH_DIR)
end

-- Where the Trash is for a file, as the OSD says it: "the Trash", or the folder it goes to instead
local function trash_name(file)
    if has_recycle_bin(file) then return "the Trash" end
    local file_project = ws.project_root(file)
    local own = file_project and ws.relative(file, file_project)
    return (own and "the project's .Trash" or ".Trash beside it") .. " (this drive has no Recycle Bin)"
end

local function move_aside(file)
    local dir = trash_folder(file)
    if not fs.is_dir(dir) and not mkdir(dir) then return false end
    local name = ws.name(file)
    -- A name taken already (the same moment removed twice): "Turtle (2).afplink.mp4"
    local stem, ext = name:match("^(.-)(%.afplink%.mp4)$")
    if not stem then stem, ext = name:match("^(.+)(%.[^.]+)$") end
    stem, ext = stem or name, ext or ""
    local target, n = ws.join(dir, name), 2
    while fs.is_file(target) or fs.is_dir(target) do
        target = ws.join(dir, stem .. " (" .. n .. ")" .. ext)
        n = n + 1
    end
    return os.rename(file, target) ~= nil and not fs.is_file(file)
end

-- The file to the Trash; done(it went). Where there's none (has_recycle_bin), aside into trash_folder instead
local function trash(file, done_given)
    -- What went, noted in the project's catalog (the Library's views read it; its folder's time on a share may not say)
    local function done(ok)
        if ok then catalog.note(nil, { file }) end
        return done_given(ok)
    end
    if not has_recycle_bin(file) then return done(move_aside(file)) end
    local args
    if macos then
        if fs.is_file("/usr/bin/trash") then
            args = { "/usr/bin/trash", file }
        else
            args = { "osascript", "-e", 'tell application "Finder" to delete (POSIX file "' .. file:gsub('[\\"]', "\\%0") .. '")' }
        end
    elseif windows then
        args = { "powershell", "-NoProfile", "-Command", "Add-Type -AssemblyName Microsoft.VisualBasic; "
                 .. "[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('" .. file:gsub("'", "''") .. "', 'OnlyErrorDialogs', 'SendToRecycleBin')" }
    else
        args = { "gio", "trash", file }
    end
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        done(r and r.status == 0 and not fs.is_file(file))
    end)
end

-- Files to the Trash one after another; done(the ones that couldn't go)
local function trash_all(files, done, failed)
    failed = failed or {}
    local file = table.remove(files, 1)
    if not file then return done(failed) end
    trash(file, function(ok)
        if not ok then failed[#failed + 1] = file end
        trash_all(files, done, failed)
    end)
end

function M.install(w)
    W = w
    fs, ws, windows, macos, catalog = W.fs, W.ws, W.windows, W.macos, W.catalog
    W.trash, W.trash_name, W.trash_all = trash, trash_name, trash_all
end

return M
