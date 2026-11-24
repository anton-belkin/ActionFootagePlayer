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
| Anton's run (video, catalog, side panel walk) | 6.7 s | 48 s | ~10 s (then he paused) |

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
  `start` = where it plays, `until` = 25 s plus a step further; a step is half the stretch it has, 10 s at least and
  `STEP_S` = 45 s at most (10, 17, 30, then 45 s: the first ones come quickly, later ones are few, since each new table
  is a new ~15 MB shader file for mpv to load). The server joins that to the stretch it has (within 60 s:
  `JOIN_S`) and reads only the new part, gently (2 threads, utility priority), forward from where it plays. Reading
  ahead also puts those parts of the file in the Mac's file cache, where the video's own reads find them.
- The new table **keeps the old one's frames** wherever the old one had them settled (copied block by block,
  `Table::copy_frames`), so nothing on screen changes as it comes in, and only the frames after them are built.
  Past the settled end the old table's frames would have moved by under a pixel (the test below measures it).
- Paused, it reads on to the clip's end, 60 s at a time. Once the stretch reaches both ends the reply is no longer
  partial, and the shader is kept on disk as before (the next open is instant).
- **Seek**: to a place the stretch doesn't have, it asks from there at once for as much as a clip opens with (20 s,
  a stretch of its own unless it overlaps the one there is: no gap read meanwhile), read at full speed (`urgent`:
  playback waits for it), and waits for it. Inside the stretch nothing changes; a stretch read before stays read (seeking back is instant).
- **A-B loop**: from the loop's start to its end must be there and settled; past the loop's end nothing more is asked
  for while it loops. A loop starting before the stretch is asked for from its start.

### When playback reaches the end of what's settled anyway

A share slower than the video itself (or a step that failed). Playing on would show the fixed correction past the
stretch, the dropout itself; and already the last 5 s before its end are the smoothing's end, which bends back to the
camera where the motion data stops: up to ~110 px from what the next table has there (vs-gyroflow/tests/table_ahead.rs),
so the picture would jump as the next table came in. So it **waits at the settled end, paused, with a card** ("Stabilization is catching up", `mp.notify` of
PR #259; plain OSD text without it), and plays on when the table comes in, like a video that buffers. A viewer who
presses play meanwhile plays on unstabilized: their choice. Holding the last stretch's smoothing instead is not
possible: without the motion data there's nothing to smooth.

## Measured (2026-11-19/20, cold: the clip out of the Mac's cache; the same 1.7 GB clip on the NAS)

`tests/gpu_ahead_e2e.py`, the first two minutes of playback; "margin": how far the table's motion data reaches past
where it plays (min / 10th percentile); the video's own reads 1 MB at a time, 2 s ahead, as mpv; the catalog: `derived`,
`footage`, `list` every 10 s on the real project (read-only). Anton was using the player on the same NAS in part of
these runs, so single worst cases are noisy.

| run | first table | after it | played unstabilized | waited | margin | video reads p95 / max | `footage` median / max |
|---|---|---|---|---|---|---|---|
| before, video + catalog | 5.8 s | whole clip in 33 s | 13.0 s | 0 | −13.1 s / −1.1 s | 25 / 501 ms | 0.23 / 17.4 s |
| before, + the side panel's walk | 6.4 s | whole clip in 35 s | 14.5 s | 0 | −14.7 s / −2.7 s | 51 / 631 ms | 0.23 / 18.7 s |
| after, video | 7.2 s | 6 steps, 4–13 s each | 0 | 0 | +14.4 s / +21.8 s | 59 / 1806 ms | – |
| after, video + catalog | 6.5 s | 6 steps, 4–10 s | 0 | 0 | +15.0 s / +23.4 s | 20 / 1596 ms | 0.22 / 7.5 s |
| after, video + catalog (again) | 5.5 s | 6 steps, 4–11 s | 0 | 0 | +15.2 s / +23.9 s | 24 / 427 ms | 0.23 / 2.1 s |

- Time to the first stabilized frame is unchanged (5–7 s cold): the first stretch is the same 24 s of the clip.
- The 1.6–1.8 s video reads came once each in two runs and not in two others (worst 344 ms, 427 ms): not explained.
- `afp-catalog` alone takes 0.2–0.3 s (0.9 s cold) on this project; during the whole-clip read 3–7 s (`stat` on the
  NAS 6 ms → 225 ms p50): it waited behind the motion read's 8 MB reads, the side panel's walk added little. During a
  step now, up to 2–7 s; between steps 0.2 s. Reading in 2 MB pieces instead of 8 MB made no clear difference (one run
  each: catalog worst 2.7 s against 2.1 s, video p95 69 ms against 24 ms), so it stays 8 MB.
- Held to a slower share (`--throttle`, the clip in the Mac's cache): 15 MB/s (1.3× the video's rate): waited 3 s once,
  nothing unstabilized; 8 MB/s (slower than the video): waited 40 s of 60, nothing unstabilized or unsettled played;
  a seek from 10 s to 100 s at 15 MB/s waited for its stretch (27 s, before `urgent`), then played stabilized; back to
  5 s: instant; an A-B loop of 12–30 s: never waited.
- A seek on the NAS, cold, with the video and the catalog (from 20 s to 110 s): waited 5.4 s for its stretch (as long
  as opening the clip there), then played stabilized to the end.

### Known limits (not done here)

- A seek that comes while a step is being read waits for that step first (the server reads a clip's motion data one
  stretch at a time): up to a step's time more, ~4–10 s on the NAS (seen: 8 s more at 30 MB/s). Making a gentle read
  give way to an urgent one is the follow-up.
- Each step re-parses all the motion data read so far (~1 s for 2 minutes of 4K60 on an M1 Pro): fine for clips of a
  few minutes, grows with longer ones.
- Every new table is a new shader file for mpv (~15 MB of text for this clip). The one swap before cost ~0.3 s of
  picture (2026-11-10); with steps it happens ~5 times in two minutes. **Not measured in a live window.**

## The workspace plugin's walks during playback

The journal of the same run showed the workspace plugin stat'ing every file of both of the project's locations (all of
`/Volumes/photo/DATA` and the DJI_Inbox folder), 16 at a time, for the whole two minutes: the side panel's clip count
(`count` → `moments.footage`), redone every minute. And `afp-catalog look --own` every minute, `derived` every 10 s.
Now: the count comes from the catalog (`afp-catalog footage`, in the background, no walk), and without a catalog the walk
runs once a project in a run. The player no longer runs `look` by itself (the catalog's own care does: the NAS Helper
watches the shares, the Library looks after its roots), only when `derived` says a moment isn't where it was; `derived`
is asked every 30 s.

## A clip with a Gyroflow project, opened cold (2026-11-24)

A clip with a project gets the whole clip's table at once (the project's smoothing is the whole clip's). The journal of
19–24 Nov showed its first frame waiting for it in proportion to the file's size, ~15 ms a MB from the NAS (200 MB →
3 s, 469 MB → 8 s), for GoPro clips with Gyroflow's own (Simple) projects, whose motion data `add_motion_data` reads
from the video. Two causes, measured with the table tool alone (`gyroflow-gpu-table --player`, the clip out of the
Mac's cache; locally the same table took 0.4 s at any size):

- **A GoPro's second metadata track.** HERO cameras write `gpmd` (the telemetry, a sample a second) and `fdsc`
  ("GoPro SOS", ~80 samples a second). telemetry-parser's GoPro reader reads only the first; `prefetch.rs` read both,
  which put a sample every ~95 KB, so on a share the whole file was read in 8 MB stretches. Now a GoPro file (its first
  metadata sample starts with GPMF's `DEVC`) has its first track alone read: 16 samples instead of ~1250 for a 16 s
  clip. Other cameras keep every metadata track (water.rs and translation.rs read them all).
- **Gyroflow's lens database, loaded for every clip.** gyroflow-core gives each `StabilizationManager` its own and loads
  it (12 409 profiles, 0.35–0.55 s of CPU) when a clip's camera is looked up: the table server paid that for every clip,
  a clip without a project twice. Now every manager made in vs-gyroflow shares one (`lens_pick::manager`; gyroflow-core's
  own clones share theirs the same way), and the server loads it as it starts, while the player opens its first clip.

Measured 2026-11-24 on the NAS (the shares mounted over the LAN; over Tailscale, earlier that morning, a 300 MB clip's table took 7.8 s before), each clip out of the Mac's cache, as a new player gets its first table
(the server started, the request 0.3 s later), the new build first and then the one before on the same clips:

| GoPro HERO7 clip | before | after |
|---|---|---|
| with a project, 53–203 MB | 0.58–1.26 s | 0.25–0.58 s |
| with a project, 299–549 MB | 2.25–3.14 s | 0.67–0.93 s |
| with a project, 837 MB / 1.56 GB | 5.47 / 8.55 s | 1.13 / 1.91 s |
| without a project (its first 20 s), 111–403 MB | 1.16–2.83 s | 0.26–0.38 s |

A clip with a project gets the same shader byte for byte. A GoPro clip without one gets what its first stretch asks for:
before, its GPMF samples were read along with the `fdsc` ones next to them, so the first "20 s" table had the whole
clip's motion in it (and the whole file read for it); now it has the first 20 s, extended as it plays like any other
camera's. So that first table differs a little from before: its smoothing and its gyro bias (from the accelerometer)
see 20 s of motion, not all of it, until the next step comes.

## Tests

- `luajit tests/gpu_ahead_test.lua`: the policy against a model of the server at share speeds from faster than the NAS
  to slower than playback: nothing ever plays without motion data, seeks, loops, paused.
- `cargo test --release --test table_ahead` with `AFP_TEST_CLIP=<a clip with its own motion data>`: the extended table
  keeps the first one's settled frames bit for bit, and is within a pixel of the whole clip's.
- `tests/gpu_ahead_e2e.py <gyroflow-gpu-table> <clip> <out> --throttle 8`: the real server and ahead.lua in real time,
  the motion reads held to 8 MB/s (`AFP_READ_MBPS`): fails if anything plays unstabilized.
