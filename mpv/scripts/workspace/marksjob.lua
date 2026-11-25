-- marksjob: the project's marks all at once (the Project tab's, the Library's batch): a moment of each mark in no
-- moment yet, or a photo of each. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local link = require 'link'
local moments_lib = require 'moments'
local transform = require 'transform'

local M = {}
-- Set by install (main.lua's W)
local W, state, ws, moments, sidecar, cache, tracked_cache, batch, markers
local function publish(...) return W.publish(...) end
local function save_cache(...) return W.save_cache(...) end
local function refresh(...) return W.refresh(...) end
local function mkdir(...) return W.mkdir(...) end
local function own_project(...) return W.own_project(...) end
local function project_tool(...) return W.project_tool(...) end
local function run_tool(...) return W.run_tool(...) end
local function in_only(...) return W.in_only(...) end
local function marks_of(...) return W.marks_of(...) end
local function fingerprint(...) return W.fingerprint(...) end
local function clock(...) return W.clock(...) end
local function need_project(...) return W.need_project(...) end
local function native(...) return W.native(...) end
local function generated_runs(...) return W.generated_runs(...) end

-- The project's marks, all at once (the Project tab's): a moment for each mark in no moment yet, 2 s on each side,
-- those that overlap joined into one (the clip's marks: its own file's, else the camera's); or a photo of each mark,
-- except those that have one somewhere in the project's folder (its link: the clip and the time). A run's go into a
-- folder of its own, Generated/<date> <time> Moments or Photos (docs/design/library-management.md §8). One at a time in the background, each with its clip's saved view (else the defaults); a
-- moment with settings of its own, as one saved with k: a copy of the clip's Gyroflow project when it has one. While it
-- runs: user-data/workspace/marks_job { kind (moments, photos), done, total, failed }
local MOMENT_AROUND = 2
local marks_job = nil

local function stills_line(relative, size, t) return relative .. "\t" .. tostring(size) .. "\t" .. string.format("%.3f", t) end
-- The photos there are of marks: a stills_line per photo (its clip's path, size and time), and the same with its
-- clip's identity in place of the path, so a clip moved since still has its photos
local function photos_made(root)
    local made = {}
    for _, p in ipairs(moments.photos(root, tracked_cache)) do
        local l = p.link
        if type(l.t) == "number" then
            made[stills_line(l.clip, l.size, l.t)] = true
            if type(l.hash) == "string" then made[l.hash .. "\t" .. string.format("%.3f", l.t)] = true end
        end
    end
    save_cache()
    return made
end

-- What's made of a source by the batches (moments and photos of all marks): its bundle, else the app's defaults (not
-- the session's: a moment is kept as made, and the session may be a try of the moment), whole; and its input, which
-- the preview or photo is rendered with too. The bundle alone goes into a moment's link
local function made_view_of(file, movie, own)
    -- `own`: a moment's own bundle, which it keeps whatever the source's is now (transform.render_settings: the
    -- Library's recipe works it out the same way)
    local bundle, rendered = transform.render_settings(sidecar.read(file), own, transform.defaults(mp.get_property_native))
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
    if j and #j.queue > 0 then
        mp.notify({ id = "marks-job", kind = "working", title = j.kind == "moments" and "Making moments of the marks" or "Saving photos of the marks",
                    detail = string.format("%d of %d", j.done, #j.queue), share = j.done / #j.queue })
    end
end

local function job_finish(message)
    local j = marks_job
    marks_job = nil
    job_progress()
    if batch and j then return batch.finished({ made = j.done - j.failed, failed = j.failed, failures = j.failures, stopped = j.stopping }) end
    if message then mp.notify({ id = "marks-job", kind = (j and j.failed > 0) and "error" or "done", title = message, seconds = 5 })
    else mp.notify({ id = "marks-job", kind = "hide" }) end
    if j and j.kind == "moments" then refresh() end
end

local job_next
function M.job_done(id, json)
    local j = marks_job
    if not j or tostring(j.id) ~= id then return end
    local r = utils.parse_json(json or "") or {}
    local item = j.queue[j.done + 1]
    if not r.ok then
        j.failed = j.failed + 1
        j.failures[#j.failures + 1] = { tostring(item and (item.relative or item.name) or ""), tostring(r.error or "failed") }
        mp.msg.warn((j.kind == "moments" and "moment " or "photo ") .. tostring(item and item.name or "") .. ": " .. tostring(r.error))
    end
    j.done = j.done + 1
    job_progress()
    job_next()
end

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
            id = id, clip = item.clip, t = item.t, view = item.rendered, reply = mp.get_script_name(), done = "job-done", out_dir = j.dir }))
        return
    end
    -- A moment, as k saves one: its link, its own settings (and project), its preview
    local root = j.root
    mkdir(j.dir)
    local file, final = moments.unique_file(j.dir, item.name)
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
    local made = kind == "photos" and photos_made(root) or nil
    local all = kind == "moments" and moments.scan(root, nil).list or nil
    local taken_names = {}
    for _, c in ipairs(moments.footage(root)) do
        local times = in_only(c.relative) and marks_of(c.file) or nil
        if times then
            local info = utils.file_info(c.file)
            local size = info and info.size
            local movie = markers and markers.movie and markers.movie(c.file) or nil
            local view, rendered = made_view_of(c.file, movie)
            if kind == "photos" then
                for _, t in ipairs(times) do
                    if not made[stills_line(c.relative, size, t)] then
                        -- A clip that moved: its photos by its fingerprint, read only where its marks are (few clips)
                        local h = not c.hash_read and fingerprint(c.file) or c.hash
                        c.hash_read, c.hash = true, h
                        if not (h and made[h .. "\t" .. string.format("%.3f", t)]) then
                            queue[#queue + 1] = { clip = c.file, relative = c.relative, size = size, t = t, view = view, rendered = rendered, name = ws.name(c.file) .. " " .. clock(t) }
                        end
                    end
                end
            else
                local taken = {}
                for _, m in ipairs(moments.of_clip(all, { relative = c.relative, size = size, created = link.created(c.file) })) do
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
    mp.notify({ id = "marks-job", kind = "working", title = "Looking for the marks", detail = "In the project's clips" })
    if batch then batch.looking() end
    local queue = marks_queue(kind, root)
    if #queue == 0 then
        local why = kind == "moments" and "Every mark is in a moment already" or "Every mark has its photo already"
        if batch then return batch.finished({ made = 0, failed = 0, nothing = why }) end
        return mp.notify({ id = "marks-job", kind = "done", title = why, seconds = 4 })
    end
    -- This run's folder (docs/design/library-management.md §8)
    local dir = ws.join(root, native(moments_lib.generated_rel(kind == "moments" and "Moments" or "Photos", os.time(), false, generated_runs(root))))
    marks_job = { kind = kind, queue = queue, done = 0, failed = 0, failures = {}, id = 0, root = root, dir = dir,
                  alone = (batch and batch.alone ~= nil) and batch.alone or cache.moments_alone == true }
    job_progress()
    job_next()
end

function M.install(w)
    W = w
    state, ws, moments, sidecar = W.state, W.ws, W.moments, W.sidecar
    cache, tracked_cache, batch, markers = W.cache, W.tracked_cache, W.batch, W.markers
    W.made_view_of, W.start_marks_job = made_view_of, start_marks_job
    W.marks_job = function() return marks_job end
end

return M
