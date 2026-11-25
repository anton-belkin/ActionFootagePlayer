-- cutout: a spoiled part cut out of the clip (Ctrl+x, the menu's cut-before and cut-after), for the workspace plugin.
-- main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local link = require 'link'
local moments_lib = require 'moments'
local sidecar_lib = require 'sidecar'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, sidecar, catalog, input, shown_time
local function publish(...) return W.publish(...) end
local function preview_for(...) return W.preview_for(...) end
local function own_project(...) return W.own_project(...) end
local function trash(...) return W.trash(...) end
local function trash_name(...) return W.trash_name(...) end
local function trash_all(...) return W.trash_all(...) end
local function tree_of(...) return W.tree_of(...) end
local function fingerprint(...) return W.fingerprint(...) end
local function plugin_tool(...) return W.plugin_tool(...) end
local function clock(...) return W.clock(...) end

-- Cutting a spoiled part out of the clip (docs/design/lossless-cut.md): Ctrl+x cuts the A-B part out (the menu: also all
-- before or after where it is). afp-cut (the gyroflow plugin's) writes the parts kept, hidden, each read back as
-- the original; only then does the original go to the Trash (with its LRF, Gyroflow project, clip preview and marks
-- file), the first part taking its name and a later one its own recording time, and its marks and moments follow.
local cut_lib = require 'cut'
local cutting = nil

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

local function cut_failed(msg)
    for _, p in ipairs(cutting and cutting.parts or {}) do
        if p.out then os.remove(p.out) end
        if p.lrf_out then os.remove(p.lrf_out) end
    end
    cutting = nil
    state.cutting = nil
    publish()
    mp.msg.warn(msg)
    local title, why = msg:match("^([^:]*): (.*)$")
    mp.notify({ id = "cut", kind = "error", title = title or msg, detail = why })
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
            local noted = {}
            for i in ipairs(c.parts) do noted[#noted + 1] = ws.join(c.dir, c.stem .. " (cut " .. i .. ")" .. c.ext) end
            catalog.note(c.project, noted)
            cutting = nil
            state.cutting = nil
            restore_idle()
            mp.commandv("loadfile", c.clip, "replace")
            return mp.notify({ id = "cut", kind = "error", title = "The original couldn't go to " .. where, detail = "It's as it was, the parts beside it as " .. c.stem .. " (cut 1)…" })
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
                p.relative = c.project and tree_of(p.file, c.project) or nil
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
            -- The parts in its place, their own files, the moments relinked: into the project's catalog (what went to
            -- the Trash is noted as it goes), so the Library shows them at once
            local noted = {}
            for _, p in ipairs(c.parts) do
                noted[#noted + 1] = p.file
                noted[#noted + 1] = sidecar.path(p.file)
                if p.lrf_out then noted[#noted + 1] = ws.join(c.dir, split_name(p.name) .. ".LRF") end
            end
            for _, m in ipairs(c.moments) do noted[#noted + 1] = m.file end
            catalog.note(c.project, noted)
            -- The part that has where it played, there
            local pos = c.pos or 0
            local at = c.parts[1]
            for _, p in ipairs(c.parts) do if pos >= p.from and pos < p.to then at = p end end
            W.wanted = { file = at.file, ["in"] = math.max(0, math.min(pos, at.to) - at.from) }
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
            mp.notify({ id = "cut", kind = "done", title = lines[1], detail = table.concat(lines, "\n", 2), seconds = 8 })
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
    mp.notify({ id = "cut", kind = "working", title = "Cutting " .. c.name, detail = #c.parts > 1 and ("Part " .. i .. " of " .. #c.parts) or "Written and checked by afp-cut",
                share = #c.parts > 1 and (i - 1) / #c.parts or nil })
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
        moments = state.moments or {}, pos = shown_time(), parts = parts,
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

function M.cut_key()
    if type(mp.get_property_native("user-data/afp/photo")) == "table" then return end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    if not (a and b and b > a) then
        return mp.osd_message("Mark the spoiled part first (l at its start, l at its end), then Ctrl+x cuts it out", 4)
    end
    cut_out(a, b)
end
function M.cut_before()
    local t = shown_time()
    if t then cut_out(0, t) end
end
function M.cut_after()
    local t = shown_time()
    if t then cut_out(t, mp.get_property_number("duration") or t) end
end

function M.install(w)
    W = w
    state, fs, ws, sidecar, catalog, input, shown_time = W.state, W.fs, W.ws, W.sidecar, W.catalog, W.input, W.shown_time
end

return M
