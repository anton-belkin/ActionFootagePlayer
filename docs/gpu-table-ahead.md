# The GPU path's table, ahead of where a clip plays

A clip without a Gyroflow project is stabilized from its own motion data. On the GPU path the player asks the table
server (`gyroflow-gpu-table --serve`, started by `mpv/scripts/gyroflow/main.lua`) for a warp shader with the clip's
table in it. From a NAS the motion data takes seconds to read, so the clip opens with the table of a stretch: the seconds
it starts playing at.

## What went wrong (2026-11-19)

Until this change the player then asked for the **whole clip's** table at once (`"full": true`) and swapped it in when
it came. On a 1.7 GB, 150 s Osmo Action 6 clip on the NAS (journal of Anton's run): first stretch 6.7 s, whole clip
48.2 s. Playback passed the stretch's 20 s long before that, and past the stretch the table holds the stretch's last
correction: the shaking picture turned by a fixed angle, which is the dropout Anton saw. Pausing let the read finish,
so ~10 s later stabilization "came back".

Why the whole clip took that long: an Action 6 clip has a ~550 B motion sample every ~190 KB of video. On a share
`prefetch.rs` reads the file in 8 MB stretches, because the NAS reads a file through ~3× faster than it seeks to each
sample (measured 2026-11-10): so the motion data of the whole clip is **the whole 1.7 GB file**, read while the video
plays from the same share. Measured cold (the clip out of the Mac's cache):

| | first table | whole clip's table | past the stretch, unstabilized |
|---|---|---|---|
| alone | 6.2 s | 36 s | 13.6 s |
| with the video's reads | 5.1 s | 35 s | ~13 s |
| Anton's run (video, catalog, side panel walk) | 6.7 s | 48 s | ~36 s |

While it read, everything else on the share waited behind its 8 MB reads (one SMB connection to the NAS): a `stat` went
from 6 ms to 225 ms (p50, max 553), `afp-catalog footage`/`list` from 0.2 s to 3–7 s, the video's own 1 MB reads to p95
251 ms, max 547 ms.

## Now: extended ahead of where it plays

The stretch is extended while the clip plays, a step at a time, from where it plays on (`mpv/scripts/gyroflow/ahead.lua`,
the policy; `gpu_ahead` in main.lua, the asking; `stretch_to_load` in `gyroflow-gpu-table`, the server):

- The reply to a stretch says what it `covers` (s into the clip) and up to where it's `settled`: the smoothing
  (`smoothing.rs`'s Predictive) is as it will be with all the motion data up to ~4 s before the end of what's read
  (`SETTLE_S` = 5 s).
- The player keeps the settled end **`LEAD_S` = 25 s** ahead of where it plays. When less is left it asks for
  `start` = where it plays, `until` = 25 + 25 s further. The server joins that to the stretch it has (within 60 s:
  `JOIN_S`) and reads only the new part, gently (2 threads, utility priority), forward from where it plays. Reading
  ahead also puts those parts of the file in the Mac's file cache, where the video's own reads find them.
- The new table **keeps the old one's frames** wherever the old one had them settled (copied block by block,
  `Table::copy_frames`), so nothing on screen changes as it comes in, and only the frames after them are built.
  Past the settled end the old table's frames would have moved by under a pixel (the test below measures it).
- Paused, it reads on to the clip's end, 60 s at a time. Once the stretch reaches both ends the reply is no longer
  partial, and the shader is kept on disk as before (the next open is instant).
- **Seek**: to a place the stretch doesn't have, it asks from there at once (joined to the stretch when within 60 s,
  else a stretch of its own) and waits for it. Inside the stretch nothing changes.
- **A-B loop**: from the loop's start to its end must be there and settled; past the loop's end nothing more is asked
  for while it loops. A loop starting before the stretch is asked for from its start.

### When playback reaches the end of what's known anyway

A share slower than the video itself (or a step that failed). Playing on would show the fixed correction past the
stretch, the dropout itself. So it **waits there, paused, with a card** ("Stabilization is catching up", `mp.notify` of
PR #259; plain OSD text without it), and plays on when the table comes in, like a video that buffers. A viewer who
presses play meanwhile plays on unstabilized: their choice. Holding the last stretch's smoothing instead is not
possible: without the motion data there's nothing to smooth.

## Measured with the change (cold, the same clip)

See the PR (#…) for the table of runs: time to the first stabilized frame, the margin over the first two minutes, with
and without the catalog's commands; `tests/gpu_ahead_e2e.py` is the rig.

## The workspace plugin's walks during playback

The journal of the same run showed the workspace plugin stat'ing every file of both of the project's locations (all of
`/Volumes/photo/DATA` and the DJI_Inbox folder), 16 at a time, for the whole two minutes: the side panel's clip count
(`count` → `moments.footage`), redone every minute. And `afp-catalog look --own` every minute, `derived` every 10 s.
Now: the count comes from the catalog (`afp-catalog footage`, in the background, no walk), and without a catalog the walk
runs once a project in a run. The player no longer runs `look` by itself (the catalog's own care does: the NAS Helper
watches the shares, the Library looks after its roots), only when `derived` says a moment isn't where it was; `derived`
is asked every 30 s.

## Tests

- `luajit tests/gpu_ahead_test.lua`: the policy against a model of the server at share speeds from faster than the NAS
  to slower than playback: nothing ever plays without motion data, seeks, loops, paused.
- `cargo test --release --test table_ahead` with `AFP_TEST_CLIP=<a clip with its own motion data>`: the extended table
  keeps the first one's settled frames bit for bit, and is within a pixel of the whole clip's.
- `tests/gpu_ahead_e2e.py <gyroflow-gpu-table> <clip> <out> --throttle 8`: the real server and ahead.lua in real time,
  the motion reads held to 8 MB/s (`AFP_READ_MBPS`): fails if anything plays unstabilized.
