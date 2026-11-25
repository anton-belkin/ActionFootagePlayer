-- The temporal denoise's switch (Ctrl+p, on or off: the clip's transform keeps it; still=yes in gyroflow.conf; none
-- with temporal_denoise=off, this computer's Performance & Quality, whatever Ctrl+p says).
--
-- The denoise itself is the warp's (main.lua's GPU path, warp.glsl's dn_ passes): each frame cleaned of its noise from
-- the frames on each side of it, while it plays and paused. This module keeps the switch and publishes it as
-- user-data/gyroflow/still, which main.lua watches to write the warp again with the denoise in it or out.
--
-- Loaded by main.lua, which gives it its options and hears when Ctrl+p switched it (init).

local mp = require 'mp'
local platform = require 'platform'

local M = {}

local o = nil
local enabled = true

-- On: Ctrl+p's switch, where this computer does the temporal denoise (temporal_denoise not off)
local function on() return enabled and o ~= nil and o.temporal_denoise ~= "off" end
local published = nil
local function publish()
    published = on()
    mp.set_property_native("user-data/gyroflow/still", published)
end

-- Ctrl+p switched it (main.lua's: the clip's transform keeps it)
local switched_by_hand = nil

-- options: main.lua's (still, temporal_denoise); by_hand(on): Ctrl+p switched it
function M.init(options, by_hand)
    o, switched_by_hand = options, by_hand
    enabled = o.still
    publish()
end

-- The temporal denoise as it is: on (Ctrl+p's switch, and this computer does it); Ctrl+p's switch alone
function M.enabled() return on() end
function M.switched() return enabled end

local function changed()
    if published ~= on() then publish() end
end

mp.add_key_binding("Ctrl+p", "still", function()
    if platform.photo_shown() then return end
    if o.temporal_denoise == "off" then
        return mp.osd_message("Temporal denoise: off on this computer (Performance & Quality)", 2)
    end
    enabled = not enabled
    mp.osd_message("Temporal denoise: " .. (enabled and "on" or "off") .. " for this clip", 1.5)
    changed()
    if switched_by_hand then switched_by_hand(enabled) end
end)

-- temporal_denoise changed (main.lua, Performance & Quality): off, or on again
function M.available_changed() changed() end

-- The clip's transform says on or off (main.lua's take: a clip's saved bundle, else the session's)
function M.set(v)
    v = v == true
    if v == enabled then return end
    enabled = v
    if o then changed() end
end

return M
