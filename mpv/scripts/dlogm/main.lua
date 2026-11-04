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
-- Looks, from the GPU shaders in shaders/ (baked from .cube LUTs by install.sh). The log curve (Ctrl+l) is one
-- choice, the look another: which looks a file has depends on its curve.
--   hdr    <curve>_hlg.glsl, a HOOK MAIN shader: log -> Rec.2100 HLG 1000-nit, nothing after it. The video is tagged
--          as HLG (vf=format, labelled dlogm) and so is the target, so mpv's color pipeline passes the HLG-encoded
--          result through as it is; target-colorspace-hint=yes switches the display to HDR (vo=gpu-next). On an SDR
--          display macOS tone-maps the HLG output down. D-Log M's LUT is luts/dlogm_hlg.cube and GP-Log's
--          luts/gplog_hlg.cube, both from Zeb Gardner's LUTs; the others are made by tools/make_hdr_lut.py from
--          their makers' published formulas (docs/luts.md).
--   maker  the camera maker's own LUT to Rec.709, in SDR: dlogm_lut3d.glsl, DJI's D-Log M -> Rec.709. D-Log M only,
--          so far.
--   nbcu   the hdr look's HLG, then NBCUniversal's HLG -> SDR conversion (nbcu_hlg2sdr.glsl, from
--          luts/3-NBCU_HLG2SDR_DL_v1.2.cube): Rec.709 without a maker's look, the same for every curve.
--   tonemap  the hdr look's HLG tone-mapped to SDR by mpv itself (libplacebo, reference white at SDR white as
--          BT.2408 has it), not by macOS: as the hdr look shows on an SDR screen on Windows.
--   dv     the hdr look's HLG shown by macOS's own video layer (--macos-sample-buffer-layer, the app's mpv) with a
--          Dolby Vision RPU on each frame (dv/dji_fit.rpu: trims fitted to DJI's LUT), so macOS's Dolby Vision display
--          management maps it for the screen, as QuickTime and Apple devices show an export carrying that RPU. Only
--          where mpv has the option (macOS, the app's mpv).
--   off    the file as it is, flat as the camera recorded it (L; not one of Ctrl+s's)
-- The SDR looks tag the video as BT.709 for mpv, and leave the target as it was: on an HDR screen they show as an
-- SDR screen shows them.
-- A file tagged HDR has its own: as it is, in HDR (hdr; L too), or in SDR by NBCU's conversion for its transfer
-- (nbcu: nbcu_hlg2sdr.glsl, nbcu_pq2sdr.glsl, in BT.2020), or by mpv's tone mapping (tonemap), or, signaled as
-- Dolby Vision (a dvvC box, as the player's own Dolby Vision exports have), by macOS with its own RPU (dv). Ctrl+l still grades one with a log curve by hand.
-- Noise reduction, with any of them and on any YUV video, log or not: shaders/denoise.glsl, on the video's planes
-- before mpv turns them into RGB (so before the LUT lifts log's shadows, and their noise with them): the color noise
-- mostly goes, the brightness keeps its detail. Off by default; playback only, exports are as before.
--
-- What a file starts in depends on the screen the window is on, HDR or SDR (macOS's EDR headroom, read once with
-- osascript; elsewhere, or when it can't tell, it counts as HDR): on an HDR screen in HDR, on an SDR screen in the
-- SDR look last chosen (at first Dolby Vision as macOS shows it with the app's mpv, mpv's tone mapping without it).
-- Choosing a look (Ctrl+s, Ctrl+h, the menu) makes it what files start in on that kind of screen, for the rest of the
-- session; an SDR look chosen on either becomes the SDR screen's. `remember` keeps both in dlogm.conf. A file not yet
-- switched by hand follows its window to a screen of the other kind.
--
-- Keys (rebind with `<key> script-binding dlogm/<name>` in input.conf):
--   Ctrl+s  sdr    the next look this file has: hdr, then the maker's and NBCU's SDR, mpv's tone mapping, Dolby Vision
--   Ctrl+h  hdr    the hdr look, as Ctrl+s chooses one
--   L       off    this file only
--   Ctrl+l  curve  this file's log curve, by hand: D-Log M, D-Log, GP-Log, GP-Log2, I-Log (10-bit) in turn
--   Ctrl+n  denoise  noise reduction on / off, for this file and the ones after it
--
-- Script messages (`script-message-to dlogm <message>`, what the right-click menu runs; mpv/menu.conf):
--   look <look>       this file's look: hdr, maker, nbcu, tonemap or dv, as Ctrl+s would
--   curve <id>|auto   this file's log curve: dlogm, dlog, gplog, gplog2, ilog, or auto: the one its metadata says
--   remember          the looks files start in on each kind of screen, and the noise reduction, as the default, in
--                     dlogm.conf
-- The state is in user-data/dlogm/ for the menu: log (a curve is applied), recorded (the curve the metadata says, or
-- ""), curve (the one applied, or ""), look (hdr, maker, nbcu, tonemap, dv, off, or "" when untouched), looks (the ones
-- the file has, in Ctrl+s's order), maker (whose LUT maker is, or ""), sdr_look (the SDR look an export in SDR
-- takes: maker or nbcu, or ""), screen (hdr, sdr, or "" when unknown), hdr (the file is tagged HDR: hlg or pq, or
-- ""), denoise (the noise reduction is on).
--
-- Options (~~/script-opts/dlogm.conf):
--   hdr_screen=hdr    what log files start in on an HDR screen: hdr, maker, nbcu, tonemap or dv
--   sdr_screen=dv     and on an SDR screen (tonemap where mpv can't do dv)
--   (mode=sdr, as before these: maker on both)
--   denoise=no   whether files start with the noise reduction on. How strong it is is the shader's:
--                glsl-shader-opts=denoise_strength=2 in mpv.conf (1 by default)
--
-- Its files are looked for in $DLOGM_DATA first, then in this folder: an app that ships the plugin read-only
-- (ActionFootagePlayer) keeps what it can't ship there, DJI's LUT and the maker shader made from it, under the same
-- names.
--
-- Everything is set as file-local options, added to the user's own glsl-shaders and vf: mpv restores them when the
-- file ends. Source files are never modified.

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'
local dji_color = require 'dji_color'
local gopro_color = require 'gopro_color'
local video_color = require 'video_color'

local o = { mode = "", hdr_screen = "", sdr_screen = "", denoise = false }
options.read_options(o, "dlogm")

local DIR = mp.get_script_directory()
local DATA = os.getenv("DLOGM_DATA")
-- Where the plugin's files are looked for, in order
local DIRS = {}
if DATA and DATA ~= "" then table.insert(DIRS, DATA) end
table.insert(DIRS, DIR)
-- The log curves, in Ctrl+l's order, with their shaders: to HLG (hdr) and the maker's to Rec.709 (sdr)
local CURVES = {
    { id = "dlogm", name = "D-Log M", hdr = "shaders/dlogm_hlg.glsl", sdr = "shaders/dlogm_lut3d.glsl", maker = "DJI",
      lut = "DJI's D-Log M to Rec.709 LUT 11.17, for the Osmo Action 6, downloaded from DJI's site at setup" },
    { id = "dlog", name = "D-Log", hdr = "shaders/dlog_hlg.glsl" },
    { id = "gplog", name = "GP-Log", hdr = "shaders/gplog_hlg.glsl" },
    { id = "gplog2", name = "GP-Log2", hdr = "shaders/gplog2_hlg.glsl" },
    { id = "ilog", name = "I-Log (10-bit)", hdr = "shaders/ilog_hlg.glsl" },
}
-- NBCU's HDR -> SDR, by the transfer it takes
local NBCU = { hlg = "shaders/nbcu_hlg2sdr.glsl", pq = "shaders/nbcu_pq2sdr.glsl" }
-- The noise reduction's shader
local DENOISE = "shaders/denoise.glsl"
-- The dv look's Dolby Vision RPU (made by afp-rpu from dv/dji_fit.json, tools/make_dv_rpu.sh) and its name
local DV_RPU = "dv/dji_fit.rpu"
local DV_LOOK = "dv/dji_fit.json"
local DV_NAME = "DJI fit"
-- The user's own trims, when changed (dv-trim): the look as JSON and its RPU, made by afp-rpu (the gyroflow plugin's,
-- beside this one), in two files taken in turn so mpv sees a new one each time
local DV_USER = (DATA and DATA ~= "") and utils.join_path(DATA, "dv") or mp.command_native({ "expand-path", "~~/dlogm-dv" })
local AFP_RPU = utils.join_path(utils.join_path(DIR, ".."), "gyroflow/afp-rpu")
-- The trims the panel offers, as DV Lab has them (12-bit, 2048 neutral)
local DV_TRIMS = { "slope", "offset", "power", "midContrast", "clipTrim", "saturation", "chromaWeight", "l3Min", "l3Avg", "l3Max" }
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
-- The dv look's, where mpv has them (the app's mpv on macOS): presenting through macOS's video layer, with an RPU
local DV_OK = mp.get_property("option-info/macos-sample-buffer-layer/name") ~= nil
if DV_OK then
    table.insert(TARGET_OPTIONS, "macos-sample-buffer-layer")
    table.insert(TARGET_OPTIONS, "macos-dovi-rpu-file")
end
-- The filter that tags the video as what the look's shaders turn it into, at the end of the chain (after the
-- stabilization); disabled when the file's own tags are right
local TAG_LABEL = "dlogm"
local HLG_TAGS = { gamma = "hlg", primaries = "bt.2020" }
local WINDOWS = package.config:sub(1, 1) == "\\"
local SDR_TAGS = { gamma = "bt.1886", primaries = "bt.709" }
-- For NBCU's on a file tagged HDR: its base layer, without Dolby Vision's reshaping, which would make it PQ (Dolby
-- Vision 8.4, as VideoToolbox writes HLG, the player's own exports among them, is HLG underneath)
local SDR_TAGS_NO_DOVI = { gamma = "bt.1886", primaries = "bt.709", dolbyvision = "no", colormatrix = "bt.2020-ncl" }
local SDR_LOOKS = { maker = true, nbcu = true, tonemap = true }
-- What a kind of screen's choice can be
local CHOICES = { hdr = true, maker = true, nbcu = true, tonemap = true, dv = true }
-- The HDR transfers, by mpv's names for them (target-trc, video-dec-params/gamma)
local HDR_NAMES = { hlg = "HLG", pq = "PQ" }

local function choice(v, default)
    return CHOICES[v] and v or default
end
-- What log files start in on each kind of screen: dlogm.conf's, then the session's choices
local old = o.mode == "sdr" and "maker" or nil
-- On an SDR screen at first: Dolby Vision as macOS shows it where mpv can present through macOS (the app's mpv), else
-- mpv's tone mapping (Homebrew's mpv; on Windows the hdr look is that too, the screen's kind unknown there)
local chosen = { hdr = choice(o.hdr_screen, old or "hdr"), sdr = choice(o.sdr_screen, old or (DV_OK and "dv" or "tonemap")) }
local denoise = o.denoise
local platform = mp.get_property("platform") or ""
-- The screens' HDR headroom by name (macOS's EDR: above 1 where it shows HDR; false: one it didn't know), and the
-- menu bar's screen, where a window opens; nil until known, {} when it can't be
local screens, main_screen = nil, nil
local waiting = {}
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
-- The kind of screen the file's look was chosen for
local shown_for = nil

-- The name of the screen the window is on, as macOS has it: mpv's display-names add a number, "Built-in Retina
-- Display (4251086178)"
local function window_screen()
    local name = (mp.get_property_native("display-names") or {})[1]
    return name and name:gsub("%s*%(%d+%)$", "")
end

-- The kind of screen the window is on (before there's a window, the one it opens on): "hdr", "sdr", or nil when it
-- can't tell
local function screen_kind()
    if not screens then return nil end
    local name = window_screen()
    if not name or name == "" then
        local wanted = mp.get_property("screen-name") or ""
        name = wanted ~= "" and wanted or main_screen
    end
    local headroom = name and screens[name]
    if not headroom then return nil end
    return headroom > 1 and "hdr" or "sdr"
end

-- The same, an unknown one counting as HDR: the display is asked for HDR and the system tone-maps it when it isn't
local function screen()
    return screen_kind() or "hdr"
end

local function label(m)
    local text
    if hdr and not curve then
        text = "HDR video (" .. HDR_NAMES[hdr.trc] .. ")"
        if m == "nbcu" then
            text = text .. " -> SDR (NBCU)"
        elseif m == "tonemap" then
            text = text .. " tone-mapped to SDR by mpv"
        elseif m == "dv" then
            text = text .. " with its Dolby Vision, as macOS shows it"
        else
            text = text .. ", as it is"
        end
    else
        local c = curve or CURVE.dlogm
        if m == "hdr" then
            text = c.name .. " -> HDR"
        elseif m == "maker" then
            text = c.name .. " -> SDR (" .. (c.maker or "the maker") .. "'s LUT)"
        elseif m == "nbcu" then
            text = c.name .. " -> SDR (NBCU)"
        elseif m == "tonemap" then
            text = c.name .. " -> HDR, tone-mapped to SDR by mpv"
        elseif m == "dv" then
            text = c.name .. " -> HDR + Dolby Vision (" .. DV_NAME .. "), as macOS shows it"
        else
            return c.name .. " handling off: the file as it is"
        end
    end
    local kind = screen_kind()
    if m == "dv" then return text .. (kind == "sdr" and " on this SDR screen" or kind == "hdr" and " on this HDR screen" or "") end
    if kind == "sdr" and (m == "hdr" or m == "off") then return text .. ", tone-mapped by macOS for this SDR screen" end
    if kind == "hdr" and SDR_LOOKS[m] then return text .. ", as an SDR screen shows it" end
    return text
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

-- The looks this file has, in Ctrl+s's order: hdr and dv (the system tones them down), mpv's tone mapping, then
-- the SDR ones whose shaders are there
local function looks()
    if not base then return {} end
    if hdr and not curve then
        local list = { "hdr" }
        if DV_OK and hdr.dv_signaled and hdr.trc == "hlg" then table.insert(list, "dv") end
        table.insert(list, "tonemap")
        if hdr.prim == "bt.2020" and not hdr.dovi and plugin_file(NBCU[hdr.trc]) then
            table.insert(list, "nbcu")
        end
        return list
    end
    if not curve then return {} end
    local list = { "hdr" }
    if DV_OK and plugin_file(DV_RPU) then table.insert(list, "dv") end
    table.insert(list, "tonemap")
    if plugin_file(NBCU.hlg) then table.insert(list, "nbcu") end
    if curve.sdr and plugin_file(curve.sdr) then table.insert(list, "maker") end
    return list
end

-- The look for a choice: itself when the file has it, else for an SDR one another SDR look (NBCU's first: the
-- neutral one), else hdr
local function resolve(want)
    local has = {}
    for _, l in ipairs(looks()) do has[l] = true end
    if has[want] then return want end
    if SDR_LOOKS[want] or want == "dv" then
        for _, l in ipairs({ "nbcu", "maker", "tonemap" }) do
            if has[l] then return l end
        end
    end
    return "hdr"
end

-- The dv look's trims: the shipped look (DJI fit), or the user's, read once
local dv_look, dv_user_rpu = nil, nil
local function read_json(path)
    local f = path and io.open(path, "r")
    if not f then return nil end
    local j = utils.parse_json(f:read("*a") or "")
    f:close()
    return j
end
local function load_dv_look()
    if dv_look then return dv_look end
    local user = read_json(utils.join_path(DV_USER, "look.json"))
    dv_look = user or read_json(plugin_file(DV_LOOK)) or {}
    if user and user.rpu then
        local rpu = utils.join_path(DV_USER, "look." .. user.rpu .. ".rpu")
        if file_exists(rpu) then dv_user_rpu = rpu end
    end
    return dv_look
end
local function dv_trims()
    if not DV_OK then return {} end
    local look, t = load_dv_look(), {}
    for _, k in ipairs(DV_TRIMS) do t[k] = look[k] or 2048 end
    return t
end

-- For the right-click menu (mpv/menu.conf)
local function publish()
    local c = curve or CURVE.dlogm
    local sdr_look = ""
    if curve then
        sdr_look = (current == "maker" or current == "nbcu") and current
            or resolve((chosen.sdr == "maker" or chosen.sdr == "nbcu") and chosen.sdr or "maker")
        if sdr_look ~= "maker" and sdr_look ~= "nbcu" then sdr_look = "" end
    end
    local state = {
        log = curve ~= nil, recorded = recorded and recorded.id or "", curve = curve and curve.id or "",
        look = current or "", looks = looks(), maker = curve and c.sdr and plugin_file(c.sdr) and c.maker or "",
        sdr_look = sdr_look, screen = screen_kind() or "", hdr = hdr and hdr.trc or "", denoise = denoise, dv = DV_OK,
        maker_lut = curve and c.sdr and plugin_file(c.sdr) and c.lut or "", dv_trims = dv_trims(), dv_custom = dv_user_rpu ~= nil,
        dv_own = hdr ~= nil and not curve,
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

-- The video tagged as `tags` (HLG_TAGS, SDR_TAGS), or as the file says (nil). The filter stays in the chain once
-- added, disabled when not needed. Changing it rebuilds the chain: Gyroflow's filter restarts, as with Ctrl+g
local function tag_video(tags)
    local vf = mp.get_property_native("vf") or {}
    local tag = nil
    for _, f in ipairs(vf) do
        if f.label == TAG_LABEL then tag = f end
    end
    if not tag then
        if not tags then return end
        tag = { name = "format", label = TAG_LABEL }
        table.insert(vf, tag)
    elseif not tags then
        if tag.enabled == false then return end
    elseif tag.enabled ~= false and tag.params and tag.params.gamma == tags.gamma and tag.params.primaries == tags.primaries
           and tag.params.dolbyvision == tags.dolbyvision then
        return
    end
    if tags then tag.params = tags end
    tag.enabled = tags ~= nil
    mp.set_property_native("file-local-options/vf", vf)
end

-- The target for HDR: the display switched to HDR (vo=gpu-next) and the signal sent to it encoded as `trc`, so mpv
-- passes HDR video in it through as it is. 1000 nits, as the hdr shader's HLG is graded for.
-- On Windows only the hint: the display is in HDR or not as Windows has it (Settings > Display > Use HDR), and an app
-- can't switch it. mpv sends HDR to one that is and tone-maps for one that isn't; HLG sent as it is to an SDR display
-- shows flat, its BT.2020 colors read as BT.709 and washed out
local function hdr_target(target, trc, prim)
    target["target-colorspace-hint"] = "yes"
    if WINDOWS then return end
    target["target-trc"] = trc
    target["target-prim"] = prim
    target["target-peak"] = "1000"
end

-- The dv look: macOS's video layer presents, with `rpu` on the frames (nil: the file's own)
local function dv_target(target, rpu)
    target["macos-sample-buffer-layer"] = "yes"
    target["macos-dovi-rpu-file"] = rpu or ""
end

-- tonemap: SDR as mpv makes it, the display left in SDR
local function sdr_target(target)
    target["target-colorspace-hint"] = "no"
end

-- The file's shaders: the user's own, the noise reduction's when it's on, and the look's (paths, in order)
local function shader_list(look)
    local shaders = {}
    for _, s in ipairs(base.shaders) do table.insert(shaders, s) end
    if denoise then
        local shader = plugin_file(DENOISE)
        if shader then
            table.insert(shaders, shader)
        else
            mp.msg.warn("missing " .. DENOISE .. " in " .. table.concat(DIRS, " and ") .. " (run install.sh)")
        end
    end
    for _, s in ipairs(look or {}) do table.insert(shaders, s) end
    return shaders
end

local function set_target(target)
    for _, opt in ipairs(TARGET_OPTIONS) do
        mp.set_property("file-local-options/" .. opt, target[opt])
    end
end

-- The plugin's shaders by name, as paths; nil (and why, on screen) when one isn't installed
local function shader_paths(names, m)
    local paths = {}
    for _, name in ipairs(names) do
        local path = plugin_file(name)
        if not path then
            mp.msg.warn("missing " .. name .. " in " .. table.concat(DIRS, " and ") .. " (run install.sh)")
            mp.osd_message(label(m) .. ": its shader isn't installed", 3)
            return nil
        end
        table.insert(paths, path)
    end
    return paths
end

-- An HDR file, without a log curve: as it is, in HDR (hdr, off), or in SDR by NBCU's conversion (nbcu) or mpv's
-- tone mapping (tonemap)
local function apply_hdr_file(m)
    local target = {}
    for k, v in pairs(base.target) do target[k] = v end
    local shaders = {}
    local tags = nil
    if m == "nbcu" then
        shaders = shader_paths({ NBCU[hdr.trc] }, m)
        if not shaders then return false end
        tags = SDR_TAGS_NO_DOVI
    elseif m == "tonemap" then
        sdr_target(target)
    elseif m == "dv" then
        -- the base layer as it is, its RPU for macOS (mpv would reshape it into PQ itself otherwise)
        hdr_target(target, hdr.trc, hdr.prim)
        dv_target(target, nil)
        tags = { gamma = hdr.trc, primaries = hdr.prim, dolbyvision = "no", colormatrix = "bt.2020-ncl" }
    else
        m = "hdr"
        hdr_target(target, hdr.trc, hdr.prim)
    end
    tag_video(tags)
    mp.set_property_native("file-local-options/glsl-shaders", shader_list(shaders))
    set_target(target)
    current = m
    shown_for = screen()
    publish()
    return true
end

local function apply(m)
    if not base then return false end
    if hdr and not curve then return apply_hdr_file(m) end
    local c = curve or CURVE.dlogm
    if m == "maker" and not c.sdr then
        mp.osd_message(c.name .. ": no maker's Rec.709 LUT for it yet", 3)
        return false
    end
    local names = { hdr = { c.hdr }, maker = { c.sdr }, nbcu = { c.hdr, NBCU.hlg }, tonemap = { c.hdr }, dv = { c.hdr } }
    local rpu = nil
    if m == "dv" then
        rpu = dv_user_rpu or plugin_file(DV_RPU)
        if not (DV_OK and rpu) then
            mp.osd_message(label(m) .. ": not with this mpv", 3)
            return false
        end
    end
    local shaders = shader_paths(names[m] or {}, m)
    if not shaders then return false end
    local target = {}
    for k, v in pairs(base.target) do target[k] = v end
    if m == "hdr" or m == "dv" then
        hdr_target(target, "hlg", "bt.2020")
        if m == "dv" then dv_target(target, rpu) end
    elseif m == "tonemap" then
        sdr_target(target)
    elseif m == "off" and hdr then
        -- off on an HDR file graded by hand: as it is, in HDR
        hdr_target(target, hdr.trc, hdr.prim)
    end
    local hlg = m == "hdr" or m == "tonemap" or m == "dv"
    tag_video(hlg and HLG_TAGS or (m == "maker" or m == "nbcu") and SDR_TAGS or nil)
    mp.set_property_native("file-local-options/glsl-shaders", shader_list(shaders))
    set_target(target)
    current = m
    shown_for = screen()
    publish()
    return true
end

-- The screens' headroom, asked of macOS once (and again for a screen it didn't know), then `done`
local SCREENS_JS = [[ObjC.import("AppKit"); var s = $.NSScreen.screens, o = [];
for (var i = 0; i < s.count; i++) { var x = s.objectAtIndex(i);
    o.push(x.localizedName.js + "\t" + x.maximumPotentialExtendedDynamicRangeColorComponentValue) }
o.join("\n")]]
local asking = false
local function ask_screens(done)
    if done then table.insert(waiting, done) end
    if asking then return end
    local function finish(found, main)
        asking = false
        -- A screen it still doesn't know stays unknown, not asked about again
        for name, headroom in pairs(screens or {}) do
            if headroom == false and found[name] == nil then found[name] = false end
        end
        screens, main_screen = found, main
        local w = waiting
        waiting = {}
        for _, f in ipairs(w) do f() end
    end
    if platform ~= "darwin" then return finish({}, nil) end
    asking = true
    -- osascript answers in a few tenths of a second; a file waits for it at most this long
    local timer = mp.add_timeout(1.5, function()
        if asking then
            mp.msg.warn("couldn't tell the screens' HDR: they count as HDR")
            finish(screens or {}, main_screen)
        end
    end)
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true,
                              args = { "osascript", "-l", "JavaScript", "-e", SCREENS_JS } }, function(ok, r)
        if not asking then return end
        timer:kill()
        local found, main = {}, nil
        if ok and r.status == 0 then
            for line in (r.stdout or ""):gmatch("[^\n]+") do
                local name, headroom = line:match("^(.-)\t([%d.]+)%s*$")
                if name then
                    found[name] = tonumber(headroom)
                    main = main or name
                    mp.msg.verbose("screen " .. name .. ": EDR headroom " .. headroom)
                end
            end
        end
        finish(found, main)
    end)
end
ask_screens()

-- After the file is opened (its metadata is known), before the video output is set up for it
mp.add_hook("on_preloaded", 50, function(hook)
    base, curve, current, announce, recorded, hdr, touched, shown_for = nil, nil, nil, false, nil, nil, false, nil
    local path = mp.get_property("path")
    if not path then return publish() end
    base = { shaders = mp.get_property_native("glsl-shaders") or {}, target = {} }
    for _, opt in ipairs(TARGET_OPTIONS) do base.target[opt] = mp.get_property(opt) end
    local trc, prim
    if not path:find("://", 1, true) then trc, prim = video_color.read(path) end
    if trc then
        -- Log is recorded tagged as SDR: what an HDR file's camera metadata says (an export's) isn't read
        hdr = { trc = trc, prim = prim or "bt.2020", tagged = true }
        -- Signaled as Dolby Vision (a dvvC box): Apple shows it with its RPU, the dv look does too
        for _, track in ipairs(mp.get_property_native("track-list") or {}) do
            if track.type == "video" and track.selected and track["dolby-vision-profile"] then hdr.dv_signaled = true end
        end
    else
        recorded = recorded_curve(path)
        curve = recorded
        if not curve then
            mp.msg.verbose("not log footage it knows")
            if denoise then mp.set_property_native("file-local-options/glsl-shaders", shader_list()) end
            return publish()
        end
    end
    -- In the look chosen for the screen, once it's known which kind that is
    local function start()
        announce = apply(resolve(chosen[screen()]))
        if announce then mp.msg.info(label(current)) end
    end
    if screens then return start() end
    hook:defer()
    ask_screens(function()
        if base then start() end
        hook:cont()
    end)
end)

-- An HDR file whose container didn't say (not MP4/MOV, or no colr box), from the decoder once it starts: in HDR as
-- it is from then on, unless the user has chosen otherwise. The decoder's, not video-params: the hdr mode's filter
-- tags the video as HLG after it
mp.observe_property("video-dec-params", "native", function(_, p)
    if not base or not p or not HDR_NAMES[p.gamma] then return end
    -- Dolby Vision without a base layer in the container's transfer (profile 5: no colr box) is only seen here, and
    -- only mpv's tone mapping takes it
    if hdr and not curve and p.colormatrix == "dolbyvision" and not hdr.tagged and not hdr.dovi then
        hdr.dovi = true
        if current == "nbcu" then apply("tonemap") end
        return publish()
    end
    if hdr or touched then return end
    hdr = { trc = p.gamma, prim = p.primaries or "bt.2020", dovi = p.colormatrix == "dolbyvision" }
    if curve then mp.msg.info(curve.name .. " in the metadata, but the video is tagged " .. HDR_NAMES[hdr.trc] .. ": not log") end
    recorded, curve = nil, nil
    if apply(resolve(chosen[screen()])) then
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

-- A look for this file, `sticky`: the one files start in on this kind of screen from now on (an SDR one, on an SDR
-- screen too, unless that's set to hdr)
local function choose(m, sticky)
    if not base then return end
    touched = true
    if hdr and not curve and m == "off" then m = "hdr" end
    if current == m then
        mp.osd_message(label(m), 1.5)
        return
    end
    if not apply(m) then return end
    if sticky then
        local c = m
        if CHOICES[c] then
            chosen[screen()] = c
            if c ~= "hdr" and chosen.sdr ~= "hdr" then chosen.sdr = c end
        end
        publish()
    end
    mp.osd_message(label(m), 2)
end

-- Ctrl+s: the file's next look
local function next_look()
    if not base then return end
    local list = looks()
    if #list == 0 then
        mp.osd_message("Not log, as the camera says: the file as it is (Ctrl+l picks a curve)", 2)
        return
    end
    local i = 0
    for k, l in ipairs(list) do
        if l == current then i = k end
    end
    choose(list[i % #list + 1], true)
end

-- The look to keep when the curve changes: the one the file is in, else what this screen starts in
local function kept_look()
    return resolve(current and current ~= "off" and current or chosen[screen()])
end

-- The next curve for this file, in the look it's in (or the screen's, when it was off or untouched)
local function next_curve()
    if not base then return end
    touched = true
    local i = 0
    for k, c in ipairs(CURVES) do
        if c == curve then i = k end
    end
    curve = CURVES[i % #CURVES + 1]
    local m = kept_look()
    if apply(m) then mp.osd_message(label(m), 2) end
end

-- A file not switched by hand follows its window to a screen of the other kind; a screen macOS hasn't been asked
-- about yet (just connected) is asked about first
local function follow_screen()
    if not screens then return end
    local name = window_screen()
    if platform == "darwin" and name and name ~= "" and screens[name] == nil and not asking then
        screens[name] = false
        return ask_screens(follow_screen)
    end
    if not base or not current or touched or screen() == shown_for then return end
    local m = resolve(chosen[screen()])
    if m ~= current and apply(m) then
        mp.osd_message(label(m), 2)
    else
        shown_for = screen()
        publish()
    end
end
mp.observe_property("display-names", "native", follow_screen)

-- The noise reduction on or off: the file's look applied again with it, or without one (not log), the shader alone
local function toggle_denoise()
    if not base then return end
    denoise = not denoise
    if current then
        apply(current)
    else
        mp.set_property_native("file-local-options/glsl-shaders", shader_list())
        publish()
    end
    mp.osd_message(denoise and "Noise reduction on" or "Noise reduction off", 1.5)
end

mp.add_key_binding("Ctrl+s", "sdr", next_look)
mp.add_key_binding("Ctrl+h", "hdr", function() choose("hdr", true) end)
mp.add_key_binding("L", "off", function() choose("off", false) end)
mp.add_key_binding("Ctrl+l", "curve", next_curve)
mp.add_key_binding("Ctrl+n", "denoise", toggle_denoise)

-- This file's curve by name, or the one its metadata says (auto: for a file it says isn't log, as it is again; an
-- HDR file in HDR)
mp.register_script_message("curve", function(id)
    if not base then return end
    touched = true
    if id == "auto" and not recorded then
        if hdr then
            curve = nil
            if apply(resolve(chosen[screen()])) then mp.osd_message(label(current), 2) end
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
    local m = kept_look()
    if apply(m) then mp.osd_message(label(m), 2) else curve = before end
end)

-- This file's look by name, as Ctrl+s would choose it
mp.register_script_message("look", function(m)
    for _, l in ipairs(looks()) do
        if l == m then return choose(m, true) end
    end
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
        mp.command_native({ name = "subprocess", playback_only = false,
                            args = WINDOWS and { "cmd", "/d", "/c", "mkdir", dir } or { "mkdir", "-p", dir } })
    end
    f = io.open(path, "w")
    if not f then return false end
    f:write(table.concat(lines, "\n") .. "\n")
    f:close()
    return true
end

-- The dv look's trims changed (from the side panel): the look written with them, its RPU made again by afp-rpu,
-- and the dv look shown with it. Coalesced: while afp-rpu runs, later changes wait, only the last is made
local dv_busy, dv_again, dv_timer = false, false, nil
local function make_dv_rpu()
    if dv_busy then dv_again = true; return end
    if not file_exists(AFP_RPU) then return mp.msg.warn("no afp-rpu at " .. AFP_RPU) end
    dv_busy = true
    local look = load_dv_look()
    local next = look.rpu == "a" and "b" or "a"
    local out = utils.join_path(DV_USER, "look." .. next .. ".rpu")
    local saved = {}
    for k, v in pairs(look) do saved[k] = v end
    saved.rpu = nil
    local tmp = utils.join_path(DV_USER, "look.tmp.json")
    mp.command_native({ name = "subprocess", playback_only = false, args = { "mkdir", "-p", DV_USER } })
    local f = io.open(tmp, "w")
    if not f then dv_busy = false; return mp.msg.warn("can't write " .. tmp) end
    f:write(utils.format_json(saved))
    f:close()
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stderr = true,
                              args = { AFP_RPU, "--look", tmp, "-o", out } }, function(_, r)
        dv_busy = false
        if r and r.status == 0 then
            look.rpu = next
            local lf = io.open(utils.join_path(DV_USER, "look.json"), "w")
            if lf then lf:write(utils.format_json(look)); lf:close() end
            dv_user_rpu = out
            if current == "dv" then apply("dv") else publish() end
        else
            mp.msg.warn("afp-rpu: " .. ((r and r.stderr) or "failed"))
        end
        if dv_again then dv_again = false; make_dv_rpu() end
    end)
end

mp.register_script_message("dv-trim", function(key, value)
    local v = tonumber(value)
    local ok = false
    for _, k in ipairs(DV_TRIMS) do ok = ok or k == key end
    if not (DV_OK and ok and v) then return end
    load_dv_look()[key] = math.floor(math.min(4095, math.max(0, v)) + 0.5)
    publish()
    if dv_timer then dv_timer:kill() end
    dv_timer = mp.add_timeout(0.12, make_dv_rpu)
end)

-- Back to the shipped trims (DJI fit)
mp.register_script_message("dv-reset", function()
    for _, n in ipairs({ "look.json", "look.a.rpu", "look.b.rpu", "look.tmp.json" }) do
        os.remove(utils.join_path(DV_USER, n))
    end
    dv_look, dv_user_rpu = nil, nil
    if current == "dv" then apply("dv") else publish() end
end)

-- The system's tone mapping read again (sdr-white changed how bright macOS takes a screen to be): macOS works it
-- out when a window comes to a screen, so the looks it tones down are shown anew, through the other path for a moment
mp.register_script_message("refresh", function()
    if current == "dv" then
        mp.set_property("file-local-options/macos-sample-buffer-layer", "no")
    elseif current == "hdr" or (current == "off" and hdr) then
        mp.set_property("file-local-options/target-colorspace-hint", "no")
    else
        return
    end
    local m = current
    mp.add_timeout(0.1, function() if current == m then apply(m) end end)
end)

mp.register_script_message("remember", function()
    local saved = save_options({ hdr_screen = chosen.hdr, sdr_screen = chosen.sdr, denoise = denoise and "yes" or "no" })
    local NAMES = { hdr = "HDR", maker = "the maker's SDR", nbcu = "NBCU's SDR", tonemap = "mpv's tone mapping to SDR",
                    dv = "HDR + Dolby Vision" }
    mp.osd_message(saved and ("Log clips start in " .. NAMES[chosen.hdr] .. " on HDR screens, in " .. NAMES[chosen.sdr]
                              .. " on SDR screens, noise reduction " .. (denoise and "on" or "off") .. " (dlogm.conf)")
                   or "Couldn't write dlogm.conf", 3)
end)
