-- dlogm: log video (DJI D-Log M, and D-Log, GoPro GP-Log2, Insta360 10-bit I-Log) shown in HDR, as mpv plays it. An
-- mpv plugin: this folder goes into ~~/scripts/.
--
-- The log curve is recognized by what the camera wrote into the file: DJI's color profile (dji_color.lua: Osmo
-- Action 6, Osmo 360) and GoPro's Protune color (gopro_color.lua: PTCL GPLOG2). DJI tags D-Log M video as BT.709
-- and names Normal-color clips *_D.MP4 too, so the metadata is the only reliable sign. Files it can't tell (other
-- DJI models, Insta360) play as they are; Ctrl+l picks the curve for one by hand.
--
-- Modes, from the GPU shaders in shaders/ (baked from .cube LUTs by install.sh):
--   hdr  <curve>_hlg.glsl, a HOOK MAIN shader: log -> Rec.2100 HLG 1000-nit. The video is tagged as HLG (vf=format,
--        labelled dlogm) and so is the target, so mpv's color pipeline passes the HLG-encoded result through as it
--        is; target-colorspace-hint=yes switches the display to HDR (vo=gpu-next). On an SDR display macOS
--        tone-maps the HLG output down, which looks better than an explicit SDR fallback. D-Log M's LUT is
--        luts/dlogm_hlg.cube; the others are made by tools/make_hdr_lut.py from their makers' published formulas
--        (docs/luts.md).
--   sdr  dlogm_lut3d.glsl, a HOOK MAIN shader: DJI's own D-Log M -> Rec.709 LUT, in SDR. D-Log M only, so far.
--   off  the file as it is, flat as the camera recorded it
--
-- Keys (rebind with `<key> script-binding dlogm/<name>` in input.conf):
--   Ctrl+h  hdr    this file, and log files after it
--   Ctrl+s  sdr    this file, and log files after it
--   L       off    this file
--   Ctrl+l  curve  this file's log curve, by hand: D-Log M, D-Log, GP-Log2, I-Log (10-bit) in turn
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
    { id = "gplog2", name = "GP-Log2", hdr = "shaders/gplog2_hlg.glsl" },
    { id = "ilog", name = "I-Log (10-bit)", hdr = "shaders/ilog_hlg.glsl" },
}
local CURVE = {}
for _, c in ipairs(CURVES) do CURVE[c.id] = c end
-- What the cameras' metadata says, as a curve
local DJI_CURVES = { dlogm = "dlogm", dlog = "dlog" }
local GOPRO_CURVES = { GPLOG2 = "gplog2" }
local PROBED_EXTENSIONS = { mp4 = true, mov = true }
-- The options this sets, as they are for the file before it does
local TARGET_OPTIONS = { "target-trc", "target-prim", "target-peak", "target-colorspace-hint" }
-- The filter that tags the video as what the hdr shader turns it into, at the end of the chain (after the
-- stabilization); enabled in hdr mode only
local TAG_LABEL = "dlogm"
local HLG_TAGS = { gamma = "hlg", primaries = "bt.2020" }

local mode = o.mode == "sdr" and "sdr" or "hdr"
-- For the current file: the user's own values of what this changes, its log curve, and the mode it's in (nil:
-- untouched)
local base = nil
local curve = nil
local current = nil
local announce = false

local function label(m)
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
    color = gopro_color.read(path)
    if color then
        mp.msg.verbose("Protune color " .. color)
        return CURVE[GOPRO_CURVES[color]]
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

local function apply(m)
    if not base then return false end
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
        target["target-trc"] = "hlg"
        target["target-prim"] = "bt.2020"
        target["target-peak"] = "1000"
        target["target-colorspace-hint"] = "yes"
    end
    tag_hlg(m == "hdr")
    mp.set_property_native("file-local-options/glsl-shaders", shaders)
    for _, opt in ipairs(TARGET_OPTIONS) do
        mp.set_property("file-local-options/" .. opt, target[opt])
    end
    current = m
    return true
end

-- After the file is opened (its metadata is known), before the video output is set up for it
mp.add_hook("on_preloaded", 50, function()
    base, curve, current, announce = nil, nil, nil, false
    local path = mp.get_property("path")
    if not path then return end
    base = { shaders = mp.get_property_native("glsl-shaders") or {}, target = {} }
    for _, opt in ipairs(TARGET_OPTIONS) do base.target[opt] = mp.get_property(opt) end
    curve = recorded_curve(path)
    if not curve then
        mp.msg.verbose("not log footage it knows")
        return
    end
    -- A curve without a Rec.709 LUT starts in HDR whatever the mode
    announce = apply(curve.sdr and mode or "hdr")
    if announce then mp.msg.info(label(current)) end
end)

mp.register_event("file-loaded", function()
    if announce then mp.osd_message(label(current), 1.5) end
    announce = false
end)

mp.register_event("end-file", function() base, curve, current = nil, nil, nil end)

local function choose(m, sticky)
    return function()
        if not base then return end
        if current == m then
            mp.osd_message(label(m), 1.5)
            return
        end
        if apply(m) then
            if sticky then mode = m end
            mp.osd_message(label(m), 2)
        end
    end
end

-- The next curve for this file, shown the way the file is shown now (HDR when it was off or untouched)
local function next_curve()
    if not base then return end
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
