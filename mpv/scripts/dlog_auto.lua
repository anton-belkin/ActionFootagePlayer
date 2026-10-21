-- Auto-apply dji_sdr or dji_hdr on file load.
--
-- Triggers on DJI D-Log M source detection:
--   filename ends with _D.MP4 / _D.MOV  (DJI Osmo convention)
--   OR container "encoder" / "make" metadata contains "DJI"
--
-- Branches by host renderer:
--   mpv standalone (vo=gpu-next, Vulkan)  ->  dji_hdr
--     HOOK OUTPUT shader bypasses mpv's color pipeline; target-trc=hlg +
--     target-colorspace-hint=yes engages macOS HDR Metal layer.
--     On an SDR display, macOS tone-maps the HLG output down, which looks
--     better than an explicit SDR fallback.
--   IINA (vo=libmpv, OpenGL)              ->  dji_sdr
--     IINA can't engage macOS HDR mode through its libmpv-OpenGL render
--     context, so use the HOOK MAIN SDR shader and stay in SDR.
--
-- Source files are never modified -- no retag is needed.

local mp = require 'mp'

local function is_dlogm()
    local path = (mp.get_property("path") or ""):lower()
    if path:match("_d%.mp4$") or path:match("_d%.mov$") then return true end
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

local applied_path = nil

local function apply()
    local path = mp.get_property("path")
    if not path or path == applied_path then return end
    if not is_dlogm() then
        applied_path = path
        mp.msg.info("dlog_auto: not D-Log M, no profile applied")
        return
    end
    applied_path = path
    local in_iina = (mp.get_property("current-vo") == "libmpv")
    if in_iina then
        mp.commandv("apply-profile", "dji_sdr")
        mp.osd_message("D-Log M -> SDR (IINA)", 1.5)
        mp.msg.info("dlog_auto: applied dji_sdr (IINA)")
    else
        mp.commandv("apply-profile", "dji_hdr")
        mp.osd_message("D-Log M -> HDR (mpv standalone)", 1.5)
        mp.msg.info("dlog_auto: applied dji_hdr (mpv standalone)")
    end
end

mp.register_event("file-loaded", function()
    applied_path = nil
    apply()
end)
