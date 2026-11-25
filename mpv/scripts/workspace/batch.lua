-- batch: batch mode, for the workspace plugin (main.lua wires it). Action Footage Library runs the player with no
-- window and AFP_BATCH set to a request file, a JSON
-- { kind, project, status, stop, photo, sdr, only, layout, alone }: kind clips or clips-again (previews of the clips
-- without one; layout: tree, flat or trips), moments (moments of all marks; alone: stabilized on their own), photos
-- (photos of all marks; photo: no, live or motion), export (all moments; sdr: in SDR), render (renders of the moments
-- in _Renders/; profiles: HDR, SDR), share (the renders the Library's Share needs: items, see render_moments). What a run makes goes into Generated/<date> <time> <what>/. The player makes them for that project as the menu used to, writes how far it is to `status`
-- (a JSON written whole: { kind, state, done, total, failed, fraction, current, message, failures }, state starting,
-- looking, running, done, stopped or failed), stops when the file `stop` appears (after the one being made; an export
-- is cancelled) and quits when it's done. `only`: the paths in the project it's kept to (clips, folders, moment files):
-- clip previews then are made for each of those without one, removed ones too
-- The request is read as the plugin loads (request: the parts that make what it asks for keep W.batch, { layout,
-- alone, focus, looking(), finished(summary) }, and W.batch_only), the run starts last (run)
local mp = require 'mp'
local utils = require 'mp.utils'
local workspace = require 'workspace'

local M = {}
-- Set by request (main.lua's W)
local W, state, fs, ws, catalog, pf
local function migrate(...) return W.migrate(...) end
local function make_clips(...) return W.make_clips(...) end
local function finish_drafts(...) return W.finish_drafts(...) end
local function start_marks_job(...) return W.start_marks_job(...) end
local function export_moments(...) return W.export_moments(...) end
local function render_moments(...) return W.render_moments(...) end
local function set_clips_draft(...) return W.set_clips_draft(...) end

-- The request (nil: not a batch), its project, how far it is
local req, root, status, batch
local started, stop_sent, ended = false, false, false

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

function M.request(w)
    W = w
    state, fs, ws, catalog = W.state, W.fs, W.ws, W.catalog
    local req_file = os.getenv("AFP_BATCH")
    if not (req_file and req_file ~= "") then return end
    local f = io.open(req_file, "r")
    local r = f and utils.parse_json(f:read("*a") or "")
    if f then f:close() end
    if type(r) ~= "table" or type(r.project) ~= "string" or type(r.status) ~= "string" then
        mp.msg.error("AFP_BATCH: not a request: " .. req_file)
        mp.add_timeout(0.1, function() mp.commandv("quit", "2") end)
        return
    end
    req = r
    root = ws.normalize(req.project)
    if type(req.only) == "table" and #req.only > 0 then W.batch_only = req.only end
    status = { kind = req.kind, project = root, state = "starting", done = 0, total = 0, failed = 0 }
    -- Chosen for this run in the Library's dialog: how clip previews are laid out (tree, flat, trips), and
    -- whether new moments are stabilized on their own
    -- and the folders the Library shows now, to make first (`focus`: the file it keeps them in)
    batch = { layout = req.layout, alone = type(req.alone) == "boolean" and req.alone or nil,
              focus = type(req.focus) == "string" and req.focus or nil }
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
        -- Done before progress was first looked at (drafts can be): counted from what it says it made
        if status.total == 0 and status.made + status.failed > 0 then
            status.total = status.made + status.failed
            status.done = status.total
        end
        status.failures = sum.failures
        -- A share's: the settings this player would have rendered each with by itself (render_moments)
        status.used = sum.used and #sum.used > 0 and sum.used or nil
        status.message = sum.nothing or sum.error
        status.fraction, status.current = nil, nil
        write_status()
        -- Once what it made is noted in the project's catalog (a quit cuts the notes under way off)
        catalog.after_notes(function() mp.add_timeout(0.2, function() mp.commandv("quit") end) end)
    end
    W.batch = batch
end

local function progress()
    if not started or ended then return end
    local clips_job, marks_job, export_job = W.clips_job(), W.marks_job(), W.export_job()
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
        set_clips_draft(req.draft == true)
        make_clips(kind == "clips-again", root)
    elseif kind == "clips-finish" then
        finish_drafts(root)
    elseif kind == "moments" then
        start_marks_job("moments", root)
    elseif kind == "photos" then
        if req.photo == "no" or req.photo == "live" or req.photo == "motion" then
            mp.set_property_native("user-data/gyroflow/live_photo", req.photo)
        end
        start_marks_job("photos", root)
    elseif kind == "export" then
        export_moments(root, req.sdr == true)
    elseif kind == "render" then
        local profiles = type(req.profiles) == "table" and req.profiles or { "HDR" }
        render_moments(root, profiles)
    elseif kind == "share" then
        -- Stills: a Live or Motion Photo is a pair, not what the cache keeps for a photo
        mp.set_property_native("user-data/gyroflow/live_photo", "no")
        render_moments(root, {}, type(req.items) == "table" and req.items or {})
    else
        batch.finished({ error = "no such batch: " .. tostring(kind) })
    end
end

-- The run: its status written, then started once the gyroflow plugin is ready
function M.run()
    if not batch then return end
    pf = W.pf
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

return M
