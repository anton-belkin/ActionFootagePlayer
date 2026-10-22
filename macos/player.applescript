-- Steady Player: plays videos in mpv with this project's plugins, which it carries (Contents/Resources/plugins/):
-- D-Log M as HDR, Gyroflow stabilization. Opened on its own, it offers its setup (Contents/Resources/setup.sh):
-- the default app for MP4 videos, the Finder Quick Actions and DJI's LUT.
--
-- mpv is started through the launcher app inside this one (Contents/Resources/mpv.app), with `open -na`:
-- started from `do shell script ... &`, mpv stayed tied to this applet, which quits right away, and macOS ran
-- it as a background process (hardware decoding and the GPU throttled, a stall at start).

on resourcesPath()
	return (POSIX path of (path to me)) & "Contents/Resources/"
end resourcesPath

-- Downloaded, the whole app carries the quarantine flag, and macOS would refuse the launcher and the plugin's
-- binaries even once the app itself is allowed
on clearQuarantine()
	try
		do shell script "/usr/bin/xattr -dr com.apple.quarantine " & quoted form of (POSIX path of (path to me))
	end try
end clearQuarantine

on startMpv(args)
	clearQuarantine()
	do shell script "/usr/bin/open -na " & quoted form of (resourcesPath() & "mpv.app") & " --args" & args
end startMpv

on open theFiles
	set args to ""
	repeat with f in theFiles
		set args to args & " " & quoted form of (POSIX path of f)
	end repeat
	startMpv(args)
end open

on run
	set question to "Steady Player plays videos in mpv: D-Log M in HDR, clips with gyro data stabilized." & return & return & ¬
		"Set it up? It becomes the default app for MP4 videos, Finder gets the Quick Actions Play Stabilized and " & ¬
		"Stabilize + LUT, and DJI's Rec.709 LUT is downloaded from DJI."
	set choice to button returned of (display dialog question buttons {"Cancel", "Open mpv", "Set Up"} ¬
		default button "Set Up" cancel button "Cancel" with title "Steady Player")
	if choice is "Set Up" then
		try
			set report to do shell script quoted form of (resourcesPath() & "setup.sh") & " all 2>&1"
			display dialog report buttons {"OK"} default button 1 with title "Steady Player"
		on error errText
			display alert "Steady Player's setup didn't finish" message errText as critical
		end try
	else
		startMpv(" --player-operation-mode=pseudo-gui")
	end if
end run
