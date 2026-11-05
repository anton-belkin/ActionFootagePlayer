-- workspace: which project the clip that plays belongs to, where that project's files for it are, and its moments
-- (docs/moments-and-workspaces.md). An mpv plugin: this folder goes into ~~/scripts/.
--
-- A project is a trip folder with `.actionfootageplayer/` in it: the nearest such folder above the clip, up to the
-- volume's top folder, and never the home folder itself. A clip in none has no project, and plays as before, until
-- its first moment makes one in its folder (above DCIM for a camera's own tree).
--
-- Moments are parts of a clip kept as small MP4s, `<project>/Moments/<name>.afplink.mp4` (subfolders too): one frame
-- in color, held for the part's length, with the link to the original inside (link.lua reads it, gyroflow-export
-- writes it, the gyroflow plugin asks it to). Finder shows the frame; the player plays the original:
--   k                   save the A-B part (l) as a moment, asking its name; on a moment playing whose part was moved:
--                       update it (Enter), or save it as a new one (a name)
--   a moment's file     plays its original instead, from its in, looping its part, titled with its name; the
--                       original's stabilization, look, highlights and telemetry with it. Not found: the preview
--                       plays, and the OSD says where the original was looked for
--   Moments/ (a folder) its moments one after another, each its part of the original, in the order they happened
-- The clip's moments are chapters on the seek bar ("Turtle", "Turtle end") and in the menu.
--
-- Clip previews: Clips/<the clip's path>.afplink.mp4, a flipbook of the clip in color (its marks, else a dozen
-- frames), made from the menu (Project -> Make clip previews), one at a time in the background, only for clips not
-- in .actionfootageplayer/clips-made.txt, so that one deleted (X, or in Finder: not interesting) stays deleted.
--   X                   this clip isn't interesting: its preview to the Trash; again: made again
-- A clip preview's file plays its original, all of it.
--
-- Exports (Shift+E, the gyroflow plugin's) go to <project>/Exports/, named in the same dialog: <name>.mov, with the
-- look in parentheses unless it's HDR ("Turtle (SDR).mov"), the link inside (kind export). A part that isn't a
-- moment yet is saved as the moment export_<name> beside it; one that is (played, or equal to one to the frame)
-- names the export. An export opened plays as it is; the menu offers its original.
--
-- Before the other plugins load the clip (on_load, early), it's published in user-data/workspace/:
--   path            what mpv plays (its `path`): the rest is for this file only, so a plugin checks it's its own
--   clip            the clip itself, as an absolute path: the original when a moment's file plays
--   moment          when a moment's file plays: { file, name, in, out } (missing = true: no original found)
--   redirected      yes when the file playing is a link (a moment, a clip preview) and mpv plays `clip` instead
--   clip_preview    when a clip preview plays: { file }
--   clips           while clip previews are made: { done, total }
--   project         the project's folder, absent for none
--   relative        the clip's path in it ("Day 1/A.MP4"), absent for none
--   gyroflow_projects  the paths the clip's Gyroflow project may have, in the order looked at
--   gyroflow_new    where a new Gyroflow project for it goes (Ctrl+e, the gyroflow plugin)
--   marks_file      where its marks are once changed (the highlights plugin's M)
--   moments         after it loads: the clip's moments, { name, file, in, out }, in the order of their in
--   moments_folder  the project's Moments/ folder
--   saving          a moment's name while it's being saved
--   export          when an export plays: { name, original (nil: not found), in, out }
-- Without this plugin, the gyroflow plugin looks next to the clip itself, which is the same place for now.
--
-- Script messages (the right-click menu): save-moment [name] (k, or without asking with a name), moment-play <i>,
-- moment-rename <i>, moment-remove <i> (to the Trash), moment-reveal <i>, moments-folder (show it); <i> is the
-- moment's place in user-data/workspace/moments, from 1. clips-make, clips-again (removed ones), clips-stop,
-- project-folder (show it). open-original: an export's original, at its part. For the
-- gyroflow plugin: export-name <json> asks the export's name and answers export-start <json> ({ clip, sdr, output,
-- link }).
--
-- workspace.lua is the lookup itself (plain Lua; tests/workspace_test.lua), and gyroflow-export has its twin
-- (vs-gyroflow/src/workspace.rs) for the Finder quick action and the command line. moments.lua is the moments'
-- part with no mpv in it (tests/moments_test.lua).

local mp = require 'mp'
local utils = require 'mp.utils'
local workspace = require 'workspace'
local link = require 'link'
local moments_lib = require 'moments'
local has_input, input = pcall(require, 'mp.input')
if not has_input then input = nil end

local platform = mp.get_property("platform") or ""
local windows = platform == "windows" or package.config:sub(1, 1) == "\\"
local macos = platform == "darwin"

-- How often the Moments/ folders are looked at for files added, removed or renamed in Finder
local WATCH_SECONDS = 5

local fs = {
    is_dir = function(p)
        local info = utils.file_info(p)
        return info ~= nil and info.is_dir
    end,
    is_file = function(p)
        local info = utils.file_info(p)
        return info ~= nil and info.is_file
    end,
    info = function(p) return utils.file_info(p) end,
    readdir = function(p, what) return utils.readdir(p, what) end,
    read_link = function(p)
        local ok, raw = pcall(link.read, p)
        return ok and raw or nil
    end,
    parse_json = function(s) return (utils.parse_json(s)) end,
    home = os.getenv(windows and "USERPROFILE" or "HOME"),
    windows = windows,
}
local ws = workspace.new(fs)
local moments = moments_lib.new(ws, fs)

local state = {}
-- The project's Moments/ as last read: every moment in it, and its folders' times
local found = { list = {}, dirs = {} }
local scanned_project = nil
local watch_timer = nil
-- The chapters this put in chapter-list, kept there when another plugin sets it
local ours = {}
local applying = false
-- Moments being saved, by request id
local pending = {}
local next_id = 0
-- The name last typed this session
local last_typed = nil
-- The moments of a folder opened (Moments/): played one after another, not each looped
local sequence = {}
-- An export's original asked for (open-original): its part, set when it loads
local wanted = nil

-- Whether this load is mpv's first of a file opened on its own, which it then loads again from the playlist of the
-- files beside it (autocreate-playlist, the app's setting). Options set now are lost then
local function before_autocreate()
    return mp.get_property("autocreate-playlist", "no") ~= "no" and (mp.get_property_number("playlist-count", 1) or 1) <= 1
       and (mp.get_property("playlist-path") or "") == ""
end

local function publish()
    mp.set_property_native("user-data/workspace", state)
end

local function clock(s)
    s = math.floor(s + 0.5)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

local function run(args)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false }, function() end)
end

-- What's read of every moment file, kept between sessions (moments.lua's cache, in mpv's cache folder): a scan then
-- only lists the folders and stats the files
local CACHE_FILE = mp.command_native({ "expand-path", "~~cache/actionfootageplayer-moments.json" })
local cache = {}
do
    local f = CACHE_FILE and io.open(CACHE_FILE, "r")
    if f then
        local t = utils.parse_json(f:read("*a") or "")
        f:close()
        if type(t) == "table" then cache = t end
    end
end
local cache_dirty = false
local function save_cache()
    if not cache_dirty or not CACHE_FILE then return end
    cache_dirty = false
    local f = io.open(CACHE_FILE .. ".tmp", "w")
    if not f then return end
    f:write(utils.format_json(cache) or "{}")
    f:close()
    os.remove(CACHE_FILE)
    os.rename(CACHE_FILE .. ".tmp", CACHE_FILE)
end
-- Marks it changed when the scan reads a file
local tracked_cache = setmetatable({}, {
    __index = cache,
    __newindex = function(_, k, v) cache[k] = v; cache_dirty = true end,
})

-- What's known about the file at `path`, as published
local function describe(path)
    if not path or path == "" then return {} end
    if path:find("://", 1, true) then return { path = path } end
    local clip = ws.absolute(path, mp.get_property("working-directory"))
    local root = ws.project_root(clip)
    return {
        path = path,
        clip = clip,
        project = root,
        relative = root and ws.relative(clip, root) or nil,
        gyroflow_projects = ws.gyroflow_projects(clip),
        gyroflow_new = ws.gyroflow_new(clip),
        marks_file = ws.marks_file(clip),
        moments_folder = root and ws.join(root, moments_lib.FOLDER) or nil,
    }
end

-- Chapters: the moments' ins and outs, put in beside what's there (the file's own, the highlights plugin's)
local function same_chapter(a, b) return a.title == b.title and math.abs(a.time - b.time) < 0.001 end

local function apply_chapters(list)
    local chapters = {}
    for _, c in ipairs(mp.get_property_native("chapter-list") or {}) do
        local mine = false
        for _, o in ipairs(ours) do if same_chapter(c, o) then mine = true end end
        if not mine then chapters[#chapters + 1] = { title = c.title, time = c.time } end
    end
    for _, c in ipairs(list) do chapters[#chapters + 1] = c end
    table.sort(chapters, function(a, b) return a.time < b.time end)
    ours = list
    applying = true
    mp.set_property_native("chapter-list", chapters)
    applying = false
end

mp.observe_property("chapter-list", "native", function(_, chapters)
    if applying or #ours == 0 then return end
    -- Another plugin set them anew: ours again, if they're gone
    for _, o in ipairs(ours) do
        local there = false
        for _, c in ipairs(chapters or {}) do if same_chapter(c, o) then there = true end end
        if not there then return apply_chapters(ours) end
    end
end)

-- The project's moments read again, and the clip's published, as chapters too
local function refresh()
    local project = state.project
    if not project or not state.clip then
        found, scanned_project = { list = {}, dirs = {} }, nil
        state.moments = nil
        if #ours > 0 then apply_chapters({}) end
        publish()
        return
    end
    found = moments.scan(project, tracked_cache)
    scanned_project = project
    save_cache()
    local info = utils.file_info(state.clip)
    local mine = moments.of_clip(found.list, { relative = state.relative, size = info and info.size, created = state.created })
    local list, chapters = {}, {}
    for _, m in ipairs(mine) do
        list[#list + 1] = { name = m.name, file = m.file, ["in"] = m.link["in"], out = m.link.out }
        chapters[#chapters + 1] = { title = m.name, time = m.link["in"] }
        chapters[#chapters + 1] = { title = m.name .. " end", time = m.link.out }
    end
    state.moments = list
    apply_chapters(chapters)
    publish()
end

-- A folder of Moments/ changed (a file added, removed or renamed in it), or Moments/ was made
local function watch()
    if not scanned_project or scanned_project ~= state.project then return end
    local root = ws.join(scanned_project, moments_lib.FOLDER)
    if found.dirs[root] == nil then
        if fs.is_dir(root) then refresh() end
        return
    end
    for dir, mtime in pairs(found.dirs) do
        local info = utils.file_info(dir)
        if not info or info.mtime ~= mtime then return refresh() end
    end
end

-- A moment's file: its original instead, from its in, looping its part (one after another in a playlist)
local function open_moment(path)
    local raw = fs.read_link(state.clip)
    if not raw then return end
    local l = utils.parse_json(raw)
    if type(l) == "table" and l.v == 1 and l.kind == "export" then
        -- It plays as it is (stabilized, in its look already); its original is a menu item
        local original = moments.original(l, moments.project_of(state.clip, moments_lib.EXPORTS))
        state.export = { name = moments_lib.display_name(state.clip), original = original,
                         ["in"] = l["in"], out = l.out }
        return
    end
    if type(l) == "table" and l.v == 1 and l.kind == "clip" then
        -- A clip preview: its original, all of it
        if before_autocreate() then return end
        local original = moments.original(l, moments.project_of(state.clip, moments_lib.CLIPS))
        if not original then
            mp.osd_message(moments_lib.display_name(state.clip) .. ": the clip isn't at " .. tostring(l.clip), 6)
            return
        end
        local file = state.clip
        mp.set_property("stream-open-filename", original)
        state = describe(original)
        state.path = path
        state.redirected = true
        state.clip_preview = { file = file }
        return
    end
    if type(l) ~= "table" or l.v ~= 1 or l.kind ~= "moment" or type(l["in"]) ~= "number" or type(l.out) ~= "number" then
        return
    end
    -- Opened on its own, mpv first makes the playlist of the files beside it (autocreate-playlist, the app's
    -- setting), then loads it again from there: redirected then. Redirected now, the playlist would be the
    -- original's folder
    if before_autocreate() then return end
    local file = state.clip
    local name = moments_lib.display_name(file)
    local project = moments.project_of(file)
    local original = moments.original(l, project)
    if not original then
        local where = project and ws.join(project, windows and tostring(l.clip):gsub("/", "\\") or tostring(l.clip)) or tostring(l.clip)
        mp.osd_message(name .. ": the original isn't at " .. where, 6)
        state.moment = { file = file, name = name, missing = true }
        return
    end
    mp.set_property("stream-open-filename", original)
    local a, b = l["in"], l.out
    mp.set_property("file-local-options/start", string.format("%.3f", a))
    if not sequence[file] then
        mp.set_property("file-local-options/ab-loop-a", string.format("%.3f", a))
        mp.set_property("file-local-options/ab-loop-b", string.format("%.3f", b))
    else
        mp.set_property("file-local-options/end", string.format("%.3f", b))
    end
    mp.set_property("file-local-options/force-media-title", name)
    state = describe(original)
    state.path = path
    state.redirected = true
    state.moment = { file = file, name = name, ["in"] = a, out = b }
end

-- Moments/ or a folder in it: its moments, in the order they happened, as the playlist
local function open_folder(dir)
    local project = moments.project_of(ws.join(dir, "x"))
    if not project then return end
    local all = moments.scan(project, tracked_cache).list
    save_cache()
    local prefix = dir:sub(-1):match("[/\\]") and dir or (dir .. (windows and "\\" or "/"))
    local lines = { "#EXTM3U" }
    sequence = {}
    for _, m in ipairs(moments.by_time(all)) do
        if m.file:sub(1, #prefix) == prefix then
            lines[#lines + 1] = m.file
            sequence[m.file] = true
        end
    end
    if #lines == 1 then return end
    mp.set_property("stream-open-filename", "memory://" .. table.concat(lines, "\n") .. "\n")
end

mp.add_hook("on_load", 10, function()
    local path = mp.get_property("path")
    state = describe(path)
    ours = {}
    if state.clip then
        local info = utils.file_info(state.clip)
        if info and info.is_dir then
            open_folder(state.clip)
        elseif info and (state.clip:lower():match("%.mp4$") or state.clip:lower():match("%.mov$")) then
            open_moment(path)
        end
    end
    -- An export's original, at its part
    if wanted and state.clip == wanted.file then
        if wanted["in"] then mp.set_property("file-local-options/start", string.format("%.3f", wanted["in"])) end
        if wanted["in"] and wanted.out then
            mp.set_property("file-local-options/ab-loop-a", string.format("%.3f", wanted["in"]))
            mp.set_property("file-local-options/ab-loop-b", string.format("%.3f", wanted.out))
        end
        if not before_autocreate() then wanted = nil end
    end
    publish()
end)

mp.register_event("file-loaded", function()
    state.created = mp.get_property("metadata/by-key/creation_time")
    state.loaded = true
    refresh()
    if not watch_timer then watch_timer = mp.add_periodic_timer(WATCH_SECONDS, watch) end
end)

-- Typed text: the console's (mp.input, mpv 0.38 and later), else none (the suggestion is taken). opts: prompt,
-- suggestion (Tab puts it in), default (the field filled), history (a file of earlier names, ↑)
local function ask(opts, done)
    if not input then return done("") end
    local id = opts.id or "workspace"
    input.get({
        prompt = opts.prompt,
        id = id,
        default_text = opts.default,
        cursor_position = opts.default and #opts.default + 1 or nil,
        history_path = opts.history,
        complete = opts.suggestion and function() return { opts.suggestion }, 1 end or nil,
        submit = function(text)
            input.terminate()
            done((text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
        end,
    })
end

local function state_file(name)
    return state.project and ws.join(ws.join(state.project, workspace.STATE_DIR), name) or nil
end

-- The names typed for moments, the last one last (mp.input keeps them there for ↑)
local function history_file() return state_file("moment-names.txt") end

local function last_name()
    if last_typed then return last_typed end
    local f = history_file() and io.open(history_file(), "r")
    if not f then return nil end
    local last
    for line in f:lines() do if line:match("%S") then last = line end end
    f:close()
    return last
end

-- What the dialog suggests: the last name typed with the next free number, else the moment's date and time by the
-- camera's clock
local function suggestion(a)
    local last = last_name()
    if last then
        local taken = {}
        for _, m in ipairs(found.list) do taken[m.name:lower()] = true end
        return moments_lib.next_name(last, taken)
    end
    local created = moments_lib.parse_time(state.created)
    if created then
        local t = created + a
        return moments_lib.time_name(t, moments_lib.zone_of_name(ws.name(state.clip), created) or moments_lib.local_zone(t))
    end
    return (ws.name(state.clip):gsub("%.[^.]*$", "")) .. " " .. clock(a):gsub(":", ".")
end

local function mkdir(dir)
    if windows then
        mp.command_native({ name = "subprocess", args = { "cmd", "/c", "mkdir", dir }, playback_only = false })
    else
        mp.command_native({ name = "subprocess", args = { "mkdir", "-p", dir }, playback_only = false })
    end
    return fs.is_dir(dir)
end

-- The clip's project, made at the first save: in its folder, or above DCIM
local function make_project()
    if state.project then return state.project end
    local root = ws.new_project_root(state.clip)
    local dir = ws.join(root, workspace.STATE_DIR)
    if not fs.is_dir(dir) and not mkdir(dir) then
        mp.osd_message("Can't write in " .. root .. "\n(a project kept elsewhere, for a card or a read-only folder, comes later)", 6)
        return nil
    end
    if windows then run({ "attrib", "+h", dir }) end
    state.project = root
    state.relative = ws.relative(state.clip, root)
    state.moments_folder = ws.join(root, moments_lib.FOLDER)
    publish()
    return root
end

-- The A-B part saved as a moment named `name`, or `update` (a moment of this clip) written again with it. `quiet`:
-- nothing on screen unless it fails (the moment that comes with an export)
local function save(name, update, typed, quiet)
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    if not (a and b and b > a) then return end
    local safe = moments_lib.safe_name(name)
    if not safe then
        mp.osd_message("Moment: that name has nothing a file name can have", 3)
        return
    end
    local first = state.project == nil
    local root = make_project()
    if not root then return end
    local file, final
    if update then
        file, final = update.file, update.name
    else
        file, final = moments.unique_file(ws.join(root, moments_lib.FOLDER), safe)
    end
    local info = utils.file_info(state.clip)
    local l = {
        v = 1, kind = "moment", clip = ws.relative(state.clip, root), size = info and info.size,
        created = state.created, ["in"] = moments_lib.ms(a), out = moments_lib.ms(b), name = final, roots = { root },
    }
    -- The preview's frame: the first highlight inside the part, else its middle
    local at
    local h = mp.get_property_native("user-data/highlights")
    for _, t in ipairs(type(h) == "table" and h.times or {}) do
        if t >= a and t <= b then at = t; break end
    end
    local dates
    local created = moments_lib.parse_time(state.created)
    if created then
        local t = created + a
        dates = moments_lib.format_time(t, moments_lib.zone_of_name(ws.name(state.clip), created) or moments_lib.local_zone(t))
    end
    next_id = next_id + 1
    local id = tostring(next_id)
    pending[id] = { name = final, file = file, update = update, typed = typed, a = a, b = b, history = typed and not update and first, quiet = quiet }
    state.saving = final
    publish()
    if not quiet then mp.osd_message((update and "Updating " or "Saving moment ") .. final .. "…", 30) end
    mp.commandv("script-message-to", "gyroflow", "preview", utils.format_json({
        id = id, clip = state.clip, from = a, to = b, at = at, link = utils.format_json(l), dates = dates, output = file,
        force = update ~= nil, reply = mp.get_script_name(),
    }))
end

mp.register_script_message("preview-done", function(id, json)
    local p = pending[id]
    pending[id] = nil
    if not p then return end
    state.saving = nil
    local r = utils.parse_json(json or "") or {}
    if not r.ok then
        publish()
        mp.osd_message("Couldn't save " .. p.name .. ": " .. tostring(r.error or "gyroflow-export failed"), 6)
        return
    end
    -- The first name typed in a new project: kept for the next suggestion and ↑, where mp.input couldn't yet
    if p.history and p.typed and history_file() then
        local f = io.open(history_file(), "a")
        if f then f:write(p.typed, "\n"); f:close() end
    end
    if p.update and state.moment and state.moment.file == p.file then
        state.moment["in"], state.moment.out = p.a, p.b
    end
    if not p.quiet then mp.osd_message((p.update and "Updated " or "Saved moment ") .. p.name, 3) end
    refresh()
end)

-- k: the A-B part as a moment; on a moment playing whose part was moved, update it or save a new one
local function save_key(given)
    if not state.clip or state.clip:find("://", 1, true) then
        return mp.osd_message("A moment is of a clip on this computer or the NAS", 3)
    end
    if state.moment and state.moment.missing then
        return mp.osd_message(state.moment.name .. ": its original isn't found", 3)
    end
    if state.saving then return mp.osd_message("Saving " .. state.saving .. "…", 2) end
    if not state.loaded then return mp.osd_message("The clip is still loading", 2) end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    if not (a and b and b > a) then
        return mp.osd_message("Mark the part first: l at its start, l at its end, then k", 4)
    end
    if not mp.get_property_native("user-data/gyroflow") then
        return mp.osd_message("Saving a moment needs the gyroflow plugin (it makes the preview)", 4)
    end
    local m = state.moment
    if given and given ~= "" then return save(given, nil, given) end
    if m then
        if math.abs(a - m["in"]) < 0.001 and math.abs(b - m.out) < 0.001 then
            return mp.osd_message(m.name .. " is saved as it is: move its start or end with l, then k", 4)
        end
        return ask({ prompt = "Update " .. m.name .. " to " .. clock(a) .. "–" .. clock(b) .. "? Enter, or a new name for a new one: ",
                     id = "workspace-update" }, function(text)
            if text == "" then save(m.name, m) else last_typed = text; save(text, nil, text) end
        end)
    end
    local suggested = suggestion(a)
    ask({ prompt = "Moment " .. clock(a) .. "–" .. clock(b) .. " name (Enter: " .. suggested .. "): ", suggestion = suggested,
          history = history_file(), id = "workspace-moment" }, function(text)
        if text ~= "" then last_typed = text end
        save(text ~= "" and text or suggested, nil, text ~= "" and text or nil)
    end)
end

mp.add_key_binding("k", "moment", function() save_key() end)
mp.register_script_message("save-moment", function(name) save_key(name) end)

-- The menu's commands on a moment of the clip, by its place in user-data/workspace/moments
local function moment_at(i)
    local m = state.moments and state.moments[tonumber(i) or 0]
    if m and fs.is_file(m.file) then return m end
    mp.osd_message("That moment isn't there any more", 2)
    refresh()
end

local function reveal(file, folder)
    if macos then run(folder and { "open", file } or { "open", "-R", file })
    elseif windows then run(folder and { "explorer", file } or { "explorer", "/select," .. file })
    else run({ "xdg-open", folder and file or (ws.parent(file) or ".") }) end
end

local function trash(file, done)
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

mp.register_script_message("moment-play", function(i)
    local m = moment_at(i)
    if m then mp.commandv("loadfile", m.file, "replace") end
end)

mp.register_script_message("moment-reveal", function(i)
    local m = moment_at(i)
    if m then reveal(m.file) end
end)

mp.register_script_message("moments-folder", function()
    if state.moments_folder and fs.is_dir(state.moments_folder) then reveal(state.moments_folder, true) end
end)

mp.register_script_message("moment-remove", function(i)
    local m = moment_at(i)
    if not m then return end
    trash(m.file, function(ok)
        mp.osd_message(ok and (m.name .. " is in the Trash") or ("Couldn't move " .. m.name .. " to the Trash"), 3)
        refresh()
    end)
end)

mp.register_script_message("moment-rename", function(i)
    local m = moment_at(i)
    if not m then return end
    ask({ prompt = "Rename " .. m.name .. " to: ", default = m.name, id = "workspace-rename" }, function(text)
        local safe = moments_lib.safe_name(text)
        if not safe or safe == m.name then return end
        local target = ws.join(ws.parent(m.file), safe .. moments_lib.EXT)
        if fs.is_file(target) or fs.is_dir(target) then
            return mp.osd_message(safe .. " is taken", 3)
        end
        local ok, err = os.rename(m.file, target)
        if not ok then return mp.osd_message("Couldn't rename " .. m.name .. ": " .. tostring(err), 4) end
        if state.moment and state.moment.file == m.file then
            state.moment.file, state.moment.name = target, safe
            mp.set_property("file-local-options/force-media-title", safe)
        end
        mp.osd_message("Renamed to " .. safe, 2)
        refresh()
    end)
end)

-- Shift+E (the gyroflow plugin asks): the export's name, in the dialog moments have. req: { sdr, label (" (SDR)"),
-- look, stabilized, reply }. The answer: export-start { clip, sdr, output, link }
mp.register_script_message("export-name", function(json)
    local req = utils.parse_json(json or "")
    if type(req) ~= "table" or not req.reply then return end
    if not state.clip or state.clip:find("://", 1, true) then return end
    if state.moment and state.moment.missing then
        return mp.osd_message(state.moment.name .. ": its original isn't found", 3)
    end
    if not state.loaded then return mp.osd_message("The clip is still loading", 2) end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local part = a and b and b > a
    if not part then a, b = nil, nil end
    -- The part a moment: the one playing, or one of the clip's, to the frame
    local frame = 0.5 / (mp.get_property_number("container-fps") or 30)
    local same
    if part then
        local candidates = {}
        if state.moment then candidates[1] = state.moment end
        for _, m in ipairs(state.moments or {}) do candidates[#candidates + 1] = m end
        -- A moment of the user's before one an earlier export made
        for _, m in ipairs(candidates) do
            if math.abs(m["in"] - a) <= frame and math.abs(m.out - b) <= frame
               and (not same or (same.name:match("^export_") and not m.name:match("^export_"))) then same = m end
        end
    end
    local label = type(req.label) == "string" and req.label or ""
    -- An earlier export's moment: the name it was exported with
    local suggested = same and (same.name:gsub("^export_", "")) or suggestion(a or 0)
    local what = part and ("Export " .. clock(a) .. "–" .. clock(b)) or "Export the clip"
    ask({ prompt = what .. label .. " as (Enter: " .. suggested .. "): ", suggestion = suggested, history = history_file(),
          id = "workspace-export" }, function(text)
        if text ~= "" then last_typed = text end
        local safe = moments_lib.safe_name(text ~= "" and text or suggested)
        if not safe then return mp.osd_message("Export: that name has nothing a file name can have", 3) end
        local first = state.project == nil
        local root = make_project()
        if not root then return end
        local output, final = moments.unique_file(ws.join(root, moments_lib.EXPORTS), safe, label .. ".mov")
        local info = utils.file_info(state.clip)
        local l = {
            v = 1, kind = "export", clip = ws.relative(state.clip, root), size = info and info.size,
            created = state.created, ["in"] = a and moments_lib.ms(a), out = b and moments_lib.ms(b), name = final,
            look = req.look, stabilized = req.stabilized == true, roots = { root },
        }
        mp.commandv("script-message-to", req.reply, "export-start", utils.format_json({
            clip = state.clip, sdr = req.sdr == true, output = output, link = utils.format_json(l),
        }))
        if first and text ~= "" and history_file() then
            local f = io.open(history_file(), "a")
            if f then f:write(text, "\n"); f:close() end
        end
        -- Every export can be found from a moment, and played in full quality after it's deleted
        if part and not same then save("export_" .. final, nil, nil, true) end
    end)
end)

-- The highlights plugin wrote the clip's marks: the clip belongs to a project from then on
mp.register_script_message("make-project", function()
    if state.clip and not state.clip:find("://", 1, true) and not state.moment then make_project() end
end)

-- An export's original, at its part
mp.register_script_message("open-original", function()
    local e = state.export
    if not e or not e.original then return mp.osd_message("The original isn't found", 2) end
    -- Its part is set as it loads (on_load): options given to loadfile are lost when mpv loads it again from the
    -- playlist it makes of its folder
    wanted = { file = e.original, ["in"] = e["in"], out = e.out }
    mp.commandv("loadfile", e.original, "replace")
end)

-- Clip previews (Clips/): one at a time, in the background, from the menu or X
local clips_job = nil
local overlay = mp.create_osd_overlay("ass-events")

-- The highlights plugin's reader of the camera's marks, when it's beside this one (the app has both)
local markers = nil
do
    local dir = mp.get_script_directory()
    local plugins = dir and dir:match("^(.*)[/\\][^/\\]+[/\\]?$")
    if plugins then
        package.path = package.path .. ";" .. utils.join_path(utils.join_path(plugins, "highlights"), "?.lua")
        local ok, m = pcall(require, "markers")
        if ok then markers = m end
    end
end

local function made_file() return state_file("clips-made.txt") end

local function read_made()
    local f = made_file() and io.open(made_file(), "r")
    if not f then return {} end
    local text = f:read("*a")
    f:close()
    return moments_lib.parse_made(text)
end

-- A clip's marks: its marks file's, else the camera's; nil for none
local function marks_of(clip)
    local f = io.open(ws.marks_file(clip), "r")
    if f then
        local doc = utils.parse_json(f:read("*a") or "")
        f:close()
        local times = {}
        for _, m in ipairs(type(doc) == "table" and type(doc.marks) == "table" and doc.marks or {}) do
            if type(m) == "table" and type(m.t) == "number" then times[#times + 1] = m.t end
        end
        table.sort(times)
        if #times > 0 then return times end
    end
    if markers then
        local ok, m = pcall(markers.read, clip)
        if ok and m and #m.times > 0 then return m.times end
    end
    return nil
end

local function native(rel) return windows and rel:gsub("/", "\\") or rel end

local function preview_of(root, relative)
    return ws.join(ws.join(root, moments_lib.CLIPS), native(moments_lib.clip_preview_relative(relative)))
end

local function draw_progress()
    if not clips_job then
        overlay:remove()
        return
    end
    overlay.data = string.format("{\\an1\\fs18\\bord1.5}Clip previews: %d of %d", clips_job.done, clips_job.total)
    overlay:update()
end

local function next_clip()
    local job = clips_job
    if not job then return end
    local c = not job.stopping and table.remove(job.queue, 1)
    if not c then
        clips_job = nil
        state.clips = nil
        publish()
        draw_progress()
        local what = job.made == 1 and "1 clip preview" or (job.made .. " clip previews")
        mp.osd_message((job.stopping and "Stopped: " or "Made ") .. what .. (job.failed > 0 and (", " .. job.failed .. " failed") or ""), 4)
        return
    end
    local exporter = mp.get_property_native("user-data/gyroflow/exporter")
    local out = preview_of(job.project, c.relative)
    local args = { exporter, "--preview", "flipbook", "--force", "--output", out }
    -- At background priority while a video plays
    if not mp.get_property_native("pause") and not mp.get_property_native("idle-active") then table.insert(args, "--background") end
    local times = marks_of(c.file)
    if times then
        local list = {}
        for i, t in ipairs(times) do list[i] = string.format("%.3f", t) end
        table.insert(args, "--at"); table.insert(args, table.concat(list, ","))
        table.insert(args, "--cover"); table.insert(args, list[1])
    end
    table.insert(args, "--link")
    table.insert(args, utils.format_json({ v = 1, kind = "clip", clip = c.relative, size = c.size, roots = { job.project } }))
    table.insert(args, c.file)
    mp.msg.verbose("clip preview: " .. table.concat(args, " "))
    job.id = mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            job.made = job.made + 1
            local f = io.open(ws.join(ws.join(job.project, workspace.STATE_DIR), "clips-made.txt"), "a")
            if f then f:write(moments_lib.made_line(c.relative, c.size), "\n"); f:close() end
        elseif not job.stopping then
            job.failed = job.failed + 1
            mp.msg.warn("clip preview of " .. c.relative .. ": " .. tostring(r and r.stderr or r and r.error_string))
        end
        job.done = job.done + 1
        state.clips = { done = job.done, total = job.total }
        publish()
        draw_progress()
        next_clip()
    end)
end

local function start_clips(root, queue, what)
    if not mp.get_property_native("user-data/gyroflow/exporter") then
        return mp.osd_message("Clip previews need the gyroflow plugin (its gyroflow-export makes them)", 4)
    end
    clips_job = { project = root, queue = queue, done = 0, total = #queue, made = 0, failed = 0 }
    state.clips = { done = 0, total = #queue }
    publish()
    draw_progress()
    mp.osd_message(what, 3)
    next_clip()
end

-- The project's clips without a preview made (again: the ones made and since deleted)
local function make_clips(again)
    if clips_job then return mp.osd_message("Clip previews are being made", 2) end
    if not state.clip or state.clip:find("://", 1, true) then return end
    local root = state.project or make_project()
    if not root then return end
    local made = read_made()
    local queue = {}
    for _, c in ipairs(moments.footage(root)) do
        local info = utils.file_info(c.file)
        c.size = info and info.size
        local listed = made[moments_lib.made_line(c.relative, c.size)]
        if (again and listed and not fs.is_file(preview_of(root, c.relative))) or (not again and not listed) then
            queue[#queue + 1] = c
        end
    end
    if #queue == 0 then
        return mp.osd_message(again and "No clip preview was removed" or "Every clip has its preview (removed ones: Make again for removed clips)", 4)
    end
    start_clips(root, queue, "Clip previews: " .. #queue .. (#queue == 1 and " clip" or " clips") .. ", in the background")
end

mp.register_script_message("clips-make", function() make_clips(false) end)
mp.register_script_message("clips-again", function() make_clips(true) end)
mp.register_script_message("clips-stop", function()
    if not clips_job then return end
    clips_job.stopping = true
    if clips_job.id then mp.abort_async_command(clips_job.id) end
end)
mp.register_script_message("project-folder", function()
    if state.project then reveal(state.project, true) end
end)

-- X: this clip isn't interesting, its preview to the Trash; without one, it's made (again)
mp.add_key_binding("X", "not-interesting", function()
    if not state.clip or state.clip:find("://", 1, true) or state.export then return end
    if not state.relative and not state.project then
        -- No project: nothing removed; the preview is made, in a new project
    elseif state.project then
        local preview = preview_of(state.project, state.relative)
        if fs.is_file(preview) then
            return trash(preview, function(ok)
                mp.osd_message(ok and "Not interesting: its preview is in the Trash (X again: made again)" or "Couldn't move the preview to the Trash", 3)
            end)
        end
    end
    if clips_job then return mp.osd_message("Clip previews are being made", 2) end
    local root = state.project or make_project()
    if not root then return end
    local info = utils.file_info(state.clip)
    local c = { file = state.clip, relative = ws.relative(state.clip, root), size = info and info.size }
    start_clips(root, { c }, "Making this clip's preview again")
end)

mp.register_event("shutdown", function()
    if clips_job and clips_job.id then clips_job.stopping = true; mp.abort_async_command(clips_job.id) end
end)
