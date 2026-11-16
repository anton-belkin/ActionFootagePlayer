# Share and photo links, made to work for real (2026-11-16)

Branch `claude/fix-share-player-4a8d39`, cut from `claude/night-integration-fd9085` (PR #241). Before this:
`docs/night-integration.md`, `docs/night-handover-share.md`, `docs/albums-photos-share-concept.md`.

## What was fixed

### 1. Share of video works, on real camera clips

Last night no video render had landed. It works now: a moment, a photo link and a whole clip, shared to a folder, are
rendered by the player's `share` batch into `_Renders/<hash>.<profile>.<ext>` and copied out, HDR and SDR, on four
real clips (see "What was run" below). Nothing in the render path itself was broken. What was missing was a real
clip and a test that ran the whole way. The test (`share_e2e::a_share_with_the_player`) now also:
- takes any clip (`AFL_SHARE_CLIP`, its path in the trip) and writes a `Project.afproject` when the trip has none;
- hands over what the cache already has, as the `share` command does (it used to wait for those for 15 minutes);
- fails when the player says it would have rendered with other settings than the recipe (item 3).

### 2. Nothing to change: the original, or its part cut losslessly

When gyroflow-export says "…not log: nothing to change" (no motion data, no log curve, no turn or crop), Share no
longer fails (`share.rs` `as_recorded`):
- **a clip**: the original is copied out as it is, under its own extension (`P1060688.mp4`); nothing goes into the
  cache;
- **a moment**: its part of the original, cut without re-encoding, from the keyframe at or before its start (so a
  moment of 0.5–2.5 s gave 2.83 s of a Lumix clip). It is cut by `afp-cut` (checked to read as the original does);
  for a file afp-cut can't read, it falls back to the player's ffmpeg copying the streams (`-c copy`, first video
  and audio track). The Lumix clip needed that fallback: afp-cut said "ctts in a track's sample tables isn't handled".
  The cut is kept as `_Renders/<hash>.<profile>.mp4`, so sharing it again is a cache hit;
- the item says which it was ("nothing to change: the original as it is" / "…its part of the original, cut without
  re-encoding").

### 3. The recipe and the render can't diverge

Why they could: the Library hashed the *inputs* it saw (the clip's `view`, its `source`, the moment's `stab`, part
of `gyroflow.conf`). The player then worked out its own settings from its in-process defaults. The gaps:
- a moment with no bundle of its own renders with the clip's saved view, which wasn't in its recipe;
- `alone` counts only for a snapped moment (`snap`), but the recipe took it from every moment;
- a moment without its own Gyroflow project uses the clip's `.gyroflow` (gyroflow-export finds it), which wasn't in
  its recipe; with no project at all, the player's preset (`default.gyroflow`) decides, and that wasn't in the recipe
  either;
- the preset's horizon, the player's built-in option defaults, and `edges` / `edges_blur` / `temporal_denoise` are
  invisible to a parse of `gyroflow.conf`;
- under `cargo test` the Library read no player defaults at all. That is why last night's `source.curve` run
  looked as if the player ignored the curve.

Now there is one rule, written down in one place and checked on both sides:
- **The settings are worked out once, by the Library, the player's way.** `renders::settings()` is a Rust copy of
  the player's `transform.render_settings()` (the new home of what `made_view_of` did): the bundle is a moment's own,
  else the clip's saved view, else the defaults; it's made whole with the defaults, and the clip's `source` (an old
  `view`'s curve, turn and underwater moved there first) goes over it. **`tests/render_settings.json`** holds 12
  cases with the expected result. `tests/render_settings_test.lua` (in `release.yml` now) and
  `renders::tests::settings_are_worked_out_as_the_player_does` (cargo) both check them. A change on one side
  only fails one of them.
- **The defaults come from the player itself.** Whenever it starts (a batch too), the player writes
  `<its data>/gyroflow/render-defaults.json` (`~/Library/Application Support/ActionFootagePlayer/gyroflow/` on a
  Mac, `%LOCALAPPDATA%\ActionFootagePlayer\gyroflow\` on Windows): its defaults bundle (with the preset's horizon),
  `edges`, `edges_blur`, `temporal_denoise` and the preset file it uses. The Library reads that file, else works the
  same values out of `gyroflow.conf` (`AFL_PLAYER_DEFAULTS` / `AFL_PLAYER_OPTS` point elsewhere).
- **The recipe (v3)**: `v, kind, clip {hash | path, size}, in / out / alone, t / crop / camera, settings, opts {edges,
  edges_blur, temporal_denoise}, gyroflow` (the project used: the moment's own, else the clip's), `preset` (its
  fingerprint, only when there is no project), `profile`. `renders::VERSION` went from 2 to 3, so every render is made
  again once.
- **The player renders with exactly what it's sent.** Each share item carries `view` (settings + opts) and `alone`.
  `render_moments` uses them instead of working its own out, and `export.lua` takes `edges`, `edges_blur`,
  `temporal_denoise` from that view.
- **…and says what it would have used by itself.** The batch's status has `used: [{ out, player }]`.
  `share::collect` compares it with the recipe (`renders::differs`). When they differ (the player's defaults changed
  since it last wrote its file) the render is still handed over and kept, because it is what its hash says, and the
  item says which settings differ. The player has rewritten its file by then, so the next share makes the new
  recipe. The real-clip runs assert that they don't differ: they did differ, until the test read the player's defaults.

### 4. The player makes photo links

Per the concept ("Photos as objects": the key makes the photo object at once, nothing is rendered while watching):
- **`Alt+s`** and **the crop mode's Enter**, in a project: the frame as it shows now (mpv's screenshot, `video`
  mode: the look and the GPU warp, no OSD) is cropped to Save frame's frame and shrunk to at most 1280 px on its long
  side by ffmpeg. `afp-meta --link` writes its link (`kind: photo`, `clip` by its path in the project, `size`,
  `created`, `t`, `crop`, `camera`, `name`, `roots`) as
  `_Stills/Generated[/<date>]/<clip>_<t>s.afplink.jpg` (named as the Library names its photo links), and the catalog
  is told. Playback doesn't stop. The Library shows it in albums, and Share renders it (the same `photo-at` path that
  was run here on all four clips). Checked headless: afp-meta's link on an ffmpeg JPEG made with the same filter
  (1280×720, 180 KB) reads back as a photo link.
- **The full render stays, as `Alt+Shift+s`** (script message `photo-full`): gyroflow-export `--photo` now, HEIC
  with a gain map, Live / Motion Photo as `live_photo` says, as `Alt+s` did. **Outside a project, `Alt+s` does the
  full render too**: with no project there is nothing to keep, show or share a link. My reasons for keeping it: a
  photo wanted right now, or a Live Photo, which a link doesn't make yet (Share renders a still).
- `workspace/main.lua` stays under the 200-locals limit: the handler is in its own `(function() … end)()`.
  `gyroflow/main.lua` has no locals to spare either, so the "last written" text is a key on its `PRESETS` table.

### 5. Clear the render cache

Project settings → **General → Renders**: `_Renders`' room ("3 files · 52 MB") and **Clear the render cache**. It says
how much it freed ("Render cache cleared: 52 MB freed (3 files)"). It removes only what Share made: files named by a
hash (`<24 hex>.<HDR|SDR|Photo>.<ext>`), renders of the old kind (a `render` link inside), and a photo's leftover
`.making` folder. Anything else in `_Renders` is kept and counted ("1 other file there is kept"), because the one-off
`photos_to_links.py --move-renders` would put real photos there. Nothing cleans it by itself.

**Where `_Renders` lives, an open point.** It is in the project folder, as built last night. The concept says
something else ("Where they're stored", decided 2026-11-11): renders go by default to **this computer's own cache**
(`~/Library/Caches/…`: left out of Time Machine, and, unlike a NAS folder, not in the NAS's backups), and only
pinned renders (*Keep rendered*) go to the project's `_Renders/`. macOS doesn't evict `~/Library/Caches` by itself
in general (only some system caches under low space), so moving there wouldn't give "the OS cleans it" either. It
would keep renders out of the NAS backups. I kept `_Renders` where it is, as the task says. Moving the default to the
per-computer cache, with pinning, is a follow-up.

Pictures (`docs/img/share-fixes/`): `settings-general-before.png`, `settings-general-after.png`,
`settings-general-after-dark.png`.

## What was run, on which real clips

Clips (copies in the session's scratch folder; the NAS was only read):

| Trip | Clip | What it is |
|---|---|---|
| dlogm | `DJI_20261022152937_0069_D.MP4` (local, `~/Movies/afp-perf-clips`) | DJI Action 6, D-Log M, 4K60, 15.5 s, 179 MB, gyro |
| gopro12 | `2025-10 Egypt/100GOPRO/GX010845.MP4` + `.afp.json` + `.gyroflow` | GoPro HERO12, not log, 4K, 2.7 s, 28 MB, underwater, its Gyroflow project |
| gopro2019 | `Indonesia 2019/GpPro/Dive 4/GX010304.MP4` + `.afp.json` + `.gyroflow` | GoPro (2019), not log, 4K, 14 s, 79 MB, an optical-flow project |
| plain | `~/Movies/P1060688.mp4` | Lumix, 640×480 HEVC, not log, no motion data: nothing to change |

Each run shares a moment (0.5–2.5 s, its link in `_Moments/Reef`), a photo link (t = 1 s, in `_Stills/Reef`) and the
whole clip into a folder, headless (`--vo=null`). It used a scratch copy of the installed player (0.13.0-dev.26) with
this branch's Lua scripts, and the same three items through the trial build's player at the end. Times are the whole
share, player start to the last copy (M1 Pro):

| Run | Time | `_Renders` / shared |
|---|---|---|
| plain HDR | 1.9 s | photo `.heic` 640×480 90 KB; moment cut `.mp4` 2.83 s 2.35 MB (ffmpeg fallback); clip copied as is 11.8 MB |
| plain HDR again | 0.9 s | all three from the cache / the original |
| dlogm HDR | 89.9 s | clip `.mov` HEVC 4K HLG (arib-std-b67, BT.2020) 15.47 s 179.5 MB; moment 2.02 s 25.0 MB; photo HEIC 3840×2160 4.8 MB |
| dlogm SDR | 84.5 s | clip BT.709 179.1 MB; moment 25.1 MB; photo from the cache |
| dlogm HDR again | 1.2 s | all three from the cache |
| gopro12 HDR | 18.8 s | clip 2.70 s 29.1 MB; moment 21.7 MB; photo 3840×2160 726 KB |
| gopro12 SDR | 11.3 s | clip 29.1 MB; moment 21.7 MB |
| gopro2019 HDR | 45.1 s | clip 14.06 s 79.4 MB; moment 11.6 MB; photo 609 KB |
| gopro12 HDR, the trial build's player | 22.0 s | same hashes as with the scratch copy |

All passed with no item saying its settings differed. Seen in the numbers: the GoPro clips aren't log, so their "HDR"
render is BT.709 like their SDR one (gyroflow-export keeps colours as recorded). The two are the same pictures under
two hashes. That is harmless, but HDR could later fall back to the SDR render for a clip that isn't log or HLG.

Tests, all passing:

| What | Result |
|---|---|
| `cd library && cargo test` | 58 pass, 13 ignored (new: the settings cases, Clear the cache; the recipe test extended) |
| `cd catalog && cargo test` | 32 pass |
| Windows: `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` in `library/` | compiles |
| The Lua tests in `.github/workflows/release.yml` (19 files, with the new `render_settings_test.lua`) | all pass |
| `node library/tests/albums.js` / `share.js` / `views.js library/ui` | 77 / 35 (+4: Clear the render cache) / 45 |
| `library/ui-tests`: `node --test`, `culling_test.js`, `atstart_test.js` | 31 / 87 / 27 |

## Installing the trial (Anton)

Built from this branch into `~/afp-night-build/`, unregistered from Launch Services, never launched by me (except
the player headless, for the check above; unregistered again after):
- `ActionFootageLibrary.app`: `tools/make_library_app.sh` from this branch (release, ad hoc signed).
- `ActionFootagePlayer.app`: **your installed player (0.13.0-dev.26+bc0b756, its mpv with the GPU path, its
  binaries) with this branch's Lua scripts put in**, version `0.13.0-dev.26+bc0b756+share-fixes.<commit>`. Why not
  `tools/dev_app.sh`: no `build/mpv-afp` exists here (it would bundle Homebrew's mpv, without the GPU path). Also,
  your installed build has lens-pick work (`local/test-230-231-232`) that this branch hasn't. Only Lua changed in
  the player tonight, so its binaries are the right ones. Its `gyroflow/main.lua` is this branch's three-way-merged
  with that build's (no conflict).

The Library uses the player beside it, so the two together try everything. To try them **without installing**:

```bash
open ~/afp-night-build/ActionFootageLibrary.app
```

Opening a clip from it (or double-clicking one) uses the trial player. Afterwards, so a double-click on an MP4 or a
project doesn't open a trial copy:

```bash
LS=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister; $LS -u ~/afp-night-build/ActionFootageLibrary.app; $LS -u ~/afp-night-build/ActionFootagePlayer.app
```

To **install** them over the ones in `~/Applications` (quit both first; the old ones are kept beside):

```bash
cd ~/Applications && mv ActionFootagePlayer.app ActionFootagePlayer-before-share-fixes.app && mv ActionFootageLibrary.app ActionFootageLibrary-before-share-fixes.app && cp -cR ~/afp-night-build/ActionFootagePlayer.app ~/afp-night-build/ActionFootageLibrary.app ~/Applications/
```

Then, as after every install, only the `~/Applications` copies registered, and MP4 still opening the player:

```bash
LS=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister; for a in ~/Applications/*-before-share-fixes.app ~/afp-night-build/*.app; do $LS -u "$a"; done; $LS -f ~/Applications/ActionFootagePlayer.app ~/Applications/ActionFootageLibrary.app; $LS -dump | grep -E "path: .*ActionFootage(Player|Library)\.app" | sort -u
```

```bash
swift -e 'import AppKit; import UniformTypeIdentifiers; print(NSWorkspace.shared.urlForApplication(toOpen: UTType.mpeg4Movie)?.path ?? "none")'
```

The second command should print `…/Applications/ActionFootagePlayer.app`; if not, the player's menu → Setup makes
it the default again. To go back: delete the two new apps and rename the `-before-share-fixes` ones.

Note: the first Share with the new Library renders everything again (recipe v3). The player writes
`~/Library/Application Support/ActionFootagePlayer/gyroflow/render-defaults.json` on every start. One is there
already, written by my headless runs from your `gyroflow.conf`.

## Still unverified (anything needing a window, and more)

- **`Alt+s` / crop Enter in the real player.** The photo-link path was compiled and its parts run separately: afp-meta
  on an ffmpeg JPEG made with the same filter. It was never pressed in a window. Untried: mpv's `screenshot video`
  on the GPU path (with the warp and the look; while paused, the denoise's ring might give a frame from before a
  camera move, as `gpu_frame`'s comment warns), the crop in camera (360) mode (the picture is cropped by Save
  frame's frame in shares of the screenshot), the colours of an HLG frame in the JPEG, and ffmpeg being in PATH on
  Windows (without it the picture is kept at full size).
- **`Alt+Shift+s`** (mpv's `Alt+S`) reaching the full photo: bound, not pressed.
- **The Library's window**: the settings row was seen only in headless Chrome against the mock. Share from the real
  sheet, the folder dialog, Clear on a real project: not clicked. A photo link made by the player, shown in an album
  and shared from there: not done together. The Share path was run headless with links the test writes in the
  same format.
- **A project on the NAS**: everything ran on local copies. Not measured: renders written to and read from `_Renders`
  over SMB, Clear's walk there.
- **Windows**: compiles; nothing run. `render-defaults.json` goes to `%LOCALAPPDATA%\ActionFootagePlayer\gyroflow\`
  (the launcher's `GYROFLOW_DATA`), and the Library reads it from there. Not tried.
- The **side panel's Save frame text** still offers Still / Live Photo / Motion Photo. In a project that choice now
  applies only to `Alt+Shift+s`, and its words don't say so yet.
- Seen and left: a later `.kv` rule in `style.css` (the Info part's two columns) is likely to override the **On this Mac**
  tab's cache rows the same way (not seen: the mock has none). My row sets its three columns inline. `style.css` is the
  layout session's to change.
