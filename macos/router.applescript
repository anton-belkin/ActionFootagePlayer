-- Steady Router: the default player for MP4/MOV when router mode is on (`gyroflow-router on`). Decides by the
-- first file it's given, once: one of ours (D-Log M, or something to stabilize it with) opens with Steady Player,
-- which then plays the rest of the folder itself; anything else opens with the player that was the default
-- before (or the one chosen in `gyroflow-router on --fallback`). The decision is Contents/Resources/route.sh.

on open theFiles
	set args to ""
	repeat with f in theFiles
		set args to args & " " & quoted form of (POSIX path of f)
	end repeat
	do shell script quoted form of ((POSIX path of (path to me)) & "Contents/Resources/route.sh") & args
end open

on run
	display dialog "Steady Router opens videos with Steady Player or your usual player. Turn it on or off in Terminal:" & return & return & "~/.config/mpv/scripts/gyroflow/gyroflow-router on" & return & "~/.config/mpv/scripts/gyroflow/gyroflow-router off" buttons {"OK"} default button 1
end run
