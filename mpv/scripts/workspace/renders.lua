-- renders: the project's moments exported all at once, and the moments' renders the Library's Render and Share
-- ask for (_Renders/, a recipe each: library/src/renders.rs works it out the same way). main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local link = require 'link'
local moments_lib = require 'moments'
local transform = require 'transform'

local M = {}
-- Set by install (main.lua's W)
local W, fs, ws, windows, macos, moments, sidecar, catalog, tracked_cache, batch, markers
local function own_project(...) return W.own_project(...) end
local function fingerprint(...) return W.fingerprint(...) end
local function save_cache(...) return W.save_cache(...) end
local function mkdir(...) return W.mkdir(...) end
local function plugin_tool(...) return W.plugin_tool(...) end
local function in_only(...) return W.in_only(...) end
local function original_of(...) return W.original_of(...) end
local function made_view_of(...) return W.made_view_of(...) end
local function copy(...) return W.copy(...) end
local function native(...) return W.native(...) end
local function generated_runs(...) return W.generated_runs(...) end

-- Export all moments (Action Footage Library): each moment of the project not exported yet, one at a time, as Shift+E
-- exports one it plays: its part, its own transform (and its own Gyroflow project), the source's input, into
-- _Exports/<name>.mov (" (SDR)" in SDR), the link inside. A moment exported in that look already (an export of the
-- same part of the same clip) and the export_<name> moments exports leave are skipped
local export_job = nil

-- Renders (docs/design/library-management.md, "Renders"): a moment rendered once, _Renders/<its path in _Moments>.<profile>.mov
-- (HDR: as Shift+E exports HDR; SDR: as it exports SDR), its link (kind render) carrying its recipe: everything that
-- decides its pixels. Current while the recipe is the moment's now; an export copies a current one instead of
-- rendering again. Deleting renders breaks nothing
local RENDERS = "_Renders"
-- 2: the clip's lens and its measured motion (#318; the Library's renders.rs VERSION 5)
local RENDER_VERSION = 2
local RENDER_PROFILES = { HDR = false, SDR = true }

-- A value as JSON keeps it: nils and empty tables left out, for comparing recipes
local function trimmed(v)
    if type(v) ~= "table" then return v end
    local out, n = {}, 0
    for k, x in pairs(v) do
        local t = trimmed(x)
        if t ~= nil then out[k] = t; n = n + 1 end
    end
    if n == 0 then return nil end
    return out
end
local function same_value(a, b)
    if type(a) ~= type(b) then return false end
    if type(a) == "number" then return math.abs(a - b) < 1e-9 end
    if type(a) ~= "table" then return a == b end
    for k, x in pairs(a) do if not same_value(x, b[k]) then return false end end
    for k in pairs(b) do if a[k] == nil then return false end end
    return true
end
-- What a render takes of the clip's lens (gyroflow-probe --lens-id: {from, print}, what the Library's index::lens_id
-- reads of the probe's lens_info), once per clip in a run (`render_moments`, `export_moments` start it again); false
-- where the probe can't say
local lens_ids = {}
local function lens_id(original)
    if lens_ids[original] == nil then
        local exe = plugin_tool("gyroflow-probe")
        local r = exe and mp.command_native({ name = "subprocess", playback_only = false, capture_stdout = true, args = { exe, "--lens-id", original } })
        local line = r and r.status == 0 and type(r.stdout) == "string" and r.stdout:match("([^\r\n]+)%s*$")
        local v = line and utils.parse_json(line)
        lens_ids[original] = type(v) == "table" and type(v.from) == "string" and { from = v.from, print = v.print or "" } or false
    end
    return lens_ids[original] or nil
end
-- The motion measured from the picture beside the clip (<stem>.afp-motion), by its fingerprint
local function motion_print(original)
    local motion = (original:gsub("%.[^./\\]+$", "")) .. ".afp-motion"
    return fs.is_file(motion) and fingerprint(motion) or nil
end
-- What a moment's render in `profile` is made from: its part of its original, its own transform and Gyroflow project,
-- the source's input, the clip's lens and measured motion, the profile, the renderer's version
local function render_recipe(m, original, profile)
    local l = m.link
    local doc = original and sidecar.read(original)
    local own = own_project(m.file)
    return trimmed({ v = RENDER_VERSION, clip = { hash = l.hash, size = l.size }, ["in"] = l["in"], out = l.out,
                     stab = l.stab, alone = l.alone, input = type(doc) == "table" and doc.source or nil,
                     gyroflow = fs.is_file(own) and fingerprint(own) or nil, lens = original and lens_id(original) or nil,
                     motion = original and motion_print(original) or nil, profile = profile })
end
-- A moment's path for its renders: in _Moments its path there (as before 0.11), else its path in the project's folder
-- (Library's index.rs renders_of does the same)
local function render_rel(root, file)
    return ws.relative(file, ws.join(root, moments_lib.FOLDER)) or ws.relative(file, root) or ws.name(file)
end
local function render_file(root, m, profile)
    local rel = render_rel(root, m.file)
    local base = rel:lower():match("%.afplink%.mp4$") and rel:sub(1, -13) or (rel:gsub("%.[^./\\]*$", ""))
    if windows then base = base:gsub("/", "\\") end
    return ws.join(ws.join(root, RENDERS), base .. "." .. profile .. ".mov")
end
local function render_current(file, recipe)
    if not fs.is_file(file) then return false end
    local raw = fs.read_link(file)
    local l = raw and utils.parse_json(raw)
    return type(l) == "table" and l.kind == "render" and same_value(trimmed(l.recipe), recipe)
end
-- A file copied (a clone on APFS, so a render copied as an export takes no room until one changes)
local function copy_file(from, to)
    local args = macos and { "/bin/cp", "-c", from, to } or windows and { "cmd", "/d", "/c", "copy", "/y", from, to } or { "cp", "--reflink=auto", from, to }
    local r = mp.command_native({ name = "subprocess", args = args, playback_only = false, capture_stderr = true })
    if not (r and r.status == 0) and macos then
        r = mp.command_native({ name = "subprocess", args = { "/bin/cp", from, to }, playback_only = false, capture_stderr = true })
    end
    return r and r.status == 0
end

-- The exports there are, wherever they are in the project's folder (found by the link inside)
local function exported_parts(root)
    local set = {}
    for _, e in ipairs(moments.scan(root, tracked_cache, nil, "export").list) do
        local l = e.link
        if type(l.clip) == "string" and l["in"] and l.out then
            local sdr = l.look == "maker" or l.look == "nbcu"
            set[l.clip .. "|" .. string.format("%.3f|%.3f", l["in"], l.out) .. "|" .. tostring(sdr)] = true
        end
    end
    save_cache()
    return set
end

local function export_next()
    local j = export_job
    if not j then return end
    if j.stopping or j.done >= #j.queue then
        export_job = nil
        if batch then batch.finished({ made = j.made, failed = j.failed, failures = j.failures, stopped = j.stopping, used = j.used }) end
        return
    end
    local m = j.queue[j.done + 1]
    j.id = j.id + 1
    j.fraction = 0
    local sdr = j.sdr
    if m.sdr ~= nil then sdr = m.sdr end
    local output, l
    if m.render_out then
        -- A render: its fixed file, made again over the stale one
        output, l = m.render_out, m.render_link
        mkdir(ws.parent(output))
        os.remove(output)
    else
        local label = sdr and " (SDR)" or ""
        local final
        output, final = moments.unique_file(j.dir, moments_lib.safe_name(m.name) or "Moment", label .. ".mov")
        l = { v = 1, kind = "export", clip = m.link.clip, size = m.link.size, created = m.link.created,
              ["in"] = m.link["in"], out = m.link.out, name = final, look = sdr and "nbcu" or "hdr",
              stabilized = m.rendered.view ~= "off", roots = { j.root }, hash = m.link.hash }
        -- A current render of it in this look: copied, its link made the export's
        if m.copy_from then
            local tool = plugin_tool("afp-cut")
            local ok = copy_file(m.copy_from, output)
            if ok and tool then
                local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true,
                                              args = { tool, "--set-key", output, link.KEY, utils.format_json(l) } })
                ok = r and r.status == 0
            end
            if ok then j.made = j.made + 1; catalog.note(j.root, { output }) else
                j.failed = j.failed + 1
                j.failures[#j.failures + 1] = { m.name, "couldn't copy its render" }
            end
            j.done = j.done + 1
            return mp.add_timeout(0, export_next)
        end
    end
    m.output = output
    -- A photo's render (a photo link shared): the frame as its clip's saved view has it, into a folder of its own
    -- first (gyroflow-export names a photo itself), then into its place in the cache (export-batch-done)
    if m.photo then
        m.making = output .. ".making"
        mkdir(m.making)
        return mp.commandv("script-message-to", "gyroflow", "photo-at", utils.format_json({
            id = tostring(j.id), clip = m.original, t = m.photo.t, view = m.rendered, crop = m.photo.crop, camera = m.photo.camera,
            link = utils.format_json(l), still = true, out_dir = m.making, export = m.export, reply = mp.get_script_name(), done = "export-batch-done" }))
    end
    mp.commandv("script-message-to", "gyroflow", "export-batch", utils.format_json({
        id = tostring(j.id), clip = m.original, view = m.rendered, project = m.project, alone = m.alone,
        from = m.link["in"], to = m.link.out, output = output, link = utils.format_json(l), sdr = sdr, export = m.export,
        progress = j.progress, reply = mp.get_script_name(), done = "export-batch-done",
    }))
end

function M.export_batch_done(id, json)
    local j = export_job
    if not j or tostring(j.id) ~= id then return end
    local r = utils.parse_json(json or "") or {}
    -- A photo's render: from the folder it was made in to its file in the cache, <hash>.<profile>.<heic or jpg>
    local photo = j.queue[j.done + 1]
    if r.ok and photo and photo.photo and type(r.out) == "string" then
        local final = photo.render_out .. "." .. (r.out:lower():match("%.heic$") and "heic" or "jpg")
        os.remove(final)
        if os.rename(r.out, final) then
            catalog.note(j.root, { r.out, final })
        else
            r = { ok = false, error = "couldn't put the photo into the cache" }
        end
        os.remove(photo.making)
    end
    if r.ok then j.made = j.made + 1 end
    -- A render made again failed: the stale one it replaced is gone (what's made is noted by the gyroflow plugin)
    local m = j.queue[j.done + 1]
    if not r.ok and m and m.render_out then catalog.note(j.root, { m.render_out }) end
    if not r.ok and not j.stopping then
        j.failed = j.failed + 1
        j.failures[#j.failures + 1] = { j.queue[j.done + 1].name, tostring(r.error or "failed") }
    end
    j.done = j.done + 1
    export_next()
end

local function export_moments(root, sdr)
    if export_job then return end
    lens_ids = {}
    if batch then batch.looking() end
    local done = exported_parts(root)
    local queue, failures = {}, {}
    for _, m in ipairs(moments.scan(root, nil).list) do
        local l = m.link
        if not m.name:match("^export_") and type(l["in"]) == "number" and type(l.out) == "number" and l.out > l["in"]
           and in_only(ws.relative(m.file, root) or "")
           and not done[tostring(l.clip) .. "|" .. string.format("%.3f|%.3f", l["in"], l.out) .. "|" .. tostring(sdr)] then
            local original = original_of(l, root, m.file)
            if original then
                local movie = markers and markers.movie and markers.movie(original) or nil
                local _, rendered = made_view_of(original, movie, l.stab)
                local own = own_project(m.file)
                m.original, m.rendered = original, rendered
                m.project = fs.is_file(own) and own or nil
                -- A moment kept as made (snap): stabilized on its own or as part of its clip, as Shift+E has it
                if l.snap then m.alone = l.alone == true else m.alone = nil end
                -- Rendered already in this look, and current: copied instead
                local profile = sdr and "SDR" or "HDR"
                local rf = render_file(root, m, profile)
                if render_current(rf, render_recipe(m, original, profile)) then m.copy_from = rf end
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
    -- This run's folder (docs/design/library-management.md §8)
    local dir = ws.join(root, native(moments_lib.generated_rel("Exports", os.time(), false, generated_runs(root))))
    mkdir(dir)
    local tmp = os.getenv(windows and "TEMP" or "TMPDIR") or "/tmp"
    export_job = { root = root, dir = dir, sdr = sdr, queue = queue, done = 0, made = 0, failed = #failures, failures = failures, id = 0,
                   progress = utils.join_path(tmp, "afp-batch-export-" .. tostring(utils.getpid()) .. ".json") }
    -- Those whose original is missing count as failed, after the rest
    export_job.total_extra = #failures
    export_next()
end

-- Renders of the moments (Action Footage Library's Render, kept to its selection): each moment in each profile asked
-- for whose render isn't current, made (again) in _Renders/. The ones current are left as they are
-- `items` (Action Footage Library's Share, its batch `share`): the renders the Library's cache lacks, each
-- { of (moment, photo or clip), name, file (a moment's link file), clip (the original), t / crop / camera (a photo),
-- sdr, out (its file in the cache, _Renders/<hash>.<profile>.mov; a photo's without its extension), link (what the
-- render carries: its recipe and hash, worked out by the Library, renders.rs) }. Made as they are asked for: what's
-- current is the Library's to know
local function render_moments(root, profiles, items)
    if export_job then return end
    lens_ids = {}
    if batch then batch.looking() end
    local queue, failures, share_used = {}, {}, {}
    for _, it in ipairs(items or {}) do
        local name = tostring(it.name or it.file or it.clip or "")
        local l = it.of == "moment" and it.file and utils.parse_json(fs.read_link(it.file) or "") or {}
        local original = it.of == "moment" and type(l) == "table" and original_of(l, root, it.file) or it.clip
        if type(l) ~= "table" or type(it.out) ~= "string" or type(it.link) ~= "table" then
            failures[#failures + 1] = { name, "not a render the player knows how to make" }
        elseif not original or not fs.is_file(original) then
            failures[#failures + 1] = { name, "its original isn't found" }
        else
            local movie = markers and markers.movie and markers.movie(original) or nil
            local _, rendered = made_view_of(original, movie, it.of == "moment" and l.stab or nil)
            local own = it.of == "moment" and own_project(it.file) or nil
            local m = {
                name = name, link = l, original = original, rendered = rendered,
                project = own and fs.is_file(own) and own or nil, alone = l.snap and (l.alone == true) or nil,
                sdr = it.sdr == true, render_out = it.out, render_link = it.link,
                photo = it.of == "photo" and { t = it.t, crop = it.crop, camera = it.camera } or nil,
                -- The Library's export settings (docs/design/export-settings.md): size, encoder, quality preset
                export = type(it.export) == "table" and it.export or nil,
            }
            -- The settings the Library's recipe has (renders.rs `settings`): rendered with them, so the render is what
            -- its hash says; what this player would have taken by itself is told back, for the Library to compare
            if type(it.view) == "table" then
                local mine = {}
                for k, v in pairs(rendered) do if k ~= "frame_aspect" then mine[k] = v end end
                mine.edges = mp.get_property_native("user-data/gyroflow/edges")
                mine.edges_blur = mp.get_property_native("user-data/gyroflow/edges_blur")
                mine.temporal_denoise = mp.get_property_native("user-data/gyroflow/temporal_denoise")
                share_used[#share_used + 1] = { out = it.out, player = mine }
                local w, h = movie and movie.width, movie and movie.height
                if w and h and (((movie.turn or 0) + (tonumber(it.view.rotation) or 0)) % 180) == 90 then w, h = h, w end
                m.rendered = copy(it.view)
                m.rendered.frame_aspect = transform.concrete(it.view.aspect, it.view.orientation, w, h)
                if it.alone ~= nil then m.alone = it.alone == true end
            end
            queue[#queue + 1] = m
        end
    end
    for _, m in ipairs(items and {} or moments.scan(root, nil).list) do
        local l = m.link
        if type(l["in"]) == "number" and type(l.out) == "number" and l.out > l["in"] and in_only(ws.relative(m.file, root) or "") then
            local original = original_of(l, root, m.file)
            if not original then
                failures[#failures + 1] = { m.name, "its original isn't found" }
            else
                local movie = markers and markers.movie and markers.movie(original) or nil
                local _, rendered = made_view_of(original, movie, l.stab)
                local own = own_project(m.file)
                for _, p in ipairs(profiles) do
                    if RENDER_PROFILES[p] ~= nil then
                        local recipe = render_recipe(m, original, p)
                        local out = render_file(root, m, p)
                        if not render_current(out, recipe) then
                            local rel = render_rel(root, m.file)
                            queue[#queue + 1] = {
                                name = m.name .. " (" .. p .. ")", link = l, original = original, rendered = rendered,
                                project = fs.is_file(own) and own or nil, alone = l.snap and (l.alone == true) or nil,
                                sdr = RENDER_PROFILES[p], render_out = out,
                                render_link = { v = 1, kind = "render", clip = l.clip, size = l.size, created = l.created, hash = l.hash,
                                                ["in"] = l["in"], out = l.out, name = m.name, moment = rel, profile = p, recipe = recipe,
                                                look = RENDER_PROFILES[p] and "nbcu" or "hdr", roots = { root } },
                            }
                        end
                    end
                end
            end
        end
    end
    if #queue == 0 then
        if batch then
            return batch.finished({ made = 0, failed = #failures, failures = failures, used = share_used,
                                    nothing = #failures == 0 and "Every render asked for is current" or nil })
        end
        return
    end
    mkdir(ws.join(root, RENDERS))
    local tmp = os.getenv(windows and "TEMP" or "TMPDIR") or "/tmp"
    export_job = { root = root, sdr = false, queue = queue, done = 0, made = 0, failed = #failures, failures = failures, id = 0, used = share_used,
                   progress = utils.join_path(tmp, "afp-batch-render-" .. tostring(utils.getpid()) .. ".json") }
    export_next()
end

function M.install(w)
    W = w
    fs, ws, windows, macos, moments, sidecar, catalog = W.fs, W.ws, W.windows, W.macos, W.moments, W.sidecar, W.catalog
    tracked_cache, batch, markers = W.tracked_cache, W.batch, W.markers
    W.export_moments, W.render_moments = export_moments, render_moments
    W.export_job = function() return export_job end
end

return M
