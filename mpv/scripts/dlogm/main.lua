-- dlogm: log video (DJI D-Log M, and D-Log, GoPro GP-Log and GP-Log2, Insta360 10-bit I-Log) shown in HDR, as mpv
-- plays it. An mpv plugin: this folder goes into ~~/scripts/.
--
-- The log curve is recognized by what the camera wrote into the file: DJI's color profile (dji_color.lua: Osmo
-- Action 6, Osmo 360) and GoPro's Protune color (gopro_color.lua: PTCL GPLOG2, or LOG from a HERO12 Black on
-- firmware 1.20 or later or a HERO13 Black, where GP-Log is what its LUT is for). DJI tags D-Log M video as BT.709
-- and names Normal-color clips *_D.MP4 too, so the metadata is the only reliable sign. Files it can't tell (other
-- DJI models, Insta360) play as they are; Ctrl+l picks the curve for one by hand.
--
-- A file tagged HDR (transfer HLG or PQ: the player's own exports, phones, GoPro's and DJI's HDR modes) plays in
-- HDR as it is, with no LUT: the target is set to match it and the display switched to HDR, as in the hdr mode. It
-- is never taken for log, whatever camera metadata or *_D name an export carries over: log is recorded tagged as
-- SDR. The tag is read from the MP4/MOV before the file plays (video_color.lua); in other containers from the
-- decoder once it starts, so there the first frames are in SDR.
--
-- Modes, from the GPU shaders in shaders/ (baked from .cube LUTs by install.sh):
--   hdr  <curve>_hlg.glsl, a HOOK MAIN shader: log -> Rec.2100 HLG 1000-nit. The video is tagged as HLG (vf=format,
--        labelled dlogm) and so is the target, so mpv's color pipeline passes the HLG-encoded result through as it
--        is; target-colorspace-hint=yes switches the display to HDR (vo=gpu-next). On an SDR display macOS
--        tone-maps the HLG output down, which looks better than an explicit SDR fallback. D-Log M's LUT is
--        luts/dlogm_hlg.cube and GP-Log's luts/gplog_hlg.cube, both from Zeb Gardner's LUTs; the others are made
--        by tools/make_hdr_lut.py from their makers' published formulas (docs/luts.md).
--   sdr  dlogm_lut3d.glsl, a HOOK MAIN shader: DJI's own D-Log M -> Rec.709 LUT, in SDR. D-Log M only, so far.
--   off  the file as it is, flat as the camera recorded it
-- An HDR file has two of its own, for that file only (the mode log files start in stays as it was): as it is, in
-- HDR (Ctrl+h, and L: the file as it is), or tone-mapped to SDR by mpv, as without the plugin (Ctrl+s). Ctrl+l still
-- grades one with a log curve by hand.
--
-- Keys (rebind with `<key> script-binding dlogm/<name>` in input.conf):
--   Ctrl+h  hdr    this file, and log files after it
--   Ctrl+s  sdr    this file, and log files after it
--   L       off    this file
--   Ctrl+l  curve  this file's log curve, by hand: D-Log M, D-Log, GP-Log, GP-Log2, I-Log (10-bit) in turn
--
-- Script messages (`script-message-to dlogm <message>`, what the right-click menu runs; mpv/menu.conf):
--   curve <id>|auto   this file's log curve: dlogm, dlog, gplog, gplog2, ilog, or auto: the one its metadata says
--   remember          the mode (hdr or sdr) as the default, in dlogm.conf
-- The state is in user-data/dlogm/ for the menu: log (a curve is applied), recorded (the curve the metadata says, or
-- ""), curve (the one applied, or ""), look (hdr, sdr, off, or "" when untouched), mode, sdr (DJI's LUT is there for
-- the curve), hdr (the file is tagged HDR: hlg or pq, or "").
--
-- Options (~~/script-opts/dlogm.conf):
--   mode=hdr     what log files start in: hdr or sdr
--
-- Its files are looked for in $DLOGM_DATA first, then in this folder: an app that ships the plugin read-only
-- (ActionFootagePlayer) keeps what it can't ship there, DJI's LUT and the sdr shader made from it, under the same names.
--
-- Everything is set as file-local options, added to the user's own glsl-shaders and vf: mpv restores them when the
-- file ends. Source files are never modified.

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'
local dji_color = require 'dji_color'
local gopro_color = require 'gopro_color'
local video_color = require 'video_color'

local o = { mode = "hdr" }
options.read_options(o, "dlogm")

local DIR = mp.get_script_directory()
local DATA = os.getenv("DLOGM_DATA")
-- Where the plugin's files are looked for, in order
local DIRS = {}
if DATA and DATA ~= "" then table.insert(DIRS, DATA) end
table.insert(DIRS, DIR)
-- The log curves, in Ctrl+l's order, with their shaders by mode
local CURVES = {
    { id = "dlogm", name = "D-Log M", hdr = "shaders/dlogm_hlg.glsl", sdr = "shaders/dlogm_lut3d.glsl" },
    { id = "dlog", name = "D-Log", hdr = "shaders/dlog_hlg.glsl" },
    { id = "gplog", name = "GP-Log", hdr = "shaders/gplog_hlg.glsl" },
    { id = "gplog2", name = "GP-Log2", hdr = "shaders/gplog2_hlg.glsl" },
    { id = "ilog", name = "I-Log (10-bit)", hdr = "shaders/ilog_hlg.glsl" },
}
local CURVE = {}
for _, c in ipairs(CURVES) do CURVE[c.id] = c end
-- What the cameras' metadata says, as a curve
local DJI_CURVES = { dlogm = "dlogm", dlog = "dlog" }
local GOPRO_CURVES = { GPLOG2 = "gplog2", LOG = "gplog" }
-- GP-Log as gplog_hlg.cube is made for: by the firmware where GoPro's own LUTs put 18 % grey where it does. Earlier
-- HERO12 firmware recorded 2 stops darker, MAX2 2 stops brighter: their clips play as they are (Ctrl+l still works)
local GPLOG_FIRMWARE = { ["HERO12 Black"] = 1.20, ["HERO13 Black"] = 0 }
local PROBED_EXTENSIONS = { mp4 = true, mov = true }
-- The options this sets, as they are for the file before it does
local TARGET_OPTIONS = { "target-trc", "target-prim", "target-peak", "target-colorspace-hint" }
-- The filter that tags the video as what the hdr shader turns it into, at the end of the chain (after the
-- stabilization); enabled in hdr mode only
local TAG_LABEL = "dlogm"
local HLG_TAGS = { gamma = "hlg", primaries = "bt.2020" }
-- The HDR transfers, by mpv's names for them (target-trc, video-dec-params/gamma)
local HDR_NAMES = { hlg = "HLG", pq = "PQ" }

local mode = o.mode == "sdr" and "sdr" or "hdr"
-- For the current file: the user's own values of what this changes, its log curve, and the mode it's in (nil:
-- untouched)
local base = nil
local curve = nil
local current = nil
local announce = false
-- What the file's metadata says
local recorded = nil
-- The file's HDR tag, { trc = "hlg" or "pq", prim = "bt.2020", ... } (nil: SDR), and whether the user has changed
-- how the file shows
local hdr = nil
local touched = false

local function label(m)
    if hdr and not curve then
        local name = "HDR video (" .. HDR_NAMES[hdr.trc] .. ")"
        if m == "sdr" then return name .. " tone-mapped to SDR" end
        return name .. ", as it is"
    end
    local name = (curve or CURVE.dlogm).name
    if m == "hdr" then return name .. " -> HDR" end
    if m == "sdr" then return name .. " -> Rec.709" end
    return name .. " handling off: the file as it is"
end

local function file_exists(path)
    local info = utils.file_info(path)
    return info ~= nil and info.is_file
end

-- A file of the plugin's, from $DLOGM_DATA or this folder; nil when it's in neither. Looked up each time: the app
-- may fetch DJI's LUT while a file plays
local function plugin_file(name)
    for _, dir in ipairs(DIRS) do
        if file_exists(utils.join_path(dir, name)) then return utils.join_path(dir, name) end
    end
    return nil
end

-- For the right-click menu (mpv/menu.conf)
local function publish()
    local c = curve or CURVE.dlogm
    local state = {
        log = curve ~= nil, recorded = recorded and recorded.id or "", curve = curve and curve.id or "",
        look = current or "", mode = mode, sdr = c.sdr ~= nil and plugin_file(c.sdr) ~= nil, hdr = hdr and hdr.trc or "",
    }
    for k, v in pairs(state) do mp.set_property_native("user-data/dlogm/" .. k, v) end
end

-- The file's log curve from the camera's metadata, nil when it isn't one (or can't be told)
local function recorded_curve(path)
    if path:find("://", 1, true) then return nil end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBED_EXTENSIONS[ext] then return nil end
    local color = dji_color.read(path)
    if color then
        mp.msg.verbose("recorded in " .. color)
        return CURVE[DJI_CURVES[color]]
    end
    local camera, firmware
    color, camera, firmware = gopro_color.read(path)
    if color then
        mp.msg.verbose("Protune color " .. color .. " on " .. (camera or "a GoPro") .. ", firmware " .. (firmware or "?"))
        local id = GOPRO_CURVES[color]
        if id == "gplog" and not (GPLOG_FIRMWARE[camera] and firmware and firmware >= GPLOG_FIRMWARE[camera]) then
            mp.msg.info("GP-Log from " .. (camera or "this GoPro") .. ": not what the GP-Log LUT is made for (Ctrl+l)")
            return nil
        end
        return CURVE[id]
    end
    return nil
end

-- The video tagged as HLG (on) or as the file says (off). The filter stays in the chain once added, disabled when
-- off. Switching it rebuilds the chain: Gyroflow's filter restarts, as with Ctrl+g
local function tag_hlg(on)
    local vf = mp.get_property_native("vf") or {}
    local tag = nil
    for _, f in ipairs(vf) do
        if f.label == TAG_LABEL then tag = f end
    end
    if not tag then
        if not on then return end
        tag = { name = "format", label = TAG_LABEL, params = HLG_TAGS }
        table.insert(vf, tag)
    elseif tag.enabled == on then
        return
    end
    tag.enabled = on
    mp.set_property_native("file-local-options/vf", vf)
end

-- The target for HDR: the display switched to HDR (vo=gpu-next) and the signal sent to it encoded as `trc`, so mpv
-- passes HDR video in it through as it is. 1000 nits, as the hdr shader's HLG is graded for
local function hdr_target(target, trc, prim)
    target["target-trc"] = trc
    target["target-prim"] = prim
    target["target-peak"] = "1000"
    target["target-colorspace-hint"] = "yes"
end

local function set_target(target)
    for _, opt in ipairs(TARGET_OPTIONS) do
        mp.set_property("file-local-options/" .. opt, target[opt])
    end
end

-- An HDR file, without a log curve: as it is, in HDR (hdr, off), or tone-mapped to SDR by mpv (sdr)
local function apply_hdr_file(m)
    local target = {}
    for k, v in pairs(base.target) do target[k] = v end
    if m ~= "sdr" then hdr_target(target, hdr.trc, hdr.prim) end
    tag_hlg(false)
    mp.set_property_native("file-local-options/glsl-shaders", base.shaders)
    set_target(target)
    current = m == "sdr" and "sdr" or "hdr"
    publish()
    return true
end

local function apply(m)
    if not base then return false end
    if hdr and not curve then return apply_hdr_file(m) end
    local c = curve or CURVE.dlogm
    if m == "sdr" and not c.sdr then
        mp.osd_message(c.name .. ": no Rec.709 LUT for it yet", 3)
        return false
    end
    local name = c[m]
    local shader = name and plugin_file(name)
    if name and not shader then
        mp.msg.warn("missing " .. name .. " in " .. table.concat(DIRS, " and ") .. " (run install.sh)")
        mp.osd_message(label(m) .. ": its shader isn't installed", 3)
        return false
    end
    local shaders = {}
    for _, s in ipairs(base.shaders) do table.insert(shaders, s) end
    local target = {}
    for k, v in pairs(base.target) do target[k] = v end
    if shader then table.insert(shaders, shader) end
    if m == "hdr" then
        hdr_target(target, "hlg", "bt.2020")
    elseif m == "off" and hdr then
        -- off on an HDR file graded by hand: as it is, in HDR
        hdr_target(target, hdr.trc, hdr.prim)
    end
    tag_hlg(m == "hdr")
    mp.set_property_native("file-local-options/glsl-shaders", shaders)
    set_target(target)
    current = m
    publish()
    return true
end

-- After the file is opened (its metadata is known), before the video output is set up for it
mp.add_hook("on_preloaded", 50, function()
    base, curve, current, announce, recorded, hdr, touched = nil, nil, nil, false, nil, nil, false
    local path = mp.get_property("path")
    if not path then return publish() end
    base = { shaders = mp.get_property_native("glsl-shaders") or {}, target = {} }
    for _, opt in ipairs(TARGET_OPTIONS) do base.target[opt] = mp.get_property(opt) end
    local trc, prim
    if not path:find("://", 1, true) then trc, prim = video_color.read(path) end
    if trc then
        -- Log is recorded tagged as SDR: what an HDR file's camera metadata says (an export's) isn't read
        hdr = { trc = trc, prim = prim or "bt.2020" }
        announce = apply("hdr")
        mp.msg.info(label(current))
        return
    end
    recorded = recorded_curve(path)
    curve = recorded
    if not curve then
        mp.msg.verbose("not log footage it knows")
        return publish()
    end
    -- A curve without a Rec.709 LUT starts in HDR whatever the mode
    announce = apply(curve.sdr and mode or "hdr")
    if announce then mp.msg.info(label(current)) end
end)

-- An HDR file whose container didn't say (not MP4/MOV, or no colr box), from the decoder once it starts: in HDR as
-- it is from then on, unless the user has chosen otherwise. The decoder's, not video-params: the hdr mode's filter
-- tags the video as HLG after it
mp.observe_property("video-dec-params", "native", function(_, p)
    if not base or hdr or touched or not p or not HDR_NAMES[p.gamma] then return end
    hdr = { trc = p.gamma, prim = p.primaries or "bt.2020" }
    if curve then mp.msg.info(curve.name .. " in the metadata, but the video is tagged " .. HDR_NAMES[hdr.trc] .. ": not log") end
    recorded, curve = nil, nil
    if apply("hdr") then
        mp.msg.info(label(current))
        mp.osd_message(label(current), 1.5)
    end
end)

mp.register_event("file-loaded", function()
    if announce then mp.osd_message(label(current), 1.5) end
    announce = false
end)

mp.register_event("end-file", function()
    base, curve, current, recorded, hdr = nil, nil, nil, nil, nil
    publish()
end)

local function choose(wanted, sticky)
    return function()
        if not base then return end
        touched = true
        local hdr_file = hdr and not curve
        -- An HDR file as it is is in HDR
        local m = hdr_file and wanted == "off" and "hdr" or wanted
        if current == m then
            mp.osd_message(label(m), 1.5)
            return
        end
        if apply(m) then
            -- An HDR file's look is its own: log files start as they did
            if sticky and not hdr_file then mode = m end
            publish()
            mp.osd_message(label(m), 2)
        end
    end
end

-- The next curve for this file, shown the way the file is shown now (HDR when it was off or untouched)
local function next_curve()
    if not base then return end
    touched = true
    local i = 0
    for k, c in ipairs(CURVES) do
        if c == curve then i = k end
    end
    curve = CURVES[i % #CURVES + 1]
    local m = current == "sdr" and curve.sdr and "sdr" or "hdr"
    if apply(m) then mp.osd_message(label(m), 2) end
end

mp.add_key_binding("Ctrl+h", "hdr", choose("hdr", true))
mp.add_key_binding("Ctrl+s", "sdr", choose("sdr", true))
mp.add_key_binding("L", "off", choose("off", false))
mp.add_key_binding("Ctrl+l", "curve", next_curve)

-- This file's curve by name, or the one its metadata says (auto: for a file it says isn't log, as it is again; an
-- HDR file in HDR)
mp.register_script_message("curve", function(id)
    if not base then return end
    touched = true
    if id == "auto" and not recorded then
        if hdr then
            curve = nil
            if apply("hdr") then mp.osd_message(label(current), 2) end
            return
        end
        if current then apply("off") end
        curve, current = nil, nil
        publish()
        mp.osd_message("Not log, as the camera says: the file as it is", 2)
        return
    end
    local c = id == "auto" and recorded or CURVE[id]
    if not c then return end
    local before = curve
    curve = c
    local m = current == "sdr" and curve.sdr and "sdr" or "hdr"
    if apply(m) then mp.osd_message(label(m), 2) else curve = before end
end)

-- Sets options in ~~/script-opts/dlogm.conf, the rest of the file as it was; false when it can't be written
local function save_options(values)
    local path = mp.command_native({ "expand-path", "~~/script-opts/dlogm.conf" })
    local lines, done = {}, {}
    local f = io.open(path, "r")
    if f then
        for line in f:lines() do
            local key = line:match("^%s*([%w_%-]+)%s*=")
            if not (key and values[key]) then
                table.insert(lines, line)
            elseif not done[key] then
                table.insert(lines, key .. "=" .. values[key])
                done[key] = true
            end
        end
        f:close()
    end
    for key, value in pairs(values) do
        if not done[key] then table.insert(lines, key .. "=" .. value) end
    end
    local dir = path:match("^(.*)[/\\]")
    if dir and not utils.file_info(dir) then
        local windows = package.config:sub(1, 1) == "\\"
        mp.command_native({ name = "subprocess", playback_only = false,
                            args = windows and { "cmd", "/d", "/c", "mkdir", dir } or { "mkdir", "-p", dir } })
    end
    f = io.open(path, "w")
    if not f then return false end
    f:write(table.concat(lines, "\n") .. "\n")
    f:close()
    return true
end

mp.register_script_message("remember", function()
    local saved = save_options({ mode = mode })
    mp.osd_message(saved and ("Log clips start in " .. (mode == "sdr" and "the standard look" or "HDR") .. " (dlogm.conf)")
                         or "Couldn't write dlogm.conf", 2)
end)
