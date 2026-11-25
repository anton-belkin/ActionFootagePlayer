-- momentsave: what the workspace plugin saves of what plays (main.lua wires it): a moment (k; the A-B part, its
-- preview made by the gyroflow plugin), the menu's commands on the clip's moments, an export's name (Shift+E asks),
-- an export's or a moment's original opened at its part, a photo link (Alt+s)
local mp = require 'mp'
local utils = require 'mp.utils'
local moments_lib = require 'moments'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, moments, catalog, cache
local function publish(...) return W.publish(...) end
local function need_project(...) return W.need_project(...) end
local function ask(...) return W.ask(...) end
local function clock(...) return W.clock(...) end
local function suggestion(...) return W.suggestion(...) end
local function history_file(...) return W.history_file(...) end
local function moment_names(...) return W.moment_names(...) end
local function remember_name(...) return W.remember_name(...) end
local function set_last_typed(...) return W.set_last_typed(...) end
local function refresh(...) return W.refresh(...) end
local function own_project(...) return W.own_project(...) end
local function project_tool(...) return W.project_tool(...) end
local function run_tool(...) return W.run_tool(...) end
local function current_view(...) return W.current_view(...) end
local function mkdir(...) return W.mkdir(...) end
local function native(...) return W.native(...) end
local function generated_runs(...) return W.generated_runs(...) end
local function tree_of(...) return W.tree_of(...) end
local function reveal(...) return W.reveal(...) end
local function trash(...) return W.trash(...) end
local function trash_name(...) return W.trash_name(...) end
local function plugin_tool(...) return W.plugin_tool(...) end

-- Moments being saved, by request id
local pending = {}
local next_id = 0

-- The A-B part saved as a moment named `name`, or `update` (a moment of this clip) written again with it. `quiet`:
-- nothing on screen unless it fails (the moment that comes with an export)
-- opts: range ({ a, b }, the clip's time: in place of the part marked), alone, stab (in place of the moment's own),
-- picture (k's: the preview from the frame shown as it was pressed, export.lua's moment-picture)
local function save(name, update, typed, quiet, opts)
    opts = opts or {}
    local PREVIEW_WAIT = 120
    local a, b
    if opts.range then
        a, b = opts.range[1], opts.range[2]
    else
        a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
        if not (a and b and b > a) then
            -- The part was unmarked while the name was typed
            if not quiet then mp.osd_message("Not saved: the part isn't marked any more (l at its start, l at its end, then k)", 6) end
            return
        end
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
        -- Into Generated/<date> Moments/, a folder a day (docs/design/library-management.md §8)
        local dir = ws.join(root, native(moments_lib.generated_rel("Moments", os.time(), true, generated_runs(root))))
        mkdir(dir)
        file, final = moments.unique_file(dir, safe)
    end
    local info = utils.file_info(state.clip)
    local l = {
        v = 1, kind = "moment", clip = state.relative or tree_of(state.clip, root), size = info and info.size,
        -- The original's fingerprint, read as it opened: gyroflow-export doesn't read it again (seconds on the NAS while
        -- it plays)
        hash = state.hash,
        created = state.created, ["in"] = moments_lib.ms(a), out = moments_lib.ms(b), name = final, roots = { root },
    }
    -- An update keeps the moment's tags, pick, stars and comment (the Library's) from its file's earlier link
    if update then
        local old = utils.parse_json(fs.read_link(update.file) or "")
        -- (not what the stores agreed on: the new file has no Finder tags yet, the next sync puts them on)
        moments_lib.carry(old, l)
    end
    -- How it's shown, fixed as it's made (snap): the transform bundle it plays with, whole (stab), its part on its
    -- own or as part of the clip (alone), and a project of its own when the clip plays with one: a copy, so the
    -- clip's project changed later doesn't change it. An update keeps them; a moment without settings of its own
    -- keeps playing as its source does
    local preview_project = nil
    if not update or update.snap or opts.own then
        local alone = opts.alone
        if alone == nil then alone = update and update.alone or (not update and cache.moments_alone == true) or false end
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
    -- A card while it's made (the preview's frame takes seconds, from a NAS half a minute): it stays until the answer
    if not quiet then
        mp.notify({ id = "moment:" .. id, kind = "working", title = (update and "Updating “" or "Saving moment “") .. final .. "”",
                    detail = "Making its picture" })
    end
    -- No answer in two minutes (the preview takes seconds): said, and k works again
    mp.add_timeout(PREVIEW_WAIT, function()
        if not pending[id] then return end
        pending[id] = nil
        state.saving = nil
        publish()
        mp.msg.error("moment " .. final .. ": no answer from the gyroflow plugin in " .. PREVIEW_WAIT .. " s")
        mp.notify({ id = "moment:" .. id, kind = "error", title = "Couldn't save “" .. final .. "”",
                    detail = "No answer from the gyroflow plugin (see the player's log)" })
    end)
    mp.commandv("script-message-to", "gyroflow", "preview", utils.format_json({
        id = id, clip = state.clip, from = a, to = b, at = at, link = utils.format_json(l), dates = dates, output = file,
        force = update ~= nil, reply = mp.get_script_name(),
        -- Its own project, or none (the preset: false); nil: the clip's, as it plays (a moment from before)
        project = preview_project, alone = l.alone == true and preview_project == false,
        -- k's: the frame on screen as it was pressed, when the gyroflow plugin took it
        picture = opts.picture,
    }))
end

function M.preview_done(id, json)
    local p = pending[id]
    pending[id] = nil
    if not p then return end
    state.saving = nil
    local r = utils.parse_json(json or "") or {}
    local card = "moment:" .. id
    if not r.ok then
        publish()
        mp.notify({ id = card, kind = "error", title = "Couldn't save “" .. p.name .. "”", detail = tostring(r.error or "gyroflow-export failed") })
        return
    end
    -- In the list at once (the catalog lists it a little later): kept until a look at the catalog has it
    if not p.update and state.clip then
        state.fresh = state.fresh or {}
        state.fresh[p.file] = { name = p.name, file = p.file, ["in"] = p.a, out = p.b, snap = p.stab ~= nil, alone = p.alone == true,
                                stab = p.stab, clip = state.clip, at = mp.get_time() }
        local list = state.moments or {}
        local there = false
        for _, m in ipairs(list) do if m.file == p.file then there = true end end
        if not there then
            list[#list + 1] = state.fresh[p.file]
            table.sort(list, function(x, y) return (x["in"] or 0) < (y["in"] or 0) end)
            state.moments = list
        end
        publish()
    end
    local done = { id = card, kind = "done", title = (p.update and "Updated “" or "Saved moment “") .. p.name .. "”",
                   detail = p.update and nil or "In Moments of this clip" }
    -- The name typed: kept for the next suggestion and ↑
    if p.typed and not p.update then remember_name(p.typed) end
    -- Updated: a new file over the old one, Finder's tags put back on it from its link
    if p.update and fs.tags_refollow then fs.tags_refollow(p.file) end
    local edited = state.moment or state.editing
    if p.update and edited and edited.file == p.file then
        edited["in"], edited.out = p.a, p.b
        if p.alone ~= nil then edited.alone = p.alone end
        if p.stab then edited.stab = p.stab end
        -- Saved: it's now as it's viewed
        -- A moment playing as a video of its own: again, with its new ends (not for its settings remembered)
        if state.moment and state.offset and not p.keep_playing then
            mp.notify(done)
            return mp.commandv("playlist-play-index", "current")
        end
    end
    if not p.quiet then mp.notify(done) end
    -- Its own Gyroflow project too (written or edited beside it): the Library's badge and filters read it
    local noted = { p.file }
    if fs.is_file(own_project(p.file)) then noted[2] = own_project(p.file) end
    catalog.note(state.project or moments.project_of(p.file), noted)
    refresh()
end

-- Another plugin wrote or removed a file of the project's (the highlights plugin's marks): noted in the catalog
function M.catalog_note(file)
    if type(file) == "string" and file ~= "" then catalog.note(state.project or moments.project_of(file), { file }) end
end

-- k: the A-B part as a moment; on a moment playing whose part was moved, update it or save a new one
function M.save_key(given)
    -- A photo shown (the live-photo plugin): no moment of a still
    if type(mp.get_property_native("user-data/afp/photo")) == "table" then return end
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
    local off = state.offset or 0
    -- Its picture: the frame on screen now, as it's decided, taken by the gyroflow plugin where that's the moment as it
    -- plays (else its preview is rendered: export.lua's moment-picture). A second to save, not up to 30 s
    mp.commandv("script-message-to", "gyroflow", "moment-picture", utils.format_json({ a = a + off, b = b + off }))
    local pic = { picture = true }
    if given and given ~= "" then return save(given, nil, given, nil, pic) end
    if m then
        if math.abs(a + off - m["in"]) < 0.001 and math.abs(b + off - m.out) < 0.001 then
            return mp.osd_message(m.name .. " is saved as it is: move its start or end with l, then k", 4)
        end
        return ask({ prompt = "Update " .. m.name .. " to " .. clock(a + off) .. "–" .. clock(b + off) .. "? Enter, or a new name for a new one: ",
                     id = "workspace-update", title = "Update “" .. m.name .. "” to " .. clock(a + off) .. "–" .. clock(b + off) .. "?",
                     subtitle = "Enter updates it; a name typed saves a new moment instead", placeholder = "A new moment's name",
                     ok = "Update", ok_typed = "Save new" }, function(text)
            if text == "" then save(m.name, m, nil, nil, pic) else set_last_typed(text); save(text, nil, text, nil, pic) end
        end)
    end
    local suggested = suggestion(a)
    ask({ prompt = "Moment " .. clock(a) .. "–" .. clock(b) .. " name (Enter: " .. suggested .. "): ", suggestion = suggested,
          history = history_file(), id = "workspace-moment", title = "Save moment " .. clock(a) .. "–" .. clock(b),
          subtitle = ws.name(state.clip) .. " · into the project “" .. ws.name(state.project) .. "”", names = moment_names(), ok = "Save" }, function(text)
        if text ~= "" then set_last_typed(text) end
        save(text ~= "" and text or suggested, nil, text ~= "" and text or nil, nil, pic)
    end)
end

-- The menu's commands on a moment of the clip, by its place in user-data/workspace/moments
local function moment_at(i)
    local m = state.moments and state.moments[tonumber(i) or 0]
    if m and fs.is_file(m.file) then return m end
    mp.osd_message("That moment isn't there any more", 2)
    refresh()
end

function M.moment_play(i)
    local m = moment_at(i)
    if m then mp.commandv("loadfile", m.file, "replace") end
end

-- A moment of the clip in the whole clip, at its part, to edit it (k updates it)
function M.moment_edit(i)
    local m = moment_at(i)
    if not m or not state.clip then return end
    W.wanted = { file = state.clip, ["in"] = m["in"], out = m.out, moment = { file = m.file, name = m.name, ["in"] = m["in"], out = m.out,
                                                                             snap = m.snap, alone = m.alone, stab = m.stab } }
    mp.commandv("loadfile", state.clip, "replace")
end

function M.moment_reveal(i)
    local m = moment_at(i)
    if m then reveal(m.file) end
end

function M.moments_folder()
    if state.moments_folder and fs.is_dir(state.moments_folder) then reveal(state.moments_folder, true) end
end

function M.moment_remove(i)
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
end

function M.moment_rename(i)
    local m = moment_at(i)
    if not m then return end
    ask({ prompt = "Rename " .. m.name .. " to: ", default = m.name, id = "workspace-rename", title = "Rename “" .. m.name .. "”", ok = "Rename" }, function(text)
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
        -- Both its paths (and its project's): the Library reads the catalog
        catalog.note(state.project or moments.project_of(target), { m.file, target, own, own_project(target) })
        if state.moment and state.moment.file == m.file then
            state.moment.file, state.moment.name = target, safe
            mp.set_property("file-local-options/force-media-title", safe)
        end
        mp.osd_message("Renamed to " .. safe, 2)
        refresh()
    end)
end

-- Shift+E (the gyroflow plugin asks): the export's name, in the dialog moments have. req: { sdr, label (" (SDR)"),
-- look, stabilized, reply }. The answer: export-start { clip, sdr, output, link }
function M.export_name(json)
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
          id = "workspace-export", title = what .. label, subtitle = ws.name(state.clip) .. " · into the project's Exports",
          names = moment_names(), ok = "Export" }, function(text)
        if text ~= "" then set_last_typed(text) end
        local safe = moments_lib.safe_name(text ~= "" and text or suggested)
        if not safe then return mp.osd_message("Export: that name has nothing a file name can have", 3) end
        local first = false
        local root = need_project("Exporting")
        if not root then return end
        local dir = ws.join(root, native(moments_lib.generated_rel("Exports", os.time(), true, generated_runs(root))))
        mkdir(dir)
        -- An export for YouTube is an MKV (gyroflow-export --youtube)
        local output, final = moments.unique_file(dir, safe, label .. (req.ext == ".mkv" and ".mkv" or ".mov"))
        local info = utils.file_info(state.clip)
        local l = {
            v = 1, kind = "export", clip = state.relative or tree_of(state.clip, root), size = info and info.size,
            created = state.created, ["in"] = a and moments_lib.ms(a), out = b and moments_lib.ms(b), name = final,
            look = req.look, stabilized = req.stabilized == true, roots = { root },
        }
        mp.commandv("script-message-to", req.reply, "export-start", utils.format_json({
            clip = state.clip, sdr = req.sdr == true, youtube = req.youtube == true, output = output, link = utils.format_json(l),
            from = a, to = b,
            -- A moment stabilized on its own, without a project: its part (or the part of it) on its own
            alone = state.snap == true and state.alone or nil,
        }))
        if text ~= "" then remember_name(text) end
        -- Every export can be found from a moment, and played in full quality after it's deleted
        if part and not same then save("export_" .. final, nil, nil, true) end
    end)
end

-- An export's original, at its part
-- An export's or a moment's whole clip, at its part (marked, not looped). A moment's is then the moment being edited:
-- k updates it
function M.open_original()
    local e = state.export
    if e then
        if not e.original then return mp.osd_message("The original isn't found", 2) end
        -- Its part is set as it loads (on_load): options given to loadfile are lost when mpv loads it again from the
        -- playlist it makes of its folder
        W.wanted = { file = e.original, ["in"] = e["in"], out = e.out }
        return mp.commandv("loadfile", e.original, "replace")
    end
    -- A photo's link, or a photo the player made (its link in its XMP): its clip, its live part marked
    local ph = state.photo
    if ph and ph.clip then
        W.wanted = { file = ph.clip, ["in"] = ph["in"], out = ph.out, at = ph.t }
        return mp.commandv("loadfile", ph.clip, "replace")
    end
    local m = state.moment
    if m and not m.missing and state.clip then
        W.wanted = { file = state.clip, ["in"] = m["in"], out = m.out, moment = { file = m.file, name = m.name, ["in"] = m["in"], out = m.out,
                                                                                 snap = m.snap, alone = m.alone, stab = m.stab } }
        return mp.commandv("loadfile", state.clip, "replace")
    end
end

-- A photo link (the gyroflow plugin's Alt+s, crop mode's Enter: docs/design/albums-photos-share-concept.md, "Photos as
-- objects"): { t (the clip's time), crop, camera (Save frame's), picture (the frame as it showed, a JPEG at the video's
-- size) }. Written as <clip>_<t>s.afplink.jpg in the project's _Stills/Generated/ (as the Library's photo links are
-- named): the picture cropped and at most 1280 px, its afp:link (kind photo, the clip by its path in the project, its
-- size and time, the crop, the camera) by afp-meta. Nothing is rendered: the Library's Share renders it when it's
-- shared
function M.photo_link(json)
    local req = utils.parse_json(json or "")
    if type(req) ~= "table" or type(req.t) ~= "number" then return end
    local function done(msg) if req.picture then os.remove(req.picture) end; if msg then mp.osd_message(msg, 4) end end
    local root = need_project("Saving a photo")
    if not root or not state.clip then return done() end
    local meta = plugin_tool("afp-meta")
    if not meta then return done("Photo: afp-meta isn't installed") end
    if not req.picture then return done("Photo: mpv couldn't take the frame") end
    local dir = ws.join(root, native(moments_lib.generated_rel("Photos", os.time(), true, generated_runs(root))))
    mkdir(dir)
    local stem = (ws.name(state.clip):gsub("%.[^.]*$", ""))
    local file, final = moments.unique_file(dir, string.format("%s_%.2fs", stem, req.t), ".afplink.jpg")
    local info = utils.file_info(state.clip)
    local l = { v = 1, kind = "photo", clip = state.relative or tree_of(state.clip, root), size = info and info.size,
                created = state.created, t = req.t, crop = req.crop, camera = req.camera, name = final, roots = { root } }
    -- Cropped (in shares of the picture) and at most 1280 px on its long side: visibly a link, not the photo
    local c = type(req.crop) == "table" and req.crop
    local vf = (c and string.format("crop=iw*%.5f:ih*%.5f:iw*%.5f:ih*%.5f,", c.w, c.h, c.x, c.y) or "")
        .. "scale='if(gte(iw,ih),min(1280,iw),-2)':'if(gte(iw,ih),-2,min(1280,ih))'"
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stderr = true,
                              args = { "ffmpeg", "-v", "error", "-y", "-i", req.picture, "-vf", vf, "-q:v", "3", file } }, function(_, r)
        -- Without ffmpeg: the picture as it is
        if not (r and r.status == 0 and fs.is_file(file)) then
            mp.msg.warn("photo link: ffmpeg: " .. tostring(r and r.stderr))
            local i, o = io.open(req.picture, "rb"), io.open(file, "wb")
            if i and o then o:write(i:read("*a")) end
            if i then i:close() end
            if o then o:close() end
        end
        local w = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true,
                                      args = { meta, file, "--link", utils.format_json(l) } })
        if not (w and w.status == 0) then
            os.remove(file)
            mp.msg.warn("photo link: afp-meta: " .. tostring(w and w.stderr))
            return done("Photo failed: its link couldn't be written")
        end
        catalog.note(root, { file })
        done("Photo: " .. final .. " (rendered when it's shared)")
    end)
end

function M.install(w)
    W = w
    state, fs, ws, moments, catalog, cache = W.state, W.fs, W.ws, W.moments, W.catalog, W.cache
    W.save = save
end

return M
