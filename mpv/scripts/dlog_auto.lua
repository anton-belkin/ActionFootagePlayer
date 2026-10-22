-- Auto-apply dji_hdr (D-Log M -> Rec.2100 HLG, shown in HDR) on file load.
--
-- Triggers on DJI D-Log M source detection:
--   the color profile in the camera's own metadata, read by ~~/gyroflow/gyroflow-probe (gyroflow_auto.lua runs it
--   on load and shares the result; this script runs it itself when gyroflow_auto.lua didn't). DJI tags D-Log M
--   video as BT.709 and names Normal-color clips *_D.MP4 too, so the metadata is the only reliable sign.
--   For files the probe can't tell (other DJI models, no probe installed), the old guess:
--     filename ends with _D.MP4 / _D.MOV  (DJI Osmo convention)
--     OR container "encoder" / "make" metadata contains "DJI"
--
-- dji_hdr's HOOK OUTPUT shader bypasses mpv's color pipeline; target-trc=hlg + target-colorspace-hint=yes
-- engages macOS's HDR Metal layer (vo=gpu-next). On an SDR display, macOS tone-maps the HLG output down, which
-- looks better than an explicit SDR fallback. dji_sdr (DJI's Rec.709 LUT) is there on Ctrl+s (input.conf).
--
-- In a playlist, the profile is undone (profile-restore) when the next file isn't D-Log M.
--
-- Source files are never modified -- no retag is needed.

local mp = require 'mp'
local utils = require 'mp.utils'

local PROBE = mp.command_native({"expand-path", "~~/gyroflow/gyroflow-probe"})
local PROBE_EXTENSIONS = { mp4 = true, mov = true }

-- "dlogm", "normal", ... from the camera's metadata, or nil when the probe can't tell
local function recorded_color(path)
    local shared = mp.get_property_native("user-data/gyroflow-probe") or {}
    if shared.path == path then return shared.color ~= "" and shared.color or nil end
    if path:find("://", 1, true) then return nil end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    local info = utils.file_info(PROBE)
    if not PROBE_EXTENSIONS[ext] or not info or not info.is_file then return nil end
    local r = mp.command_native({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = false })
    local out = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
    return out and out.color or nil
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
    local color = recorded_color(path)
    if color then
        mp.msg.verbose("dlog_auto: recorded in " .. color)
        return color == "dlogm"
    end
    return guessed_dlogm(path)
end

local applied_path = nil
local active_profile = nil

local function set_profile(name)
    if active_profile == name then return end
    if active_profile then mp.commandv("apply-profile", active_profile, "restore") end
    if name then mp.commandv("apply-profile", name) end
    active_profile = name
end

local function apply()
    local path = mp.get_property("path")
    if not path or path == applied_path then return end
    applied_path = path
    if not is_dlogm(path) then
        set_profile(nil)
        mp.msg.info("dlog_auto: not D-Log M, no profile applied")
        return
    end
    set_profile("dji_hdr")
    mp.osd_message("D-Log M -> HDR", 1.5)
    mp.msg.info("dlog_auto: applied dji_hdr")
end

mp.register_event("file-loaded", function()
    applied_path = nil
    apply()
end)
