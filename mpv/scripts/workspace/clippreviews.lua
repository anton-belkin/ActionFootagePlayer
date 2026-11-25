-- clippreviews: clip previews, made one at a time in the background (the menu, X, the Library's batch), and the
-- project's clips counted for the side panel. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local moments_lib = require 'moments'
local transform = require 'transform'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, moments, sidecar, catalog, counting, cache, tracked_cache
local function publish(...) return W.publish(...) end
local function save_cache(...) return W.save_cache(...) end
local function need_project(...) return W.need_project(...) end
local function native(...) return W.native(...) end
local function generated_runs(...) return W.generated_runs(...) end
local function mkdir(...) return W.mkdir(...) end
local function reveal(...) return W.reveal(...) end
local function trash(...) return W.trash(...) end
local function trash_name(...) return W.trash_name(...) end
local function fingerprint(...) return W.fingerprint(...) end
local function plugin_tool(...) return W.plugin_tool(...) end
local extras, clips_layout, write_empty

-- Clip previews (_Clips/): one at a time, in the background, from the menu or X
local clips_job = nil
-- The next run of clip previews makes drafts (the Library's batch asks)
local clips_draft = false
-- Batch mode (Action Footage Library runs the player without a window to make them): { finished(summary), ... }
local batch = nil
-- What a batch is kept to (Action Footage Library's selection): paths in the project ("Day 1/A.MP4", a folder
-- "Day 1", a moment "_Moments/Reef/Turtle.afplink.mp4"), each taking what's under it too; nil: the whole project
local batch_only = nil
local function in_only(relative)
    if not batch_only then return true end
    for _, p in ipairs(batch_only) do
        if relative == p or relative:sub(1, #p + 1) == p .. "/" then return true end
    end
    return false
end
local recount

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
-- The next run's drafts or not (the batch's request)
local function set_clips_draft(v) clips_draft = v end


-- The clip previews there are, found by the link inside each wherever they are in the project's folder: renamed, or
-- moved into folders of one's own, they're still the clip's. { [the clip's path in the project] = { file, size } }, read again a minute on
local previews_cache = { time = -1e9, index = {} }
local function previews(force)
    local project = state.project
    if not project then return {} end
    if not force and previews_cache.project == project and mp.get_time() - previews_cache.time < 60 then return previews_cache.index end
    local index = {}
    -- From the project's catalog (no walk of its folder, no file looked at), else its folder walked
    local items = catalog.now({ "list", project, "--kind", "clip" })
    if items then
        for _, it in ipairs(items) do
            if type(it.link) == "table" and type(it.link.clip) == "string" then index[it.link.clip] = { file = it.path, size = it.link.size, seen = true } end
        end
    else
        for _, p in ipairs(moments.scan(project, tracked_cache, nil, "clip").list) do
            if type(p.link.clip) == "string" then index[p.link.clip] = { file = p.file, size = p.link.size } end
        end
        save_cache()
    end
    previews_cache = { project = project, time = mp.get_time(), index = index }
    return index
end

-- A clip's preview file, nil for none
local function preview_for(relative, size)
    local p = relative and previews()[relative]
    if p and (p.size == nil or size == nil or p.size == size) and (p.seen or fs.is_file(p.file)) then return p.file end
    return nil
end

-- Where a new preview goes: at the clip's path in the run's folder (Generated/<date> <time> Clip previews; flat: in
-- that folder itself). A name another clip's preview has gets the clip's folder in parentheses ("GX010123 (100GOPRO)"),
-- then a number
local function new_preview_path(dir, c, flat)
    if c.place then c.place() end
    local file = ws.join(dir, native(c.preview_rel or moments_lib.clip_preview_relative(c.relative, flat)))
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

-- The clip previews being made: a card (the side panel's), how many of how many
local function draw_progress()
    if not clips_job then return end
    mp.notify({ id = "clip-previews", kind = "working", title = "Making clip previews",
                detail = string.format("%d of %d", clips_job.done, clips_job.total), share = clips_job.done / math.max(1, clips_job.total) })
end

-- Which of the queue's clips comes next: the first in a folder the Library shows now (batch.focus: a file it rewrites
-- as its folder changes, { dirs = [absolute folders] }; a clip's preview or the clip itself in one), else nil (the
-- first one). Read for each clip: the Library's own file, tiny
local function focus_first(queue)
    local file = batch and batch.focus
    if not file or #queue < 2 then return nil end
    local f = io.open(file, "r")
    local v = f and utils.parse_json(f:read("*a") or "")
    if f then f:close() end
    local dirs = type(v) == "table" and v.dirs
    if type(dirs) ~= "table" or #dirs == 0 then return nil end
    local function norm(p) return (p:gsub("\\", "/"):gsub("/+$", "")) end
    for i, c in ipairs(queue) do
        for _, d in ipairs(dirs) do
            if type(d) == "string" and d ~= "" then
                local pre = norm(d) .. "/"
                if (c.draft_path and norm(c.draft_path):sub(1, #pre) == pre) or (c.file and norm(c.file):sub(1, #pre) == pre) then return i end
            end
        end
    end
    return nil
end

local function next_clip()
    local job = clips_job
    if not job then return end
    local c = not job.stopping and table.remove(job.queue, focus_first(job.queue) or 1)
    if not c then
        clips_job = nil
        state.clips = nil
        -- What was written, into the project's catalog (the Library and the other computers see it at once)
        if #job.notes > 0 then catalog.note(job.project, job.notes) end
        publish()
        draw_progress()
        -- Why the ones that failed did: in the player's log (and the batch's status, which the Library shows)
        if #job.failures > 0 then
            for _, f in ipairs(job.failures) do mp.msg.warn("clip preview of " .. tostring(f[1]) .. " not made: " .. tostring(f[2])) end
        end
        recount()
        if batch then
            return catalog.after_notes(function() batch.finished({ made = job.made, failed = job.failed, failures = job.failures, stopped = job.stopping }) end)
        end
        local what = job.made == 1 and "1 clip preview" or (job.made .. " clip previews")
        mp.notify({ id = "clip-previews", kind = job.failed > 0 and "error" or "done", title = (job.stopping and "Stopped: " or "Made ") .. what,
                    detail = job.failed > 0 and (job.failed .. " couldn't be made: " .. tostring(job.failures[1][2])) or nil })
        return
    end
    local exporter = mp.get_property_native("user-data/gyroflow/exporter")
    -- Finishing a draft: made beside it under a hidden name, then put in its place (where it is now: found again by
    -- its link when it was moved meanwhile)
    local out = c.draft_path and (c.draft_path:gsub("([^/\\]+)$", ".tmp-finish-%1")) or new_preview_path(job.dir, c, job.flat)
    local args = { exporter, "--preview", "flipbook", "--force", "--output", out }
    -- At background priority while a video plays (not in a batch: macOS's background priority all but stops it, reads
    -- from the NAS too: a minute and more for a clip)
    if not mp.get_property_native("pause") and not mp.get_property_native("idle-active") then table.insert(args, "--background") end
    local times = marks_of(c.file)
    if times then
        local list = {}
        for i, t in ipairs(times) do list[i] = string.format("%.3f", t) end
        table.insert(args, "--at"); table.insert(args, table.concat(list, ","))
        table.insert(args, "--cover"); table.insert(args, list[1])
    end
    -- The underwater colour, where the clip has it on (its source's input, else uwcolor.conf's): in it, as it plays (the
    -- settings JSON gyroflow-export takes, docs/reference/contracts.md; the rest of a clip preview is its own: unstabilized)
    do
        local _, rendered = transform.render_settings(sidecar.read(c.file), nil, transform.defaults(mp.get_property_native))
        if rendered.uw == true then
            table.insert(args, "--settings")
            table.insert(args, utils.format_json({ uw = true, uw_strength = tonumber(rendered.uw_strength) or 1, uw_highlights = rendered.uw_highlights == true }))
        end
    end
    table.insert(args, "--link")
    table.insert(args, utils.format_json({ v = 1, kind = "clip", clip = c.relative, size = c.size, roots = { job.project } }))
    table.insert(args, c.file)
    mp.msg.verbose("clip preview: " .. table.concat(args, " "))
    job.id = mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 and c.draft_path then
            local dest = c.draft_path
            if not fs.is_file(dest) then
                dest = nil
                for _, it in ipairs(catalog.now({ "list", job.project, "--kind", "clip" }) or {}) do
                    if type(it.link) == "table" and it.link.clip == c.relative and it.link.draft then dest = it.path; break end
                end
            end
            if dest then
                os.remove(dest)
                local ok = os.rename(out, dest)
                -- Into the catalog at once: the Library shows its picture as it comes
                if ok then job.made = job.made + 1; catalog.note(job.project, { dest }) else os.remove(out); job.failed = job.failed + 1 end
            else
                -- The draft is gone (removed meanwhile): its finished preview isn't wanted either
                os.remove(out)
            end
        elseif r and r.status == 0 then
            job.made = job.made + 1
            -- Into the catalog as it comes (notes asked meanwhile go together): the Library shows each at once, and a
            -- run stopped or ended halfway has what it made noted
            catalog.note(job.project, { out })
        else
            -- Its half-made one, under its hidden name, goes (stopped too: it's made again next time)
            if c.draft_path then os.remove(out) end
            if not job.stopping then
                job.failed = job.failed + 1
                local err = r and r.stderr or ""
                local last
                for line in err:gmatch("[^\r\n]+") do if line:match("%S") then last = line:gsub("^%s+", "") end end
                table.insert(job.failures, { c.relative, last or (r and r.error_string) or "gyroflow-export failed" })
                mp.msg.warn("clip preview of " .. c.relative .. ": " .. err)
            end
        end
        job.done = job.done + 1
        state.clips = { done = job.done, total = job.total }
        publish()
        draw_progress()
        next_clip()
    end)
end

-- Empty previews (Anton, 2026-11-10: every clip's preview there at once, to arrange the footage before any picture
-- is made): one dark frame with the link inside, written by afp-cut 200 at a time (~6 ms each on the NAS);
-- the "clips-finish" job queued after makes each one real in its place, the folder the Library shows first. Their
-- names as new_preview_path gives them: in Generated/ (no folder per run) a name another clip's preview has, in the
-- catalog or written in this run, gets the clip's folder in parentheses, then a number; one that's there all the same
-- (not in the catalog yet) is tried again under the next name
write_empty = function(job)
    local list, taken, of = {}, {}, {}
    for _, it in ipairs(catalog.now({ "list", job.project, "--kind", "clip" }) or {}) do
        if type(it.path) == "string" then taken[it.path] = true end
    end
    local trips = clips_layout() == "trips"
    local function name_for(c)
        local rel = (trips and c.trip) and (c.trip .. "/" .. moments_lib.clip_preview_relative(c.relative, true)) or moments_lib.clip_preview_relative(c.relative, job.flat)
        local file = ws.join(job.dir, native(rel))
        if taken[file] then
            local dir, stem = ws.parent(file), moments_lib.display_name(file)
            local folder = c.relative:match("([^/]+)/[^/]+$")
            local alt = folder and ws.join(dir, stem .. " (" .. folder .. ")" .. moments_lib.EXT)
            local n = 2
            while not alt or taken[alt] do alt = ws.join(dir, stem .. " " .. n .. moments_lib.EXT); n = n + 1 end
            file = alt
        end
        taken[file] = true
        of[file] = c
        list[#list + 1] = { out = file, link = utils.format_json({ v = 1, kind = "clip", clip = c.relative, size = c.size, roots = { job.project }, draft = true }) }
    end
    for _, c in ipairs(job.queue) do name_for(c) end
    -- 200 at a time, each 200 into the catalog as they're written: the progress moves, Footage fills as it goes
    local exe = plugin_tool("afp-cut")
    job.queue = {}
    local at = 1
    local function chunk()
        if job.stopping or at > #list then return next_clip() end
        local part = {}
        for i = at, math.min(at + 199, #list) do part[#part + 1] = list[i] end
        at = at + #part
        local function took(written, failed, there)
            -- Names taken all the same: again under the next one (at the end of the list; counted when written)
            for _, f in ipairs(there or {}) do
                if of[f] then taken[f] = true; job.total = job.total + 1; name_for(of[f]) end
            end
            job.made = job.made + #written
            job.done = math.min(job.total, job.done + #part)
            catalog.note(job.project, written)
            for _, x in ipairs(failed) do
                job.failed = job.failed + 1
                table.insert(job.failures, { type(x) == "table" and x[1] or "?", type(x) == "table" and x[2] or tostring(x) })
            end
            state.clips = { done = job.done, total = job.total }
            publish()
            chunk()
        end
        if not exe then return took({}, { { job.dir, "afp-cut isn't installed (the gyroflow plugin's)" } }) end
        job.id = mp.command_native_async({ name = "subprocess", args = { exe, "--empty-previews" }, stdin_data = utils.format_json(part),
                                           playback_only = false, capture_stdout = true, capture_stderr = true }, function(_, r)
            job.id = nil
            local v = r and r.status == 0 and utils.parse_json(r.stdout or "")
            if type(v) ~= "table" then
                -- Stopped: what it had written is found by the next look; not a failure
                if job.stopping then return took({}, {}) end
                return took({}, { { job.dir, (r and r.stderr or ""):match("([^\r\n]+)%s*$") or "afp-cut failed" } })
            end
            took(v.written or {}, v.failed or {}, v.exists)
        end)
    end
    chunk()
end

local function start_clips(root, queue, what)
    if not mp.get_property_native("user-data/gyroflow/exporter") then
        return mp.osd_message("Clip previews need the gyroflow plugin (its gyroflow-export makes them)", 4)
    end
    -- A folder of this run's own, from where they're moved anywhere (docs/design/library-management.md §8)
    local dir = ws.join(root, native(moments_lib.generated_rel("Clip previews", os.time(), false, generated_runs(root))))
    mkdir(dir)
    clips_job = { project = root, dir = dir, queue = queue, done = 0, total = #queue, made = 0, failed = 0, failures = {},
                  flat = clips_layout(root) == "flat", draft = clips_draft, notes = {} }
    state.clips = { done = 0, total = #queue }
    publish()
    draw_progress()
    mp.osd_message(what, 3)
    if clips_job.draft then return write_empty(clips_job) end
    next_clip()
end

-- How a run's clip previews are laid out in its folder: "tree" (as the footage is), "flat" (all in it), "trips" (in a
-- folder per trip: clips at most 2 days apart). Chosen for each run: the Library's request says, else this computer's
-- last choice (kept with the player's cache)
local LAYOUTS = { tree = true, flat = true, trips = true }
clips_layout = function()
    if batch and LAYOUTS[batch.layout] then return batch.layout end
    return LAYOUTS[cache.clips_layout] and cache.clips_layout or "tree"
end

-- Where each of `queue`'s previews goes in the run's folder, out of all the project's clips (`all`): an identical
-- clip's (the same size and fingerprint as one before it, or as one that has its preview) into Duplicates/, under its
-- location's name and its folders there, as Sources show them; else by trips when that's the layout, into its trip's
-- folder. Sets c.preview_rel, c.duplicate_of
local function place_previews(root, all, queue)
    local by_size = {}
    for _, c in ipairs(all) do
        if c.size then local k = tostring(c.size); by_size[k] = by_size[k] or {}; table.insert(by_size[k], c) end
    end
    -- The one kept in place: the first location's (the project's order), then the first in the tree
    for _, list in pairs(by_size) do
        table.sort(list, function(x, y) if (x.li or 0) ~= (y.li or 0) then return (x.li or 0) < (y.li or 0) end return x.relative < y.relative end)
    end
    local hashes = {}
    local function hash(c)
        if hashes[c.file] == nil then hashes[c.file] = fingerprint(c.file) or false end
        return hashes[c.file]
    end
    local trips = clips_layout() == "trips"
    if trips then moments_lib.trips(all) end
    -- Whether a clip is identical to another is found as its turn comes (fingerprints read over the share take a
    -- while: not all of them before the first preview)
    for _, c in ipairs(queue) do
        c.place = function()
            c.place = nil
            local mates = c.size and by_size[tostring(c.size)]
            if mates and #mates > 1 and hash(c) then
                local first
                for _, m in ipairs(mates) do
                    if m.relative ~= c.relative and preview_for(m.relative, m.size) and hash(m) == hash(c) then first = m break end
                end
                if not first then for _, m in ipairs(mates) do if hash(m) == hash(c) then first = m break end end end
                if first and first.relative ~= c.relative then c.duplicate_of = first.relative end
            end
            if c.duplicate_of then
                c.preview_rel = "Duplicates/" .. moments_lib.safe_name(c.loc or "Location") .. "/" .. moments_lib.clip_preview_relative(c.loc_rel or c.relative, false)
            elseif trips and c.trip then
                c.preview_rel = c.trip .. "/" .. moments_lib.clip_preview_relative(c.relative, true)
            end
        end
    end
end

-- The project's clips without a preview (anywhere in the project's folder). Nothing remembers the ones made and
-- deleted: made again when asked (docs/design/library-management.md §8). `root`: the project's folder, in batch mode
-- (no clip plays)
-- The project's draft clip previews made real (the batch "clips-finish"): every draft, wherever it is now
local function finish_drafts(root)
    local drafts = {}
    local items = catalog.now({ "list", root, "--kind", "clip" })
    if not items then
        items = {}
        for _, p in ipairs(moments.scan(root, tracked_cache, nil, "clip").list) do items[#items + 1] = { path = p.file, link = p.link } end
    end
    local footage = {}
    for _, c in ipairs(catalog.now({ "footage", root }) or moments.footage(root)) do footage[c.relative] = c end
    local queue = {}
    for _, it in ipairs(items) do
        local l = it.link
        local c = type(l) == "table" and l.draft and footage[l.clip]
        if c then queue[#queue + 1] = { file = c.file, relative = c.relative, size = c.size, draft_path = it.path } end
    end
    if #queue == 0 then
        if batch then return batch.finished({ made = 0, failed = 0, nothing = "No draft clip previews to finish" }) end
        return mp.osd_message("No draft clip previews to finish", 3)
    end
    clips_draft = false
    start_clips(root, queue, "Finishing " .. #queue .. " draft clip previews, in their places, in the background")
end

local function make_clips(again, root)
    if clips_job then return mp.osd_message("Clip previews are being made", 2) end
    if not root then
        if not state.clip or state.clip:find("://", 1, true) then return end
        root = need_project("Making clip previews")
        if not root then return end
    end
    local queue = {}
    previews(true)
    -- The clips from the project's catalog (no walk of the shares), else its locations walked
    local all = catalog.now({ "footage", root }) or moments.footage(root)
    for _, c in ipairs(all) do
        if (not batch_only or in_only(c.relative)) and not preview_for(c.relative, c.size) then queue[#queue + 1] = c end
    end
    if #queue == 0 then
        local why = "Every clip has its preview"
        if batch then return batch.finished({ made = 0, failed = 0, nothing = why }) end
        return mp.osd_message(why, 4)
    end
    place_previews(root, all, queue)
    start_clips(root, queue, "Clip previews: " .. #queue .. (#queue == 1 and " clip" or " clips") .. " (one identical to another's into Duplicates), in the background")
end

function M.clips_stop()
    if not clips_job then return end
    clips_job.stopping = true
    if clips_job.id then mp.abort_async_command(clips_job.id) end
end

-- This clip out of its album, its link to the Trash (the clip itself stays): the menu's and the panel's. Links are
-- made in Action Footage Library (a selection in Originals, Add to an album…); on a clip in no album it only says so.
-- X was its key before culling: now Reject (docs/design/tags-and-culling.md)
function M.not_interesting()
    if not state.clip or state.clip:find("://", 1, true) or state.export then return end
    if not need_project("Taking a clip out of its album") then return end
    do
        local info = utils.file_info(state.clip)
        local preview = preview_for(state.relative, info and info.size)
        if preview then
            local where = trash_name(preview)
            return trash(preview, function(ok)
                mp.osd_message(ok and ("Out of its album: its link is in " .. where .. ". The clip itself is untouched") or ("Couldn't move its album link to " .. where), 3)
                recount()
            end)
        end
    end
    mp.osd_message("This clip is in no album. Action Footage Library puts it in one: Originals, Add to an album…", 4)
end

function M.shutdown()
    if clips_job and clips_job.id then clips_job.stopping = true; mp.abort_async_command(clips_job.id) end
end

-- The project's clips counted, and this clip's preview: from the project's catalog (afp-catalog footage, in the
-- background: no walk of the shares; asked again after a minute), else a walk of its folders, once a project in a
-- run (made or removed previews count it again: recount). The walk went through every location's every file: on a
-- NAS thousands of stat calls a minute, again each minute, for the whole time a clip played (2026-11-19: the clip's
-- own reads and its motion data's waited behind them). A step at a time (turn, with the file calls): the first one
-- now, the rest on mpv's next turns, published when it's through. A count asked for again replaces the one under
-- way; a quit, or another clip, doesn't wait for it
local stats = { cache = {}, SECONDS = 60 }
-- afp-catalog asked from the count's coroutine without holding mpv up: it waits a tenth of a second at a time (step);
-- the parsed JSON's items, nil without a catalog (or outside the coroutine)
function stats.ask(args_after)
    local exe = catalog.tool()
    if not exe or not counting.co or coroutine.running() ~= counting.co then return nil end
    local args = { exe }
    for _, a in ipairs(args_after) do args[#args + 1] = a end
    local done, items = false, nil
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true, args = args }, function(ok, r)
        local v = ok and r and r.status == 0 and utils.parse_json(r.stdout or "")
        if type(v) == "table" and v.catalog == true then items = v.items or {} end
        done = true
    end)
    while not done do coroutine.yield("wait") end
    return items
end
function stats.of(list)
    local total, previews, new = 0, 0, 0
    for _, c in ipairs(list) do
        total = total + 1
        if preview_for(c.relative, c.size) then previews = previews + 1 else new = new + 1 end
    end
    return { total = total, previews = previews, new = new }
end
local function count(project, force, s)
    local cached = stats.cache[project]
    if force or not cached or (cached.catalog and mp.get_time() - cached.time > stats.SECONDS) then
        local list = not (cached and cached.catalog == false) and stats.ask({ "footage", project }) or nil
        if list then
            cached = { time = mp.get_time(), catalog = true, stats = stats.of(list) }
        else
            cached = { time = mp.get_time(), catalog = false, stats = stats.of(moments.footage(project)) }
        end
        stats.cache[project] = cached
    end
    -- Another clip by now: its own count tells it
    if s ~= W.generation then return end
    state.clips_stats = cached.stats
    local info = fs.info(state.clip)
    if state.relative and preview_for(state.relative, info and info.size) then state.clip_preview = "made"
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
    -- Waiting for afp-catalog (coroutine_ask): looked at again in a tenth of a second
    mp.add_timeout(err == "wait" and 0.1 or 0, function() if step(co) then publish() end end)
end

extras = function(force)
    local project = state.project
    if not project or not state.clip then
        counting.co = nil
        state.clips_stats, state.clip_preview = nil, nil
        return
    end
    local s = W.generation
    counting.co, counting.calls = coroutine.create(function() count(project, force, s) end), 0
    step(counting.co)
    state.moments_alone = cache.moments_alone == true
end

-- Counted again after clip previews are made or removed
recount = function()
    previews_cache.time = -1e9
    extras(true)
    publish()
end

function M.clips_folder()
    local dir = state.project and ws.join(state.project, moments_lib.CLIPS)
    if dir and fs.is_dir(dir) then reveal(dir, true) end
end

function M.install(w)
    W = w
    state, fs, ws, moments, sidecar, catalog, counting = W.state, W.fs, W.ws, W.moments, W.sidecar, W.catalog, W.counting
    cache, tracked_cache, batch, batch_only = W.cache, W.tracked_cache, W.batch, W.batch_only
    W.markers, W.marks_of, W.in_only, W.preview_for, W.make_clips, W.finish_drafts = markers, marks_of, in_only, preview_for, make_clips, finish_drafts
    W.extras, W.recount, W.set_clips_draft = extras, recount, set_clips_draft
    W.clips_job = function() return clips_job end
end

return M
