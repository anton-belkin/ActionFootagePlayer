-- viewsave: the transform and the input saved (the side panel's Source and Transform: Save, Revert, Auto save,
-- settings of its own), Autoplay next, a moment's ends from its Gyroflow project. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local moments_lib = require 'moments'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, sidecar, catalog
local function publish(...) return W.publish(...) end
local function current_view(...) return W.current_view(...) end
local function copy(...) return W.copy(...) end
local function stamp(...) return W.stamp(...) end
local function save(...) return W.save(...) end
local function rewrite_link(...) return W.rewrite_link(...) end
local function own_project(...) return W.own_project(...) end
local function trash(...) return W.trash(...) end
local function project_tool(...) return W.project_tool(...) end
local function run_tool(...) return W.run_tool(...) end
local function clock(...) return W.clock(...) end
local adopt_gyroflow_trim

-- Input and transform (transform.lua, docs/design/moments-and-workspaces.md "Source and transform"): the source's input is
-- read from its own file; the transform bundle that plays is a moment's or clip preview's own (with settings of its
-- own), else the source's, else none (nil: the session's, the gyroflow plugin's)
local transform = require 'transform'
-- Changes written as they're made (the side panel's Auto save): this session only
local auto_save = false
-- Next files in the folder play on at the end (off: each pauses at its end), this session
local autoplay = true
local function autoplay_on() return autoplay end

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
    local full, added = transform.complete(raw, transform.session(mp.get_property_native))
    local input = state.view_target ~= "source" and transform.input_of(raw) ~= nil
    if (added or input) and write_bundle(full, true) then
        mp.msg.info("transform completed and written: " .. utils.format_json(full))
        if state.view then state.view = copy(full) end
    end
end

local PLACES = { source = "the source's file", moment = "the moment", preview = "the clip preview" }
function M.view_save()
    local t = state.view_target
    if not t then return mp.osd_message("Nothing to save the transform into", 2) end
    local v = current_view()
    if write_bundle(v, false) then
        -- It plays as saved from now on; one Auto save wrote as it opened plays as the session's to its end
        if not state.fresh then state.view = copy(v) end
        if t ~= "moment" then mp.osd_message("Transform saved in " .. PLACES[t], 2) end
    end
    publish_view(true)
end

-- Back to what's saved, for this viewing
function M.view_revert()
    if not state.saved then return end
    mp.commandv("script-message-to", "gyroflow", "apply-view", utils.format_json(state.saved))
    mp.osd_message("Transform as saved", 1.5)
end

-- Auto save: each change written as it's made, this session; where nothing's saved, what plays as it opens
function M.view_auto()
    auto_save = not auto_save
    autosave_fresh()
    publish_view(true)
    mp.osd_message(auto_save and "Auto save on: the transform is saved as it's changed, this session" or "Auto save off", 3)
end

-- The source's input as it's shown (input-set <curve|rotation|underwater|uw|uw_highlights|uw_strength> <value>, the
-- dlogm and gyroflow plugins', as they apply a change): for viewing until Save on the Source tab (input-save) writes
-- it into the source's own file, whole, and so for every moment and clip preview of it. Transform's Save doesn't touch
-- it. The underwater colour's keys: where the source has none, uwcolor.conf's (the dlogm plugin's defaults)
local INPUT_DEFAULTS = { curve = "auto", rotation = 0, underwater = "auto", uw = false, uw_highlights = false, uw_strength = 1 }
local INPUT_ORDER = { "curve", "rotation", "underwater", "uw", "uw_highlights", "uw_strength" }
local function input_pack(from)
    local pack = {}
    local u = mp.get_property_native("user-data/uwcolor")
    local uw_defaults = type(u) == "table" and type(u.defaults) == "table" and u.defaults or {}
    for k, d in pairs(INPUT_DEFAULTS) do
        pack[k] = (from or {})[k]
        if pack[k] == nil then pack[k] = uw_defaults[k] end
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
function M.input_set(key, value)
    if INPUT_DEFAULTS[key] == nil or not input_place() then return end
    if key == "rotation" then value = tonumber(value) or 0
    elseif key == "uw_strength" then value = tonumber(value) or 1
    elseif key == "uw" or key == "uw_highlights" then value = value == "yes" or value == "true" end
    state.input_now = state.input_now or input_pack(state.input)
    state.input_now[key] = value
    state.input_status = input_status()
    publish()
end
function M.input_save()
    if not input_place() then return mp.osd_message("Not saved: this file isn't a source", 2) end
    local pack = input_pack(state.input_now or state.input)
    -- The input's keys written over the source's; its others kept (the lens type the Library sets there)
    local ok, err = sidecar.update(state.clip, function(d)
        d.source = type(d.source) == "table" and d.source or {}
        for k, v in pairs(pack) do d.source[k] = v end
        stamp(d)
    end)
    if not ok then return mp.osd_message("Couldn't save the input in the source's file: " .. tostring(err), 4) end
    state.input, state.input_now = pack, nil
    state.input_status = input_status()
    publish()
    mp.osd_message("Input saved in the source's file: for all its moments and clips", 2)
end
-- Back to the saved input, for this viewing: each plugin applies its part again (and says so with input-set)
function M.input_revert()
    if not state.input_now then return end
    local saved = input_pack(state.input)
    local now = state.input_now
    if tostring(now.curve) ~= tostring(saved.curve) then mp.commandv("script-message-to", "dlogm", "curve", tostring(saved.curve)) end
    if tostring(now.rotation) ~= tostring(saved.rotation) then mp.commandv("script-message-to", "gyroflow", "rotate", tostring(saved.rotation)) end
    if tostring(now.underwater) ~= tostring(saved.underwater) then mp.commandv("script-message-to", "gyroflow", "underwater", tostring(saved.underwater)) end
    for _, k in ipairs({ "uw", "uw_highlights", "uw_strength" }) do
        if tostring(now[k]) ~= tostring(saved[k]) then
            local v = saved[k]
            mp.commandv("script-message-to", "dlogm", "uw-set", k, type(v) == "boolean" and (v and "yes" or "no") or string.format("%.2f", tonumber(v) or 1))
        end
    end
    mp.osd_message("Input as saved", 1.5)
end

-- A moment stabilized on its own or with its clip: written into it (and its project's trim range), played again
function M.moment_alone(value)
    local m = state.moment
    if not (m and m.snap) then return end
    local alone = value == "yes" or (value ~= "no" and not state.alone)
    save(m.name, m, nil, false, { range = { m["in"], m.out }, stab = state.saved or current_view(), alone = alone })
end

-- Autoplay next (the panel's): on, the next file in the folder plays at the end; off, each pauses at its end
local KEEP_OPEN = mp.get_property("keep-open") or "no"
if KEEP_OPEN == "always" then KEEP_OPEN = "no" end
function M.autoplay(value)
    if value == "toggle" then autoplay = not autoplay else autoplay = value == "yes" end
    mp.set_property("keep-open", autoplay and KEEP_OPEN or "always")
    if state.moment and not state.moment.missing then
        mp.set_property("file-local-options/keep-open", autoplay and "yes" or "always")
    end
    state.autoplay = autoplay
    publish()
    mp.osd_message(autoplay and "Autoplay next: on" or "Autoplay next: off, each file pauses at its end", 2)
end


-- Settings of its own for the moment or clip preview playing, or the clip's: on, what plays now is written into it
-- (with a copy of the clip's Gyroflow project, if it has one); off, they're taken out (its own project to the
-- Trash) and it plays with the clip's
function M.view_own()
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
        if run_tool({ tool, "--copy", g.project, state.clip, own }) then catalog.note(nil, { own }) end
    end
    if not rewrite_link(l.file, function(x) x.own, x.stab = 1, v end) then
        return mp.osd_message("Couldn't change " .. moments_lib.display_name(l.file), 4)
    end
    mp.osd_message(moments_lib.display_name(l.file) .. ": settings of its own" .. (fs.is_file(own) and ", and a Gyroflow project (Ctrl+e opens it)" or ""), 3)
    again()
end

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
M.view_changed = view_changed
function M.loaded()
    settle_bundle()
    autosave_fresh()
    publish_view(true)
end

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

function M.install(w)
    W = w
    state, fs, sidecar, catalog = W.state, W.fs, W.sidecar, W.catalog
    state.autoplay = autoplay
    W.adopt_gyroflow_trim, W.autoplay_on = adopt_gyroflow_trim, autoplay_on
end

return M
