# Testing
Checked against 295101c (2026-11-25).

What tests there are, how to run them, and the rigs that need a real clip, a running player or the NAS. The rules
(a fixed bug gets a test, ask before a test takes the screen, "not verified" gets an issue) are in
[CLAUDE.md](../../CLAUDE.md). Traps met while testing are in [gotchas.md](gotchas.md).

## Before you start

- Install `luajit`, `python3`, `node` (22, as CI) and Rust. Cargo is often not on PATH on the Mac: add
  `$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin` (or Homebrew's keg-only `/opt/homebrew/opt/rustup/bin`).
- `ffmpeg` on PATH turns on the parts of `camera_subs_test.lua` and `link_test.lua` that make MP4s. Without it they
  skip and say so.
- `tags_test.lua` checks Finder tags against `plutil` and `xattr` where they exist (a Mac).
- Work in your own worktree. Other sessions build and run tests in theirs at the same time.

## Everything CI runs, locally

```
sh tools/run_tests.sh                       # every tests/*_test.{lua,py,sh}
(cd vs-gyroflow && cargo test --release)
(cd catalog && cargo test --release)        # CI uses --release: the random run is slow without it
(cd library && cargo test)
(cd library/ui-tests && npm ci && npm test) # the Library page in jsdom
GYROFLOW_EXPORT=vs-gyroflow/target/release/gyroflow-export luajit tests/link_test.lua
```

The last line is CI's "Preview test": a preview made by this build's `gyroflow-export`, read back by the player's
link reader.

### tools/run_tests.sh

- It finds tests by name: `tests/*_test.lua` (run with `luajit`), `tests/*_test.py` (`python3`), `tests/*_test.sh`
  (`sh`). A new test needs no list anywhere.
- A `*_test.lua` with a `*_test.sh` of the same name is run through the `.sh` only (`journal_mpv_test`).
- Each runs from the repository's folder. All run even when one fails. The exit is 1 if any failed, and the last line
  names them.
- Filter by a part of the path: `sh tools/run_tests.sh workspace` runs every file whose path contains "workspace".
- Rigs are named otherwise on purpose (`*_ipc.py`, `*_e2e.py`, `photo_link_frame.py`, `uw_export_check.py`), so
  the runner skips them. Helpers that are not tests (`twins.py`, `make_equirect.py`) skip the same way.
- The whole run takes a few seconds.

### The cargo crates

| Crate | Command | What |
|---|---|---|
| `vs-gyroflow` | `cargo test --release` | Unit tests in `src/` (workspace cases, gyroflow.conf cases, the user shader parser on every `.glsl` of ours, the GPU table, …) and `tests/` (`export_gpu.rs`, `photo_dates.rs`, `table_ahead.rs`). The `tests/` files and some unit tests need a real clip named by an environment variable and skip without it (below). |
| `catalog` | `cargo test --release` | `tests/scenarios.rs` (one computer, two, Finder, lost connections), `tests/random.rs` (three computers and Finder on a flaky simulated NAS, 200 seeds; `CATALOG_SEEDS=n` for more), `tests/safeguards.rs` (docs/design/catalog-safeguards.md's promises), `tests/cost.rs` (ignored: prices, not checks). |
| `library` | `cargo test` | The Library's Rust side: renders and their recipe (`tests/render_settings.json`, `tests/gyroflow_conf_cases.json`), the index and `GEOTAG`, the scrub sidecar rule, tags, the catalog glue. Tests that set `SEEN` take `seen::TESTS`; tests get their own data and cache folders through `AFP_HOME`. |

Tests that need a clip, a share or a player:

| Test | How to run | What it needs |
|---|---|---|
| `vs-gyroflow/tests/export_gpu.rs` | `AFP_GPU_CLIP=<clip> AFP_GPU_FFMPEG=<ffmpeg with libplacebo> cargo test --release --test export_gpu` | GPU export against the pipes, 2 s each way. `AFP_GPU_EXPORT=<app's gyroflow-export>` instead of the ffmpeg. `AFP_GPU_SDR_CLIPS` for the SDR colour check. |
| `vs-gyroflow/tests/table_ahead.rs` | `AFP_TEST_CLIP=<clip with motion data> cargo test --release --test table_ahead` | The table extended ahead is bit for bit the same where settled. |
| `vs-gyroflow/tests/photo_dates.rs` | `AFP_PHOTO_CLIPS="<clip>\|<start, local>\|<zone or ->;…" cargo test --release --test photo_dates` | `exiftool`. A photo carries the moment it shows. |
| `afp_motion::tests::same_motion_as_the_project` | `AFP_MOTION_CLIP=<clip with an optical Gyroflow project> cargo test --release --lib afp_motion::tests::same_motion_as_the_project -- --nocapture` | `.afp-motion` and the project give the same correction. |
| `gpu_table` tests | `AFP_GPU_CLIP=<project.gyroflow> cargo test --release --lib gpu_table` | The warp GLSL run through naga on the GPU against gyroflow-core. |
| Library `#[ignore]` tests | `cargo test -- --ignored <name> --nocapture` | `audit_on_a_share` (`AFL_AUDIT_DIR` on a share), `batches_show_as_they_land` (`AFL_E2E_DIR`, `AFL_E2E_CLIP`, `AFL_PLAYER` = a player app), `where_the_time_goes` (`AFL_PERF_DIR`, docs/design/library-responsiveness.md). |
| catalog `cost` | `cargo test --release --test cost -- --ignored --nocapture` | Nothing; prints what each step costs at 10k and 100k clips. |

List every ignored test of a crate with `cargo test -- --ignored --list`. Grep for `env::var` in a test to see what
it wants.

### library/ui-tests

The Library's page (`library/ui`) runs in jsdom against a stand-in for the app. It lives outside `ui/` because Tauri
bundles `ui/` whole.

- `npm ci && npm test` runs `node --test *_test.js`. CI runs the same.
- `page.js` opens the page as the window loads it; `fakeapp.js` answers the changing commands as the disk would;
  `mock.js` is a small project in memory.
- A new test is a new `*_test.js` there. No list.
- `npm run perf` (`page_perf.js`) times the page on a big project. See docs/design/library-responsiveness.md.

Older page tests by hand live in `library/tests/` (`albums.js`, `share.js`, `views.js`, `ui_fixes.js`). Run them with
`JSDOM=<path to node_modules/jsdom> node library/tests/<file>.js library/ui`. They are not in CI.

## What CI runs

One workflow, [.github/workflows/release.yml](../../.github/workflows/release.yml). It runs on every pull request,
every push to `main`, every `v*` tag, and by hand. A newer push to a PR cancels its older run.

| Job | Runner | When | Tests |
|---|---|---|---|
| `macos` | Anton's Mac (`anton-mac`) for this repository's PRs and `main`; macos-15 for tags, forks, by hand | always | Builds our mpv (incrementally on the Mac, cached on GitHub), then `sh tools/run_tests.sh`, `cargo test --release` in `vs-gyroflow`, the release build, then the "Preview test" (`link_test.lua` with this build's `gyroflow-export`). On the Mac, `dist/` is emptied first and a crate's `target/` over 20 GB is cleaned at the end. |
| `tests` | as `macos` | always | `catalog`: `cargo test --release`; `library`: `cargo test`; `library/ui-tests`: `npm ci && npm test`. |
| `plugin` | ubuntu-22.04, windows-2022, windows-11-arm | `main`, tags, by hand | Builds the gyroflow plugin zip; on Windows the app's installer and a smoke test: silent install, files present, mpv plays a test source with `--vo=null`, `gyroflow-probe` runs (no GPU there: no stabilization). |
| `nas-helper` | ubuntu-latest | `main`, tags, by hand | Builds the Docker image; smoke test: `--help`, `--check` on a fake project, `mpv --version` inside. |
| `plugins` | ubuntu-latest | `main`, tags, by hand | Bundles all plugins per system. |

`publish` and `website` run only for a tag ([release.md](release.md)). `main` is protected (admins too): a change
goes in by a PR, up to date with `main`, with `macos` and `tests` green. The Windows, Linux and NAS jobs don't run on
pull requests; they run on every push to `main` and every release, so after a merge look at `main`'s run
(`gh run list --branch main`) for a break there, and fix it in a PR of its own.

CI has no GPU, no screen, no NAS and no real clips. Everything below is for you to run.

## What each file in tests/ covers

"CI" runs in `tools/run_tests.sh`. "Live" needs a player, clips, the GPU or the NAS: run it yourself.

| File | What | Where |
|---|---|---|
| `agent_test.lua` | Agent plugin: IPC socket and `session.json`; a batch (`AFP_BATCH`) touches neither | CI |
| `camera_subs_test.lua` | Telemetry subtitles a camera wrote (DJI drone `.SRT`, embedded tracks) | CI (ffmpeg part skips without it) |
| `curves_twin_test.lua` | Every curve the player plays reaches `gyroflow-export` as a curve it takes (#291) | CI |
| `cut_test.lua` | Cutting a spoiled part out: parts kept, names, marks and moments | CI |
| `dnstep_test.lua` | Frame steps with the GPU denoise, against a model of the shader's ring | CI |
| `export_args_test.lua` | Every export route hands `gyroflow-export` the clean-up as it plays | CI |
| `export_choices_test.py` | Export choices agree: player, Share dialog, render recipe, `gyroflow-export` | CI |
| `exportkind_test.lua` | The Output tab's Export button, its kinds and list | CI |
| `exportsettings_test.lua` | Export settings, defaults and their arguments | CI |
| `fmp4time_test.py` | `tools/stream/fmp4time.py`: render segments on one timeline | CI |
| `gpu_ahead_test.lua` | `ahead.lua` keeps the table ahead of playback, against a model server | CI |
| `gyroflow_conf_test.lua` | gyroflow.conf as defaults, from `gyroflow_conf_cases.json` (also read by cargo in `library/`) | CI |
| `gyroflow_lens_test.lua` | The output lens over a clip's own Gyroflow project (#196) | CI |
| `jellyfin_test.py` | `tools/stream/jellyfin.py`: where items go while the catalog is pending | CI |
| `journal_test.lua` | Responsiveness journal, player part, with mpv stood in for | CI |
| `journal_mpv_test.sh` (+ `.lua`) | The journal's hooks in a real mpv (`--vo=null`): `$MPV_BIN`, else `build/mpv-afp/out/mpv`, else PATH; skips with none | CI |
| `journal_report_test.py` | `tools/journal_report.py` on a made-up journal | CI |
| `librarylist_test.lua` | A list opened from the Library: which files, their head | CI |
| `link_test.lua` | Link reader over MP4s ffmpeg made and changed; with `GYROFLOW_EXPORT`, over a real preview | CI (twice) |
| `live_photo_test.lua` | What a photo is: Exif, Motion Photo XMP, Live Photo MOV, chains. `AFP_LIVE_PHOTOS`/`AFP_MOTION_PHOTOS` for real files | CI |
| `longpath_test.lua` | Windows long paths (`\\?\`), no Windows needed | CI |
| `moments_test.lua` | Moments: names, dates, originals, the Moments folder | CI |
| `navkeys_test.lua` | Modifier arrows per platform; a key in your input.conf is left alone | CI |
| `notify_test.lua` | Cards and the name dialog | CI |
| `packaging_test.py` | Every Lua module a plugin requires is copied by `make_app.sh`, `make_windows_app.sh` and `install.sh` | CI |
| `photo_link_fold_test.lua` | A photo link's crop folded into its camera | CI |
| `project_folders_test.py` | The project's own folders are left out everywhere footage is listed | CI |
| `projectcache_test.lua` | Project lookup kept between runs, through `.afplocation` | CI |
| `projectfile_test.lua` | `Project.afproject`: read, written with unknown keys kept | CI |
| `projectwatch_test.lua` | Ctrl+e's watch on a Gyroflow project save | CI |
| `render_keys_test.py` | Render keys and valid values: `transform.lua` and `renders.rs` | CI |
| `render_settings_test.lua` | Render settings case by case from `render_settings.json` (also read by cargo in `library/`) | CI |
| `script_messages_test.py` | Every `script-message-to` reaches a registered message; no binding takes one over (#225) | CI |
| `scrub_test.lua` | Seek bar picture: sidecar names (`scrub_sidecar_cases.txt`, shared with the Library), size, place | CI |
| `sidecar_test.lua` | The clip's own file `<clip>.afp.json` | CI |
| `sidepanel_test.lua` | The side panel over a stand-in mpv: draws, and clicks land where it drew | CI |
| `tags_test.lua` | Tags, rating, Finder tags as a binary plist | CI |
| `timeline_test.lua` | Navigation bar and Playlist rows | CI |
| `transform_test.lua` | Input and transform bundles | CI |
| `user_data_test.py` | Every `user-data/<key>` read is written somewhere | CI |
| `uwcolor_test.lua` | Underwater colour rule on made-up frames (`uwcolor_cases.json`) | CI |
| `view360_test.lua` | 360 view geometry, gestures, the view360 script | CI |
| `workspace_test.lua` | Workspace lookup over `workspace_cases.txt` (also read by cargo in `vs-gyroflow` and `catalog`) | CI |
| `afp_motion_ipc.py` | A clip with `.afp-motion` plays stabilized; Ctrl+m measures; lens assigned | Live, headless |
| `dnstep_ipc.py` | Frame steps and the paused frame with the GPU denoise, in the app's mpv | Live, headless |
| `gyroflow_reload_ipc.py` | The open player takes up a Gyroflow project saved while it plays | Live, headless |
| `lens_dialog_ipc.py` | The player's lens dialog: Quick Find, Enter, measured again | Live, headless |
| `librarylist_ipc.py` | A Library list played as a playlist | Live, headless |
| `live_photo_ipc.py` | Live photos: hover, Space, chains, lists, HDR stills | Live, headless |
| `navkeys_ipc.py` | Modifier arrows in a running mpv | Live, headless |
| `scrub_ipc.py` | Seek bar pictures with `afp-scrub` | Live, headless |
| `gpu_ahead_e2e.py` | The table kept ahead, real time, against `gyroflow-gpu-table --serve` | Live, headless (a real clip; a share for the real case) |
| `photo_link_frame.py` | A photo link's frame in the player against Share's render | Live (two pictures you make) |
| `uw_export_check.py` | Underwater colour in exports against the player's shader | Live, headless (real clips, the app's ffmpeg) |
| `make_equirect.py` | Makes synthetic 360 clips for view360 | Helper |
| `twins.py` | Helpers for the guard tests | Helper |

## Live rigs

### Rules for every live run

- **A rig that takes the screen needs Anton's yes first,** in this session, for a stated time. Ask again after any
  break and before going past the time. Say what will run, for how long, and what he will see. Offer a short run and
  a longer one with room for follow-ups. A go from another session does not count.
- Headless rigs (`--vo=null`, no window, no focus taken) need no ask. All `tests/*_ipc.py` are headless.
- **mpv with `vo=gpu-next` hangs while the Mac is locked or its display sleeps.** Check before a windowed run:

  ```
  ioreg -n Root -d1 | grep CGSSessionScreenIsLocked
  ```

  Run windowed tests under `caffeinate -d` (only with Anton's yes: it keeps his display awake). If the screen is
  locked, measure headless instead and leave a script for later.
- Rigs clone the app you pass (some default to `~/Applications`, which others replace at any time) into a temporary
  folder. Those clones register with Launch Services: unregister them after ([release.md](release.md)).
- IPC socket paths must be short (macOS: under 104 characters). The rigs use `/tmp/<short>.sock`.
- Write a rig's output to a file. Output piped through `grep` or `cut` is buffered until exit and lost if you kill
  the run.
- Never point a rig at a clip in Anton's Library with `--force`, and check the first output path before looping
  (see [gotchas.md](gotchas.md), "Exports").

### The tests/*_ipc.py rigs

They start the app's own mpv (`Contents/Helpers/mpv`) with `--vo=null`, this checkout's plugins copied into a clone of
the app, and drive it over mpv's JSON IPC. The stabilization runs with `vo=null` (the rig patches the plugin's
gpu-next check): mpv parses the warp but draws nothing. The data folders, the journal and gyroflow.conf go into the
rig's own folder. `--keep` leaves the rig's folder for a look afterwards.

```
python3 tests/afp_motion_ipc.py [--keep] [--shots <dir>] <ActionFootagePlayer.app> <clip with an optical Gyroflow project beside it>
python3 tests/dnstep_ipc.py [--keep] <app built from this checkout> <clip with motion data>
python3 tests/gyroflow_reload_ipc.py [--lua <dir>] [--table <gyroflow-gpu-table>] [--cases edit,twice,appear] [--keep] <app> <clip with motion data>
python3 tests/lens_dialog_ipc.py <repo> <out folder> <folder with a clip and its .afp-motion>
python3 tests/librarylist_ipc.py --mpv <mpv> [--afp-cut <afp-cut>] [--work <dir>]
python3 tests/live_photo_ipc.py <folder of live photos in a row> [<a plain photo>]
python3 tests/navkeys_ipc.py --mpv <mpv> [--work <dir>] [--platform-mac | --platform-other]
python3 tests/scrub_ipc.py --mpv <mpv> --helper <afp-scrub> --clip <clip with .LRF> [--clip2 <clip without>]
```

- `afp_motion_ipc.py` and `lens_dialog_ipc.py` also copy `vs-gyroflow/target/release` binaries in: build them first
  (`cargo build --release`). Lenses go to the rig's own folder (`AFP_CURATED_LENSES`), never the user's.
- `librarylist_ipc.py`, `navkeys_ipc.py` and `scrub_ipc.py` need `ffmpeg`. Build `afp-scrub` with
  `sh tools/scrub/build.sh <out>`.
- Each script's docstring lists its cases and what passes. Read it first.

### Panel render rig (pictures of the side panel without a window)

The side panel writes what it draws as an `.ass` file. The rigs put it over a frame as subtitles:

```
script-message-to side_panel dump /abs/path/panel.ass 1920 1080 1
sub-add /abs/path/panel.ass select
screenshot-to-file /abs/path/panel.png subtitles
sub-remove
```

- Send these over IPC to a headless player, with absolute paths, and wait for the `.ass` before `sub-add`.
  `afp_motion_ipc.py --shots <dir>` does this for two tabs; copy its `shots()` (`… side_panel tab <name>` first).
- `vo=image` and `vo=null` draw no OSD, so the panel comes in as subtitles. Homebrew's ffmpeg has no libass.
- `tests/sidepanel_test.lua` loads the panel over a fake mpv and clicks where it drew. Copy it for a new panel test.

### Library page pictures (headless Chrome)

For the before/after pictures a UI PR carries. Chrome runs with its own profile; Anton's Chrome is not touched.

```
cd library/ui-tests && npm ci
node shot.js <state> <out.png> [--ui <another ui folder, e.g. main's>]
node tiles_shots.js <out folder> [--ui <ui>] [--prefix before-]
node source_shots.js <out folder> [--ui <ui>] [--prefix before-]
node rename_shots.js <state> <out.png> [--ui <older ui>] [--dark]
```

- The states are at the top of each script. For "before", pass `--ui` with an older checkout's `library/ui`.
- `CHROME=<path>` names another browser. `SHOT_DEBUG=1` dumps the DOM instead of a picture.
- `library/tests/shots.js` and `views_shots.js` are older, puppeteer-based:
  `PUPPETEER=<path to node_modules/puppeteer-core> node library/tests/shots.js <ui folder> <out folder> [dark]`.
- Windows compile check of the Library on the Mac (needs `llvm-rc`):
  `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` in `library/`.

### gpu_ahead_e2e.py (the table ahead of playback)

```
tests/gpu_ahead_e2e.py vs-gyroflow/target/release/gyroflow-gpu-table <clip> <out prefix> [--seconds 120] [--video]
    [--throttle MBps] [--catalog <project>] [--walk <dir>...] [--old] [--seek t:pos,...] [--loop a:b] [--cold]
```

- Without a NAS, `--throttle 8` on any clip with motion data acts as a slow share.
- `--cold` drops the clip from this Mac's file cache (`msync MS_INVALIDATE`, no sudo). The NAS's own cache stays.
- It sets `HOME` to a scratch folder next to `<out prefix>`, so shaders and caches don't land in
  `~/Library/Caches/ActionFootagePlayer`. Do the same for any rig that runs `gyroflow-gpu-table` or `gyroflow-export`
  by hand: they cache by `HOME`.
- Runs on the NAS share read only. Measure the NAS's disk I/O before and after, and run only what Anton started
  (CLAUDE.md, "The NAS runs only what Anton starts").

### uw_export_check.py (underwater colour, export against player)

```
python3 tests/uw_export_check.py --exporter vs-gyroflow/target/release/gyroflow-export \
    --app ~/Applications/ActionFootagePlayer.app <clip>@<seconds> ...
```

It runs the player's shaders through the app's ffmpeg (libplacebo filter) with the app's MoltenVK
(`VK_ICD_FILENAMES`), PARAM defaults filled in. Writes only into `--work`. Needs `numpy`.

### photo_link_frame.py

```
python3 tests/photo_link_frame.py <player's frame> <gyroflow-export's photo> [<control>]
```

Make the player's frame with `screenshot-to-file <out> video` on the link, paused. Make the photo with
`gyroflow-export --photo <t> --format jpeg --curve none --sdr-image nbcu --camera … --crop … <clip>`. The header has a
worked example. Pass: no shift, edge correlation well above the control. Needs `numpy`.

### tools/pipebench (shader pipeline builds)

docs/design/gpu-render.md, "Pipeline builds". Two tools:

- `pipebench.c`, offscreen, no window:

  ```
  tools/pipebench/build.sh <dir> [--plain]     # --plain: without tools/libplacebo-patches, for "before"
  VK_ICD_FILENAMES=<app>/Contents/Resources/vulkan/icd.d/MoltenVK_icd.json \
      <dir>/pipebench [name=value ...] -- a.glsl b.glsl -- a.glsl b.glsl c.glsl
  ```

  Each `--` starts a phase (the shader list after a change). It prints SPIR-V compiles, pipelines and their ms per
  phase. `PB_CACHE=<dir>/<prefix>` keeps a disk cache like mpv's; `PB_SIZE`, `PB_OUT`, `PB_FRAMES`, `PB_DYNAMIC=1`,
  `PB_LOG=1`. Needs Homebrew's shaderc, little-cms2, vulkan-loader, vulkan-headers.
- `live_starts.py`, the real player. **It opens a window: ask Anton first.**

  ```
  tools/pipebench/live_starts.py <ActionFootagePlayer.app> <out dir> <clip> [<clip> ...]
  ```

  One process per clip, plays 8 s, turns the underwater colour on and off, quits. It uses the shared shader cache
  (`~/Library/Caches/io.mpv`) but copies the plugins' data and journal into `<out dir>`.

Measure with `pipebench` before a live run. libplacebo logs "Spent N ms" only above 10 ms; use
`--msg-level=vo/gpu-next/libplacebo=debug` to get every pass's source.

### tools/catalog_rig.py and catalog/examples/rig.rs (the catalog on a real NAS)

- `tools/catalog_rig.py` runs on the NAS side. It makes a test project fast on the NAS's own disk, changes it as
  Finder on another computer would, and appends journal lines as another computer:

  ```
  python3 catalog_rig.py make <base> <days> <per>
  python3 catalog_rig.py finder <base> <seed> <n>
  python3 catalog_rig.py append <base> <computer> <n>
  python3 catalog_rig.py touch <dir>
  ```

- `catalog/examples/rig.rs` runs on the Mac against the share and prints time and disk operations per step:

  ```
  cd catalog && cargo run --release --example rig -- <project> <footage> <computer> <cache> \
      build | load | player <hash> | refresh | sweep | deep | compact | clean | check | note <path> | stress <s> <seed> | visible <path> <s>
  ```

- Put test projects in a folder of their own and remove them afterwards. Deletes on a share may land in the share's
  `#recycle` folder: clean that too.
- Ask Anton before any NAS run. It needs access he grants for a limited time.

### tools/latency360 (input to light, 360 view)

```
tools/latency360/run.sh <log> <clip> [--pause] [--hz=N] [--seconds=S] [--screen=NAME] [--fs] [--sync=X] [-- mpv args]
python3 tools/latency360/summarize.py <log>
```

**Takes the screen** for the run's seconds plus ~3 s: ask first. Uses `build/mpv-afp/out/mpv` (or `$MPV`) with
patch 0010's `--macos-latency-log`. `--hz=N` makes synthetic two-finger scrolls; without it, real fingers.
`--screen` takes the NSScreen name (for example "Built-in Retina Display").

### tools/scrub (seek bar pictures)

```
sh tools/scrub/build.sh <out>                                   # afp-scrub
tools/scrub/scrubbench.py [--helper path] [--cold] [--n N] <clip> [<sidecar>]
tools/scrub/dnrepro.py <clip>                                   # frame steps with the denoise ring, headless
```

`dnrepro.py` uses the installed app's mpv with `--vo=null`. See docs/design/seek-and-scrub.md.

### tools/edgefill (sphere edge fill, offscreen)

See [tools/edgefill/README.md](../../tools/edgefill/README.md) and
[tools/edgefill/synth/README.md](../../tools/edgefill/synth/README.md). In short:

1. Keep the export's shader:
   `AFP_GPU_KEEP=1 gyroflow-export --gpu on --edges sphere --wide --sdr --from <t> --to <t+0.2> --output x.mp4 <clip>`
   leaves `.x.partial.glsl` next to the output. It needs an ffmpeg with libplacebo (`AFP_GPU_FFMPEG`, or the app's
   `Contents/Helpers/ffmpeg`).
2. `cc -O2 -o afpchain afpchain.c $(pkg-config --cflags --libs libplacebo)`.
3. `chain.sh <shader> <clip> <start> <frames> <W> <H> - -` for per-pass GPU times; `clip.sh … out.mp4` for a video.
4. `synth/run.py` compares the shader with a plain model on synthetic scenes.

### tools/export-gpu (the export's GPU path)

- `build_ffmpeg_libplacebo.sh`: ffmpeg with our libplacebo, into `FFPL_DIR` (default
  `~/Library/Caches/afp-ffmpeg-build`, no spaces). About 10 minutes once; `tools/dev_app.sh` runs it by itself.
- `check_ffmpeg_range.sh <ffmpeg>`: a full-range clip stays full range (patch 0002). `cmp_frames.py A B W H n...`:
  per-plane differences of two videos. `afp_render_spike.m`: the zero-copy spike (docs/design/export-gpu-path.md).
- To iterate on an export shader without rebuilding: `AFP_GPU_KEEP=1 AFP_TIMES=1 gyroflow-export …` keeps the
  shader and prints the ffmpeg command. Edit the kept `.partial.glsl` and run that command again.

### Other measuring tools

- `tools/journal_report.py [--since 7d] [--json <file>]`: where the waiting is in the responsiveness journal
  (`~/Library/Logs/ActionFootagePlayer/journal`). docs/design/responsiveness-journal.md.
- Memory over a session: drive settings changes over IPC (`script-message-to gyroflow …`) and watch
  `footprint -p <mpv pid>`.
