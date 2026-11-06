-- workspace: which project the clip that plays belongs to, where that project's files for it are, and its moments
-- (docs/moments-and-workspaces.md). An mpv plugin: this folder goes into ~~/scripts/.
--
-- A project is a trip folder with `Project.afproject` in it (or, before 0.9, `_ActionFootagePlayer/`, which moves
-- into it: projectfile.lua): the nearest such folder above the clip, up to the
-- volume's top folder, and never the home folder itself. A clip in none has no project, and plays as before. One is
-- never made by itself: the side panel's Project tab (or the menu's Project) initializes it, in the folder chosen
-- among those above the clip, and what needs one (k, exports, clip previews, X) opens that tab when there's none.
-- Marks need none: they're beside the clip.
--
-- Moments are parts of a clip kept as small MP4s, `<project>/_Moments/<name>.afplink.mp4` (subfolders too): one frame
-- in color, held for the part's length, with the link to the original inside (link.lua reads it, gyroflow-export
-- writes it, the gyroflow plugin asks it to). Finder shows the frame; the player plays the original:
--   k                   save the A-B part (l) as a moment, asking its name; on a moment playing whose part was moved:
--                       update it (Enter), or save it as a new one (a name)
--   a moment's file     plays its original instead, from its in, looping its part, titled with its name; the
--                       original's stabilization, look, highlights and telemetry with it. Not found: the preview
--                       plays, and the OSD says where the original was looked for
--   _Moments/ (a folder) its moments one after another, each its part of the original, in the order they happened
-- The clip's moments are chapters on the seek bar ("Turtle", "Turtle end") and in the menu.
--
-- Clip previews: _Clips/<the clip's path>.afplink.mp4, a flipbook of the clip in color (its marks, else a dozen
-- frames), made by Action Footage Library (the batch mode at the end of this file), one at a time in the background,
-- only for clips not in the project file's clips_made, so that one deleted (X, or in Finder: not interesting) stays
-- deleted.
--   X                   this clip isn't interesting: its preview to the Trash; again: made again
-- A clip preview's file plays its original, all of it.
--
-- A clip spoiled in part (docs/lossless-cut.md): Ctrl+x cuts the A-B part out, the menu also all before or after
-- where it plays. Asked first; afp-cut writes what's kept without re-encoding and reads it back as the original,
-- then the original goes to the Trash (with its LRF, Gyroflow project, clip preview, marks file), the first part
-- takes its name and a second one its own recording time (DJI's names), and the marks and moments follow.
--
-- Exports (Shift+E, the gyroflow plugin's) go to <project>/_Exports/, named in the same dialog: <name>.mov, with the
-- look in parentheses unless it's HDR ("Turtle (SDR).mov"), the link inside (kind export). A part that isn't a
-- moment yet is saved as the moment export_<name> beside it; one that is (played, or equal to one to the frame)
-- names the export. An export opened plays as it is; the menu offers its original.
--
-- Before the other plugins load the clip (on_load, early), it's published in user-data/workspace/:
--   path            what mpv plays (its `path`): the rest is for this file only, so a plugin checks it's its own
--   clip            the clip itself, as an absolute path: the original when a moment's file plays
--   moment          when a moment's file plays: { file, name, in, out } (missing = true: no original found)
--   editing         a moment opened in its whole clip (open-original): { file, name, in, out }; k updates it
--   redirected      yes when the file playing is a link (a moment, a clip preview) and mpv plays `clip` instead
--   offset, length  a moment plays as a video of its own, its part of the clip: the time playing plus offset is the
--                   clip's (stabilization, telemetry, marks, exports, moments saved take it)
--   playing_preview when a clip preview plays: { file }
--   clips           while clip previews are made: { done, total }
--   project_choices with no project: the folders one can be started in, from the top down; project_new: the one the
--                   panel suggests (init-project <folder> initializes it)
--   moments_total   the project's moments; clips_stats: { total (clips), previews (made, there), new (never made) }
--   clip_preview    this clip's preview: made, removed (made, then deleted: not interesting) or none
--   project         the project's folder, absent for none
--   relative        the clip's path in it ("Day 1/A.MP4"), absent for none
--   gyroflow_projects  the paths the clip's Gyroflow project may have, in the order looked at
--   gyroflow_new    where a new Gyroflow project for it goes (Ctrl+e, the gyroflow plugin)
--   sidecar_file    the clip's own file beside it, its marks and saved view (sidecar.lua)
--   moments         after it loads: the clip's moments, { name, file, in, out }, in the order of their in
--   moments_folder  the project's _Moments/ folder
--   saving          a moment's name while it's being saved
--   cutting         while a part is cut out: { name, part, parts }
--   export          when an export plays: { name, original (nil: not found), in, out }
--   input           the source's input saved in its own file: { curve, rotation, underwater } (transform.lua); input_now:
--                   as it's shown when changed and not saved; input_status: saved, changed, none (nothing saved)
--   view            the transform bundle saved for it that plays (a moment's or clip preview's own, else the source's);
--                   absent: the session's plays (the gyroflow plugin's). saved: what's saved where Save writes
--   view_target     where Save writes: moment, preview or source (absent: nowhere); view_from: where the playing
--                   bundle is defined (those, or session); view_status: saved, changed, none; auto_save; fresh (Auto
--                   save wrote it as it opened: it plays as the session's to its end); autoplay
-- Without this plugin, the gyroflow plugin looks next to the clip itself, which is the same place for now.
--
-- The transform and the input (the side panel's Source and Transform): view-save, view-revert, view-auto (Auto save),
-- view-own (settings of its own), input-set <curve|rotation|underwater> <value> (the dlogm and gyroflow plugins', as
-- they apply one: for viewing), input-save (into the source's file), input-revert, moment-alone <yes|no|toggle>,
-- autoplay <yes|no|toggle>.
-- Script messages (the right-click menu): save-moment [name] (k, or without asking with a name), moment-play <i>,
-- moment-rename <i>, moment-remove <i> (to the Trash), moment-reveal <i>, moments-folder (show it); <i> is the
-- moment's place in user-data/workspace/moments, from 1. clips-make, clips-again (removed ones), clips-stop,
-- project-folder, clips-folder (show them), init-project <folder>. cut-before, cut-after (all before or after where it
-- plays cut out; Ctrl+x: the A-B part). open-original: an export's original, at its part. For the
-- gyroflow plugin: export-name <json> asks the export's name and answers export-start <json> ({ clip, sdr, output,
-- link }).
--
-- workspace.lua is the lookup itself (plain Lua; tests/workspace_test.lua), and gyroflow-export has its twin
-- (vs-gyroflow/src/workspace.rs) for the Finder quick action and the command line. moments.lua is the moments'
-- part with no mpv in it (tests/moments_test.lua). longpath.lua lets the Lua file calls take Windows paths past 260
-- characters, here and in the plugins that read or write beside the clip (gyroflow, dlogm, highlights, dji-telemetry).

local mp = require 'mp'
local utils = require 'mp.utils'
-- Windows: paths past 260 characters (a clip deep in a trip's folders) for the file calls here and in the modules
local longpath = require 'longpath'
longpath.install(utils)
local workspace = require 'workspace'
local link = require 'link'
local moments_lib = require 'moments'
local sidecar_lib = require 'sidecar'
local projectfile = require 'projectfile'
local has_input, input = pcall(require, 'mp.input')
if not has_input then input = nil end

local platform = mp.get_property("platform") or ""
local windows = platform == "windows" or package.config:sub(1, 1) == "\\"
local macos = platform == "darwin"

-- How often the _Moments/ folders are looked at for files added, removed or renamed in Finder
local WATCH_SECONDS = 5

-- The project counted for the side panel (extras, further down) runs as a coroutine that gives mpv a turn every
-- TURN_CALLS of these file calls: on a NAS each is a round trip, a few hundred clips take a minute, and mpv waits for
-- a script to finish what it's doing before it quits or the next clip's hooks run
local TURN_CALLS = 16
local counting = { co = nil, calls = 0 }
local function turn()
    if counting.co and coroutine.running() == counting.co then
        counting.calls = counting.calls + 1
        if counting.calls % TURN_CALLS == 0 then coroutine.yield() end
    end
end

local fs = {
    is_dir = function(p)
        turn()
        local info = utils.file_info(p)
        return info ~= nil and info.is_dir
    end,
    is_file = function(p)
        turn()
        local info = utils.file_info(p)
        return info ~= nil and info.is_file
    end,
    info = function(p) turn(); return utils.file_info(p) end,
    readdir = function(p, what) turn(); return utils.readdir(p, what) end,
    read_link = function(p)
        turn()
        local ok, raw = pcall(link.read, p)
        return ok and raw or nil
    end,
    parse_json = function(s) return (utils.parse_json(s)) end,
    home = os.getenv(windows and "USERPROFILE" or "HOME"),
    windows = windows,
}
local ws = workspace.new(fs)
local moments = moments_lib.new(ws, fs)
local sidecar = sidecar_lib.new({ parse_json = fs.parse_json, format_json = utils.format_json, windows = windows })
-- The project's own file, Project.afproject (projectfile.lua)
local pf = projectfile.new({
    parse_json = fs.parse_json, format_json = utils.format_json, join = function(a, b) return ws.join(a, b) end,
    is_dir = fs.is_dir, is_file = fs.is_file,
    rmdir = function(dir)
        if windows then
            mp.command_native({ name = "subprocess", args = { "cmd", "/d", "/c", "rmdir", dir }, playback_only = false, capture_stderr = true })
        else
            os.remove(dir)
        end
    end,
})

local state = {}
-- The project's _Moments/ as last read: every moment in it, and its folders' times
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

-- The project's counts and this clip's preview, for the side panel (set further down, with the clip previews)
local extras = nil
-- The project's settings (further down)
local settings
-- A moment's ends from its project's trim changed in Gyroflow (further down)
local adopt_gyroflow_trim
-- Input and transform (transform.lua, docs/moments-and-workspaces.md "Source and transform"): the source's input is
-- read from its own file; the transform bundle that plays is a moment's or clip preview's own (with settings of its
-- own), else the source's, else none (nil: the session's, the gyroflow plugin's)
local transform = require 'transform'
-- Changes written as they're made (the side panel's Auto save): this session only
local auto_save = false
-- Next files in the folder play on at the end (off: each pauses at its end), this session
local autoplay = true
local function copy(t)
    local out = {}
    for k, v in pairs(type(t) == "table" and t or {}) do out[k] = v end
    return out
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

-- A project of before 0.9, its state folder: moved into Project.afproject the first time it's met (once a session)
local migrated = {}
local function migrate(root)
    if not root or migrated[root] then return end
    migrated[root] = true
    if fs.is_dir(pf.legacy_dir(root)) and pf.migrate(root) then mp.msg.info("project moved into " .. pf.path(root)) end
end

-- What's known about the file at `path`, as published
local function describe(path)
    if not path or path == "" then return {} end
    if path:find("://", 1, true) then return { path = path } end
    local clip = ws.absolute(path, mp.get_property("working-directory"))
    local root = ws.project_root(clip)
    migrate(root)
    return {
        path = path,
        clip = clip,
        project = root,
        relative = root and ws.relative(clip, root) or nil,
        gyroflow_projects = ws.gyroflow_projects(clip),
        gyroflow_new = ws.gyroflow_new(clip),
        sidecar_file = sidecar.path(clip),
        moments_folder = root and ws.join(root, moments_lib.FOLDER) or nil,
        project_choices = not root and ws.project_choices(clip) or nil,
        project_new = not root and ws.new_project_root(clip) or nil,
    }
end

-- A moment's or clip preview's own Gyroflow project: beside it, its name with .gyroflow (Turtle.gyroflow beside
-- Turtle.afplink.mp4)
local function own_project(file)
    return ws.join(ws.parent(file), moments_lib.display_name(file) .. ".gyroflow")
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
        list[#list + 1] = { name = m.name, file = m.file, ["in"] = m.link["in"], out = m.link.out,
                            snap = m.link.snap == 1, alone = m.link.alone == true, stab = m.link.stab }
        -- A moment playing is a video of its own: the clip's moments aren't on its seek bar
        if not state.offset then
            chapters[#chapters + 1] = { title = m.name, time = m.link["in"] }
            chapters[#chapters + 1] = { title = m.name .. " end", time = m.link.out }
        end
    end
    state.moments = list
    state.moments_total = #found.list
    apply_chapters(chapters)
    if extras then extras() end
    publish()
end

-- A folder of _Moments/ changed (a file added, removed or renamed in it), or _Moments/ was made
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
        state.playing_preview = { file = file }
        -- A variant of the clip's: its own settings (own), and a Gyroflow project of its own beside it; else the clip's
        local own = own_project(file)
        if fs.is_file(own) then
            state.gyroflow_projects, state.gyroflow_new, state.own_project = { own }, own, own
        end
        state.link = { file = file, own = l.own == 1, input = transform.input_of(l.stab), raw = l.stab }
        if l.own == 1 then
            state.view = transform.complete(l.stab, mp.get_property_native("user-data/gyroflow/session"))
            state.saved = copy(state.view)
            state.view_target = "preview"
        end
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
    local a, b = l["in"], l.out
    -- Its part as a video of its own (mpv's EDL: that part of the original, its time from 0): the seek bar is the
    -- moment's. Then the next moment in the playlist (the folder's), or paused at its end when it's the last. The
    -- other plugins add `offset` to the time to get the clip's (stabilization, telemetry, marks, exports)
    mp.set_property("stream-open-filename", string.format("edl://!no_chapters;%%%d%%%s,start=%.3f,length=%.3f", #original, original, a, b - a))
    -- Then the next one, unless autoplay is off: each pauses at its end
    mp.set_property("file-local-options/keep-open", autoplay and "yes" or "always")
    mp.set_property("file-local-options/force-media-title", name)
    state = describe(original)
    state.path = path
    state.redirected = true
    state.moment = { file = file, name = name, ["in"] = a, out = b, snap = l.snap == 1, alone = l.alone == true, stab = l.stab }
    state.link = { file = file, own = l.snap == 1, moment = true, input = transform.input_of(l.stab), raw = l.stab }
    state.offset, state.length = a, b - a
    -- Stabilized as it was made (snap, settings of its own): its own project, else the preset with its bundle, its
    -- part on its own (trim) when alone. A moment without plays as its source does
    if l.snap == 1 then
        local own = own_project(file)
        state.snap = true
        state.alone = l.alone == true
        state.view = transform.complete(l.stab, mp.get_property_native("user-data/gyroflow/session"))
        state.saved = copy(state.view)
        state.view_target = "moment"
        state.gyroflow_projects, state.gyroflow_new = { own }, own
        state.own_project = fs.is_file(own) and own or nil
        if state.alone and not state.own_project then state.trim = { ["in"] = a, out = b } end
    end
    -- An EDL has no metadata of its own: the original's creation time from the link
    state.created = l.created
end

-- _Moments/ or a folder in it: its moments, in the order they happened, as the playlist
local function open_folder(dir)
    local project = moments.project_of(ws.join(dir, "x"))
    if not project then return end
    local all = moments.scan(project, tracked_cache).list
    save_cache()
    local prefix = dir:sub(-1):match("[/\\]") and dir or (dir .. (windows and "\\" or "/"))
    local lines = { "#EXTM3U" }
    for _, m in ipairs(moments.by_time(all)) do
        if m.file:sub(1, #prefix) == prefix then lines[#lines + 1] = m.file end
    end
    if #lines == 1 then return end
    mp.set_property("stream-open-filename", "memory://" .. table.concat(lines, "\n") .. "\n")
end

-- The source's identity in its own file, where it isn't yet: which clip the file is for
local function stamp(d)
    if type(d.clip) ~= "table" then
        local info = utils.file_info(state.clip)
        d.clip = { file = ws.name(state.clip), size = info and info.size, created = state.created }
    end
    if d.clip.hash == nil then d.clip.hash = state.hash end
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
    -- The source's own file beside it: its input, always; its bundle, unless a link with settings of its own plays.
    -- An old file's input moves out of its view, and a link's input it didn't have yet goes into it
    if state.clip and not state.export and not (state.moment and state.moment.missing) and not state.clip:find("://", 1, true) then
        local doc, _, problem = sidecar.read(state.clip)
        if problem then mp.msg.warn(problem) end
        local from_link = state.link and state.link.input
        local function takes(d)
            local changed = transform.migrate_source(d)
            for k, v in pairs(from_link or {}) do
                if v ~= "project" then
                    d.source = type(d.source) == "table" and d.source or {}
                    if d.source[k] == nil then d.source[k], changed = v, true end
                end
            end
            if changed then stamp(d) end
            return changed
        end
        if doc and not problem and takes(doc) then
            local ok, err = sidecar.update(state.clip, takes)
            if not ok then mp.msg.warn("couldn't move the input into " .. sidecar.path(state.clip) .. ": " .. tostring(err)) end
            doc = sidecar.read(state.clip) or doc
        elseif not doc and from_link then
            sidecar.update(state.clip, takes)
            doc = sidecar.read(state.clip)
        end
        state.input = doc and type(doc.source) == "table" and copy(doc.source) or nil
        state.input_status = state.input and "saved" or "none"
        if not state.view_target then
            state.view_target = "source"
            local raw = doc and transform.pick(doc.view)
            state.source_raw = raw
            if raw then
                state.view = transform.complete(raw, mp.get_property_native("user-data/gyroflow/session"))
                state.saved = copy(state.view)
            end
        end
    end
    state.view_from = state.view and state.view_target or "session"
    -- An export's original, at its part
    if wanted and state.clip == wanted.file then
        if wanted["in"] then mp.set_property("file-local-options/start", string.format("%.3f", wanted["in"])) end
        if wanted["in"] and wanted.out then
            -- The part marked, not looped: the whole clip plays on, before and after it
            mp.set_property("file-local-options/ab-loop-count", "0")
            mp.set_property("file-local-options/ab-loop-a", string.format("%.3f", wanted["in"]))
            mp.set_property("file-local-options/ab-loop-b", string.format("%.3f", wanted.out))
        end
        -- A moment opened in its clip: k updates it
        if wanted.moment then state.editing = wanted.moment end
        if not before_autocreate() then wanted = nil end
    end
    publish()
end)

mp.register_event("file-loaded", function()
    state.created = mp.get_property("metadata/by-key/creation_time") or state.created
    state.loaded = true
    if adopt_gyroflow_trim then adopt_gyroflow_trim() end
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

-- The project's own file read (projectfile.lua), and a change to it; nil and false without a project
local function project_doc() return state.project and pf.read(state.project) or nil end
local function project_update(fn) return state.project ~= nil and pf.update(state.project, fn) end

-- The names typed for moments, the last one last: the project's moment_names, and a name typed is added to them
local function last_name()
    if last_typed then return last_typed end
    local doc = project_doc()
    local names = doc and doc.moment_names or {}
    return names[#names]
end

local function remember_name(text)
    if not text or not text:match("%S") then return end
    project_update(function(d)
        if d.moment_names[#d.moment_names] ~= text then d.moment_names[#d.moment_names + 1] = text end
        -- The last few hundred are enough for ↑
        while #d.moment_names > 500 do table.remove(d.moment_names, 1) end
    end)
end

-- mp.input's ↑ reads earlier names from a file: the project's, written out for it before it asks
local history_path = nil
local function history_file()
    local doc = project_doc()
    if not doc then return nil end
    if not history_path then
        local tmp = os.getenv(windows and "TEMP" or "TMPDIR") or "/tmp"
        history_path = utils.join_path(tmp, "afp-moment-names-" .. tostring(utils.getpid()) .. ".txt")
    end
    local f = io.open(history_path, "w")
    if not f then return nil end
    for _, n in ipairs(doc.moment_names) do f:write(n, "\n") end
    f:close()
    return history_path
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
        mp.command_native({ name = "subprocess", args = { "cmd", "/c", "mkdir", longpath.extended(dir) }, playback_only = false })
    else
        mp.command_native({ name = "subprocess", args = { "mkdir", "-p", dir }, playback_only = false })
    end
    return fs.is_dir(dir)
end

-- gyroflow-project, beside the gyroflow plugin's gyroflow-export
local function project_tool()
    local exporter = mp.get_property_native("user-data/gyroflow/exporter")
    if type(exporter) ~= "string" then return nil end
    return (exporter:gsub("gyroflow%-export(%.?%a*)$", "gyroflow-project%1"))
end

local function run_tool(args)
    local r = mp.command_native({ name = "subprocess", args = args, playback_only = false, capture_stderr = true })
    if not r or r.status ~= 0 then
        mp.msg.warn(table.concat(args, " ") .. ": " .. tostring(r and r.stderr))
        return false
    end
    return true
end

-- The transform bundle as it plays (the gyroflow plugin's), whole: what a moment, a clip preview or the source keeps.
-- What a Gyroflow project owns (and a wider view or none over it, for viewing only) stays as it was saved there, else
-- the session's: the project decides it, and it's kept only to come back if the project goes
local function current_view()
    local g = mp.get_property_native("user-data/gyroflow")
    if type(g) ~= "table" then return copy(state.saved) end
    local v = {}
    for _, k in ipairs(transform.KEYS) do v[k] = g[k] end
    -- The gyroflow plugin's `lens` is the lens profile; the bundle's lens (corrected, as recorded) is its lens_mode
    v.lens = g.lens_mode
    if g.kind == "project" then
        local keep = state.saved or g.session or {}
        for k in pairs(transform.PROJECT_KEYS) do v[k] = keep[k] end
    end
    return (transform.complete(v, g.session))
end

-- The clip's project, for what needs one (moments, exports, clip previews). None is made by itself: the side panel's
-- Project tab opens, where one is initialized in a folder of the user's choice (init-project). Marks need none
local function need_project(what)
    if state.project then return state.project end
    mp.commandv("script-message-to", "side_panel", "tab", "project")
    mp.osd_message(what .. " needs the project: initialize it first, in the folder of your choice\n"
        .. "(the side panel's Project tab, or right click: Project)", 5)
    return nil
end

-- The A-B part saved as a moment named `name`, or `update` (a moment of this clip) written again with it. `quiet`:
-- nothing on screen unless it fails (the moment that comes with an export)
-- opts: range ({ a, b }, the clip's time: in place of the part marked), alone, stab (in place of the moment's own)
local function save(name, update, typed, quiet, opts)
    opts = opts or {}
    local a, b
    if opts.range then
        a, b = opts.range[1], opts.range[2]
    else
        a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
        if not (a and b and b > a) then return end
        -- In the clip's time (a moment playing starts at its in)
        a, b = a + (state.offset or 0), b + (state.offset or 0)
    end
    local safe = moments_lib.safe_name(name)
    if not safe then
        mp.osd_message("Moment: that name has nothing a file name can have", 3)
        return
    end
    local first = false
    local root = need_project("Saving a moment")
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
    -- How it's shown, fixed as it's made (snap): the transform bundle it plays with, whole (stab), its part on its
    -- own or as part of the clip (alone), and a project of its own when the clip plays with one: a copy, so the
    -- clip's project changed later doesn't change it. An update keeps them; a moment without settings of its own
    -- keeps playing as its source does
    local preview_project = nil
    if not update or update.snap or opts.own then
        local alone = opts.alone
        if alone == nil then alone = update and update.alone or (not update and settings().moments_alone == true) or false end
        local stab = opts.stab or (update and update.stab) or current_view()
        l.snap, l.alone, l.stab = 1, alone, stab
        local own = own_project(file)
        local tool = project_tool()
        local args = { "--trim", string.format("%.3f,%.3f", a, b), "--alone", alone and "yes" or "no" }
        -- Only its part and that switch go into the project: the rest of it is Gyroflow's (the player writes nothing
        -- else there)
        -- The options before the files: the project (and the clip, copying)
        local function with(list)
            local files = list[#list - 1] == state.clip and 2 or 1
            for _, x in ipairs(args) do table.insert(list, #list - files + 1, x) end
            return list
        end
        if fs.is_file(own) then
            if tool then run_tool(with({ tool, "--edit", own })) end
            preview_project = own
        elseif not update or opts.own then
            local g = mp.get_property_native("user-data/gyroflow")
            if tool and type(g) == "table" and g.kind == "project" and (g.project or "") ~= ""
               and run_tool(with({ tool, "--copy", g.project, state.clip, own })) then
                preview_project = own
            end
        end
        -- With a project of its own, what it has is the project's: the bundle keeps those only to come back if the
        -- project goes
        if not preview_project then preview_project = false end
    end
    -- The preview's frame: the first highlight inside the part, else its middle
    local at
    local h = mp.get_property_native("user-data/highlights")
    for _, t in ipairs(type(h) == "table" and h.times or {}) do
        t = t + (type(h) == "table" and h.offset or 0)
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
    pending[id] = { name = final, file = file, update = update, typed = typed, a = a, b = b, history = typed and not update and first, quiet = quiet,
                    keep_playing = opts.keep_playing, alone = l.alone, stab = l.stab }
    state.saving = final
    publish()
    if not quiet then mp.osd_message((update and "Updating " or "Saving moment ") .. final .. "…", 30) end
    mp.commandv("script-message-to", "gyroflow", "preview", utils.format_json({
        id = id, clip = state.clip, from = a, to = b, at = at, link = utils.format_json(l), dates = dates, output = file,
        force = update ~= nil, reply = mp.get_script_name(),
        -- Its own project, or none (the preset: false); nil: the clip's, as it plays (a moment from before)
        project = preview_project, alone = l.alone == true and preview_project == false,
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
    -- The name typed: kept for the next suggestion and ↑
    if p.typed and not p.update then remember_name(p.typed) end
    local edited = state.moment or state.editing
    if p.update and edited and edited.file == p.file then
        edited["in"], edited.out = p.a, p.b
        if p.alone ~= nil then edited.alone = p.alone end
        if p.stab then edited.stab = p.stab end
        -- Saved: it's now as it's viewed
        -- A moment playing as a video of its own: again, with its new ends (not for its settings remembered)
        if state.moment and state.offset and not p.keep_playing then
            mp.osd_message("Updated " .. p.name, 3)
            return mp.commandv("playlist-play-index", "current")
        end
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
    -- Asked before the name: there's nowhere to save it yet
    if not need_project("Saving a moment") then return end
    local m = state.moment or state.editing
    if given and given ~= "" then return save(given, nil, given) end
    local off = state.offset or 0
    if m then
        if math.abs(a + off - m["in"]) < 0.001 and math.abs(b + off - m.out) < 0.001 then
            return mp.osd_message(m.name .. " is saved as it is: move its start or end with l, then k", 4)
        end
        return ask({ prompt = "Update " .. m.name .. " to " .. clock(a + off) .. "–" .. clock(b + off) .. "? Enter, or a new name for a new one: ",
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
local function trash(file, done)
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

mp.register_script_message("moment-play", function(i)
    local m = moment_at(i)
    if m then mp.commandv("loadfile", m.file, "replace") end
end)

-- A moment of the clip in the whole clip, at its part, to edit it (k updates it)
mp.register_script_message("moment-edit", function(i)
    local m = moment_at(i)
    if not m or not state.clip then return end
    wanted = { file = state.clip, ["in"] = m["in"], out = m.out, moment = { file = m.file, name = m.name, ["in"] = m["in"], out = m.out,
                                                                             snap = m.snap, alone = m.alone, stab = m.stab } }
    mp.commandv("loadfile", state.clip, "replace")
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
    local where = trash_name(m.file)
    trash(m.file, function(ok)
        mp.osd_message(ok and (m.name .. " is in " .. where) or ("Couldn't move " .. m.name .. " to " .. where), 3)
        -- Its own Gyroflow project goes with it
        local own = own_project(m.file)
        if ok and fs.is_file(own) then trash(own, function() end) end
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
        local own = own_project(m.file)
        local ok, err = os.rename(m.file, target)
        if not ok then return mp.osd_message("Couldn't rename " .. m.name .. ": " .. tostring(err), 4) end
        -- Its own Gyroflow project, renamed with it (Gyroflow finds the clip by its own path inside)
        if fs.is_file(own) then os.rename(own, own_project(target)) end
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
    if not need_project("Exporting") then return end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local part = a and b and b > a
    if part then
        a, b = a + (state.offset or 0), b + (state.offset or 0)
    elseif state.offset and state.moment then
        -- A moment playing, nothing marked in it: the moment
        a, b, part = state.moment["in"], state.moment.out, true
    else
        a, b = nil, nil
    end
    -- The part a moment: the one playing, or one of the clip's, to the frame
    local frame = 0.5 / (mp.get_property_number("container-fps") or 30)
    local same
    if part then
        local candidates = {}
        if state.moment then candidates[1] = state.moment end
        if state.editing then candidates[#candidates + 1] = state.editing end
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
        local first = false
        local root = need_project("Exporting")
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
            from = a, to = b,
            -- A moment stabilized on its own, without a project: its part (or the part of it) on its own
            alone = state.snap == true and state.alone or nil,
        }))
        if text ~= "" then remember_name(text) end
        -- Every export can be found from a moment, and played in full quality after it's deleted
        if part and not same then save("export_" .. final, nil, nil, true) end
    end)
end)

-- An export's original, at its part
-- An export's or a moment's whole clip, at its part (marked, not looped). A moment's is then the moment being edited:
-- k updates it
mp.register_script_message("open-original", function()
    local e = state.export
    if e then
        if not e.original then return mp.osd_message("The original isn't found", 2) end
        -- Its part is set as it loads (on_load): options given to loadfile are lost when mpv loads it again from the
        -- playlist it makes of its folder
        wanted = { file = e.original, ["in"] = e["in"], out = e.out }
        return mp.commandv("loadfile", e.original, "replace")
    end
    local m = state.moment
    if m and not m.missing and state.clip then
        wanted = { file = state.clip, ["in"] = m["in"], out = m.out, moment = { file = m.file, name = m.name, ["in"] = m["in"], out = m.out,
                                                                                 snap = m.snap, alone = m.alone, stab = m.stab } }
        return mp.commandv("loadfile", state.clip, "replace")
    end
end)

-- Clip previews (_Clips/): one at a time, in the background, from the menu or X
local clips_job = nil
-- Batch mode (Action Footage Library runs the player without a window to make them): { finished(summary), ... }
local batch = nil
local recount
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

-- The clips a preview was made for (the project file's clips_made), as a set of made_line()s
local function read_made(root)
    root = root or state.project
    if not root then return {} end
    return moments_lib.parse_made(table.concat(pf.read(root).clips_made, "\n"))
end

-- A clip's marks: its marks file's, else the camera's; nil for none
local function marks_of(clip)
    local doc = sidecar.read(clip)
    if doc and type(doc.marks) == "table" then
        local times = {}
        for _, m in ipairs(doc.marks) do
            if type(m) == "table" and type(m.t) == "number" then times[#times + 1] = m.t end
        end
        table.sort(times)
        return #times > 0 and times or nil
    end
    if markers then
        local ok, m = pcall(markers.read, clip)
        if ok and m and #m.times > 0 then return m.times end
    end
    return nil
end

local function native(rel) return windows and rel:gsub("/", "\\") or rel end

-- The project's settings (the project file's): clips_flat (clip previews all in _Clips/ itself), moments_alone
settings = function()
    local doc = project_doc()
    return doc and doc.settings or {}
end

-- The clip previews there are, found by the link inside each: renamed, or moved into folders of one's own within
-- _Clips/, they're still the clip's. { [the clip's path in the project] = { file, size } }, read again a minute on
local previews_cache = { time = -1e9, index = {} }
local function previews(force)
    local project = state.project
    if not project then return {} end
    if not force and previews_cache.project == project and mp.get_time() - previews_cache.time < 60 then return previews_cache.index end
    local index = {}
    for _, p in ipairs(moments.scan(project, tracked_cache, moments_lib.CLIPS, "clip").list) do
        if type(p.link.clip) == "string" then index[p.link.clip] = { file = p.file, size = p.link.size } end
    end
    save_cache()
    previews_cache = { project = project, time = mp.get_time(), index = index }
    return index
end

-- A clip's preview file, nil for none
local function preview_for(relative, size)
    local p = relative and previews()[relative]
    if p and (p.size == nil or size == nil or p.size == size) and fs.is_file(p.file) then return p.file end
    return nil
end

-- Where a new preview goes: at the clip's path in _Clips/ (flat: in _Clips/ itself). A name another clip's preview has
-- gets the clip's folder in parentheses ("GX010123 (100GOPRO)"), then a number
local function new_preview_path(root, c, flat)
    local file = ws.join(ws.join(root, moments_lib.CLIPS), native(moments_lib.clip_preview_relative(c.relative, flat)))
    if not fs.is_file(file) then return file end
    local raw = fs.read_link(file)
    local l = raw and utils.parse_json(raw)
    if type(l) == "table" and l.clip == c.relative then return file end
    local dir, stem = ws.parent(file), moments_lib.display_name(file)
    local folder = c.relative:match("([^/]+)/[^/]+$")
    if folder then
        local alt = ws.join(dir, stem .. " (" .. folder .. ")" .. moments_lib.EXT)
        if not fs.is_file(alt) then return alt end
        stem = stem .. " (" .. folder .. ")"
    end
    return (moments.unique_file(dir, stem))
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
        -- Why the ones that failed did, in the project file (Action Footage Library shows them)
        if #job.failures > 0 or not job.stopping then
            pf.update(job.project, function(d) d.clips_failed = job.failures end)
        end
        recount()
        if batch then return batch.finished({ made = job.made, failed = job.failed, failures = job.failures, stopped = job.stopping }) end
        local what = job.made == 1 and "1 clip preview" or (job.made .. " clip previews")
        mp.osd_message((job.stopping and "Stopped: " or "Made ") .. what .. (job.failed > 0 and (", " .. job.failed .. " couldn't be made: "
            .. job.failures[1][2]) or ""), job.failed > 0 and 6 or 4)
        return
    end
    local exporter = mp.get_property_native("user-data/gyroflow/exporter")
    local out = new_preview_path(job.project, c, job.flat)
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
            local line = moments_lib.made_line(c.relative, c.size)
            pf.update(job.project, function(d) d.clips_made[#d.clips_made + 1] = line end)
        elseif not job.stopping then
            job.failed = job.failed + 1
            local err = r and r.stderr or ""
            local last
            for line in err:gmatch("[^\r\n]+") do if line:match("%S") then last = line:gsub("^%s+", "") end end
            table.insert(job.failures, { c.relative, last or (r and r.error_string) or "gyroflow-export failed" })
            mp.msg.warn("clip preview of " .. c.relative .. ": " .. err)
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
    clips_job = { project = root, queue = queue, done = 0, total = #queue, made = 0, failed = 0, failures = {},
                  flat = pf.read(root).settings.clips_flat == true }
    state.clips = { done = 0, total = #queue }
    publish()
    draw_progress()
    mp.osd_message(what, 3)
    next_clip()
end

-- The project's clips without a preview made (again: the ones made and since deleted)
-- `root`: the project's folder, in batch mode (no clip plays)
local function make_clips(again, root)
    if clips_job then return mp.osd_message("Clip previews are being made", 2) end
    if not root then
        if not state.clip or state.clip:find("://", 1, true) then return end
        root = need_project("Making clip previews")
        if not root then return end
    end
    local made = read_made(root)
    local queue = {}
    for _, c in ipairs(moments.footage(root)) do
        local listed = made[moments_lib.made_line(c.relative, c.size)]
        if (again and listed and not preview_for(c.relative, c.size)) or (not again and not listed) then
            queue[#queue + 1] = c
        end
    end
    if #queue == 0 then
        local why = again and "No clip preview was removed" or "Every clip has its preview (removed ones: Make again for removed clips)"
        if batch then return batch.finished({ made = 0, failed = 0, nothing = why }) end
        return mp.osd_message(why, 4)
    end
    start_clips(root, queue, "Clip previews: " .. #queue .. (#queue == 1 and " clip" or " clips") .. ", in the background")
end

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
    if not need_project("Marking a clip not interesting") then return end
    do
        local info = utils.file_info(state.clip)
        local preview = preview_for(state.relative, info and info.size)
        if preview then
            local where = trash_name(preview)
            return trash(preview, function(ok)
                mp.osd_message(ok and ("Not interesting: its preview is in " .. where .. " (X again: made again)") or ("Couldn't move the preview to " .. where), 3)
                recount()
            end)
        end
    end
    if clips_job then return mp.osd_message("Clip previews are being made", 2) end
    local root = state.project
    local info = utils.file_info(state.clip)
    local c = { file = state.clip, relative = ws.relative(state.clip, root), size = info and info.size }
    start_clips(root, { c }, "Making this clip's preview again")
end)

mp.register_event("shutdown", function()
    if clips_job and clips_job.id then clips_job.stopping = true; mp.abort_async_command(clips_job.id) end
end)

-- The project's clips counted (a walk of its folders, kept a minute), and this clip's preview. A step at a time (turn,
-- with the file calls): the first one now, the rest on mpv's next turns, published when it's through. A count asked
-- for again replaces the one under way; a quit, or another clip, doesn't wait for it
local stats_cache = {}
local function count(project, force, s)
    local made = read_made()
    local cached = stats_cache[project]
    if force or not cached or mp.get_time() - cached.time > 60 then
        local total, previews, new = 0, 0, 0
        for _, c in ipairs(moments.footage(project)) do
            total = total + 1
            if preview_for(c.relative, c.size) then previews = previews + 1 end
            if not made[moments_lib.made_line(c.relative, c.size)] then new = new + 1 end
        end
        cached = { time = mp.get_time(), stats = { total = total, previews = previews, new = new } }
        stats_cache[project] = cached
    end
    -- Another clip by now: its own count tells it
    if s ~= state then return end
    state.clips_stats = cached.stats
    local info = fs.info(state.clip)
    if state.relative and preview_for(state.relative, info and info.size) then state.clip_preview = "made"
    elseif state.relative and made[moments_lib.made_line(state.relative, info and info.size)] then state.clip_preview = "removed"
    else state.clip_preview = "none" end
end

local function step(co)
    if counting.co ~= co then return end
    local ok, err = coroutine.resume(co)
    if not ok then
        counting.co = nil
        return mp.msg.warn("counting the project's clips: " .. tostring(err))
    end
    if coroutine.status(co) == "dead" then
        counting.co = nil
        return true
    end
    mp.add_timeout(0, function() if step(co) then publish() end end)
end

extras = function(force)
    local project = state.project
    if not project or not state.clip then
        counting.co = nil
        state.clips_stats, state.clip_preview = nil, nil
        return
    end
    local s = state
    counting.co, counting.calls = coroutine.create(function() count(project, force, s) end), 0
    step(counting.co)
    local doc = pf.read(project)
    state.moments_alone = doc.settings.moments_alone == true
    state.clips_failed = #doc.clips_failed > 0 and #doc.clips_failed or nil
end

-- Counted again after clip previews are made or removed
recount = function()
    previews_cache.time = -1e9
    extras(true)
    publish()
end

mp.register_script_message("clips-folder", function()
    local dir = state.project and ws.join(state.project, moments_lib.CLIPS)
    if dir and fs.is_dir(dir) then reveal(dir, true) end
end)

-- The side panel's Initialize the project here (and the menu's): in one of the folders above the clip
mp.register_script_message("init-project", function(dir)
    if not state.clip or state.project or not dir then return end
    local allowed = false
    for _, d in ipairs(ws.project_choices(state.clip)) do if d == dir then allowed = true end end
    if not allowed then return end
    -- Its file, Project.afproject, makes it one
    if not pf.update(dir, function() end) then
        return mp.osd_message("Can't write in " .. dir, 4)
    end
    state.project = dir
    state.relative = ws.relative(state.clip, dir)
    state.moments_folder = ws.join(dir, moments_lib.FOLDER)
    state.project_choices, state.project_new = nil, nil
    mp.osd_message("Project initialized: " .. ws.name(dir), 3)
    refresh()
end)


-- Cutting a spoiled part out of the clip (docs/lossless-cut.md): Ctrl+x cuts the A-B part out (the menu: also all
-- before or after where it is). afp-cut (the gyroflow plugin's) writes the parts kept, hidden, each read back as
-- the original; only then does the original go to the Trash (with its LRF, Gyroflow project, clip preview and marks
-- file), the first part taking its name and a later one its own recording time, and its marks and moments follow.
local cut_lib = require 'cut'
local cutting = nil

-- A tool of the gyroflow plugin beside this one (the app has both)
local function plugin_tool(name)
    local dir = mp.get_script_directory()
    local plugins = dir and dir:match("^(.*)[/\\][^/\\]+[/\\]?$")
    if not plugins then return nil end
    local exe = utils.join_path(utils.join_path(plugins, "gyroflow"), name .. (windows and ".exe" or ""))
    return fs.is_file(exe) and exe or nil
end

-- A file's fingerprint (gyroflow-export --fingerprint: SHA-256 of its first and last MiB), as links keep their
-- original's (gyroflow-export adds it) and marks files their clip's, to find it again once it's moved or renamed
local function fingerprint_args(file)
    local exe = plugin_tool("gyroflow-export")
    return exe and { name = "subprocess", playback_only = false, capture_stdout = true, args = { exe, "--fingerprint", file } }
end
local function fingerprint_of(r)
    local h = r and r.status == 0 and type(r.stdout) == "string" and r.stdout:match("^(%x+)")
    return h and #h == 64 and h or nil
end
local function fingerprint(file)
    local cmd = fingerprint_args(file)
    return cmd and fingerprint_of(mp.command_native(cmd)) or nil
end

-- The clip's, read in the background once it's loaded (2 MiB): published as hash, for the highlights plugin's marks
mp.register_event("file-loaded", function()
    local clip = state.clip
    local cmd = clip and fs.is_file(clip) and fingerprint_args(clip)
    if not cmd then return end
    mp.command_native_async(cmd, function(ok, r)
        local h = ok and fingerprint_of(r)
        if h and state.clip == clip then
            state.hash = h
            publish()
        end
    end)
end)

local function split_name(name)
    local stem, ext = name:match("^(.*)(%.[^.]*)$")
    return stem or name, ext or ""
end

local function write_json(file, t)
    local f = io.open(file .. ".tmp", "w")
    if not f then return false end
    f:write(utils.format_json(t) or "{}", "\n")
    f:close()
    os.remove(file)
    return os.rename(file .. ".tmp", file) ~= nil
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

local function cut_failed(msg)
    for _, p in ipairs(cutting and cutting.parts or {}) do
        if p.out then os.remove(p.out) end
        if p.lrf_out then os.remove(p.lrf_out) end
    end
    cutting = nil
    state.cutting = nil
    publish()
    mp.msg.warn(msg)
    mp.osd_message(msg, 8)
end

-- The parts written and checked: the original to the Trash, the parts in its place, what follows the clip
local function cut_finish()
    local c = cutting
    local doc
    local marks_file
    doc, marks_file = sidecar.read(c.clip)
    -- mpv lets go of the file first (Windows can't move an open file), idle meanwhile
    c.idle = mp.get_property("idle")
    mp.set_property("idle", "yes")
    mp.commandv("stop", "keep-playlist")
    local originals = { c.clip }
    if c.lrf then originals[#originals + 1] = c.lrf end
    local project_file = ws.gyroflow_project(c.clip)
    if project_file then originals[#originals + 1] = project_file end
    local preview = c.relative and preview_for(c.relative, c.size)
    if preview then
        originals[#originals + 1] = preview
        if fs.is_file(own_project(preview)) then originals[#originals + 1] = own_project(preview) end
    end
    if marks_file and fs.is_file(marks_file) then originals[#originals + 1] = marks_file end
    local where = trash_name(c.clip)
    trash(c.clip, function(ok)
        local function restore_idle() if c.idle then mp.set_property("idle", c.idle) end end
        if not ok then
            -- The parts are kept next to it, visible, the original as it was
            for i, p in ipairs(c.parts) do
                local final = ws.join(c.dir, c.stem .. " (cut " .. i .. ")" .. c.ext)
                os.rename(p.out, final)
                if p.lrf_out then os.rename(p.lrf_out, ws.join(c.dir, c.stem .. " (cut " .. i .. ").LRF")) end
            end
            cutting = nil
            state.cutting = nil
            restore_idle()
            mp.commandv("loadfile", c.clip, "replace")
            return mp.osd_message("The original couldn't go to " .. where .. ": it's as it was, the parts beside it as " .. c.stem .. " (cut 1)…", 8)
        end
        table.remove(originals, 1)
        trash_all(originals, function(left)
            -- The parts take their names: the first the original's, a later one its own recording time
            local taken = function(n) return fs.is_file(ws.join(c.dir, n)) end
            for i, p in ipairs(c.parts) do
                p.name = i == 1 and c.name or (cut_lib.part_name(c.name, p.from, taken) or (c.stem .. " (cut " .. i .. ")" .. c.ext))
                p.file = ws.join(c.dir, p.name)
                os.rename(p.out, p.file)
                if p.lrf_out then os.rename(p.lrf_out, ws.join(c.dir, split_name(p.name) .. ".LRF")) end
                local info = utils.file_info(p.file)
                p.size = info and info.size
                p.created = cut_lib.later(c.created, p.from)
                p.relative = c.project and ws.relative(p.file, c.project) or nil
                p.hash = fingerprint(p.file)
            end
            -- Marks: each part's own, at its time
            local marks_kept = 0
            if doc then
                for _, p in ipairs(c.parts) do
                    local d = cut_lib.part_marks(doc, p.from, p.to, { file = p.name, size = p.size, duration = moments_lib.ms(p.to - p.from), created = p.created })
                    if d then
                        d.clip.hash = p.hash
                        d.format = sidecar_lib.FORMAT
                        write_json(sidecar.path(p.file), d)
                        marks_kept = marks_kept + #(d.marks or {})
                    end
                end
            end
            -- Moments: relinked to the part they're in, at its time
            local spans = {}
            for i, p in ipairs(c.parts) do spans[i] = { from = p.from, to = p.to } end
            -- One all in the part cut out goes to the Trash with the original: its link would name the first part
            -- A moment's own Gyroflow project (its sync, trim and video are the original's, in its time) goes too: the
            -- moment then plays with its saved settings over the preset
            local relinked, orphans, orphan_files, own_gone = 0, {}, {}, {}
            local exe = c.exe
            for _, m in ipairs(c.moments) do
                local i, a, b = cut_lib.moment_part(m["in"], m.out, spans)
                local raw = i and c.parts[i].relative and fs.read_link(m.file)
                local l = raw and utils.parse_json(raw)
                if type(l) == "table" then
                    local p = c.parts[i]
                    l.clip, l.size, l.created, l.hash, l["in"], l.out = p.relative, p.size, p.created, p.hash, a, b
                    local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true,
                        args = { exe, "--set-key", m.file, link.KEY, utils.format_json(l) } })
                    if r and r.status == 0 then relinked = relinked + 1 else orphans[#orphans + 1] = m.name; orphan_files[#orphan_files + 1] = m.file end
                    if r and r.status == 0 and fs.is_file(own_project(m.file)) then
                        own_gone[#own_gone + 1] = m.name
                        orphan_files[#orphan_files + 1] = own_project(m.file)
                    end
                else
                    orphans[#orphans + 1] = m.name
                    orphan_files[#orphan_files + 1] = m.file
                    if fs.is_file(own_project(m.file)) then orphan_files[#orphan_files + 1] = own_project(m.file) end
                end
            end
            trash_all(orphan_files, function(left2)
            for _, f in ipairs(left2) do left[#left + 1] = f end
            -- The part that has where it played, there
            local pos = c.pos or 0
            local at = c.parts[1]
            for _, p in ipairs(c.parts) do if pos >= p.from and pos < p.to then at = p end end
            wanted = { file = at.file, ["in"] = math.max(0, math.min(pos, at.to) - at.from) }
            cutting = nil
            state.cutting = nil
            restore_idle()
            mp.commandv("loadfile", at.file, "replace")
            local lines = { #c.parts == 1 and ("Cut out: " .. at.name .. " is the part kept")
                or ("Cut out: " .. c.parts[1].name .. " and " .. c.parts[2].name .. " are the parts kept"),
                "The original is in " .. where }
            if doc then lines[#lines + 1] = marks_kept .. " marks kept" end
            if #c.moments > 0 then lines[#lines + 1] = relinked .. " moments relinked" end
            if #orphans > 0 then lines[#lines + 1] = "Moments in the part cut out, to the Trash with it: " .. table.concat(orphans, ", ") end
            if project_file then lines[#lines + 1] = "Its Gyroflow project went with it: Ctrl+e makes one again" end
            if #own_gone > 0 then lines[#lines + 1] = "Their own Gyroflow projects to the Trash (made for the original), they play with their settings: " .. table.concat(own_gone, ", ") end
            if #left > 0 then lines[#lines + 1] = "Not moved to the Trash: " .. table.concat(left, ", ") end
            mp.msg.info(table.concat(lines, "; "))
            mp.osd_message(table.concat(lines, "\n"), 8)
            end)
        end)
    end)
end

-- Each part cut and checked by afp-cut, one after another
local function cut_next(i)
    local c = cutting
    local p = c.parts[i]
    if not p then return cut_finish() end
    p.out = ws.join(c.dir, "." .. c.stem .. ".cut" .. i .. c.ext)
    local args = { c.exe, c.clip, "--from", string.format("%.3f", p.from), "--out", p.out, "--json" }
    if p.to then args[#args + 1] = "--to"; args[#args + 1] = string.format("%.3f", p.to) end
    if c.lrf then args[#args + 1] = "--lrf" end
    state.cutting = { name = c.name, part = i, parts = #c.parts }
    publish()
    mp.osd_message("Cutting " .. c.name .. (#c.parts > 1 and (", part " .. i .. " of " .. #c.parts) or "") .. "…", 30)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stdout = true, capture_stderr = true }, function(_, r)
        local out = r and utils.parse_json(r.stdout or "")
        local results = type(out) == "table" and out.results or {}
        local main = results[1]
        if not (r and r.status == 0 and type(main) == "table" and main.status == 0) then
            local why = type(main) == "table" and (main.error or (main.verified and table.concat(main.verified.problems or {}, "; "))) or (r and r.stderr) or "afp-cut failed"
            return cut_failed("Not cut, the clip is as it was: " .. tostring(why))
        end
        p.from, p.to = main.plan.start, main.plan["end"]
        if c.lrf then p.lrf_out = p.out:gsub("%.[^.]*$", ".LRF") end
        p.unknown = main.known_camera == false
        cut_next(i + 1)
    end)
end

-- [a, b) of the clip cut out, after asking
local function cut_out(a, b)
    if cutting then return mp.osd_message("Already cutting " .. cutting.name, 2) end
    if not state.clip or state.redirected or state.moment or state.export or state.offset or state.path ~= mp.get_property("path") then
        return mp.osd_message("Cutting works on a clip itself, not on a moment, a clip preview or an export", 4)
    end
    if not (state.clip:lower():match("%.mp4$") or state.clip:lower():match("%.mov$")) then
        return mp.osd_message("Only MP4 and MOV clips are cut", 3)
    end
    local exe = plugin_tool("afp-cut")
    if not exe then return mp.osd_message("afp-cut isn't installed (the gyroflow plugin's)", 4) end
    if not (input and input.select) then return mp.osd_message("Cutting needs mpv's console (mpv 0.39 and later)", 4) end
    local duration = mp.get_property_number("duration")
    if not duration then return end
    a, b = math.max(0, a), math.min(duration, b)
    local parts = cut_lib.parts(a, b, duration)
    if #parts == 0 then
        return mp.osd_message("Nothing would be left of the clip: to remove it all, move it to the Trash", 4)
    end
    for _, p in ipairs(parts) do p.to = p.to or duration end
    local name = ws.name(state.clip)
    local kept = {}
    for _, p in ipairs(parts) do
        kept[#kept + 1] = clock(p.from) .. "–" .. (p.to >= duration - 0.001 and "end" or clock(p.to))
    end
    local base = state.clip:match("^(.*)%.[^./\\]+$") or state.clip
    local lrf = fs.is_file(base .. ".LRF") and base .. ".LRF" or (fs.is_file(base .. ".lrf") and base .. ".lrf") or nil
    local info = utils.file_info(state.clip)
    local stem, ext = split_name(name)
    local snapshot = {
        clip = state.clip, name = name, stem = stem, ext = ext, dir = ws.parent(state.clip), exe = exe, lrf = lrf,
        size = info and info.size, created = state.created, project = state.project, relative = state.relative,
        moments = state.moments or {}, pos = mp.get_property_number("time-pos"), parts = parts,
    }
    input.select({
        prompt = "Cut " .. clock(a) .. "–" .. (b >= duration - 0.001 and "end" or clock(b)) .. " out of " .. name .. "? Kept: "
            .. table.concat(kept, " and ") .. (#parts > 1 and " (two clips; the second from the keyframe at or before " .. clock(b) .. ")" or "")
            .. ", without re-encoding. The original goes to " .. trash_name(state.clip) .. ".",
        items = { "Cut it out", "Keep the clip as it is" },
        submit = function(i)
            if i ~= 1 then return end
            cutting = snapshot
            mp.set_property_bool("pause", true)
            cut_next(1)
        end,
    })
end

mp.add_key_binding("Ctrl+x", "cut-out", function()
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    if not (a and b and b > a) then
        return mp.osd_message("Mark the spoiled part first (l at its start, l at its end), then Ctrl+x cuts it out", 4)
    end
    cut_out(a, b)
end)
mp.register_script_message("cut-before", function()
    local t = mp.get_property_number("time-pos")
    if t then cut_out(0, t) end
end)
mp.register_script_message("cut-after", function()
    local t = mp.get_property_number("time-pos")
    if t then cut_out(t, mp.get_property_number("duration") or t) end
end)

-- A link's JSON rewritten in place (afp-cut --set-key), fn changing it; true when written
local function rewrite_link(file, fn)
    local exe = plugin_tool("afp-cut")
    local raw = exe and fs.read_link(file)
    local l = raw and utils.parse_json(raw)
    if type(l) ~= "table" then return false end
    fn(l)
    local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true,
                                  args = { exe, "--set-key", file, link.KEY, utils.format_json(l) } })
    if not (r and r.status == 0) then mp.msg.warn("couldn't write the link of " .. file .. ": " .. tostring(r and r.stderr)) end
    return r and r.status == 0
end

-- How the transform stands (the panel's): saved, changed (differs from what's saved: for viewing only), or none
-- (nothing saved where it'd go: the session's plays)
local function view_status()
    local g = mp.get_property_native("user-data/gyroflow") or {}
    return transform.status(state.saved, current_view(), g.kind == "project")
end

local function publish_view(force)
    local status = state.loaded and view_status() or nil
    state.view_from = state.view and state.view_target or "session"
    if force or status ~= state.view_status or state.auto_save ~= auto_save then
        state.view_status, state.auto_save = status, auto_save
        publish()
    end
end

-- A bundle written whole where Save writes (state.view_target). quick: a moment's into its link only (Auto save), not
-- made again with its preview
local function write_bundle(v, quick)
    local t = state.view_target
    if not t or not state.loaded then return false end
    v = copy(v)
    if t == "source" then
        local ok, err = sidecar.update(state.clip, function(d) d.view = v; stamp(d) end)
        if not ok then mp.osd_message("Couldn't save the transform: " .. tostring(err), 4); return false end
    elseif t == "moment" and not quick then
        local m = state.moment
        save(m.name, m, nil, false, { range = { m["in"], m.out }, stab = v, alone = state.alone, keep_playing = true })
    else
        if not rewrite_link(state.link.file, function(l) l.stab = v end) then
            mp.osd_message("Couldn't save the transform into " .. moments_lib.display_name(state.link.file), 4)
            return false
        end
        if state.moment then state.moment.stab = v end
    end
    state.saved = copy(v)
    return true
end

-- Nothing saved where Save writes, with Auto save on: the session's bundle written there now, as it opens. It then
-- plays as the session's to the end of this viewing (fresh): a change is written and sticks for the next ones too
local function autosave_fresh()
    if not (auto_save and state.loaded and state.view_target) or state.saved then return end
    if write_bundle(current_view(), true) then state.fresh = true end
end

-- A bundle written before it had every key (the empty "stab":[] of Moments for marks, one from before the frame):
-- filled with what it plays with and written at once, Auto save or not; a link's input keys go with it (they're the
-- source's now)
local function settle_bundle()
    local raw
    if state.view_target == "source" then raw = state.source_raw
    elseif state.link and state.link.own then raw = state.link.raw end
    if raw == nil then return end
    local full, added = transform.complete(raw, mp.get_property_native("user-data/gyroflow/session"))
    local input = state.view_target ~= "source" and transform.input_of(raw) ~= nil
    if (added or input) and write_bundle(full, true) then
        mp.msg.info("transform completed and written: " .. utils.format_json(full))
        if state.view then state.view = copy(full) end
    end
end

local PLACES = { source = "the source's file", moment = "the moment", preview = "the clip preview" }
mp.register_script_message("view-save", function()
    local t = state.view_target
    if not t then return mp.osd_message("Nothing to save the transform into", 2) end
    local v = current_view()
    if write_bundle(v, false) then
        -- It plays as saved from now on; one Auto save wrote as it opened plays as the session's to its end
        if not state.fresh then state.view = copy(v) end
        if t ~= "moment" then mp.osd_message("Transform saved in " .. PLACES[t], 2) end
    end
    publish_view(true)
end)

-- Back to what's saved, for this viewing
mp.register_script_message("view-revert", function()
    if not state.saved then return end
    mp.commandv("script-message-to", "gyroflow", "apply-view", utils.format_json(state.saved))
    mp.osd_message("Transform as saved", 1.5)
end)

-- Auto save: each change written as it's made, this session; where nothing's saved, what plays as it opens
mp.register_script_message("view-auto", function()
    auto_save = not auto_save
    autosave_fresh()
    publish_view(true)
    mp.osd_message(auto_save and "Auto save on: the transform is saved as it's changed, this session" or "Auto save off", 3)
end)

-- The source's input as it's shown (input-set <curve|rotation|underwater> <value>, the dlogm and gyroflow plugins', as
-- they apply a change): for viewing until Save on the Source tab (input-save) writes it into the source's own file,
-- whole, and so for every moment and clip preview of it. Transform's Save doesn't touch it
local INPUT_DEFAULTS = { curve = "auto", rotation = 0, underwater = "auto" }
local INPUT_ORDER = { "curve", "rotation", "underwater" }
local function input_pack(from)
    local pack = {}
    for k, d in pairs(INPUT_DEFAULTS) do
        pack[k] = (from or {})[k]
        if pack[k] == nil then pack[k] = d end
    end
    return pack
end
local function input_status()
    if not state.input_now then return state.input and "saved" or "none" end
    local saved = input_pack(state.input)
    for _, k in ipairs(INPUT_ORDER) do
        if tostring(state.input_now[k]) ~= tostring(saved[k]) then return "changed" end
    end
    return state.input and "saved" or "none"
end
local function input_place()
    return state.clip and not state.export and not state.clip:find("://", 1, true)
end
mp.register_script_message("input-set", function(key, value)
    if INPUT_DEFAULTS[key] == nil or not input_place() then return end
    if key == "rotation" then value = tonumber(value) or 0 end
    state.input_now = state.input_now or input_pack(state.input)
    state.input_now[key] = value
    state.input_status = input_status()
    publish()
end)
mp.register_script_message("input-save", function()
    if not input_place() then return mp.osd_message("Not saved: this file isn't a source", 2) end
    local pack = input_pack(state.input_now or state.input)
    local ok, err = sidecar.update(state.clip, function(d) d.source = pack; stamp(d) end)
    if not ok then return mp.osd_message("Couldn't save the input in the source's file: " .. tostring(err), 4) end
    state.input, state.input_now = pack, nil
    state.input_status = input_status()
    publish()
    mp.osd_message("Input saved in the source's file: for all its moments and clips", 2)
end)
-- Back to the saved input, for this viewing: each plugin applies its part again (and says so with input-set)
mp.register_script_message("input-revert", function()
    if not state.input_now then return end
    local saved = input_pack(state.input)
    local now = state.input_now
    if tostring(now.curve) ~= tostring(saved.curve) then mp.commandv("script-message-to", "dlogm", "curve", tostring(saved.curve)) end
    if tostring(now.rotation) ~= tostring(saved.rotation) then mp.commandv("script-message-to", "gyroflow", "rotate", tostring(saved.rotation)) end
    if tostring(now.underwater) ~= tostring(saved.underwater) then mp.commandv("script-message-to", "gyroflow", "underwater", tostring(saved.underwater)) end
    mp.osd_message("Input as saved", 1.5)
end)

-- A moment stabilized on its own or with its clip: written into it (and its project's trim range), played again
mp.register_script_message("moment-alone", function(value)
    local m = state.moment
    if not (m and m.snap) then return end
    local alone = value == "yes" or (value ~= "no" and not state.alone)
    save(m.name, m, nil, false, { range = { m["in"], m.out }, stab = state.saved or current_view(), alone = alone })
end)

-- Autoplay next (the panel's): on, the next file in the folder plays at the end; off, each pauses at its end
local KEEP_OPEN = mp.get_property("keep-open") or "no"
if KEEP_OPEN == "always" then KEEP_OPEN = "no" end
mp.register_script_message("autoplay", function(value)
    if value == "toggle" then autoplay = not autoplay else autoplay = value == "yes" end
    mp.set_property("keep-open", autoplay and KEEP_OPEN or "always")
    if state.moment and not state.moment.missing then
        mp.set_property("file-local-options/keep-open", autoplay and "yes" or "always")
    end
    state.autoplay = autoplay
    publish()
    mp.osd_message(autoplay and "Autoplay next: on" or "Autoplay next: off, each file pauses at its end", 2)
end)
state.autoplay = autoplay

-- Settings of its own for the moment or clip preview playing, or the clip's: on, what plays now is written into it
-- (with a copy of the clip's Gyroflow project, if it has one); off, they're taken out (its own project to the
-- Trash) and it plays with the clip's
mp.register_script_message("view-own", function()
    local l = state.link
    if not l then return mp.osd_message("Settings of its own are for a moment or a clip preview playing", 3) end
    local function again() mp.commandv("playlist-play-index", "current") end
    local own = own_project(l.file)
    if l.own then
        if not rewrite_link(l.file, function(x) x.stab, x.own = nil, nil; if l.moment then x.snap, x.alone = nil, nil end end) then
            return mp.osd_message("Couldn't change " .. moments_lib.display_name(l.file), 4)
        end
        local function done()
            mp.osd_message(moments_lib.display_name(l.file) .. ": the clip's settings, as the clip plays", 3)
            again()
        end
        if fs.is_file(own) then return trash(own, function() done() end) end
        return done()
    end
    if l.moment then
        local m = state.moment
        save(m.name, m, nil, false, { range = { m["in"], m.out }, stab = current_view(), own = true })
        return
    end
    -- A clip preview: its settings, and a copy of the clip's project when it plays with one
    local v = current_view()
    local g = mp.get_property_native("user-data/gyroflow") or {}
    local tool = project_tool()
    if tool and g.kind == "project" and (g.project or "") ~= "" and not fs.is_file(own) then
        run_tool({ tool, "--copy", g.project, state.clip, own })
    end
    if not rewrite_link(l.file, function(x) x.own, x.stab = 1, v end) then
        return mp.osd_message("Couldn't change " .. moments_lib.display_name(l.file), 4)
    end
    mp.osd_message(moments_lib.display_name(l.file) .. ": settings of its own" .. (fs.is_file(own) and ", and a Gyroflow project (Ctrl+e opens it)" or ""), 3)
    again()
end)

-- What it plays in changed: the panel's Saved / Changed, and Auto save writes it; a bundle playing is then the one
-- written
local auto_timer = nil
local function view_changed()
    if not state.loaded then return end
    if auto_save and state.view_target and view_status() == "changed" then
        if auto_timer then auto_timer:kill() end
        auto_timer = mp.add_timeout(0.7, function()
            auto_timer = nil
            if auto_save and state.view_target and view_status() == "changed" then
                local v = current_view()
                if write_bundle(v, true) and state.view then state.view = copy(v) end
                publish_view(true)
            end
        end)
    end
    publish_view()
end
mp.observe_property("user-data/gyroflow", "native", view_changed)
mp.register_event("file-loaded", function()
    settle_bundle()
    autosave_fresh()
    publish_view(true)
end)

-- New moments stabilized on their own (a project's setting)
mp.register_script_message("moments-alone", function()
    if not state.project then return end
    local alone
    project_update(function(d) d.settings.moments_alone = not (d.settings.moments_alone == true); alone = d.settings.moments_alone end)
    local t = { moments_alone = alone }
    state.moments_alone = alone
    publish()
    mp.osd_message(t.moments_alone and "New moments: stabilized on their own" or "New moments: stabilized as part of their clip", 3)
end)

-- A moment's own project whose trim was changed in Gyroflow: its ends from there, as it loads (the last change wins)
adopt_gyroflow_trim = function()
    local m = state.moment
    local tool = state.own_project and m and m.snap and project_tool()
    if not tool then return end
    local r = mp.command_native({ name = "subprocess", args = { tool, "--print-trim", state.own_project }, playback_only = false, capture_stdout = true })
    local a, b = (r and r.stdout or ""):match("^([%d%.]+),([%d%.]+)")
    a, b = tonumber(a), tonumber(b)
    if a and b and (math.abs(a - m["in"]) > 0.01 or math.abs(b - m.out) > 0.01) then
        mp.osd_message(m.name .. ": its ends from Gyroflow, " .. clock(a) .. "–" .. clock(b), 3)
        save(m.name, m, nil, true, { range = { a, b } })
    end
end

-- The project's marks, all at once (the Project tab's): a moment for each mark in no moment yet, 2 s on each side,
-- those that overlap joined into one (the clip's marks: its own file's, else the camera's); or a photo of each mark,
-- into _Stills, except those made before (the project file's stills_made: a line per clip and mark, so a photo
-- deleted stays deleted). One at a time in the background, each with its clip's saved view (else the defaults); a
-- moment with settings of its own, as one saved with k: a copy of the clip's Gyroflow project when it has one. While it
-- runs: user-data/workspace/marks_job { kind (moments, photos), done, total, failed }
local MOMENT_AROUND = 2
local marks_job = nil

local function stills_line(relative, size, t) return relative .. "\t" .. tostring(size) .. "\t" .. string.format("%.3f", t) end
local function read_stills_made(root)
    local made = {}
    for _, line in ipairs(pf.read(root).stills_made) do made[line] = true end
    return made
end

-- What's made of a source by the batches (moments and photos of all marks): its bundle, else the app's defaults (not
-- the session's: a moment is kept as made, and the session may be a try of the moment), whole; and its input, which
-- the preview or photo is rendered with too. The bundle alone goes into a moment's link
local function made_view_of(file, movie, own)
    local doc = sidecar.read(file)
    local defaults = mp.get_property_native("user-data/gyroflow/defaults")
    -- `own`: a moment's own bundle, which it keeps whatever the source's is now
    local bundle = transform.complete(transform.pick(own) or transform.pick(doc and doc.view) or defaults, defaults)
    local rendered = copy(bundle)
    if doc then transform.migrate_source(doc) end
    for k, v in pairs(doc and doc.source or {}) do rendered[k] = v end
    -- The frame's W:H for this source: its ratio turned for its orientation and the source's shape, upright (the
    -- file's turn and the input's)
    local w, h = movie and movie.width, movie and movie.height
    if w and h and (((movie.turn or 0) + (tonumber(rendered.rotation) or 0)) % 180) == 90 then w, h = h, w end
    rendered.frame_aspect = transform.concrete(bundle.aspect, bundle.orientation, w, h)
    return bundle, rendered
end

local function job_progress()
    local j = marks_job
    state.marks_job = j and { kind = j.kind, done = j.done, total = #j.queue, failed = j.failed } or nil
    publish()
end

local function job_finish(message)
    local j = marks_job
    marks_job = nil
    job_progress()
    if batch and j then return batch.finished({ made = j.done - j.failed, failed = j.failed, failures = j.failures, stopped = j.stopping }) end
    if message then mp.osd_message(message, 5) end
    if j and j.kind == "moments" then refresh() end
end

local job_next
mp.register_script_message("job-done", function(id, json)
    local j = marks_job
    if not j or tostring(j.id) ~= id then return end
    local r = utils.parse_json(json or "") or {}
    local item = j.queue[j.done + 1]
    if r.ok then
        if j.kind == "photos" and item then
            local line = stills_line(item.relative, item.size, item.t)
            pf.update(j.root, function(d) d.stills_made[#d.stills_made + 1] = line end)
        end
    else
        j.failed = j.failed + 1
        j.failures[#j.failures + 1] = { tostring(item and (item.relative or item.name) or ""), tostring(r.error or "failed") }
        mp.msg.warn((j.kind == "moments" and "moment " or "photo ") .. tostring(item and item.name or "") .. ": " .. tostring(r.error))
    end
    j.done = j.done + 1
    job_progress()
    job_next()
end)

job_next = function()
    local j = marks_job
    if not j then return end
    if j.stopping or j.done >= #j.queue then
        local made = j.done - j.failed
        local what = j.kind == "moments" and (made == 1 and "1 moment" or (made .. " moments")) or (made == 1 and "1 photo" or (made .. " photos"))
        return job_finish((j.stopping and "Stopped: " or "Made ") .. what .. (j.failed > 0 and (", " .. j.failed .. " failed (see the log)") or ""))
    end
    local item = j.queue[j.done + 1]
    j.id = j.id + 1
    local id = tostring(j.id)
    if j.kind == "photos" then
        mp.commandv("script-message-to", "gyroflow", "photo-at", utils.format_json({
            id = id, clip = item.clip, t = item.t, view = item.rendered, reply = mp.get_script_name(), done = "job-done" }))
        return
    end
    -- A moment, as k saves one: its link, its own settings (and project), its preview
    local root = j.root
    local file, final = moments.unique_file(ws.join(root, moments_lib.FOLDER), item.name)
    local l = { v = 1, kind = "moment", clip = item.relative, size = item.size, created = item.created_text,
                ["in"] = moments_lib.ms(item.a), out = moments_lib.ms(item.b), name = final, roots = { root },
                snap = 1, alone = j.alone, stab = item.view }
    local project = false
    local clip_project = ws.gyroflow_project(item.clip)
    local tool = project_tool()
    if clip_project and tool then
        local own = own_project(file)
        if run_tool({ tool, "--trim", string.format("%.3f,%.3f", item.a, item.b), "--alone", j.alone and "yes" or "no",
                      "--copy", clip_project, item.clip, own }) then
            project = own
        end
    end
    local dates
    if item.created then
        local t = item.created + item.a
        dates = moments_lib.format_time(t, moments_lib.zone_of_name(ws.name(item.clip), item.created) or moments_lib.local_zone(t))
    end
    item.name = final
    mp.commandv("script-message-to", "gyroflow", "preview", utils.format_json({
        id = id, clip = item.clip, from = item.a, to = item.b, at = item.at, link = utils.format_json(l), dates = dates, output = file,
        reply = mp.get_script_name(), project = project, alone = j.alone and project == false, view = item.rendered,
        done = "job-done",
    }))
end

-- What there is to make, over the project's clips (their headers read: a second or so per hundred clips)
local function marks_queue(kind, root)
    local queue = {}
    local made = kind == "photos" and read_stills_made(root) or nil
    local all = kind == "moments" and moments.scan(root, nil).list or nil
    local taken_names = {}
    for _, c in ipairs(moments.footage(root)) do
        local times = marks_of(c.file)
        if times then
            local info = utils.file_info(c.file)
            local size = info and info.size
            local movie = markers and markers.movie and markers.movie(c.file) or nil
            local view, rendered = made_view_of(c.file, movie)
            if kind == "photos" then
                for _, t in ipairs(times) do
                    if not made[stills_line(c.relative, size, t)] then
                        queue[#queue + 1] = { clip = c.file, relative = c.relative, size = size, t = t, view = view, rendered = rendered, name = ws.name(c.file) .. " " .. clock(t) }
                    end
                end
            else
                local taken = {}
                for _, m in ipairs(moments.of_clip(all, { relative = c.relative, size = size })) do
                    taken[#taken + 1] = { m.link["in"], m.link.out }
                end
                for _, p in ipairs(moments_lib.loose_parts(times, taken, movie and movie.duration, MOMENT_AROUND)) do
                    local created = movie and movie.created
                    local name
                    if created then
                        local t = created + p.a
                        name = moments_lib.time_name(t, moments_lib.zone_of_name(ws.name(c.file), created) or moments_lib.local_zone(t))
                    else
                        name = (ws.name(c.file):gsub("%.[^.]*$", "")) .. " " .. clock(p.a):gsub(":", ".")
                    end
                    name = moments_lib.safe_name(name) or "Moment"
                    -- Two parts of one name (the same second in two clips): kept apart here, the folder checked as each is made
                    local base, n = name, 1
                    while taken_names[name:lower()] do n = n + 1; name = base .. " " .. n end
                    taken_names[name:lower()] = true
                    queue[#queue + 1] = { clip = c.file, relative = c.relative, size = size, a = p.a, b = p.b, at = p.marks[1],
                                          created = created, created_text = created and os.date("!%Y-%m-%dT%H:%M:%S.000000Z", created) or nil,
                                          view = view, rendered = rendered, name = name }
                end
            end
        end
    end
    return queue
end

-- `root`: the project's folder, in batch mode (no clip plays)
local function start_marks_job(kind, root)
    if marks_job then return mp.osd_message("Already making " .. marks_job.kind .. ": " .. marks_job.done .. " of " .. #marks_job.queue, 3) end
    root = root or need_project(kind == "moments" and "Moments of the marks" or "Photos of the marks")
    if not root then return end
    if not markers then
        if batch then return batch.finished({ made = 0, failed = 0, error = "the highlights plugin isn't there" }) end
        return mp.osd_message("This needs the highlights plugin", 3)
    end
    mp.osd_message("Looking for the marks in the project's clips…", 30)
    if batch then batch.looking() end
    local queue = marks_queue(kind, root)
    if #queue == 0 then
        local why = kind == "moments" and "Every mark is in a moment already" or "Every mark has its photo already (deleted ones aren't made again)"
        if batch then return batch.finished({ made = 0, failed = 0, nothing = why }) end
        return mp.osd_message(why, 4)
    end
    marks_job = { kind = kind, queue = queue, done = 0, failed = 0, failures = {}, id = 0, root = root, alone = pf.read(root).settings.moments_alone == true }
    mp.osd_message((kind == "moments" and "Making " .. #queue .. " moments" or "Saving " .. #queue .. " photos") .. " in the background…", 3)
    job_progress()
    job_next()
end

-- The project opened in Action Footage Library, where its batches are made (clip previews, moments and photos of
-- all marks, exports of all moments): its file, Project.afproject, opened as the system opens it
mp.register_script_message("open-library", function()
    local root = state.project
    if not root then return need_project("Action Footage Library") end
    migrate(root)
    local file = pf.path(root)
    if not fs.is_file(file) and not pf.update(root, function() end) then
        return mp.osd_message("Can't write " .. file, 4)
    end
    local args = macos and { "open", file } or windows and { "cmd", "/d", "/c", "start", "", file } or { "xdg-open", file }
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if not r or r.status ~= 0 then
            mp.osd_message("Action Footage Library isn't installed: it opens " .. workspace.PROJECT_FILE, 4)
        end
    end)
end)

-- Export all moments (Action Footage Library): each moment of the project not exported yet, one at a time, as Shift+E
-- exports one it plays: its part, its own transform (and its own Gyroflow project), the source's input, into
-- _Exports/<name>.mov (" (SDR)" in SDR), the link inside. A moment exported in that look already (an export of the
-- same part of the same clip) and the export_<name> moments exports leave are skipped
local export_job = nil

local function exported_parts(root)
    local set = {}
    local dir = ws.join(root, moments_lib.EXPORTS)
    for _, name in ipairs(utils.readdir(dir, "files") or {}) do
        local lower = name:lower()
        if lower:match("%.mov$") or lower:match("%.mp4$") then
            local raw = fs.read_link(ws.join(dir, name))
            local l = raw and fs.parse_json(raw)
            if type(l) == "table" and l.kind == "export" and type(l.clip) == "string" and l["in"] and l.out then
                local sdr = l.look == "maker" or l.look == "nbcu"
                set[l.clip .. "|" .. string.format("%.3f|%.3f", l["in"], l.out) .. "|" .. tostring(sdr)] = true
            end
        end
    end
    return set
end

local function export_next()
    local j = export_job
    if not j then return end
    if j.stopping or j.done >= #j.queue then
        export_job = nil
        if batch then batch.finished({ made = j.made, failed = j.failed, failures = j.failures, stopped = j.stopping }) end
        return
    end
    local m = j.queue[j.done + 1]
    j.id = j.id + 1
    j.fraction = 0
    local label = j.sdr and " (SDR)" or ""
    local output, final = moments.unique_file(ws.join(j.root, moments_lib.EXPORTS), moments_lib.safe_name(m.name) or "Moment", label .. ".mov")
    local l = { v = 1, kind = "export", clip = m.link.clip, size = m.link.size, created = m.link.created,
                ["in"] = m.link["in"], out = m.link.out, name = final, look = j.sdr and "nbcu" or "hdr",
                stabilized = m.rendered.view ~= "off", roots = { j.root } }
    m.output = output
    mp.commandv("script-message-to", "gyroflow", "export-batch", utils.format_json({
        id = tostring(j.id), clip = m.original, view = m.rendered, project = m.project, alone = m.alone,
        from = m.link["in"], to = m.link.out, output = output, link = utils.format_json(l), sdr = j.sdr,
        progress = j.progress, reply = mp.get_script_name(), done = "export-batch-done",
    }))
end

mp.register_script_message("export-batch-done", function(id, json)
    local j = export_job
    if not j or tostring(j.id) ~= id then return end
    local r = utils.parse_json(json or "") or {}
    if r.ok then j.made = j.made + 1 end
    if not r.ok and not j.stopping then
        j.failed = j.failed + 1
        j.failures[#j.failures + 1] = { j.queue[j.done + 1].name, tostring(r.error or "failed") }
    end
    j.done = j.done + 1
    export_next()
end)

local function export_moments(root, sdr)
    if export_job then return end
    if batch then batch.looking() end
    local done = exported_parts(root)
    local queue, failures = {}, {}
    for _, m in ipairs(moments.scan(root, nil).list) do
        local l = m.link
        if not m.name:match("^export_") and type(l["in"]) == "number" and type(l.out) == "number" and l.out > l["in"]
           and not done[tostring(l.clip) .. "|" .. string.format("%.3f|%.3f", l["in"], l.out) .. "|" .. tostring(sdr)] then
            local original = moments.original(l, root)
            if original then
                local movie = markers and markers.movie and markers.movie(original) or nil
                local _, rendered = made_view_of(original, movie, l.stab)
                local own = own_project(m.file)
                m.original, m.rendered = original, rendered
                m.project = fs.is_file(own) and own or nil
                -- A moment kept as made (snap): stabilized on its own or as part of its clip, as Shift+E has it
                if l.snap then m.alone = l.alone == true else m.alone = nil end
                queue[#queue + 1] = m
            else
                failures[#failures + 1] = { m.name, "its original isn't found" }
            end
        end
    end
    table.sort(queue, function(a, b) return a.name:lower() < b.name:lower() end)
    if #queue == 0 then
        if batch then
            return batch.finished({ made = 0, failed = #failures, failures = failures,
                                    nothing = #failures == 0 and "Every moment is exported already" or nil })
        end
        return
    end
    mkdir(ws.join(root, moments_lib.EXPORTS))
    local tmp = os.getenv(windows and "TEMP" or "TMPDIR") or "/tmp"
    export_job = { root = root, sdr = sdr, queue = queue, done = 0, made = 0, failed = #failures, failures = failures, id = 0,
                   progress = utils.join_path(tmp, "afp-batch-export-" .. tostring(utils.getpid()) .. ".json") }
    -- Those whose original is missing count as failed, after the rest
    export_job.total_extra = #failures
    export_next()
end

-- Batch mode: Action Footage Library runs the player with no window and AFP_BATCH set to a request file, a JSON
-- { kind, project, status, stop, photo, sdr }: kind clips (previews of the clips without one), clips-again (the ones
-- removed), moments (moments of all marks), photos (photos of all marks; photo: no, live or motion), export (all
-- moments; sdr: in SDR). The player makes them for that project as the menu used to, writes how far it is to `status`
-- (a JSON written whole: { kind, state, done, total, failed, fraction, current, message, failures }, state starting,
-- looking, running, done, stopped or failed), stops when the file `stop` appears (after the one being made; an export
-- is cancelled) and quits when it's done
do
    local req_file = os.getenv("AFP_BATCH")
    if req_file and req_file ~= "" then
        local f = io.open(req_file, "r")
        local req = f and utils.parse_json(f:read("*a") or "")
        if f then f:close() end
        if type(req) ~= "table" or type(req.project) ~= "string" or type(req.status) ~= "string" then
            mp.msg.error("AFP_BATCH: not a request: " .. req_file)
            mp.add_timeout(0.1, function() mp.commandv("quit", "2") end)
        else
            local root = ws.normalize(req.project)
            local status = { kind = req.kind, project = root, state = "starting", done = 0, total = 0, failed = 0 }
            local function write_status()
                local out = io.open(req.status .. ".tmp", "w")
                if not out then return end
                out:write(utils.format_json(status) or "{}")
                out:close()
                if not os.rename(req.status .. ".tmp", req.status) then
                    os.remove(req.status)
                    os.rename(req.status .. ".tmp", req.status)
                end
            end
            local started, stop_sent, ended = false, false, false
            batch = {}
            function batch.looking()
                status.state = "looking"
                write_status()
            end
            function batch.finished(sum)
                if ended then return end
                ended = true
                status.state = sum.stopped and "stopped" or (sum.error and "failed") or "done"
                status.made, status.failed = sum.made or 0, sum.failed or 0
                status.done = status.total > 0 and (sum.stopped and status.done or status.total) or status.done
                status.failures = sum.failures
                status.message = sum.nothing or sum.error
                status.fraction, status.current = nil, nil
                write_status()
                mp.add_timeout(0.2, function() mp.commandv("quit") end)
            end
            local function progress()
                if not started or ended then return end
                local j = clips_job or marks_job or export_job
                if j then
                    status.state = stop_sent and "stopping" or "running"
                    status.total = j.total or #j.queue
                    status.done = j.done
                    status.failed = j.failed
                    local item = j.queue and j.queue[j.done + 1]
                    if clips_job then item = nil end
                    status.current = item and (item.name or item.relative) or nil
                    if export_job and export_job.progress then
                        local pf_ = io.open(export_job.progress, "r")
                        local st = pf_ and utils.parse_json(pf_:read("*a") or "")
                        if pf_ then pf_:close() end
                        status.fraction = type(st) == "table" and st.total and st.total > 0 and st.done / st.total or nil
                    end
                    write_status()
                end
                -- Stop: after the one being made (clip previews: that one too; an export: cancelled)
                if req.stop and not stop_sent and fs.is_file(req.stop) then
                    stop_sent = true
                    status.state = "stopping"
                    write_status()
                    if clips_job then
                        clips_job.stopping = true
                        if clips_job.id then mp.abort_async_command(clips_job.id) end
                    elseif marks_job then
                        marks_job.stopping = true
                    elseif export_job then
                        export_job.stopping = true
                        mp.commandv("script-message-to", "gyroflow", "export-batch-stop", tostring(export_job.id))
                    else
                        batch.finished({ stopped = true })
                    end
                end
            end
            local function start()
                if started then return end
                started = true
                state.project = root
                migrate(root)
                if not (fs.is_file(pf.path(root)) or fs.is_dir(pf.legacy_dir(root))) then
                    return batch.finished({ error = root .. " isn't a project (no " .. workspace.PROJECT_FILE .. " in it)" })
                end
                write_status()
                local kind = req.kind
                if kind == "clips" or kind == "clips-again" then
                    make_clips(kind == "clips-again", root)
                elseif kind == "moments" then
                    start_marks_job("moments", root)
                elseif kind == "photos" then
                    if req.photo == "no" or req.photo == "live" or req.photo == "motion" then
                        mp.set_property_native("user-data/gyroflow/live_photo", req.photo)
                    end
                    start_marks_job("photos", root)
                elseif kind == "export" then
                    export_moments(root, req.sdr == true)
                else
                    batch.finished({ error = "no such batch: " .. tostring(kind) })
                end
            end
            write_status()
            mp.add_periodic_timer(0.5, progress)
            -- Once the gyroflow plugin has said where gyroflow-export is and what the app's defaults are
            local function ready()
                if mp.get_property_native("user-data/gyroflow/exporter") and mp.get_property_native("user-data/gyroflow/defaults") then
                    start()
                end
            end
            mp.observe_property("user-data/gyroflow/exporter", "native", ready)
            mp.observe_property("user-data/gyroflow/defaults", "native", ready)
            mp.add_timeout(20, function()
                if not started then batch.finished({ error = "the gyroflow plugin didn't start" }) end
            end)
        end
    end
end
