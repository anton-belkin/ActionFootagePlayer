-- workspace: which project the clip that plays belongs to, where that project's files for it are, and its moments
-- (docs/design/moments-and-workspaces.md). An mpv plugin: this folder goes into ~~/scripts/.
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
-- Tags and culling (tags.lua, docs/design/tags-and-culling.md): P accept, X reject, U neither, 1-5 stars, 0 none, K a custom
-- tag, on the original or the moment playing; kept in the original's own file or the link, synced with the library's
-- stores (Finder's tags): tagging.lua.
--
-- Clip previews: <the clip's path>.afplink.mp4 in a run's Generated/<date> <time> Clip previews/ (moved anywhere in
-- the project's folder from there), a flipbook of the clip in color (its marks, else a dozen frames), made by Action
-- Footage Library (the batch mode, batch.lua), one at a time in the background, for the clips without
-- one. Nothing remembers a deleted one (docs/design/library-management.md §8): asked again, it's made again.
--   (menu, the panel)   this clip out of its album: its link to the Trash (the clip stays). X was it: now Reject
--                       (docs/design/tags-and-culling.md)
-- A clip preview's file plays its original, all of it.
--
-- A clip spoiled in part (docs/design/lossless-cut.md): Ctrl+x cuts the A-B part out, the menu also all before or after
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
--   moments_total   the project's moments; clips_stats: { total (clips), previews (there), new (without one) }
--   clip_preview    this clip's preview: made, removed (made, then deleted: not interesting) or none
--   project         the project's folder, absent for none
--   relative        the clip's path in it ("Day 1/A.MP4"), absent for none
--   gyroflow_projects  the paths the clip's Gyroflow project may have, in the order looked at
--   gyroflow_new    where a new Gyroflow project for it goes (Ctrl+e, the gyroflow plugin)
--   sidecar_file    the clip's own file, its marks and saved view (sidecar.lua): beside the clip, in whichever of the
--                   project's locations has it (a working copy's is the archive's clip's)
--   own_others      the clip's own files beside its other copies (in other locations): read, never written; the main
--                   one is sidecar_file (workspace.lua's own_file)
--   moments         after it loads: the clip's moments, { name, file, in, out }, in the order of their in
--   tags            once it loads: { kind (original, moment), pick, rating, list (custom), recent, stores }
--   moments_folder  where the project's new moments go: its Generated/ folder (_Moments/ in a project of before)
--   saving          a moment's name while it's being saved
--   cutting         while a part is cut out: { name, part, parts }
--   export          when an export plays: { name, original (nil: not found), in, out }
--   input           the source's input saved in its own file: { curve, rotation, underwater, uw, uw_highlights,
--                   uw_strength } (transform.lua); input_now:
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
-- gyroflow plugin: export-name <json> asks the export's name (ext: .mkv for YouTube's, else .mov) and answers
-- export-start <json> ({ clip, sdr, youtube, output, link }).
--
-- workspace.lua is the lookup itself (plain Lua; tests/workspace_test.lua), and gyroflow-export has its twin
-- (vs-gyroflow/src/workspace.rs) for the Finder quick action and the command line. moments.lua is the moments'
-- part with no mpv in it (tests/moments_test.lua). longpath.lua lets the Lua file calls take Windows paths past 260
-- characters, here and in the plugins that read or write beside the clip (gyroflow, dlogm, highlights, dji-telemetry).
--
-- The plugin's parts, each a module of this folder: main.lua wires them, registering every event, key and script
-- message here, in the order they always were. places.lua (the platform, the file calls, the project's locations),
-- catalogtool.lua (afp-catalog), common.lua (small helpers, the name dialog, the tools beside the gyroflow plugin),
-- trash.lua, momentlist.lua (the clip's moments, as chapters too; the scan's cache), opening.lua (what a file opens
-- as; the project initialized), momentsave.lua (k, the menu's moment commands, an export's name, a photo link),
-- clippreviews.lua (and the project's counts), cutout.lua (Ctrl+x), viewsave.lua (the transform and the input saved,
-- Autoplay next), marksjob.lua (moments or photos of all marks), renders.lua (exports of all moments, renders),
-- tagging.lua (tags and culling), inbox.lua (the Library's changes heard), batch.lua (batch mode, AFP_BATCH).

local mp = require 'mp'

-- The time of the frame on screen (docs/design/seek-and-scrub.md): with the GPU path's temporal denoise the picture is up to
-- its delay behind time-pos (the newest frame decoded), or the frame asked for while paused (gyroflow's
-- user-data/gyroflow/gpu_delay: { delay, restart, show }); otherwise time-pos. As side-panel timeline.lua's shown_time
local function shown_time()
    local t = mp.get_property_number("time-pos")
    local d = mp.get_property_native("user-data/gyroflow/gpu_delay")
    if not t or type(d) ~= "table" or not d.delay or d.delay <= 0 then return t end
    if type(d.show) == "number" and d.show >= 0 then return d.show end
    local restart = (type(d.restart) == "number" and d.restart >= 0) and math.min(d.restart, t) or t
    return math.max(restart, t - d.delay)
end
-- What it says on screen goes to the side panel's cards and dialog (side-panel/notify_client.lua), when it's there
pcall(function()
    package.path = package.path .. ";" .. mp.get_script_directory():gsub("[^/\\]*$", "") .. "side-panel/?.lua"
    require 'notify_client'
end)
-- (without it: mpv's own text, and the console asks)
mp.notify = mp.notify or function(t) if t.kind ~= "hide" then mp.osd_message((t.title or "") .. (t.detail and ("\n" .. t.detail) or ""), t.kind == "working" and 60 or 4) end end
mp.ask = mp.ask or function() return false end
local utils = require 'mp.utils'
-- Windows: paths past 260 characters (a clip deep in a trip's folders) for the file calls here and in the modules
local longpath = require 'longpath'
longpath.install(utils)
-- The responsiveness journal (journal.lua, docs/design/responsiveness-journal.md): every plugin's keys and steps by default;
-- this one also keeps the player's own (its start, files opened, seeks, the first frame).
-- Protected, as in the other plugins: the journal never stops this one
do
    local ok, j = pcall(require, 'journal')
    if ok and type(j) == "table" and j.core then pcall(j.core) end
end
local sidecar_lib = require 'sidecar'
local projectfile = require 'projectfile'
-- A list opened from Action Footage Library: the playlist, from the item opened
require('librarylist').install(mp)
local has_input, input = pcall(require, 'mp.input')
if not has_input then input = nil end

-- The parts. Each one's install(W) takes what it needs of W and puts in it what it offers the others; a part calls
-- another's functions through W as it runs, so only the values in W (fs, the catalog, the cache...) need the order
-- they're installed in below
local places = require 'places'
local catalogtool = require 'catalogtool'
local common = require 'common'
local trash = require 'trash'
local momentlist = require 'momentlist'
local clippreviews = require 'clippreviews'
local opening = require 'opening'
local momentsave = require 'momentsave'
local cutout = require 'cutout'
local viewsave = require 'viewsave'
local marksjob = require 'marksjob'
local renders = require 'renders'
local tagging = require 'tagging'
local inbox = require 'inbox'
local batch = require 'batch'

-- What's published as user-data/workspace (the list above): one table for the plugin's life, its fields replaced as
-- each file opens (replace_state, opening.lua's), so a part keeps it; generation counts the files described
local state = {}
local W = { state = state, generation = 0, input = input, shown_time = shown_time }
function W.replace_state(t)
    for k in pairs(state) do state[k] = nil end
    for k, v in pairs(t) do state[k] = v end
    W.generation = W.generation + 1
end
-- An export's original asked for (open-original, a moment edited, a cut, the Library's lens type): its part, set
-- when it loads (opening.lua's on_load)
W.wanted = nil

local function publish()
    mp.set_property_native("user-data/workspace", state)
end
W.publish = publish

places.install(W)
local fs, ws, windows = W.fs, W.ws, W.windows
catalogtool.install(W)
local catalog = W.catalog
-- The Library's batch request (AFP_BATCH), read before the parts that make what it asks for: they keep W.batch
batch.request(W)
local sidecar = sidecar_lib.new({ parse_json = fs.parse_json, format_json = utils.format_json, windows = windows, own_path = W.own_path,
                                  note = function(f) catalog.note(state and state.project, { f }) end })
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
W.sidecar, W.pf = sidecar, pf
for _, part in ipairs({ common, trash, momentlist, clippreviews, opening, momentsave, cutout, viewsave, marksjob, renders, tagging, inbox }) do
    part.install(W)
end

mp.observe_property("chapter-list", "native", momentlist.chapters_changed)
mp.add_hook("on_load", 10, opening.on_load)
mp.register_event("file-loaded", momentlist.loaded)
mp.register_script_message("preview-done", momentsave.preview_done)
-- Another plugin wrote or removed a file of the project's (the highlights plugin's marks): noted in the catalog
mp.register_script_message("catalog-note", momentsave.catalog_note)
mp.add_key_binding("k", "moment", function() momentsave.save_key() end)
mp.register_script_message("save-moment", function(name) momentsave.save_key(name) end)
mp.register_script_message("moment-play", momentsave.moment_play)
mp.register_script_message("moment-edit", momentsave.moment_edit)
mp.register_script_message("moment-reveal", momentsave.moment_reveal)
mp.register_script_message("moments-folder", momentsave.moments_folder)
mp.register_script_message("moment-remove", momentsave.moment_remove)
mp.register_script_message("moment-rename", momentsave.moment_rename)
mp.register_script_message("export-name", momentsave.export_name)
mp.register_script_message("open-original", momentsave.open_original)
mp.register_script_message("clips-stop", clippreviews.clips_stop)
mp.register_script_message("project-folder", opening.project_folder)
mp.add_key_binding(nil, "not-interesting", clippreviews.not_interesting)
mp.register_event("shutdown", clippreviews.shutdown)
mp.register_script_message("clips-folder", clippreviews.clips_folder)
mp.register_script_message("init-project", opening.init_project)
mp.register_event("file-loaded", momentlist.hash_loaded)
mp.add_key_binding("Ctrl+x", "cut-out", cutout.cut_key)
mp.register_script_message("cut-before", cutout.cut_before)
mp.register_script_message("cut-after", cutout.cut_after)
mp.register_script_message("view-save", viewsave.view_save)
mp.register_script_message("view-revert", viewsave.view_revert)
mp.register_script_message("view-auto", viewsave.view_auto)
mp.register_script_message("input-set", viewsave.input_set)
mp.register_script_message("input-save", viewsave.input_save)
mp.register_script_message("input-revert", viewsave.input_revert)
mp.register_script_message("moment-alone", viewsave.moment_alone)
mp.register_script_message("autoplay", viewsave.autoplay)
mp.register_script_message("view-own", viewsave.view_own)
mp.observe_property("user-data/gyroflow", "native", viewsave.view_changed)
mp.observe_property("user-data/uwcolor", "native", viewsave.view_changed)
mp.register_event("file-loaded", viewsave.loaded)
mp.register_script_message("moments-alone", momentlist.moments_alone)
mp.register_script_message("job-done", marksjob.job_done)
mp.register_script_message("open-library", opening.open_library)
mp.register_script_message("export-batch-done", renders.export_batch_done)
mp.add_key_binding("K", "tags", tagging.ask)
mp.add_key_binding("P", "accept", function() tagging.pick("accept") end)
mp.add_key_binding("X", "reject", function() tagging.pick("reject") end)
mp.add_key_binding("U", "unpick", function() tagging.pick("none") end)
for n = 0, 5 do mp.add_key_binding(tostring(n), "rate-" .. n, function() tagging.rate(n) end) end
mp.register_script_message("tag-ask", tagging.ask)
mp.register_script_message("tag", tagging.tag)
mp.register_script_message("tag-toggle", tagging.toggle)
mp.register_script_message("tag-rate", tagging.rate)
mp.register_script_message("tag-pick", tagging.pick_message)
-- Each item synced with the stores as it loads: what was changed in Finder meanwhile comes in
mp.register_event("file-loaded", function() tagging.reload(true) end)
mp.register_event("file-loaded", inbox.loaded)
inbox.start()
mp.register_script_message("photo-link", momentsave.photo_link)
-- Last: a batch starts once the gyroflow plugin is ready
batch.run()
