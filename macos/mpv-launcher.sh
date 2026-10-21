#!/bin/sh
# Starts mpv as an app of its own, for DJI Router.app (`open -na mpv.app --args <file>`).
# mpv started from the router's `do shell script ... &` stayed attached to the applet, which quits right
# away, and macOS ran it as a background process: hardware decoding and the GPU throttled, a 4-5 s
# stall at start and stabilized playback at a fraction of realtime.
exec @MPV@ "$@"
