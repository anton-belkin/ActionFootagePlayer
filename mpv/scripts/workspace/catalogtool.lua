-- catalogtool: the project's catalog as the workspace plugin uses it (afp-catalog, beside the gyroflow plugin's
-- tools): what's written noted, a clip's moments and the project's clips asked. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local moments_lib = require 'moments'

local M = {}
-- Set by install (main.lua's W): what plays, the file calls, the platform
local state, fs, windows

-- The project's catalog (afp-catalog, docs/design/library-management.md §9): a clip's moments without looking through the
-- project's folder (asked in the background: playing never waits for it), and what the player writes noted, for the
-- Library and the other computers. The player never looks through folders for it nor folds it: the Library does
local catalog = {}
function catalog.tool()
    local dir = mp.get_script_directory()
    local plugins = dir and dir:match("^(.*)[/\\][^/\\]+[/\\]?$")
    if not plugins then return nil end
    local exe = utils.join_path(utils.join_path(plugins, "gyroflow"), "afp-catalog" .. (windows and ".exe" or ""))
    return fs.is_file(exe) and exe or nil
end
-- Files written or removed: noted in the background (their folders touched, a line in this computer's journal).
-- project nil: found from the files (their location's .afplocation)
-- One note at a time (this computer's journal has one writer): what's asked meanwhile waits, a project's files put
-- together, and goes next
catalog.pending = 0
local note_queue, noting = {}, false
local function note_next()
    if noting or #note_queue == 0 then return end
    local exe = catalog.tool()
    local first = table.remove(note_queue, 1)
    local files = first.files
    for i = #note_queue, 1, -1 do
        if note_queue[i].project == first.project then
            for _, f in ipairs(table.remove(note_queue, i).files) do files[#files + 1] = f end
            first.merged = first.merged + 1
        end
    end
    local args = { exe, "note", first.project or "-" }
    for _, f in ipairs(files) do args[#args + 1] = f end
    noting = true
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stderr = true, args = args }, function(ok, r)
        noting = false
        catalog.pending = catalog.pending - 1 - first.merged
        if not (ok and r and r.status == 0) then mp.msg.verbose("afp-catalog note: " .. tostring(r and r.stderr)) end
        note_next()
    end)
end
function catalog.note(project, files)
    local exe = catalog.tool()
    if not exe or #files == 0 then return end
    local list = {}
    for i, f in ipairs(files) do list[i] = f end
    catalog.pending = catalog.pending + 1
    note_queue[#note_queue + 1] = { project = project, files = list, merged = 0 }
    note_next()
end
-- cb once the notes under way are through (a batch quits only then: a quit cuts them off), a minute at most
function catalog.after_notes(cb)
    local waited = 0
    local function check()
        if catalog.pending <= 0 or waited >= 60 then return cb() end
        waited = waited + 0.2
        mp.add_timeout(0.2, check)
    end
    check()
end
-- The project's own files with a link of a kind ("moment", "clip", "export", "photo"; nil: all), in the background:
-- cb(list as moments.scan's: { file, name, link, kind }), cb(nil) when the project has no catalog (or no afp-catalog)
function catalog.ask(args_after, cb)
    local exe = catalog.tool()
    if not exe then return cb(nil) end
    local args = { exe }
    for _, a in ipairs(args_after) do args[#args + 1] = a end
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true, args = args }, function(ok, r)
        local v = ok and r and r.status == 0 and utils.parse_json(r.stdout or "")
        if type(v) ~= "table" or v.catalog ~= true then return cb(nil) end
        local list = {}
        for _, it in ipairs(v.items or {}) do
            if type(it.link) == "table" and type(it.path) == "string" then
                list[#list + 1] = { file = it.path, name = moments_lib.display_name(it.path), link = it.link, kind = it.kind, tags = it.tags, missing = it.missing }
            end
        end
        cb(list)
    end)
end
-- The project's own folders looked at (afp-catalog look --own, at low priority): what Finder changed there noted;
-- the clip's moments asked again when something changed. Not sooner than `every` s after the last look on this
-- computer (by anyone); one at a time
local looking = false
function catalog.look(project, every)
    local exe = catalog.tool()
    if not exe or looking then return end
    looking = true
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true,
                              args = { exe, "look", project, "--own", "--every", tostring(every or 20) } }, function(ok, r)
        looking = false
        local v = ok and r and r.status == 0 and utils.parse_json(r.stdout or "")
        if type(v) == "table" and (v.changes or 0) > 0 and state.project == project and catalog.changed then catalog.changed() end
    end)
end
-- The same, waited for (a batch's list, half a second): the parsed JSON's items, nil without a catalog
function catalog.now(args_after)
    local exe = catalog.tool()
    if not exe then return nil end
    local args = { exe }
    for _, a in ipairs(args_after) do args[#args + 1] = a end
    local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true, args = args })
    local v = r and r.status == 0 and utils.parse_json(r.stdout or "")
    if type(v) ~= "table" or v.catalog ~= true then return nil end
    return v.items or {}
end
function catalog.derived(project, hash, clip, cb)
    local args = { "derived", project }
    if hash then args[#args + 1] = "--hash"; args[#args + 1] = hash end
    if clip then args[#args + 1] = "--clip"; args[#args + 1] = clip end
    catalog.ask(args, cb)
end

function M.install(W)
    state, fs, windows = W.state, W.fs, W.windows
    W.catalog = catalog
end

return M
