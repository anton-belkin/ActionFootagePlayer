-- Routes video files between mpv (HDR D-Log) and IINA (everything else).
-- DJI Osmo's D-Log mode names files *_D.MP4 / *_D.MOV. Those go to mpv,
-- which our ~/.config/mpv/scripts/dlog_auto.lua then auto-promotes to the
-- dji_hdr profile (HOOK OUTPUT shader + HLG Metal layer). Other files
-- open in IINA.

on open theFiles
	repeat with f in theFiles
		set posixPath to POSIX path of f
		set lowerPath to do shell script "echo " & quoted form of posixPath & " | tr 'A-Z' 'a-z'"
		if (lowerPath ends with "_d.mp4") or (lowerPath ends with "_d.mov") then
			-- As an app of its own (~/Applications/mpv.app): mpv run from `do shell script ... &` stays tied to
			-- this applet, which quits, and macOS throttles it as a background process (slow start, stutter)
			set mpvApp to (POSIX path of (path to applications folder from user domain)) & "mpv.app"
			do shell script "/usr/bin/open -na " & quoted form of mpvApp & " --args " & quoted form of posixPath
		else
			do shell script "/usr/bin/open -a IINA " & quoted form of posixPath
		end if
	end repeat
end open


