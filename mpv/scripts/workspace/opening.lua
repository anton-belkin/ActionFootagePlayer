-- opening: what the workspace plugin does as a file opens (main.lua wires it): what's known about it (describe), a
-- moment's, clip preview's, photo link's or export's file played as its original (open_moment), a folder of moments
-- as the playlist, the source's own file read; and the project: initialized, its folder shown, opened in the Library
local mp = require 'mp'
local utils = require 'mp.utils'
local workspace = require 'workspace'
local moments_lib = require 'moments'
local transform = require 'transform'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, windows, macos, moments, sidecar, pf, tracked_cache
local function save_cache(...) return W.save_cache(...) end
local function replace_state(...) return W.replace_state(...) end
local function publish(...) return W.publish(...) end
local function forget_chapters(...) return W.forget_chapters(...) end
local function refresh(...) return W.refresh(...) end
local function copy(...) return W.copy(...) end
local function locations_of(...) return W.locations_of(...) end
local function tree_of(...) return W.tree_of(...) end
local function original_of(...) return W.original_of(...) end
local function own_project(...) return W.own_project(...) end
local function autoplay_on(...) return W.autoplay_on(...) end
local function reveal(...) return W.reveal(...) end
local function need_project(...) return W.need_project(...) end

-- Whether this load is mpv's first of a file opened on its own, which it then loads again from the playlist of the
-- files beside it (autocreate-playlist, the app's setting). Options set now are lost then
local function before_autocreate()
    return mp.get_property("autocreate-playlist", "no") ~= "no" and (mp.get_property_number("playlist-count", 1) or 1) <= 1
       and (mp.get_property("playlist-path") or "") == ""
end

-- A project of before 0.9, its state folder: moved into Project.afproject the first time it's met (once a session)
local migrated = {}
local function migrate(root)
    if not root or migrated[root] then return end
    migrated[root] = true
    if fs.is_dir(pf.legacy_dir(root)) and pf.migrate(root) then mp.msg.info("project moved into " .. pf.path(root)) end
end

-- What's known about the file at `path`, as published
local told_others = {}
-- Where a project's made files are shown from (the menu's Moments folder): _Moments/ (its Generated/ in it), else the
-- Generated/ at the top of a project of before
local function moments_home(root)
    local home = ws.join(root, moments_lib.FOLDER)
    if fs.is_dir(home) then return home end
    local old = ws.join(root, moments_lib.GENERATED)
    return fs.is_dir(old) and old or home
end
-- Whether each run gets a folder of its own in Generated/ (the project's generated_runs: the Library's project
-- settings; off by default)
local function generated_runs(root)
    local doc = root and fs.read_json(pf.path(root))
    return type(doc) == "table" and doc.generated_runs == true
end
local function describe(path)
    if not path or path == "" then return {} end
    if path:find("://", 1, true) then return { path = path } end
    local clip = ws.absolute(path, mp.get_property("working-directory"))
    local root = ws.project_root(clip)
    migrate(root)
    local locs = locations_of(root)
    local tree = root and ws.tree_path(clip, locs) or nil
    -- Its own file and its Gyroflow project are the clip's wherever it is: looked for beside it in every location
    local own, others = ws.own_file(clip, root and locs or nil)
    local gyroflow_projects = {}
    if tree then
        for _, c in ipairs(ws.files_at(tree, locs)) do
            for _, g in ipairs(ws.gyroflow_projects(c.file)) do gyroflow_projects[#gyroflow_projects + 1] = g end
        end
    end
    if #gyroflow_projects == 0 then gyroflow_projects = ws.gyroflow_projects(clip) end
    -- A new one goes beside its own file's clip (the archive's, not a working copy)
    local gyroflow_new = own and own:gsub("%.afp%.json$", ".gyroflow") or ws.gyroflow_new(clip)
    return {
        path = path,
        clip = clip,
        project = root,
        relative = tree,
        gyroflow_projects = gyroflow_projects,
        gyroflow_new = gyroflow_new,
        sidecar_file = own,
        own_others = others and #others > 0 and others or nil,
        moments_folder = root and moments_home(root) or nil,
        project_choices = not root and ws.project_choices(clip) or nil,
        project_new = not root and ws.new_project_root(clip) or nil,
    }
end

-- A moment's file: its original instead, from its in, looping its part (one after another in a playlist)
local function open_moment(path)
    -- A photo's link file (<name>.afplink.jpg: a small picture with the link in its XMP, made by the Library): its
    -- original's part around the photo's frame instead, as a video of its own (as a moment), paused at the frame: the
    -- live-photo plugin shows it as a live photo, the part its live part (1.5 s on each side, or the run of the links
    -- after it: its user-data/afp/photo span). A rendered photo (a HEIC, a JPEG without that name) opens as the
    -- picture it is
    if state.clip:lower():match("%.afplink%.jpg$") then
        local praw = fs.read_photo_link(state.clip)
        local p = praw and utils.parse_json(praw)
        if type(p) ~= "table" or p.v ~= 1 or p.kind ~= "photo" or type(p.t) ~= "number" then return end
        if before_autocreate() then return end
        local file = state.clip
        local original = original_of(p, moments.project_of(file), file)
        if not original then
            mp.osd_message(moments_lib.display_name(file) .. ": the clip isn't at " .. tostring(p.clip), 6)
            return
        end
        local lp = mp.get_property_native("user-data/afp/photo")
        local span = type(lp) == "table" and lp.file == file and type(lp.span) == "table" and lp.span or { math.max(0, p.t - 1.5), p.t + 1.5 }
        mp.set_property("stream-open-filename", string.format("edl://!no_chapters;%%%d%%%s,start=%.3f,length=%.3f", #original, original, span[1], span[2] - span[1]))
        mp.set_property("file-local-options/start", string.format("%.3f", p.t - span[1]))
        mp.set_property("file-local-options/pause", "yes")
        mp.set_property("file-local-options/force-media-title", (moments_lib.display_name(file):gsub("%.afplink$", "")))
        replace_state(describe(original))
        state.path = path
        state.redirected = true
        state.offset, state.length = span[1], span[2] - span[1]
        state.photo = { file = file, t = p.t, crop = p.crop, camera = p.camera, name = (moments_lib.display_name(file):gsub("%.afplink$", "")),
                        ["in"] = span[1], out = span[2], clip = original }
        state.created = p.created
        return
    end
    local raw = fs.read_link(state.clip)
    if not raw then return end
    local l = utils.parse_json(raw)
    if type(l) == "table" and l.v == 1 and l.kind == "export" then
        -- It plays as it is (stabilized, in its look already); its original is a menu item
        local original = original_of(l, moments.project_of(state.clip), state.clip)
        state.export = { name = moments_lib.display_name(state.clip), original = original,
                         ["in"] = l["in"], out = l.out }
        return
    end
    if type(l) == "table" and l.v == 1 and l.kind == "clip" then
        -- A clip preview: its original, all of it
        if before_autocreate() then return end
        local original = original_of(l, moments.project_of(state.clip), state.clip)
        if not original then
            mp.osd_message(moments_lib.display_name(state.clip) .. ": the clip isn't at " .. tostring(l.clip), 6)
            return
        end
        local file = state.clip
        mp.set_property("stream-open-filename", original)
        replace_state(describe(original))
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
            state.view = transform.complete(l.stab, transform.session(mp.get_property_native))
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
    local original = original_of(l, project, file)
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
    mp.set_property("file-local-options/keep-open", autoplay_on() and "yes" or "always")
    mp.set_property("file-local-options/force-media-title", name)
    replace_state(describe(original))
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
        state.view = transform.complete(l.stab, transform.session(mp.get_property_native))
        state.saved = copy(state.view)
        state.view_target = "moment"
        state.gyroflow_projects, state.gyroflow_new = { own }, own
        state.own_project = fs.is_file(own) and own or nil
        if state.alone and not state.own_project then state.trim = { ["in"] = a, out = b } end
    end
    -- An EDL has no metadata of its own: the original's creation time from the link
    state.created = l.created
end

-- A folder of the project's own (not among its footage) with moments in it: its moments, in the order they
-- happened, as the playlist
local function open_folder(dir)
    local project = moments.project_of(ws.join(dir, "x"))
    if not project then return end
    local rel = ws.relative(dir, project)
    local top = rel and rel:match("^([^/]+)")
    if not (top and moments_lib.OWN_FOLDERS[top:lower()]) then
        for _, l in ipairs(locations_of(project) or {}) do
            if l.dir and (l.dir == dir or ws.relative(dir, l.dir)) then return end
        end
    end
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

function M.on_load()
    local path = mp.get_property("path")
    replace_state(describe(path))
    forget_chapters()
    if state.clip then
        local info = utils.file_info(state.clip)
        if info and info.is_dir then
            open_folder(state.clip)
        elseif info and (state.clip:lower():match("%.mp4$") or state.clip:lower():match("%.mov$")) then
            open_moment(path)
        elseif info and state.clip:lower():match("%.afplink%.jpg$") then
            open_moment(path) -- a photo's link
        elseif info and (state.clip:lower():match("%.heic$") or state.clip:lower():match("%.jpe?g$")) then
            -- A photo the player made (gyroflow-export, afp-photo: its link in its XMP): its clip, for Open the whole
            -- clip (found quietly: a photo copied out of its project just has none)
            local praw = fs.read_photo_link(state.clip)
            local p = praw and utils.parse_json(praw)
            if type(p) == "table" and p.kind == "photo" and type(p.t) == "number" then
                local original = moments.original(p, moments.project_of(state.clip), { hint_only = true })
                if original then
                    state.photo = { file = state.clip, t = p.t, name = moments_lib.display_name(state.clip), clip = original,
                                    ["in"] = math.max(0, p.t - 1.5), out = p.t + 1.5, made = true }
                end
            end
        end
    end
    -- Other own files of this clip (beside its copies in other locations): the main one plays and is saved, they're
    -- read only. Said once a session per clip
    if state.own_others and not told_others[state.clip] then
        told_others[state.clip] = true
        mp.msg.warn("other own files of this clip, not written: " .. table.concat(state.own_others, ", ") .. "; saved to " .. tostring(state.sidecar_file))
        mp.osd_message("This clip has another file of its own:\n" .. table.concat(state.own_others, "\n") .. "\nIt's not used: saved to\n" .. tostring(state.sidecar_file), 6)
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
                state.view = transform.complete(raw, transform.session(mp.get_property_native))
                state.saved = copy(state.view)
            end
        end
    end
    state.view_from = state.view and state.view_target or "session"
    -- An export's original, at its part
    local wanted = W.wanted
    if wanted and state.clip == wanted.file then
        -- (A photo's clip: at its frame, paused, its live part marked)
        if wanted.at or wanted["in"] then mp.set_property("file-local-options/start", string.format("%.3f", wanted.at or wanted["in"])) end
        if wanted.at then mp.set_property("file-local-options/pause", "yes") end
        if wanted["in"] and wanted.out then
            -- The part marked, not looped: the whole clip plays on, before and after it
            mp.set_property("file-local-options/ab-loop-count", "0")
            mp.set_property("file-local-options/ab-loop-a", string.format("%.3f", wanted["in"]))
            mp.set_property("file-local-options/ab-loop-b", string.format("%.3f", wanted.out))
        end
        -- A moment opened in its clip: k updates it
        if wanted.moment then state.editing = wanted.moment end
        if not before_autocreate() then W.wanted = nil end
    end
    publish()
end

-- The side panel's Initialize the project here (and the menu's): in one of the folders above the clip
function M.init_project(dir)
    if not state.clip or state.project or not dir then return end
    local allowed = false
    for _, d in ipairs(ws.project_choices(state.clip)) do if d == dir then allowed = true end end
    if not allowed then return end
    -- Its file, Project.afproject, makes it one
    if not pf.update(dir, function() end) then
        return mp.osd_message("Can't write in " .. dir, 4)
    end
    state.project = dir
    state.relative = tree_of(state.clip, dir)
    state.moments_folder = moments_home(dir)
    state.project_choices, state.project_new = nil, nil
    mp.osd_message("Project initialized: " .. ws.name(dir), 3)
    refresh()
end

function M.project_folder()
    if state.project then reveal(state.project, true) end
end

-- The project opened in Action Footage Library, where its batches are made (clip previews, moments and photos of
-- all marks, exports of all moments): its file, Project.afproject, opened as the system opens it
function M.open_library()
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
end

function M.install(w)
    W = w
    state, fs, ws, windows, macos = W.state, W.fs, W.ws, W.windows, W.macos
    moments, sidecar, pf, tracked_cache = W.moments, W.sidecar, W.pf, W.tracked_cache
    W.migrate, W.generated_runs, W.stamp = migrate, generated_runs, stamp
end

return M
