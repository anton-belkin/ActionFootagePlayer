-- Started by Finder (macos/player.sh loads this then), the app is up with an empty window while the videos' Apple
-- Events come (pseudo-gui). As the first file starts, that window goes, and the video gets a window of its own, as
-- when the app is started with a video.
--
-- In HDR the window's Metal layer switches to HLG with the first frame. MoltenVK makes that switch from mpv's render
-- thread, outside any Core Animation transaction, and it only reaches the screen with one of the main thread's: a
-- new window's. On the empty window, already on screen, it went with the window's resize to the video's size, when
-- the two happened together: about one clip in five stayed at SDR brightness (the layer HDR, the screen given no
-- headroom), until it was opened again.
local done = false
mp.register_event("start-file", function()
    if done then return end
    done = true
    -- Without a forced window, mpv closes the empty one now and opens one with the video. Files after it each get a
    -- window of their own too
    mp.set_property_native("force-window", false)
end)
