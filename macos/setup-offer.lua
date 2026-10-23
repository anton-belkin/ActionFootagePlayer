-- ActionFootagePlayer's setup, offered at the app's first launch (macos/player.sh loads this then): DJI's LUT, the
-- default app for MP4 videos and the Finder Quick Actions (setup.sh all, beside this in Contents/Resources/).
-- Asked from inside mpv, once it runs as the app: an osascript started before it would be taken for the app by
-- macOS, and get the videos opened with it.

-- Its folder: mpv names the chunk "@<path>"
local res = debug.getinfo(1, "S").source:match("^@(.*)/")

local DIALOG = [[
on run {theMessage, iconPath, okButton}
    set theButtons to {"OK"}
    if okButton is not "OK" then set theButtons to {"Not Now", okButton}
    activate
    return button returned of (display dialog theMessage buttons theButtons default button okButton ¬
        with title "ActionFootagePlayer" with icon (POSIX file iconPath as alias))
end run]]

local function run(args, done)
    mp.command_native_async({name = "subprocess", args = args, playback_only = false,
        capture_stdout = true, capture_stderr = true}, function(_, r)
        done(((r and r.stdout or "") .. (r and r.stderr or "")):gsub("%s+$", ""))
    end)
end

local function dialog(message, ok_button, done)
    run({"osascript", "-e", DIALOG, message, res .. "/AppIcon.icns", ok_button}, done)
end

local question = "ActionFootagePlayer plays videos in mpv: D-Log M in HDR, clips with gyro data stabilized.\n\n" ..
    "Set it up? It becomes the default app for MP4 videos, Finder gets the Quick Actions Play Stabilized and " ..
    "Stabilize + LUT, and DJI's Rec.709 LUT is downloaded from DJI."

-- A video opened with the app waits while it asks
local paused = mp.get_property_native("pause")
mp.set_property_native("pause", true)
dialog(question, "Set Up", function(choice)
    mp.set_property_native("pause", paused)
    if choice == "Set Up" then
        run({res .. "/setup.sh", "all"}, function(report)
            dialog(report, "OK", function() end)
        end)
    end
end)
