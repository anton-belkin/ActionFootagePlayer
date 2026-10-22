-- Steady Player: plays videos in mpv with this project's setup (Gyroflow stabilization, D-Log M as HDR).
-- It's listed in Finder's Open With for videos but never takes them over: becoming the default player is the
-- router's job (router.applescript), and only when that's turned on.
--
-- mpv is started through the launcher app inside this one (Contents/Resources/mpv.app), with `open -na`:
-- started from `do shell script ... &`, mpv stayed tied to this applet, which quits right away, and macOS ran
-- it as a background process (hardware decoding and the GPU throttled, a stall at start).

on mpvLauncher()
	return (POSIX path of (path to me)) & "Contents/Resources/mpv.app"
end mpvLauncher

on open theFiles
	set args to ""
	repeat with f in theFiles
		set args to args & " " & quoted form of (POSIX path of f)
	end repeat
	do shell script "/usr/bin/open -na " & quoted form of my mpvLauncher() & " --args" & args
end open

on run
	do shell script "/usr/bin/open -na " & quoted form of my mpvLauncher() & " --args --player-operation-mode=pseudo-gui"
end run
