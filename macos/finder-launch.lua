-- Started by Finder, the app waits for the videos' Apple Events without a window (macos/player.sh: pseudo-gui with
-- force-window=no, and this script): the window comes up with the video, as when it's started with one. Up before
-- it, the window's Metal layer is made for SDR and switched to HDR (the dlogm plugin's target) once the video is
-- there, on a window already on screen, which now and then doesn't take: the clip played at SDR brightness until it
-- was opened again. Opened by itself, or with a file without video, the app gets its window here.

-- How long Finder's videos may take to come
local WAIT = 1

mp.add_timeout(WAIT, function()
    if mp.get_property_native("idle-active") then mp.set_property_native("force-window", true) end
end)

mp.register_event("file-loaded", function()
    if not mp.get_property_native("current-tracks/video") then mp.set_property_native("force-window", true) end
end)
