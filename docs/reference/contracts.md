# Contracts between processes
Covers: `mpv/scripts/workspace/{main,batch,renders}.lua`, `mpv/scripts/gyroflow/{main,bundle,gpupath,export,ahead}.lua`, `library/src/{main,index,seen,share}.rs`, `catalog/src/live.rs`, `vs-gyroflow/src/{gpu_table.rs,bin/*.rs}`, `tools/stream/`, `tools/nas-helper/`

Checked against 295101c (2026-11-25); the settings JSON on its branch (2026-11-25).

What one process hands another: request files, status files, lines on a socket, JSON on stdout. The files a project
keeps on disk (sidecars, links, `Project.afproject`, the catalog) are in [files-and-formats.md](files-and-formats.md);
which setting reaches which flag is in [settings-flow.md](settings-flow.md); who runs whom is in
[architecture.md](architecture.md). The lists that can be read from the sources are generated at the end.

General rules that hold for every contract here:

- **Status files are written whole:** to `<file>.tmp`, then renamed over `<file>`. A reader never sees half a file.
- **One JSON line on stdout** is a tool's answer (`gyroflow-gpu-table`, `gyroflow-probe`, `afp-catalog`,
  `gyroflow-optical --check`). Readers take the last (or first) line and ignore stderr.
- **Missing keys mean the default.** Writers leave a key out rather than write `null`, except where noted.
- **A change to an answer's shape raises a version** where a cache keeps it (`GEOTAG` for the probe, the render
  recipe's `VERSION`); see [../../CLAUDE.md](../../CLAUDE.md).

## The Library's batch (`AFP_BATCH`)

The Library makes clip previews, moments, photos, exports and renders by running the player with no window. One job
runs at a time (`library/src/batch.rs` `start_job`, `tick`). The player side is
`mpv/scripts/workspace/batch.lua` (the request, its status, the run) and `renders.lua` (`render_moments`).

### How it starts

Each job has a folder `<temp>/ActionFootageLibrary-<library pid>/<job id>/` with:

| File | Written by | What |
|---|---|---|
| `request.json` | Library | The request (below) |
| `status.json` | player | How far it is (below) |
| `stop` | Library | Appears when the job should stop; its content doesn't matter |
| `log.txt` | player (`--log-file`) | mpv's log; its last `[e]`/`[f]` line becomes the job's error when the player exits without a status |

The Library starts the player (`start_player`):

- **macOS, Linux:** `<ActionFootagePlayer.app>/Contents/MacOS/ActionFootagePlayer --idle=yes --vo=null --ao=null
  --force-window=no --terminal=no --log-file=<log.txt>` (Linux: `mpv`), with `AFP_BATCH=<request.json>`.
- **Windows:** `ActionFootagePlayer.exe --batch <request.json>` with `CREATE_NO_WINDOW`; the launcher sets `AFP_BATCH`.
- Also `AFP_JOURNAL_PARENT=L<library pid>.job<id>` when the journal is on. On macOS the Library runs
  `caffeinate -i -w <pid>` beside it, so the Mac doesn't sleep mid-job.
- Where the player is: `AFL_PLAYER`, else beside the Library app, else `/Applications`, else `~/Applications`.

Kinds `optical`, `lens-find` and `lens-estimate` run `gyroflow-optical --batch <request.json>` instead (stderr is
`log.txt`). Everything else in this section holds for them too.

### The request

`add_job` (`library/src/batch.rs`) builds it; `start_job` adds `status`, `stop` and `focus`.

| Key | Type | Kinds | Meaning |
|---|---|---|---|
| `kind` | string | all | `clips`, `clips-again`, `clips-finish`, `moments`, `photos`, `export`, `render`, `share` (player); `optical`, `lens-find`, `lens-estimate` (gyroflow-optical) |
| `project` | path | player kinds | The project's folder (with `Project.afproject`). Required by the player |
| `status` | path | all | Where to write the status. Required by the player |
| `stop` | path | all | The stop file to watch |
| `focus` | path | player kinds | A file the Library rewrites as its window moves: `{ "dirs": [absolute folders] }`. Clip previews in those folders are made first (`focus_first`) |
| `only` | [path] | player kinds | Kept to these paths in the project (clips, folders, moment files). Clip previews: made for each without one, removed ones too |
| `draft` | bool | `clips` | `true`: empty clip previews (a dark frame with the link) at once; `clips-finish` makes their pictures later |
| `layout` | string | `clips`, `clips-again` | `tree`, `flat` or `trips`: how clip previews are laid out in the run's folder |
| `alone` | bool | `moments` | New moments stabilized on their own (their part's motion only) |
| `photo` | string | `photos` | `no` (stills), `live`, `motion`. Set as `user-data/gyroflow/live_photo` |
| `sdr` | bool | `export` | Export in SDR (NBCU) |
| `profiles` | [string] | `render` | Of `index::RENDER_PROFILES` (`HDR`, `SDR`, …); default `["HDR"]` |
| `items` | [object] | `share` | The renders the Library's cache lacks (below) |
| `clips` | [path] | gyroflow-optical | The clips, absolute |
| `redo` | bool | gyroflow-optical | Measure again clips that have motion already |
| `action` | string | `lens-*` | `find-lens` or `estimate-lens` (default `measure`) |
| `quick` | bool | `lens-find` | Find stops at the first lens that fits (default `true`) |
| `strength`, `tracking` | number, string | `optical` | gyroflow-optical's measure settings (100, `klt`) |

A `share` item (`library/src/share.rs` `plan`, read by `render_moments`):

| Key | Meaning |
|---|---|
| `of` | `moment`, `photo` or `clip` |
| `name` | Shown in the status's `current` and in failures |
| `file` | A moment's link file (`.afplink.mp4`) |
| `clip` | The original, absolute |
| `t`, `crop`, `camera` | A photo's time, crop and Save frame camera |
| `sdr` | The SDR profile |
| `out` | The render's file in the cache, `_Renders/<hash>.<profile>.mov` (a photo's without extension) |
| `link` | What the render carries: its recipe and hash (`renders.rs`) |
| `view` | The recipe's render settings; when present the player renders with them, not its own |
| `alone` | Stabilized on its own |
| `export` | Share's export settings (size, encode, quality); absent: the defaults |

### The status

Written by the player (`write_status`) at start, every 0.5 s while running, and at the end. Read by `read_status`.

| Key | Type | Meaning |
|---|---|---|
| `kind`, `project` | | Echo of the request |
| `state` | string | `starting`, `looking`, `running`, `stopping`, then one of `done`, `stopped`, `failed` |
| `done`, `total` | int | Items done of all |
| `failed` | int | Items that failed |
| `made` | int | Files made (at the end) |
| `fraction` | number | Share of the current item done (exports: from `gyroflow-export`'s progress file) |
| `current` | string | The item being made |
| `message` | string | Why it failed, or "nothing to do" (e.g. "Every render asked for is current") |
| `failures` | [[name, why]] | One pair per failed item |
| `used` | [{ out, player }] | `share` only: the settings this player would have used by itself, per render, for the Library to compare with its recipe |
| `left_out`, `made_paths`, `results` | | gyroflow-optical only: clips left out `[[name, why]]`, clips measured (a rerun leaves them out), Find/Estimate results |

### Lifecycle

1. The Library writes `request.json`, removes an old `status.json` and `stop`, and starts the process. Job state
   `starting`.
2. The player checks the request. Not JSON, or no `project`/`status`: it logs `AFP_BATCH: not a request` and quits
   with status 2.
3. It waits until the gyroflow plugin publishes `user-data/gyroflow/exporter` and `user-data/gyroflow/defaults`. If
   that takes over 20 s, it finishes `failed` ("the gyroflow plugin didn't start").
4. A folder that isn't a project finishes `failed`. Otherwise it runs the kind; output goes into the project's
   `Generated/<date> <time> <what>/` (renders into `_Renders/`).
5. When `stop` appears it writes `stopping` and stops after the item being made (an export is cancelled with
   `export-batch-stop`; clip previews abort the current one).
6. At the end it writes the final status, waits for its catalog notes, and quits.
7. The Library's side: no status 40 s after start → kill, `failed` ("didn't start the batch"). Stop asked and still
   running 20 s later → kill. Process gone without a final state → `stopped` if a stop was asked, else `failed` with
   the log's last error. After each job it looks at the catalog again (`seen::look_at_everything`).
8. A `clips-finish` job steps aside for any other job: the Library writes `stop`, then queues it again.

## The live channel: Library and player on one computer

`catalog/src/live.rs`. Folder `<cache>/live/`, where `<cache>` is `~/Library/Caches/ActionFootage` (macOS),
`%LOCALAPPDATA%\ActionFootage` (Windows), `$XDG_CACHE_HOME/ActionFootage` (Linux), or `$AFP_HOME/cache` in tests.

### Player inbox: `player-<pid>.in`

- **Who makes it:** each player window's workspace plugin, at start (not in batch mode). Removed at shutdown.
- **Who writes:** the Library, `seen.rs` after it writes into a project (`afp_catalog::live::tell_players`), one line
  appended to every inbox whose pid is alive. Dead pids' inboxes are removed.
- **Format:** one JSON object per line, `\n`-terminated. There is a single message form, no verbs:

  ```json
  {"project": "<project folder>", "files": ["<absolute path>", "..."]}
  ```

  `files` are what the Library changed: a moment renamed or removed, a tag, a clip's own file (lens type), a Take in.
- **Reading:** the player stats the file four times a second and reads only whole new lines from where it stopped.
- **What it does** (`heard`): only when the project is the one playing, or one of the files is in it. It refreshes
  the moments, recounts the project, reloads tags, and when the playing clip's own file changed its lens type, loads
  the clip again where it was.

### Library listener: `library-<pid>.json`

- **Who makes it:** the Library at start (`live::listen`): `{"pid": <pid>, "port": <port>}`, a TCP listener on
  `127.0.0.1:<port>`.
- **Who sends:** `afp-catalog note` after a successful note, to every listening Library: the same
  `{"project", "files"}` line. The Library then reads the journals at once (`seen::read_now`).
- A Library that's gone costs a failed connection; its file is removed when its pid is dead.

Between computers there is no channel: each reads the others' journals in `.afp/catalog/` on the share.

## The settings

One JSON object says how a clip is shown: what the warp does, the clean-up, Ctrl+j, the colour and look, a 360
camera's sphere, an export's own choices. It's built once and taken as it is by everyone who renders the clip:

- **Built by** the gyroflow plugin for the clip playing (`gyroflow/gpupath.lua` `gpu_settings`, published as
  `user-data/gyroflow/settings` and in `render_args`' dump), and by `gyroflow/settings.lua` `of_view` for a saved
  view (a moment's, a clip of the Project tab, a Library recipe's `view`). `export.lua` `export_settings` is the one
  function every route makes its settings with: the plugin's own, or `of_view`'s; the dlogm plugin's look and colour
  over the clip playing's (`as_it_looks`); the export's choices over both (`exportsettings.lua` `apply`).
- **Taken by** `gyroflow-gpu-table`: its request is the settings' keys it reads (`settings.lua` `request`) with what
  only playback adds (below, "request"); and by `gyroflow-export --settings <json|file|->` for every export, photo,
  Live Photo, moment preview, 360 export and Ctrl+j's measuring (`apply_settings` in `gyroflow-export/options.rs`).
- **Through** the Library's batches and renders (the recipe's `view` goes to `export-batch` and `photo-at`, which
  make it into settings with `of_view`) and the NAS stream (`render_args`' dump: `render.user_data`, the table's
  request, and `render.settings`, what it's made of).
- **Rules:** missing keys mean the default. Flags after `--settings` change what it set (the command line, Finder's
  Quick Actions). A key the export doesn't know is left alone (another reader's); a key it reads with a wrong value is
  an error. No program of ours passes a flag a key stands for (`tests/settings_json_test.py`). `project` may be the
  video itself (a clip without a project): the export then finds the clip's own project as it always did. `lens: off`
  is nothing done (the table is never asked then). The clean-up comes as the clip's switch (`denoise`), how much
  (`temporal_denoise`), its Stronger and the grain kept; a photo is cleaned up from 5 frames each side where the
  denoise is on, else none, unless `still_radius` says. `translation` takes Ctrl+j's jerks out where they're computed
  for the clip, on the GPU path; elsewhere it's left out without an error (`--with-translation` insists).
- **Checked** by `tests/settings_cases.json`: a saved view's settings (`tests/settings_json_test.lua`) set the same
  options as the flags the player passed before (`gyroflow-export` tests `settings_set_what_their_flags_did`); every
  key the registry says the export reads is read (`every_key_the_export_takes_is_read`); the table's own key list is
  the registry's (`gpu_table.rs` `the_request_is_the_settings_json_s`).

The keys are generated [below](#the-settings-keys).

## The player and `gyroflow-gpu-table`

`mpv/scripts/gyroflow/bundle.lua` (`player_args`), `gpupath.lua` (`gpu_user_data`, `gpu_table`, `gpu_ahead`),
`vs-gyroflow/src/bin/gyroflow-gpu-table.rs` (`player`, `serve`), `vs-gyroflow/src/gpu_table.rs` (`PlayerClip`,
`motion_range`, `PLAYER_GEOMETRY_KEYS`).

### Invocation

- `gyroflow-gpu-table --serve <socket>`: one per player, socket `<temp>/mpv-gyroflow-<mpv pid>.sock`. Not on Windows.
  It exits when its parent pid changes or after an hour without a request.
- `gyroflow-gpu-table --player <json> <out.glsl> [--server <socket>]`: one per warp. With `--server` it forwards the
  request (connect retries for 300 ms, for a server just started); without one, or when the server doesn't answer, it
  computes the shader itself. Output file: `<temp>/mpv-gyroflow-<pid>-<n>.glsl`.

On the socket: one line in, `{"json": "<the request, as a JSON string>", "out": "<out.glsl>"}`, one line back, the
reply below.

### The request

The [settings](#the-settings)' keys the table reads, plus what only playback adds (`gyroflow/gpupath.lua`
`gpu_user_data`, `gpu_table`): the video's size, `gpu_chromatic` (the settings' `chromatic` unless this screen skips
it, `chromatic_skip`), `gpu_denoise` (`denoise` where `temporal_denoise` isn't off), `gpu_translation` (gyroflow.conf
`translation`: the jerks in the shader where computed, on and off by its PARAM), `translation_rev`, `lookahead`, and
the stretch asked for.

| Key | Meaning | Geometry? |
|---|---|---|
| `project` | The `.gyroflow`, or the video itself when it plays without a project. Required | yes |
| `preset` | Preset `.gyroflow` applied to a clip without a project | yes |
| `fov` | Gyroflow's FOV in place of the project's | yes |
| `level` | `weak`, `medium`, `strong` (no project) | yes |
| `wide` | The wider view (Ctrl+g) | no: a pick among loaded views |
| `horizon_lock` | 0–100; absent: the project's | yes |
| `underwater` | `yes`, `no`, `auto`, `project` | yes |
| `trim_start`, `trim_end`, `alone` | The project's trim range and "only within the trim range" as viewed | yes |
| `rotation`, `aspect`, `projection` | The frame: turn (deg), output aspect, output projection 0–4 | yes |
| `lens` | `stabilized`, `recorded`, `only` | yes |
| `input_width`, `input_height` | The video's size | yes |
| `version` | The source's version; a change reloads | (in the server's key) |
| `interpolation` | `bilinear`, `bicubic`, `lanczos` (also `bspline`, `ewa`) | no |
| `edges`, `edges_blur` | Edge fill `off`, `sphere`, `smooth`; blur % of the height | no |
| `gpu_chromatic` | Fringes under water taken out | no |
| `gpu_sphere` | Sphere edge fill allowed (default true) | no |
| `gpu_denoise` | Temporal denoise passes (Ctrl+p) | no |
| `gpu_translation`, `translation_rev` | Translation compensation (Ctrl+j) and its revision | no |
| `time_offset` | Where a moment starts in its clip (s) | no |
| `moment_end` | A moment's end in its clip: only that part's motion is read | — |
| `start`, `until` | Stretch of motion data asked for (s into the clip); without `until`, 20 s | — |
| `urgent` | Playback waits for it (a seek): read at full speed | — |
| `full` | All the motion data at once | — |
| `lookahead`, `translation_rev` | Sent; not read by the table (in its cache keys, as every key is) | — |

The server keeps up to four loaded clips, keyed by `PLAYER_GEOMETRY_KEYS` plus the project's, preset's and motion
file's stamps and the lens state. A request that changes only non-geometry keys rewrites the shader alone (~40 ms).
Shaders for complete tables are also kept on disk (`disk_cache`, under 400 MB), keyed by every key but the per-request
ones (`start`, `until`, `urgent`, `full`, …), the files' stamps and the tool's build.

### The reply

One JSON line on stdout:

| Key | Meaning |
|---|---|
| `shader` | The `.glsl` written (the `out` asked for) |
| `output` | `[w, h]` the warp renders at |
| `mode` | `grid` (RGB, at `MAIN`) or `planes` (at `LUMA` and `CHROMA`) |
| `chromatic` | The fringes are being taken out (the lens is under water somewhere) |
| `sphere` | The sphere edge fill is in |
| `focal` | `[fx, fy]` of the output |
| `translation` | The translation compensation is in |
| `duration` | The clip's length (s) |
| `partial` | The table covers a stretch, not the whole clip |
| `covers` | `[a, b]` the stretch, when partial |
| `settled` | Up to where (s) the frames won't change with more motion data |
| `ms` | Time taken |
| `unsupported` | Instead of `shader`: why the warp can't do this clip (`gpu_table::unsupported`); the player plays it as recorded with a card |
| `error` | Instead of `shader`: what went wrong |

The player adds `ms`, `denoise`, `reused` and keeps the request with a partial reply. A partial reply makes
`ahead.lua` ask for more ahead of the playhead (`start`/`until`, `urgent` on a seek); each reply replaces the shader in
mpv's list via `gyroflow-shader` to dlogm.

`gyroflow-gpu-table --sphere360 <video> <out.glsl> [width [distance [time offset]]]` answers
`{shader, output, joined, ms}` or `{error}`, for a 360 camera's own file.

## gyroflow-export as a subprocess

`mpv/scripts/gyroflow/export.lua`. What it renders comes as one JSON, `--settings` ([The settings](#the-settings));
the other flags are the job's: what to make (`--photo`, `--live`, `--preview`, `--360`, `--reframe`, `--youtube`),
which part (`--from`, `--to`, `--alone`, `--whole`, `--at`), where (`--output`, `--out`), its link and dates, crop
mode's `--camera` and `--crop`, the player's frame (`--image`, `--picture`). Flags are listed
[below](#gyroflow-export-flags); the ones a settings key stands for stay for the command line and Finder's Quick
Actions.

- **Progress:** `--progress <file>`, JSON rewritten twice a second: `{pid, done, total, fps, eta, background, partial}`
  (`partial`: the unfinished file); `{pid, stage: "measuring", …}` while it measures; `{pid, translation: <%>}` for
  Ctrl+j's pass.
- **Priority and stop:** SIGUSR1 = background priority (it and its ffmpegs), SIGUSR2 = normal, SIGTERM = stop and remove
  the unfinished file. On Windows: `--control <file>` holding the word `background`, `normal` or `stop`, polled.
- **Result:** exit 0, and a stderr line `  -> <file>` per file written. `export.lua` notes each in the catalog.

Between the workspace and gyroflow plugins (all `script-message-to gyroflow <msg> <json>`; the answer is
`script-message-to <reply> <done> <id> <json>`):

| Message | Request keys | Answer |
|---|---|---|
| `preview` | `id, clip, from, to, at, link, dates, output, force, view, project, alone, reply, done` | `preview-done` `{ok, error}` |
| `export-batch` | `id, clip, view, project, alone, from, to, output, link, sdr, export, progress, reply, done` | `export-batch-done` `{ok, error, out}` |
| `export-batch-stop` | `<id>` | the job's answer, `error: "cancelled"` |
| `photo-at` | `id, clip, t, view, crop, camera, link, still, export, out_dir, reply, done` | `photo-done` `{ok, error, out}` |
| `export-name` → `export-start` | gyroflow asks workspace for the name and place; workspace answers `export-start` | — |

## gyroflow-probe

`vs-gyroflow/src/bin/gyroflow-probe.rs` has the full description of each mode in its header. Every mode prints one
JSON line.

- `gyroflow-probe <video>` (the gyroflow plugin, as a clip loads): `mode` (`default`, or `null` with `reason`),
  `width`, `height`, `motion` (`optical` with `measured` for an `.afp-motion`), `lens`, `camera`, `color`, `water`,
  `stabilized_in_camera`, `rotation`, `spherical`, `rig360`.
- `--lens <video>` (side panel's Source tab; the Library's lens info): `camera`, `lens`, `from`, `types`, `type`,
  `recorded`, `default`, `mode`, `mode_key`, `candidates`, `curated_file`, `project`.
- `--save-lens`, `--assign-lens <video> <id>`, `--forget-lens` (the Library): change the user's list or the player's
  lens database; `{error}` and exit 1 on failure.
- `--headings`, `--reframe`, `--rig` (view360 and the gyroflow plugin's 360 mode).
- `--library <video>` (the Library's index, `library/src/index.rs`, one clip at a time in the background): all of the
  plain answer, plus `camera`, `movie {duration, created, width, height, turn}`, `marks {camera, times}`,
  `clock {utc, basis, zone_min, why}`, `gps {lat, lon, alt, t}`, and `geotag` (its version). The Library keeps answers
  per file by size and time; an answer with `geotag` below `index.rs` `GEOTAG` (now 11) is asked again. Change both
  together: the test `geotag_goes_with_what_the_probe_answers` checks it. `AFL_PROBE` overrides where the tool is.

## gyroflow-optical (from the Library)

Lines on stdout, one JSON object per clip: `--lens-types <video>...` (`{path, camera, types: [[id, label]], type,
recorded, mode}`), `--lenses <video>` (the lens dialog's choices), `--lens-for <video> keep|assign <n|id>`
(`{name, how, from, profile}`, which the Library writes into the clip's own file), `--keep-lens <video> <n>`,
`--promote-lens <video>`, `--try-lens`. `--batch` is the job protocol above. `AFL_OPTICAL` overrides the tool.

## afp-meta, afp-cut and afp-catalog

- `afp-meta <file>...`: `{file, type: video|photo, link, tags, keys}` per file. `afp-meta <file> --link <json>`,
  `--tags <json list>`, `--key <name> <value>` write; a HEIC goes through `afp-photo` (macOS only). The workspace
  plugin writes photo links with it; the Library writes a photo's link and tags.
- `afp-cut --set-key <mp4> ch.actionfootageplayer.link <json>`: the Library rewrites an MP4's link in place (Find moved
  files, tags on moments). `afp-cut <video> --from … --json` is the player's Ctrl+x; exit 0 cut, 2 not cut, 3 cut but
  not the same (removed).
- `afp-catalog note <project|-> <file>...`: the player after every write, one at a time (one journal writer per
  computer). `derived`, `footage`, `list`, `look --own` answer JSON the player reads in the background. `watch` is the
  NAS Helper's. Usage: `catalog/src/cli.rs` `USAGE`.

## The mpv IPC socket

mpv's JSON IPC (one JSON command per line). The socket is `input-ipc-server`:

- the agent plugin sets one when none is set: `<data>/mpv.sock` (`$AGENT_DATA`, in the app
  `~/Library/Application Support/ActionFootagePlayer/agent/`), or a short path in `$TMPDIR` when too long; on Windows
  `\\.\pipe\ActionFootagePlayer-agent`. It writes `<data>/session.json` `{socket, pid, mpv, started}` and
  `user-data/agent` `{socket, reports}`.
- the Windows launcher sets `\\.\pipe\ActionFootagePlayer`: files opened while a player runs are sent down it
  (`loadfile`), and the Library polls `["get_property", "path"]` there until the item plays (`player_pipe`).
- `side-panel/scrub.lua` makes one of its own if none is set, for `afp-scrub`.

What talks on it:

- **The AI agent:** `get_property`, `set_property`, `script-message-to agent snapshot|say|report`.
- **afp-scrub** (`tools/scrub/afp-scrub.c --ipc <socket> --pid <pid> --to side_panel`): it observes
  `user-data/afp-scrub/request` (a tab-separated line: `<id> <file> <time> <exact|key> <out.bgra> <w> <h> [<clip>]`, or
  `open-exact`/`open-key`) and answers `script-message-to side_panel scrub "<id> ok <ms> <kind> <t>"` (or `opened`,
  `dropped`, `error`). The newest request wins.
- **Test rigs** (`tests/*_ipc.py`, `*_e2e.py`, the Library's perf test): `loadfile`, `seek`, `keypress`, `get_property`,
  `set_property`, `screenshot-to-file`, `playlist-*`, `quit`, and `script-message-to` the plugins (e.g.
  `gyroflow lens-dialog`, `side_panel tab|dump|popover`, `workspace open-original`, `highlights mark`). Each message
  they send must be registered: `tests/script_messages_test.py`.

## Opening files from the Library

- `open_items`: macOS `open -a <player.app> <files>`; elsewhere the player with the files as arguments (Windows: the
  launcher, which hands them to a running player).
- `open_list`: an M3U written to the Library's cache, `lists/<ms>.afplist.m3u8`, opened as a file:
  `#EXTM3U`, `#PLAYLIST:<label>`, `#AFP-START:<index from 0>`, then `#EXTINF:-1,<title>` and the path per item.
  `workspace/librarylist.lua` starts the playlist at that item and publishes `user-data/library-list`.
- Journal hand-off: the Library writes `handoff.json` `{a, w, paths}` into the journal folder; the player's
  `journal.lua` takes it as the opened file loads ([../design/responsiveness-journal.md](../design/responsiveness-journal.md)).

## The stream and the NAS Helper

- **How a file plays, asked of the player:** `afp_stream.py` runs `mpv --no-config --config-dir=<player config>
  --load-scripts=no` with the player's `workspace`, `gyroflow` and `dlogm` plugins and
  `--script-opts-append=gyroflow-render_args=<file> -- <clip>`. After the first frame the gyroflow plugin writes
  `{clip, offset, length, moment {name, in, out, missing}, render {stabilize, kind, user_data}, dlogm {curve, look, log},
  title, duration}` and quits. `render.user_data` is the `gyroflow-gpu-table --player` request above; the stream passes
  it to `gyroflow-gpu-table` itself, then `afp-render` (`tools/stream/afp-render.c`) renders HLS.
- **What the stream takes of it:** the look from `dlogm.curve` (any log curve in registry.json: the dlogm plugin's
  `<curve>_hlg.glsl`, its `hdr`, which the image bakes for each with `tools/stream/make_looks.py`); the temporal
  denoise from `render.settings` (`denoise`, `temporal_denoise`, `denoise_strong`, `denoise_grain`) when
  `AFP_DENOISE=player`: `gpu_denoise` in the table's request, `afp-render -behind <frames> -set gyroflow_dn_…=<v>`
  (the shown frame that many behind the newest, as gyroflow-export's GPU path renders it); the footage without the
  project's own folders (registry.json's `own_folders` at a project's top, `stabilized/` anywhere).
- **HTTP** (`AFP_PORT`, 8099): `/`, `/playlist.m3u`, `/clip/<id>/index.m3u8`, `/clip/<id>/<segment>`,
  `/watch/<id>`, `/prerender?folder=…&what=opening|whole` (only when asked), `/status`, `/catalog.json`,
  `/poster/<id>.jpg`, and `/settings` when it runs in the NAS Helper. `jellyfin.py` reads `/catalog.json`.
- **NAS Helper:** `helper.py` starts `afp-catalog watch <project>`, `afp_stream.py` and `jellyfin.py` as children and
  writes `/data/helper.json` `{version, started, parts: {<name>: {…}}}`, shown on the stream's `/settings` page.
- **The Library and the NAS Helper** share no channel. They meet in the project's files and `.afp/catalog/` on the share.

## The Library page and its commands

The page calls the Rust side's Tauri commands (`library/src/main.rs`'s `invoke_handler` lists them; they live in a module per area). Rules they keep
([../design/library-responsiveness.md](../design/library-responsiveness.md)):

- **A command that changes items answers what they are now:** `changed::Changed { items, gone, gone_files, reload,
  probes, problems, answer, links, links_gone }`, by view (`footage`, `moments`, `stills`, `exports`), made by
  `changed::of_items` / `of_clips` / `of_album`. The page takes those items as they are and reads no view again;
  `reload` names the views it must read whole. Don't follow a command with `refresh()`.
- **Screen first:** a command joins the page's queue with `Writes.define(name, { item | patch, send, … })`
  (`ui/writes.js`): the change shows at once, the writes run one at a time in order. `item()`/`patch()` must do to the
  shown item exactly what the command does to the file.
- **Catching up:** `changes_since { since }` → `{ upto, changed }` (null `changed`: read the views again);
  `view_items { view, ids }` → `Changed`; `writes_pending` tells the Rust side the page's queue length, so a close
  of the window waits for it (`close_decision`).
- **Share:** `share { items: [{ kind: moment|photo|clip|file, path, name }], dest: "folder", folder?, look?, settings? }`.
  What the cache lacks goes to the player as one batch of kind `share` ([above](#the-request)). `share::save` never
  writes over a file (`Turtle 2.mov`): it copies to `.<name>.part` and renames. A new destination is a line in
  `share.rs` `DESTINATIONS`, one in `share.js` `SHARE_DESTS` and an arm in `share::deliver`.

## Generated lists

### Script messages
<!-- BEGIN GENERATED: script-messages -->
<!-- Made by tools/gen_reference.py: don't edit by hand -->

| Plugin | Message | Registered in | Sent by name from |
|---|---|---|---|
| agent | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| agent | `report` | `mpv/scripts/agent/main.lua` | — |
| agent | `say` | `mpv/scripts/agent/main.lua` | — |
| agent | `snapshot` | `mpv/scripts/agent/main.lua` | — |
| dji_telemetry | `field` | `mpv/scripts/dji-telemetry/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| dji_telemetry | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| dji_telemetry | `position` | `mpv/scripts/dji-telemetry/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| dji_telemetry | `remember` | `mpv/scripts/dji-telemetry/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| dji_telemetry | `units` | `mpv/scripts/dji-telemetry/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| dlogm | `curve` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/viewsave.lua` |
| dlogm | `denoise` | `mpv/scripts/dlogm/main.lua` | — |
| dlogm | `dv-neutral` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/side-panel/main.lua` |
| dlogm | `dv-reset` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/side-panel/main.lua` |
| dlogm | `dv-trim` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/side-panel/main.lua` |
| dlogm | `gyroflow-shader` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/gyroflow/shaders.lua` |
| dlogm | `look` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| dlogm | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| dlogm | `remember` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| dlogm | `uw-color` | `mpv/scripts/dlogm/main.lua` | — |
| dlogm | `uw-set` | `mpv/scripts/dlogm/uwcolor.lua` | `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/viewsave.lua`, `tests/sidepanel_test.lua`, `tools/pipebench/live_starts.py` |
| footage_menu | `copy-path` | `mpv/scripts/footage-menu/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| footage_menu | `help` | `mpv/scripts/footage-menu/main.lua` | `mpv/scripts/footage-menu/main.lua` |
| footage_menu | `keys` | `mpv/scripts/footage-menu/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| footage_menu | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| footage_menu | `report` | `mpv/scripts/footage-menu/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| footage_menu | `reveal` | `mpv/scripts/footage-menu/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| footage_menu | `setup` | `mpv/scripts/footage-menu/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `apply-view` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/workspace/viewsave.lua` |
| gyroflow | `aspect` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `assign-lens` | `mpv/scripts/gyroflow/measure.lua` | `tests/afp_motion_ipc.py` |
| gyroflow | `cancel-export` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `chromatic` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `crop` | `mpv/scripts/gyroflow/crop.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `crop-aspect` | `mpv/scripts/gyroflow/crop.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `crop-dump` | `mpv/scripts/gyroflow/crop.lua` | — |
| gyroflow | `crop-reset` | `mpv/scripts/gyroflow/crop.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `crop-set` | `mpv/scripts/gyroflow/crop.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `denoise-strong` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `edges` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `export` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `tests/exportkind_test.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `export-batch` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/workspace/renders.lua` |
| gyroflow | `export-batch-stop` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/workspace/batch.lua` |
| gyroflow | `export-setting` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `export-start` | `mpv/scripts/gyroflow/export.lua` | — |
| gyroflow | `forget-lens` | `mpv/scripts/gyroflow/measure.lua` | — |
| gyroflow | `gesture` | `mpv/scripts/gyroflow/gestures.lua` | — |
| gyroflow | `horizon-lock` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `tests/afp_motion_ipc.py` |
| gyroflow | `lens` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `lens-dialog` | `mpv/scripts/gyroflow/measure.lua` | `tests/lens_dialog_ipc.py` |
| gyroflow | `lens-estimate` | `mpv/scripts/gyroflow/measure.lua` | — |
| gyroflow | `lens-find` | `mpv/scripts/gyroflow/measure.lua` | `tests/lens_dialog_ipc.py` |
| gyroflow | `lens-forget` | `mpv/scripts/gyroflow/measure.lua` | `tests/lens_dialog_ipc.py` |
| gyroflow | `lens-mode` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua` |
| gyroflow | `lens-quick` | `mpv/scripts/gyroflow/measure.lua` | — |
| gyroflow | `lens-try` | `mpv/scripts/gyroflow/measure.lua` | `tests/lens_dialog_ipc.py` |
| gyroflow | `lens-use` | `mpv/scripts/gyroflow/measure.lua` | — |
| gyroflow | `level` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `live-photo` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `moment-picture` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/workspace/momentsave.lua` |
| gyroflow | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| gyroflow | `orientation` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `photo` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/gyroflow/crop.lua`, `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| gyroflow | `photo-at` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/workspace/marksjob.lua`, `mpv/scripts/workspace/renders.lua` |
| gyroflow | `photo-full` | `mpv/scripts/gyroflow/export.lua` | — |
| gyroflow | `preview` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/workspace/marksjob.lua`, `mpv/scripts/workspace/momentsave.lua` |
| gyroflow | `projection` | `mpv/scripts/gyroflow/main.lua` | — |
| gyroflow | `quality` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `remember` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `remember-quality` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/side-panel/main.lua` |
| gyroflow | `reveal-last-export` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| gyroflow | `rotate` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/viewsave.lua` |
| gyroflow | `underwater` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/viewsave.lua` |
| gyroflow | `view` | `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| highlights | `go` | `mpv/scripts/highlights/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| highlights | `mark` | `mpv/scripts/highlights/main.lua` | `library/src/tests/live_e2e.rs` |
| highlights | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| live_photo | `live-photo-dump` | `mpv/scripts/live-photo/main.lua` | — |
| live_photo | `live-photo-layout` | `mpv/scripts/live-photo/main.lua` | — |
| live_photo | `live-photo-view` | `mpv/scripts/live-photo/main.lua` | — |
| side_panel | `ask` | `mpv/scripts/side-panel/notify.lua` | `mpv/scripts/side-panel/notify_client.lua` |
| side_panel | `bar` | `mpv/scripts/side-panel/main.lua` | `mpv/scripts/agent/main.lua` |
| side_panel | `dump` | `mpv/scripts/side-panel/main.lua` | `tests/afp_motion_ipc.py`, `tests/lens_dialog_ipc.py`, `tests/scrub_ipc.py` |
| side_panel | `notify` | `mpv/scripts/side-panel/notify.lua` | `mpv/scripts/side-panel/notify_client.lua` |
| side_panel | `part-a` | `mpv/scripts/side-panel/main.lua` | `mpv/scripts/side-panel/main.lua` |
| side_panel | `part-b` | `mpv/scripts/side-panel/main.lua` | `mpv/scripts/side-panel/main.lua` |
| side_panel | `popover` | `mpv/scripts/side-panel/main.lua` | `tests/lens_dialog_ipc.py` |
| side_panel | `scrub` | `mpv/scripts/side-panel/main.lua` | — |
| side_panel | `tab` | `mpv/scripts/side-panel/main.lua` | `mpv/scripts/workspace/common.lua`, `tests/afp_motion_ipc.py`, `tests/lens_dialog_ipc.py` |
| view360 | `gesture` | `mpv/scripts/gyroflow/gestures.lua` | — |
| view360 | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| view360 | `view360` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-clear` | `mpv/scripts/view360/main.lua` | `mpv/scripts/side-panel/main.lua` |
| view360 | `view360-frame` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-heading` | `mpv/scripts/view360/main.lua` | `tests/sidepanel_test.lua` |
| view360 | `view360-play-recording` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-projection` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-record` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-reset` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-set` | `mpv/scripts/view360/main.lua` | `mpv/scripts/side-panel/main.lua` |
| view360 | `view360-size` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-tidy` | `mpv/scripts/view360/main.lua` | — |
| view360 | `view360-undo` | `mpv/scripts/view360/main.lua` | — |
| workspace | `autoplay` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/side-panel/main.lua` |
| workspace | `catalog-note` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/gyroflowapp.lua`, `mpv/scripts/gyroflow/measure.lua`, `mpv/scripts/highlights/main.lua` and 1 more |
| workspace | `clips-folder` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/side-panel/main.lua` |
| workspace | `clips-stop` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `cut-after` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua` |
| workspace | `cut-before` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua` |
| workspace | `export-batch-done` | `mpv/scripts/workspace/main.lua` | — |
| workspace | `export-name` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/gyroflow/export.lua` |
| workspace | `init-project` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `input-revert` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/side-panel/main.lua` |
| workspace | `input-save` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/side-panel/main.lua`, `tests/sidepanel_test.lua` |
| workspace | `input-set` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/dlogm/main.lua`, `mpv/scripts/dlogm/uwcolor.lua`, `mpv/scripts/gyroflow/controls.lua`, `mpv/scripts/gyroflow/framing.lua` |
| workspace | `job-done` | `mpv/scripts/workspace/main.lua` | — |
| workspace | `moment-alone` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moment-edit` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moment-play` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moment-remove` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moment-rename` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moment-reveal` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moments-alone` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `moments-folder` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `notify-answer` | `mpv/scripts/side-panel/notify_client.lua` | — |
| workspace | `open-library` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `open-original` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua`, `tests/live_photo_ipc.py` |
| workspace | `photo-link` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/gyroflow/export.lua` |
| workspace | `preview-done` | `mpv/scripts/workspace/main.lua` | — |
| workspace | `project-folder` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `save-moment` | `mpv/scripts/workspace/main.lua` | — |
| workspace | `tag` | `mpv/scripts/workspace/main.lua` | — |
| workspace | `tag-ask` | `mpv/scripts/workspace/main.lua` | `tests/sidepanel_test.lua` |
| workspace | `tag-pick` | `mpv/scripts/workspace/main.lua` | `tests/sidepanel_test.lua` |
| workspace | `tag-rate` | `mpv/scripts/workspace/main.lua` | `tests/sidepanel_test.lua` |
| workspace | `tag-toggle` | `mpv/scripts/workspace/main.lua` | `tests/sidepanel_test.lua` |
| workspace | `view-auto` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/side-panel/main.lua` |
| workspace | `view-own` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |
| workspace | `view-revert` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/side-panel/main.lua` |
| workspace | `view-save` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/main.lua` |

139 messages in 10 plugins. A message with no sender by name is sent by the panel's or menu's tables, a forwarding helper, or the Library's IPC with a computed name.
<!-- END GENERATED: script-messages -->
### user-data keys
<!-- BEGIN GENERATED: user-data -->
<!-- Made by tools/gen_reference.py: don't edit by hand -->

| Key (`user-data/…`) | Written by | Read by |
|---|---|---|
| `afp-scrub/request` | `mpv/scripts/side-panel/scrub.lua` | `tools/scrub/afp-scrub.c` |
| `afp-scrub/shown` | `mpv/scripts/side-panel/scrub.lua` | — |
| `afp/journal/act` | `mpv/scripts/workspace/journal.lua` | `mpv/scripts/workspace/journal.lua` |
| `afp/journal/anchor` | `mpv/scripts/workspace/journal.lua` | `mpv/scripts/workspace/journal.lua` |
| `afp/journal/net` | `mpv/scripts/workspace/journal.lua` | `mpv/scripts/workspace/journal.lua` |
| `afp/photo` | `mpv/scripts/live-photo/main.lua` | `mpv/scripts/gyroflow/crop.lua`, `mpv/scripts/gyroflow/platform.lua`, `mpv/scripts/highlights/main.lua`, `mpv/scripts/live-photo/main.lua` and 6 more |
| `agent` | `mpv/scripts/agent/main.lua` | `mpv/scripts/side-panel/main.lua` |
| `dji_telemetry` | — | `mpv/scripts/side-panel/main.lua` |
| `dji_telemetry/available` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/model` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/position` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/altitude` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/depth` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/exposure` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/gps` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/heartrate` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/height` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/pressure` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/speed` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/temperature` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/shown/water` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/subtitles` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/units` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dji_telemetry/visible` | `mpv/scripts/dji-telemetry/main.lua` | — |
| `dlogm` | — | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/main.lua`, `mpv/scripts/sdr-check/main.lua`, `mpv/scripts/side-panel/main.lua` |
| `dlogm/curve` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/dlogm/uwcolor.lua`, `mpv/scripts/gyroflow/export.lua` |
| `dlogm/denoise` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/dv` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/dv_custom` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/dv_live` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/dv_neutral` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/dv_own` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/gyroflow/export.lua` |
| `dlogm/dv_profile` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/dv_trims` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/hdr` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/log` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/look` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/sdr-check/main.lua` |
| `dlogm/looks` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/maker` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/maker_lut` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/prim` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/recorded` | `mpv/scripts/dlogm/main.lua` | — |
| `dlogm/screen` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/live-photo/main.lua`, `mpv/scripts/sdr-check/main.lua` |
| `dlogm/sdr_look` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/gyroflow/export.lua` |
| `dlogm/uw_ok` | `mpv/scripts/dlogm/main.lua` | `mpv/scripts/dlogm/uwcolor.lua` |
| `footage_menu` | — | `mpv/scripts/side-panel/main.lua` |
| `footage_menu/loaded` | `mpv/scripts/footage-menu/main.lua` | — |
| `gyroflow` | — | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/common.lua`, `mpv/scripts/workspace/main.lua` and 2 more |
| `gyroflow/ask_when_slow` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/aspect` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/available` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/chromatic` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/chromatic_available` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/chromatic_skip` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/crop` | `mpv/scripts/gyroflow/crop.lua` | `mpv/scripts/gyroflow/denoise.lua`, `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/view360/main.lua` |
| `gyroflow/defaults` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/workspace/batch.lua`, `mpv/scripts/workspace/transform.lua` |
| `gyroflow/denoise` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/denoise_strong` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/edges` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/workspace/renders.lua` |
| `gyroflow/edges_blur` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/workspace/renders.lua` |
| `gyroflow/export_settings` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/main.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/exporter` | `mpv/scripts/gyroflow/export.lua` | `mpv/scripts/workspace/batch.lua`, `mpv/scripts/workspace/clippreviews.lua`, `mpv/scripts/workspace/common.lua` |
| `gyroflow/exporting` | `mpv/scripts/gyroflow/export.lua` | — |
| `gyroflow/file_rotation` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/filter` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/frame_aspect` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/gpu` | `mpv/scripts/gyroflow/camera360.lua`, `mpv/scripts/gyroflow/gpupath.lua`, `mpv/scripts/gyroflow/play.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/gpu_delay` | `mpv/scripts/gyroflow/denoise.lua` | `mpv/scripts/dji-telemetry/main.lua`, `mpv/scripts/dlogm/uwcolor.lua`, `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/gyroflow/export.lua` and 4 more |
| `gyroflow/headings360` | `mpv/scripts/gyroflow/camera360.lua`, `mpv/scripts/gyroflow/play.lua` | `mpv/scripts/view360/main.lua` |
| `gyroflow/horizon_lock` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/kind` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/last_export` | `mpv/scripts/gyroflow/export.lua` | — |
| `gyroflow/lens` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/lens_job` | `mpv/scripts/gyroflow/measure.lua` | — |
| `gyroflow/lens_mode` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/lens_quick` | `mpv/scripts/gyroflow/measure.lua` | — |
| `gyroflow/lenses` | `mpv/scripts/gyroflow/measure.lua` | — |
| `gyroflow/level` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/live_photo` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/main.lua`, `mpv/scripts/workspace/batch.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/measuring` | `mpv/scripts/gyroflow/measure.lua` | — |
| `gyroflow/motion` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/optical` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/orientation` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/project` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/project_info` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/projection` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/resolution` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/rig360` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/view360/main.lua` |
| `gyroflow/rotation` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/session` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/workspace/transform.lua` |
| `gyroflow/settings` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/gyroflow/export.lua` |
| `gyroflow/source_aspect` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/spherical` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/view360/main.lua` |
| `gyroflow/still` | `mpv/scripts/gyroflow/still.lua` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/main.lua` |
| `gyroflow/temporal_denoise` | `mpv/scripts/gyroflow/bundle.lua` | `mpv/scripts/workspace/renders.lua` |
| `gyroflow/translation` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/translation_progress` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/underwater` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/unstabilized` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/view` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/waiting` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `gyroflow/water` | `mpv/scripts/gyroflow/bundle.lua` | — |
| `highlights` | `mpv/scripts/highlights/main.lua` | `mpv/scripts/side-panel/bar.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/momentsave.lua` |
| `library-list` | `mpv/scripts/workspace/librarylist.lua` | `mpv/scripts/side-panel/main.lua` |
| `osc/visibility` | — | `mpv/scripts/agent/main.lua` |
| `side_panel/loaded` | `mpv/scripts/side-panel/main.lua` | `mpv/scripts/agent/main.lua`, `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/side-panel/notify_client.lua` |
| `uwcolor` | `mpv/scripts/dlogm/uwcolor.lua` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/workspace/main.lua`, `mpv/scripts/workspace/viewsave.lua` |
| `uwcolor/defaults` | — | `mpv/scripts/gyroflow/bundle.lua`, `mpv/scripts/workspace/transform.lua` |
| `view360` | `mpv/scripts/view360/main.lua` | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/side-panel/main.lua`, `mpv/scripts/side-panel/scrub.lua` |
| `workspace` | `mpv/scripts/workspace/main.lua` | `mpv/scripts/agent/main.lua`, `mpv/scripts/dji-telemetry/main.lua`, `mpv/scripts/dlogm/main.lua`, `mpv/scripts/dlogm/uwcolor.lua` and 13 more |
| `workspace/clip` | — | `mpv/scripts/gyroflow/export.lua` |
| `workspace/moments` | — | `library/src/tests/live_e2e.rs` |

A key read with nothing listed as its writer is covered by a key above or below it (a table written whole, or its fields one by one); `tests/user_data_test.py` checks that every read has a writer.
<!-- END GENERATED: user-data -->
### AFP_* environment variables
<!-- BEGIN GENERATED: env -->
<!-- Made by tools/gen_reference.py: don't edit by hand -->

| Variable | In the product | In tests, rigs and tools |
|---|---|---|
| `AFP_360_CLIP` | `vs-gyroflow/src/nav.rs`, `vs-gyroflow/src/sphere360.rs` | `tools/sphere360/horizon.py` |
| `AFP_360_DISTANCE` | `vs-gyroflow/src/sphere360.rs` | — |
| `AFP_360_IMU` | `vs-gyroflow/src/sphere360.rs` | — |
| `AFP_360_OUT` | `vs-gyroflow/src/sphere360.rs` | `tools/sphere360/horizon.py` |
| `AFP_360_TIMES` | `vs-gyroflow/src/sphere360.rs` | `tools/sphere360/horizon.py` |
| `AFP_360_WIDTH` | `vs-gyroflow/src/sphere360.rs` | `tools/sphere360/horizon.py` |
| `AFP_ARGS` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_BATCH` | `library/src/batch.rs`, `library/src/main.rs`, `macos/player.sh`, `mpv/scripts/agent/main.lua` and 6 more | `tests/agent_test.lua` |
| `AFP_BIN` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_BITRATE` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_CACHE` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_CACHE_GB` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_CATALOG_TIMES` | `catalog/src/cli.rs` | — |
| `AFP_CHROMATIC` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_CHROMATIC_GLSL` | `vs-gyroflow/src/bin/gyroflow-export/post.rs` | — |
| `AFP_COLLABORATION` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_COMPUTER_NAME` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_CURATED_LENSES` | `library/src/index.rs`, `vs-gyroflow/src/lens_pick.rs` | `tests/afp_motion_ipc.py`, `tests/lens_dialog_ipc.py` |
| `AFP_DENOISE` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | `tests/stream_looks_test.py` |
| `AFP_DJI_LUT` | `vs-gyroflow/src/youtube.rs` | — |
| `AFP_EMPTY_THREADS` | `vs-gyroflow/src/bin/afp-cut.rs` | — |
| `AFP_GENERATED` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_GPU_CLIP` | `vs-gyroflow/src/gpu_table.rs` | `vs-gyroflow/tests/export_gpu.rs` |
| `AFP_GPU_EXPORT` | — | `vs-gyroflow/tests/export_gpu.rs` |
| `AFP_GPU_FFMPEG` | `vs-gyroflow/src/bin/gyroflow-export/gpu.rs` | `vs-gyroflow/tests/export_gpu.rs` |
| `AFP_GPU_KEEP` | `vs-gyroflow/src/bin/gyroflow-export/gpu.rs` | `tools/edgefill/chain.sh`, `tools/edgefill/synth/run.py` |
| `AFP_GPU_OUTLIERS` | `vs-gyroflow/src/gpu_table.rs` | — |
| `AFP_GPU_SDR_CLIPS` | — | `vs-gyroflow/tests/export_gpu.rs` |
| `AFP_GPU_SERVER_LOG` | `vs-gyroflow/src/bin/gyroflow-gpu-table.rs` | `tests/gpu_ahead_e2e.py` |
| `AFP_GPU_STRICT` | `vs-gyroflow/src/gpu_table.rs` | — |
| `AFP_GRACE` | — | `catalog/examples/rig.rs` |
| `AFP_HEIGHT` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_HELPER_STATUS` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_HOME` | `catalog/src/project.rs`, `library/src/seen.rs`, `mpv/scripts/live-photo/main.lua`, `mpv/scripts/workspace/inbox.lua` and 1 more | `library/src/tests/e2e.rs`, `library/src/tests/live_e2e.rs`, `library/src/tests/tests.rs`, `tests/gpu_ahead_e2e.py` and 1 more |
| `AFP_INTERP` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_JELLYFIN` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_EVERY` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_KEY_FILE` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_NAMES` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_PATH` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_SCOPE` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_URL` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JELLYFIN_USER` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_JOURNAL` | `library/src/journal.rs`, `mpv/scripts/workspace/journal.lua` | — |
| `AFP_JOURNAL_DIR` | `library/src/journal.rs`, `mpv/scripts/workspace/journal.lua` | `tests/gyroflow_reload_ipc.py`, `tests/journal_mpv_test.sh`, `tests/journal_test.lua`, `tools/journal_report.py` and 1 more |
| `AFP_JOURNAL_PARENT` | `library/src/batch.rs`, `mpv/scripts/workspace/journal.lua` | — |
| `AFP_LIBRARY` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_LISTS` | `library/src/lists.rs`, `library/src/main.rs` | `library/ui-tests/lists.js`, `library/ui/albums.js`, `library/ui/share.js`, `library/ui/source.js` and 1 more |
| `AFP_LIVE_PHOTOS` | — | `tests/live_photo_test.lua` |
| `AFP_LOG_REQUESTS` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_MEASURE_CLIP` | `vs-gyroflow/src/measure.rs` | — |
| `AFP_MOTION_CLIP` | `vs-gyroflow/src/afp_motion.rs` | — |
| `AFP_MOTION_PHOTOS` | — | `tests/live_photo_test.lua` |
| `AFP_NAS_MS` | `catalog/src/store.rs` | — |
| `AFP_OPENING` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_OPTICAL_APPLIES` | `vs-gyroflow/src/optical.rs` | — |
| `AFP_OPTICAL_PROJECT` | `vs-gyroflow/src/optical.rs` | — |
| `AFP_PHOTO` | — | `tests/live_photo_ipc.py` |
| `AFP_PHOTO_CLIPS` | — | `vs-gyroflow/tests/photo_dates.rs` |
| `AFP_PHOTO_RENDITIONS` | `vs-gyroflow/src/bin/gyroflow-export/photo.rs` | `tests/uw_export_check.py` |
| `AFP_PLAYER` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_PLAYER_CONFIG` | `tools/nas-helper/helper.py`, `tools/stream/afp_stream.py` | — |
| `AFP_PORT` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_PROJECTS` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_PUBLIC_URL` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_RANGE_CLIPS` | `catalog/src/range.rs` | — |
| `AFP_RAW_OUT` | — | `tools/edgefill/afpchain.c`, `tools/edgefill/clip.sh`, `tools/edgefill/synth/run.py` |
| `AFP_READ_MBPS` | `vs-gyroflow/src/prefetch.rs` | `tests/gpu_ahead_e2e.py` |
| `AFP_SEGMENT` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_SHARES` | `mpv/scripts/workspace/places.lua`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_STREAM` | `tools/nas-helper/compose.yaml`, `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py` | — |
| `AFP_TEST_CLIP` | — | `vs-gyroflow/tests/table_ahead.rs` |
| `AFP_TEST_ROOT` | — | `tests/journal_mpv_test.lua`, `tests/journal_mpv_test.sh` |
| `AFP_TIMES` | `vs-gyroflow/src/bin/gyroflow-export/gpu.rs`, `vs-gyroflow/src/bin/gyroflow-export/main.rs`, `vs-gyroflow/src/bin/gyroflow-export/photo.rs`, `vs-gyroflow/src/bin/gyroflow-export/post.rs` and 2 more | — |
| `AFP_TURN_CLIP` | `vs-gyroflow/src/in_camera_turn.rs` | — |
| `AFP_TURN_DUMP` | `vs-gyroflow/src/in_camera_turn.rs` | — |
| `AFP_TURN_STEADY` | `vs-gyroflow/src/in_camera_turn.rs` | — |
| `AFP_TURN_WATER` | `vs-gyroflow/src/in_camera_turn.rs` | — |
| `AFP_TV_LOOK` | `tools/nas-helper/helper.py`, `tools/nas-helper/nas_settings.py`, `tools/stream/afp_stream.py` | — |
| `AFP_VERSION` | `.github/workflows/release.yml`, `tools/nas-helper/compose.yaml` | — |
| `AFP_WARP_TEMPLATE` | `vs-gyroflow/src/gpu_table.rs` | — |

81 variables, found by name in code and comments (not docs/). Not variables: `AFP_NS`, an XML namespace constant (albums.rs); `AFP_RPU`, the afp-rpu path in dlogm/main.lua.
<!-- END GENERATED: env -->
### The settings' keys
<!-- BEGIN GENERATED: settings-json -->
<!-- Made by tools/gen_reference.py: don't edit by hand -->

| Key | Takes | Read by | Stands for | What |
|---|---|---|---|---|
| `project` | path | table, export | `--project` | The Gyroflow project the clip plays with; without one, the video itself (gyroflow-export: a .gyroflow only, else it finds the clip's own) |
| `preset` | path | table, export | — | The preset a clip without a project is stabilized with |
| `fov` | number | table, export | `--fov` | Gyroflow's FOV in place of the project's |
| `level` | weak \| medium \| strong | table, export | `--level` |  |
| `wide` | bool | table, export | `--wide` | The wider view (Ctrl+g) |
| `horizon_lock` | number 0–100 | table, export | `--horizon-lock` | Absent: the project's or preset's |
| `underwater` | yes \| no \| auto \| project | table, export | `--underwater` |  |
| `trim_start` | number | table | — | Not the export's: the export's part is the job's (--from, --to, --alone, --whole) |
| `trim_end` | number | table | — | Not the export's: the job's, as trim_start |
| `alone` | bool | table | — | Not the export's: the job's, as trim_start |
| `time_offset` | number | table | — | Not the export's: a moment playing as a video of its own; the export's --from |
| `rotation` | number | table, export | `--rotation` |  |
| `aspect` | string | table, export | `--aspect` | The frame's W:H as it shows |
| `projection` | number 0–4 | table, export | `--projection` | Gyroflow's output projection, 0-4 |
| `lens` | stabilized \| recorded \| only \| off | table, export | `--lens-recorded`, `--lens-only`, `--no-stabilize` | What the warp does: stabilized, stabilized with the lens as recorded, the lens alone; off: nothing (the table is never asked then) |
| `interpolation` | bilinear \| bspline \| bicubic \| lanczos \| ewa | table, export | `--gpu-interpolation` | The GPU warp's (bicubic: bicubic9); absent, the export's by the frame rate, as the player's |
| `engine_interpolation` | bilinear \| bicubic \| lanczos | export | `--interpolation` | The export's pipes path's warp (the quality preset's); absent, Lanczos |
| `edges` | off \| sphere \| smooth | table, export | `--edges` |  |
| `edges_blur` | number | table, export | `--edges-blur` |  |
| `chromatic` | bool | export | `--chromatic` | The water's colour fringes taken out (Ctrl+a), the clip's; the table's gpu_chromatic is it unless this screen skips it |
| `denoise` | bool | export | `--denoise` | The temporal denoise on for the clip (Ctrl+p) |
| `temporal_denoise` | full \| light \| off | export | `--denoise` | How much of it: this computer's, a recipe's, a quality preset's |
| `denoise_strong` | bool | export | `--denoise-strong` |  |
| `denoise_grain` | number 0–1 | export | `--grain` | How much grain the clean-up keeps (gyroflow.conf still_grain) |
| `still_radius` | number 0–7 | export | `--radius` | A photo cleaned up from this many frames each side; absent: 5 where the denoise is on, else 0 |
| `translation` | bool | export | `--with-translation` | Ctrl+j's sideways jerks taken out, where they're computed for the clip (the export: on the GPU path) |
| `curve` | string | export | `--curve` | The log curve (a registry curve, or none); absent: from the metadata |
| `uw` | bool | export | `--uw-color` | The underwater colour on a clip with no curve |
| `uw_strength` | number 0–1 | export | `--uw-strength` |  |
| `uw_highlights` | bool | export | `--uw-highlights` |  |
| `uw_params` | array | export | `--uw-params` | The underwater colour's numbers as the player has them now: dr, db, k, ev |
| `sdr` | maker \| nbcu | export | `--sdr`, `--nbcu` | In SDR: by the maker's LUT or by NBCU's conversion; absent: HDR |
| `sdr_image` | maker \| nbcu \| macos | export | `--sdr-image` | A photo's SDR image |
| `dv_rpu` | path | export | `--dv-rpu` |  |
| `dv_live_l1` | bool | export | `--dv-live-l1` |  |
| `resolution` | string | export | `--resolution` | The export setting size |
| `encode` | fast \| quality | export | `--encode` |  |
| `sphere_width` | number | export | `--sphere-width` | A 360 camera's sphere width (gyroflow.conf sphere_width; 0: the default) |
| `stitching_distance` | number | export | — | Where a 360 camera's lenses' seam is right, in metres (gyroflow.conf; 0: the camera's own) |
| `heading` | travel \| camera \| north \| world | export | `--heading` | What straight ahead follows in a 360 export |
| `input_width` | number | table (request: playback adds it) | — | The video's size |
| `input_height` | number | table (request: playback adds it) | — |  |
| `version` | number | table (request: playback adds it) | — | The source's version; a change reloads |
| `gpu_chromatic` | bool | table (request: playback adds it) | — | chromatic, unless this screen skips it (chromatic_skip) |
| `gpu_denoise` | bool | table (request: playback adds it) | — | denoise, where temporal_denoise isn't off |
| `gpu_translation` | bool | table (request: playback adds it) | — | The translation compensation in the shader where computed (gyroflow.conf translation); on and off by its PARAM |
| `translation_rev` | number | — (request: playback adds it) | — | How often Ctrl+j computed it this session: not read, a new request (and cache key) once it's computed |
| `gpu_sphere` | bool | table (request: playback adds it) | — | The sphere edge fill allowed (default true) |
| `moment_end` | number | table (request: playback adds it) | — | A moment's end in its clip: only its part's motion read |
| `start` | number | table (request: playback adds it) | — |  |
| `until` | number | table (request: playback adds it) | — |  |
| `urgent` | bool | table (request: playback adds it) | — |  |
| `full` | bool | table (request: playback adds it) | — |  |
| `lookahead` | number | — (request: playback adds it) | — | Sent, not read any more |

54 keys, from `mpv/scripts/workspace/registry.json` `settings_json`.
<!-- END GENERATED: settings-json -->

### gyroflow-export flags
<!-- BEGIN GENERATED: export-flags -->
<!-- Made by tools/gen_reference.py: don't edit by hand -->

| Flag | Takes | What (from `--help`) | Passed by name from |
|---|---|---|---|
| `--fingerprint` | <file> | each file's fingerprint, as a link keeps its original's | `mpv/scripts/workspace/common.lua` |
| `--settings` | <json\|file\|-> | the settings JSON the player builds (docs/reference/contracts.md, "The settings"; its keys | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/settings.lua`, `mpv/scripts/gyroflow/translation.lua`, `mpv/scripts/workspace/clippreviews.lua` |
| `--curve` | <curve> | the log curve | — |
| `--uw-color` | <yes\|no> | the player's underwater colour (#273 | — |
| `--uw-strength` | <0..1> | its strength, between the camera's colour (0) and the correction (1, the default) | — |
| `--uw-highlights` | <yes\|no> | the light end stretched into HDR's room (2.5x SDR white for the camera's white); no by default | — |
| `--uw-params` | <dr,db,k,ev> | its numbers as given (the player's at the frame shown), for every frame, not measured | — |
| `--sdr` |  | D-Log M to Rec.709 (DJI's LUT) instead of HLG | `macos/player.sh`, `tools/stream/afp_stream.py`, `windows/launcher/src/main.rs` |
| `--nbcu` |  | to Rec.709 through NBCUniversal's HLG -> SDR conversion instead of HLG (any curve); an HDR file that isn't log through NBCU's HLG or PQ -> SDR | `mpv/scripts/gyroflow/export.lua` |
| `--no-stabilize` |  | the color conversion only, as a clip plays with the stabilization off | — |
| `--lens-only` |  | the lens corrected (and --projection), nothing smoothed | — |
| `--lens-recorded` |  | stabilized with the lens left as recorded (no lens correction, no projection) | — |
| `--horizon-lock` | <%> | the horizon lock's amount (0-100, 0 off) instead of the project's or preset's | — |
| `--underwater` | <yes\|no\|auto\|project> | the lens under water or not (Gyroflow's "Lens is underwater"), or as the camera says over the clip, the default: its In water / Out of water (Osmo Action 6), … | — |
| `--wide` |  | the gyroflow plugin's wider view (Ctrl+g) | — |
| `--level` | <level> | for a video without a project, the stabilization's level | — |
| `--fov` | <x> | Gyroflow's FOV (Advanced) instead of the project's | — |
| `--rotation` | <deg> | the picture turned clockwise by 0, 90, 180 or 270 degrees (the camera on its side), in place of the project's rotation; not stabilized, ffmpeg's transpose | — |
| `--aspect` | <W:H> | the output cut to this aspect ratio after the rotation, such as 16:9, 4:3, 1:1, 4:5, 9:16 | — |
| `--projection` | <p> | Gyroflow's output projection | — |
| `--edges` | <mode> | what fills the edges the stabilized frame leaves | — |
| `--edges-blur` | <%> | blur of the edge fill, % of the frame height (default 2) | — |
| `--denoise` | <off\|light\|full> | the temporal denoise as the player's GPU path plays it (Ctrl+p; its shader, warp.glsl's dn_ passes) | — |
| `--denoise-strong` |  | the denoise's Stronger (the clip's transform | — |
| `--chromatic` | <yes\|no> | the water's color fringes taken out where the lens is under water (Ctrl+a; the player's chromatic.glsl), yes by default; no | — |
| `--prores` |  | ProRes 422 HQ instead of HEVC 10-bit | — |
| `--dv-rpu` | <file> | HLG as Dolby Vision 8.4 (PQ as 8.1 | — |
| `--dv-live-l1` |  | with --dv-rpu | — |
| `--youtube` | <look> | for YouTube | `mpv/scripts/footage-menu/main.lua`, `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/side-panel/exportkind.lua`, `mpv/scripts/workspace/momentsave.lua` |
| `--youtube-lut` | <full\|data> | the LUT's levels | — |
| `--from` | <time> | start at this time (seconds, or m:ss) | `library/src/share.rs`, `mpv/scripts/gyroflow/export.lua` |
| `--to` | <time> | end at this time | `library/src/share.rs`, `mpv/scripts/gyroflow/export.lua` |
| `--out` | <folder> | write here instead of <project>/Exports (or stabilized/) | `library/src/index.rs`, `library/src/share.rs`, `mpv/scripts/gyroflow/export.lua` |
| `--output` | <file> | write this file (one video only) | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/clippreviews.lua` |
| `--alone` |  | --from to --to stabilized on its own | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/momentsave.lua` |
| `--whole` |  | with a project, its "only within the trim range" off, whatever it says | `mpv/scripts/gyroflow/export.lua` |
| `--preview` |  | still  one frame, stabilized as with the options above, in SDR (the maker's LUT for D-Log M, else NBCU's conversion; HLG through NBCU's), held for --from to … | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/clippreviews.lua` |
| `--at` | <time> | the frame's time (default: the middle of --from to --to); a flipbook's | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/clippreviews.lua` |
| `--cover` | <time> | a flipbook's cover art (its icon) | `mpv/scripts/workspace/clippreviews.lua` |
| `--link` | <json> | the link to the original the player keeps in an export (an MP4 key, ch.actionfootageplayer.link | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/clippreviews.lua`, `mpv/scripts/workspace/momentsave.lua` |
| `--dates` | <time> | its date, ISO 8601 with the time zone (2026-10-22T15:28:42+02:00) | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/moments.lua` |
| `--project` | <file> | stabilize with this Gyroflow project instead of the video's own (one video only) | — |
| `--force` |  | replace an existing output instead of skipping the video | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/workspace/clippreviews.lua` |
| `--interpolation` |  | bilinear \| bicubic \| lanczos (default lanczos) | — |
| `--encoder` | <name> | ffmpeg's HEVC encoder, e.g. hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265 (default: VideoToolbox on macOS, else the first of these that works) | — |
| `--resolution` | <r> | the output's short side at most | `mpv/scripts/gyroflow/exportsettings.lua` |
| `--encode` | <fast\|quality> | fast (the default) | `mpv/scripts/gyroflow/exportsettings.lua` |
| `--gpu` | <auto\|on\|off> | the GPU path | — |
| `--gpu-interpolation` | <i> | its warp's interpolation | — |
| `--with-translation` |  | the translation compensation kept for the clip (the player's Ctrl+j), on the GPU path | — |
| `--progress` | <file> | write the progress there as JSON, twice a second | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/translation.lua`, `mpv/scripts/view360/main.lua` |
| `--control` | <file> | read stop / background / normal from this file while running (for Windows, which has no SIGUSR1/SIGUSR2) | `mpv/scripts/gyroflow/export.lua` |
| `--photo` | <time> | the frame at that time as a still instead, with a gain map where there's an HDR look | `mpv/scripts/gyroflow/export.lua` |
| `--translation` |  | the hand's sideways jerks computed for each video instead of exporting it, and kept for | `mpv/scripts/gyroflow/main.lua`, `mpv/scripts/gyroflow/translation.lua` |
| `--radius` | <n> | clean it up from n frames on each side (0-7, default 5; 0 | — |
| `--grain` | <x> | how much of its grain the clean-up keeps (0-1, default 0.25) | — |
| `--camera` | <yaw,pitch,roll,zoom> | the camera turned and zoomed as crop mode has it (the gyroflow plugin's crop.lua; a video export takes it too) | `mpv/scripts/gyroflow/crop.lua`, `mpv/scripts/gyroflow/export.lua` |
| `--crop` | <x,y,w,h> | the part of the frame to keep, in shares of its width and height (0-1; a video export's too), of the frame as --rotation and --aspect have it | `mpv/scripts/gyroflow/export.lua` |
| `--image` | <file.png> | the photo's frame as the player rendered it on the GPU (its warp, crop mode's camera, the look in HLG) | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/gyroflow/exportsettings.lua` |
| `--picture` | <image> | a still's frame as given (the player's frame as shown, an SDR picture) | `mpv/scripts/gyroflow/export.lua` |
| `--sdr-image` | <maker\|nbcu\|macos> | the SDR image | — |
| `--format` | <heic\|jpeg> | HEIC (macOS: afp-photo) or Ultra HDR JPEG (the default elsewhere) | `mpv/scripts/gyroflow/export.lua` |
| `--quality` | <0-100> | the image's quality (default 92) | — |
| `--live` |  | with the motion around it, 1.5 s on each side, as the export plays it (--sdr, --nbcu, --dv-rpu say how; at most 1920 wide or high) | `mpv/scripts/gyroflow/export.lua` |
| `--360` |  | a stabilized 360 video | `mpv/scripts/gyroflow/export.lua` |
| `--reframe` | <keys> | a flat video looking along the head's keyframes, "t:yaw,pitch,roll,fov;..." (seconds of the clip, degrees | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/view360/main.lua` |
| `--measure-hold` |  | with --reframe recording | `mpv/scripts/view360/main.lua` |
| `--heading` | <mode> | what straight ahead follows | — |
| `--sphere-width` | <px> | the sphere's width (default 5760; the 360 video's size, its height half of it) | — |
| `--size` | <WxH> | a reframe's size (default 1920x1080) | `mpv/scripts/gyroflow/export.lua` |
| `--view` | <p> | a reframe's projection | — |
| `--background` |  | start at background priority (SIGUSR1 / SIGUSR2 switch it later) | `mpv/scripts/gyroflow/export.lua`, `mpv/scripts/view360/main.lua`, `mpv/scripts/workspace/clippreviews.lua` |

72 flags, from the argument match in `vs-gyroflow/src/bin/gyroflow-export/options.rs`; the full text is `gyroflow-export --help`.
<!-- END GENERATED: export-flags -->
