-- ActionFootagePlayer's setup, offered at the app's first launch (macos/player.sh loads this then): the default app
-- for MP4 videos, DJI's LUT and the Finder Quick Actions (setup.sh all, beside this in Contents/Resources/).
-- Asked from inside mpv, once it runs as the app: an osascript started before it would be taken for the app by
-- macOS, and get the videos opened with it.
--
-- Answered (Set Up or Not Now), it's marked as offered (setup.sh marks it too) and isn't asked again; the menu's
-- Support › Set up again… offers it later. Started from the disk image (AFP_SETUP_MOVE, from player.sh), it can't be
-- set up there: it says to drag the app to Applications, and leaves the mark alone, so it asks once it's opened from
-- there.

-- Its folder: mpv names the chunk "@<path>"
local res = debug.getinfo(1, "S").source:match("^@(.*)/")
local data = (os.getenv("HOME") or "") .. "/Library/Application Support/ActionFootagePlayer"

local DIALOG = [[
on run {theMessage, iconPath, okButton}
    set theButtons to {"OK"}
    if okButton is not "OK" then set theButtons to {"Not Now", okButton}
    activate
    return button returned of (display dialog theMessage buttons theButtons default button okButton ¬
        with title "Action Footage Player" with icon (POSIX file iconPath as alias))
end run]]

local function run(args, done)
    mp.command_native_async({name = "subprocess", args = args, playback_only = false,
        capture_stdout = true, capture_stderr = true}, function(_, r)
        done(((r and r.stdout or "") .. (r and r.stderr or "")):gsub("%s+$", ""), r and r.status or -1)
    end)
end

local function dialog(message, ok_button, done)
    run({"osascript", "-e", DIALOG, message, res .. "/AppIcon.icns", ok_button}, done)
end

local function mark()
    os.execute("mkdir -p '" .. data:gsub("'", "'\\''") .. "'")
    local f = io.open(data .. "/setup-offered", "w")
    if f then f:close() end
end

local MOVE = "To finish installing, drag Action Footage Player into the Applications folder, then open it from " ..
    "there.\n\nIt's running from the disk image now, where it can't be set up."

local QUESTION = "Open your camera's videos with Action Footage Player?\n\n" ..
    "It becomes the app for MP4 videos: double-click a clip from your camera, and it plays here in color and " ..
    "steady. macOS asks you to confirm.\n\n" ..
    "It also downloads DJI's own color look from DJI, and adds Play Stabilized to Finder's Quick Actions."

-- What setup.sh said, in a word for the user: all set, set once macOS's dialog is confirmed, or what went wrong
local function summary(report, status)
    if status ~= 0 or report:find("Couldn't", 1, true) then
        return "Not everything could be set up:\n\n" .. report
    end
    local done = "All set. Double-click a clip from your camera: it opens here."
    if report:find("once you confirm", 1, true) then
        done = done .. "\n\nIf macOS asks whether to use Action Footage Player for MP4 videos, confirm it."
    end
    return done
end

-- A video opened with the app waits while it asks
local paused = mp.get_property_native("pause")
mp.set_property_native("pause", true)
if os.getenv("AFP_SETUP_MOVE") then
    dialog(MOVE, "OK", function() mp.set_property_native("pause", paused) end)
    return
end
dialog(QUESTION, "Set Up", function(choice)
    mp.set_property_native("pause", paused)
    if choice == "Set Up" then
        run({res .. "/setup.sh", "all"}, function(report, status)
            mark()
            dialog(summary(report, status), "OK", function() end)
        end)
    elseif choice == "Not Now" then
        mark()
    end
end)
