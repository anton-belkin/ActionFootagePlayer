-- dlogm: DJI D-Log M video shown in HDR, as mpv plays it. An mpv plugin: this folder goes into ~~/scripts/.
--
-- D-Log M is recognized by the color profile the camera wrote into the file (dji_color.lua: Osmo Action 6,
-- Osmo 360). DJI tags D-Log M video as BT.709 and names Normal-color clips *_D.MP4 too, so the metadata is the only
-- reliable sign. For files it can't tell (other DJI models), the old guess:
--   file name ends with _D.MP4 / _D.MOV  (DJI Osmo convention)
--   OR container "encoder" / "make" metadata contains "DJI"
--
-- Modes, from the GPU shaders in shaders/ (baked from .cube LUTs by install.sh):
--   hdr  dlogm_hlg.glsl, a HOOK OUTPUT shader: D-Log M -> Rec.2100 HLG 1000-nit. It replaces mpv's final output,
--        so mpv's color pipeline doesn't get a chance to mangle the HLG-encoded result; target-trc=hlg and
--        target-colorspace-hint=yes switch the display to HDR (vo=gpu-next). On an SDR display macOS tone-maps
--        the HLG output down, which looks better than an explicit SDR fallback.
--   sdr  dlogm_lut3d.glsl, a HOOK MAIN shader: DJI's own D-Log M -> Rec.709 LUT, in SDR.
--   off  the file as it is, for *_D files that aren't D-Log M (e.g. HDR photos)
--
-- Keys (rebind with `<key> script-binding dlogm/<name>` in input.conf):
--   Ctrl+h  hdr  this file, and D-Log M files after it
--   Ctrl+s  sdr  this file, and D-Log M files after it
--   L       off  this file
--
-- Options (~~/script-opts/dlogm.conf):
--   mode=hdr     what D-Log M files start in: hdr or sdr
--
-- Its files are looked for in $DLOGM_DATA first, then in this folder: an app that ships the plugin read-only
-- (ActionFootagePlayer) keeps what it can't ship there, DJI's LUT and the sdr shader made from it, under the same names.
--
-- Everything is set as file-local options, added to the user's own glsl-shaders: mpv restores them when the
-- file ends. Source files are never modified.

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'
local dji_color = require 'dji_color'

local o = { mode = "hdr" }
options.read_options(o, "dlogm")

local DIR = mp.get_script_directory()
local DATA = os.getenv("DLOGM_DATA")
-- Where the plugin's files are looked for, in order
local DIRS = {}
if DATA and DATA ~= "" then table.insert(DIRS, DATA) end
table.insert(DIRS, DIR)
local SHADERS = {
    hdr = "shaders/dlogm_hlg.glsl",
    sdr = "shaders/dlogm_lut3d.glsl",
}
local LABELS = { hdr = "D-Log M -> HDR", sdr = "D-Log M -> Rec.709", off = "D-Log M handling off: the file as it is" }
local PROBED_EXTENSIONS = { mp4 = true, mov = true }
-- The options this sets, as they are for the file before it does
local TARGET_OPTIONS = { "target-trc", "target-prim", "target-peak", "target-colorspace-hint" }

local mode = o.mode == "sdr" and "sdr" or "hdr"
-- For the current file: the user's own values of what this changes, and the mode it's in (nil: untouched)
local base = nil
local current = nil
local announce = false

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

local function guessed_dlogm(path)
    local lower = path:lower()
    if lower:match("_d%.mp4$") or lower:match("_d%.mov$") then return true end
    local meta = mp.get_property_native("metadata") or {}
    for k, v in pairs(meta) do
        if type(v) == "string" and v:lower():find("dji", 1, true) then
            local kl = k:lower()
            if kl == "encoder" or kl == "make" or kl == "com.apple.quicktime.make" then
                return true
            end
        end
    end
    return false
end

local function is_dlogm(path)
    if path:find("://", 1, true) then return false end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    local color = PROBED_EXTENSIONS[ext] and dji_color.read(path) or nil
    if color then
        mp.msg.verbose("recorded in " .. color)
        return color == "dlogm"
    end
    return guessed_dlogm(path)
end

local function apply(m)
    if not base then return false end
    local shader = SHADERS[m] and plugin_file(SHADERS[m])
    if SHADERS[m] and not shader then
        mp.msg.warn("missing " .. SHADERS[m] .. " in " .. table.concat(DIRS, " and ") .. " (run install.sh)")
        mp.osd_message(LABELS[m] .. ": its shader isn't installed", 3)
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
    mp.set_property_native("file-local-options/glsl-shaders", shaders)
    for _, name in ipairs(TARGET_OPTIONS) do
        mp.set_property("file-local-options/" .. name, target[name])
    end
    current = m
    return true
end

-- After the file is opened (its metadata is known), before the video output is set up for it
mp.add_hook("on_preloaded", 50, function()
    base, current, announce = nil, nil, false
    local path = mp.get_property("path")
    if not path then return end
    base = { shaders = mp.get_property_native("glsl-shaders") or {}, target = {} }
    for _, name in ipairs(TARGET_OPTIONS) do base.target[name] = mp.get_property(name) end
    if not is_dlogm(path) then
        mp.msg.verbose("not D-Log M")
        return
    end
    announce = apply(mode)
    if announce then mp.msg.info(LABELS[mode]) end
end)

mp.register_event("file-loaded", function()
    if announce then mp.osd_message(LABELS[current], 1.5) end
    announce = false
end)

mp.register_event("end-file", function() base, current = nil, nil end)

local function choose(m, sticky)
    return function()
        if not base then return end
        if current == m then
            mp.osd_message(LABELS[m], 1.5)
            return
        end
        if apply(m) then
            if sticky then mode = m end
            mp.osd_message(LABELS[m], 2)
        end
    end
end

mp.add_key_binding("Ctrl+h", "hdr", choose("hdr", true))
mp.add_key_binding("Ctrl+s", "sdr", choose("sdr", true))
mp.add_key_binding("L", "off", choose("off", false))
