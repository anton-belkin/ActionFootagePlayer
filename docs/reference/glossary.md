# Glossary

Covers: the words the code, the docs and Anton use, each with where it lives in the code.
Checked against 295101c (2026-11-25).

The same word means the same thing in Lua, Rust, JS and the docs. When a word here and the code disagree, the code
is right and this file is wrong: fix it in your PR. Formats are in [files-and-formats.md](files-and-formats.md),
processes in [architecture.md](architecture.md), settings in [settings-flow.md](settings-flow.md).

## Footage and what's made from it

| Word | Meaning | In the code |
|---|---|---|
| **Original**, **clip**, **source** | A video as the camera wrote it. Never written to: everything about it goes beside it or into the project. "Source" is the clip seen from something made from it (a moment's source). | `workspace/main.lua`, `library/src/index.rs` |
| **Trip**, **project** | The folder with a `Project.afproject` in it, and everything under it and its locations. Made only when asked (the side panel's Project tab, the Library). What `Project.afproject` holds: [files-and-formats.md](files-and-formats.md). | `workspace/projectfile.lua`, `catalog/src/project.rs` |
| **Workspace** | The player's name for a clip's project, found by walking up from the clip, or through a pointer. The workspace plugin is the code that finds it. | `workspace/workspace.lua`, `vs-gyroflow/src/workspace.rs` |
| **Location** | A folder outside the project's own tree whose footage belongs to the project (a card's copy on a NAS share, another disk). Listed in `Project.afproject` `locations`, with the `hosts` it can be reached by. | `catalog/src/locations.rs` |
| **Pointer** | A `.afplocation` file at a location's root, saying which project (by id) it belongs to. Counts only for its project's id. | `workspace.lua`, `workspace.rs`, `catalog` `pointer_project` |
| **Own file**, **sidecar** | `<clip>.afp.json` beside an original: its marks, tags, comment, source settings (`source`), view (`view`), 360 recording. "Sidecar" in older code and docs means the same file. | `workspace/sidecar.lua`, `workspace.rs`, `library/src/tags.rs` `clip_own_in` |
| **Own folders** | The folders the project makes, never counted as footage: `_Moments`, `_Clips`, `_Stills`, `_Exports`, `_Renders`, `Generated`, `.afp`. | `OWN_FOLDERS` in `index.rs`, `inspect.rs`, `moments.lua`; `checks.rs` `OWN`; `afp_stream.py` |
| **Link** | A small MP4 (or a photo) that stands for a part of an original: one frame, held, plus the JSON key `ch.actionfootageplayer.link` saying which original (by path and `hash`) and which part. Played, it plays the original's part. Named `*.afplink.mp4`. | writer `vs-gyroflow/src/mp4meta.rs`, reader `workspace/link.lua` |
| **Fingerprint**, **hash** | SHA-256 of a file's first and last MiB: how a link finds its original after a move. | `vs-gyroflow/src/workspace.rs` `fingerprint`, `gyroflow-export --fingerprint` |
| **Moment** | A named part of a clip, kept as a link in `_Moments/` (`k` in the player). Has its own transform when made with one, and its own Gyroflow project copy when the clip has one. | `workspace/moments.lua`, `workspace/main.lua` |
| **Album**, **clip preview** | A folder in `_Clips/` of links to whole clips (light links: one thumbnail). Older clip previews (flipbooks) still play as album items. | `library/ui/albums.js`, `library/src/albums.rs` |
| **Generated** | Where what's made lands when it's in no album: `<kind folder>/Generated/` (`_Moments/Generated`, `_Stills/Generated`, `_Exports/Generated`), with a folder per run or day when the project's `generated_runs` is on. From there it may be moved anywhere in the project. | `workspace/moments.lua` `generated_rel` |
| **My albums**, **media-type album** | Places in the Library page only: "My albums" lists the user's albums (still the folders in `_Clips`, `_Moments`, `_Stills`); a media-type album (Videos, Moments, Photos, Live Photos, 360° videos) is everything of one kind, built from what the page has, with no scan of its own. | `library/ui/albumviews.js` (`ALBUM_VIEWS`, `AV_MEDIA`) |
| **Still**, **photo** | A frame saved as a picture in `_Stills/`: HEIC (macOS) or Ultra HDR JPEG, with a gain map for an HDR look; a link to its original in its XMP (`afp:link`). | `vs-gyroflow/src/still.rs`, `gyroflow-export --photo` |
| **Live Photo**, **Motion Photo** | A still with the motion around it (1.5 s each side): Apple's pair (HEIC + MOV) or Android's single JPEG with the MP4 inside. | `gyroflow-export --live`, `mpv/scripts/live-photo/` |
| **Export** | A rendered video file in `_Exports/` (or `stabilized/` beside a clip without a project), made by `gyroflow-export`. | `gyroflow/export.lua`, `bin/gyroflow-export.rs` |
| **Render** | A file in the Library's render cache `_Renders/<hash>.<profile>.<ext>`: an item's pixels in one profile (HDR, SDR, Photo), found by its recipe's hash, not by path. What Share hands out. | `library/src/renders.rs` |
| **Recipe** | Everything that decides a render's pixels (the item, its bundle, the look, the renderer's `VERSION`); its hash names the render. Renders from before 0.11 (`_Renders/<the moment's path>.<profile>.mov`) carry a v1 recipe in their link, worked out the same way by the player and the Library (`RENDER_VERSION` in both). | `renders.rs` `recipe`, `VERSION`; v1: `workspace/main.lua` and `index.rs` `RENDER_VERSION` |
| **Share** | The Library's "send the selected items somewhere": renders from the cache, made first by the player when missing. Replaced the old export menu in the Library. | `library/src/share.rs`, `library/ui/share.js` |
| **Take in** | Bringing footage into a project: from a card (per-camera destinations on this computer), or one project into another, once. | `library/src/takein.rs` |
| **Mark**, **highlight** | A time the camera (or you, `m`) marked in a clip: the camera's highlights read from the file, yours kept in the own file. | `mpv/scripts/highlights/` (`markers.lua`), `sidecar.lua` |
| **Tags**, **rating**, **pick** | Culling: Finder tags, stars and pick/reject on originals, moments and stills, kept two-way with Finder. | `workspace/tags.lua`, `library/src/tags.rs`, `catalog/src/finder.rs`, `library/ui/culling.js` |

## How a clip is read and shown

| Word | Meaning | In the code |
|---|---|---|
| **Input**, **source settings** | How to read a source right, set once, kept only in its own file's `source`: the log curve, the turn, the lens under water, the underwater colour, the lens type. Shared by every moment of it. | `workspace/transform.lua` `INPUT_KEYS`, the Library's Source settings |
| **Transform**, **view** | How it's shown, a matter of taste: stabilization on/wide/off, level, horizon lock, fringes, frame (aspect, turn), projection, lens corrected, denoise. Always kept whole. | `transform.lua` `KEYS` |
| **Bundle** | One transform, all its keys at once. A source's (`view` in its own file), or a moment's own (`stab` in its link). The topmost one there is plays; bundles never mix. | `transform.lua`, `renders.rs` `BUNDLE_KEYS` |
| **Defaults**, **session settings** | What plays when there's no bundle: the gyroflow plugin's settings from `gyroflow.conf`. | `gyroflow/main.lua` `defaults_bundle`, `renders.rs` `defaults_of` |
| **Curve**, **log curve** | How the camera encoded light: `dlogm`, `dlog`, `gplog`, `gplog2`, `ilog`, `none`, plus `uw` (the underwater colour, for a plain SDR clip). | `dlogm/main.lua` `CURVES`, `gyroflow-export.rs` `CURVES` |
| **Look** | How a curve is shown: HDR (HLG), HDR with Dolby Vision, SDR through the maker's LUT, SDR through NBCU's conversion, the TV look. | `dlogm/main.lua`, `shaders/` |
| **uw**, **underwater colour** | The correction that takes the camera's colour cast out of an SDR underwater clip, measured from the picture a few times a second. | `dlogm/uwcolor.lua`, `vs-gyroflow/src/uwcolor.rs`, `shaders/underwater_hlg.glsl` |
| **Preset** | A Gyroflow preset for clips without a project: Gyroflow's default, then `default.gyroflow` in App Support, then the plugin's `underwater.gyroflow`. Also: a Gyroflow lens profile preset. | `gyroflow/main.lua` |
| **Gyroflow project** | A `.gyroflow` file beside the clip (`<name>.gyroflow` or `<name.ext>.gyroflow`). Owns the smoothing, horizon, frame and turn when there is one. | `workspace.lua`, `gyroflow-project` |
| **Motion data** | What the stabilization works from: the camera's gyro (in the file), a Gyroflow project, or motion measured from the picture (`.afp-motion`). | `gyroflow/main.lua`, `bin/gyroflow-optical.rs` |
| **Optical motion**, **`.afp-motion`** | Motion measured from the picture by Gyroflow's analysis, kept beside the clip, for clips with no usable gyro. | `vs-gyroflow/src/optical.rs`, `afp_motion.rs`, `gyroflow/measure.lua` |
| **Lens profile**, **lens pick** | The lens model the warp corrects with: from the project, else picked from Gyroflow's lens DB, a curated list and the user's list. | `vs-gyroflow/src/lens_pick.rs` |
| **Level** | The stabilization's strength without a project: weak, medium, strong (crop and smoothness together). | `gyroflow/main.lua`, `--level` |
| **Edge fill** | What fills the edges the stabilized frame leaves: off, smooth, sphere. | `warp.glsl`, `--edges` |
| **Temporal denoise**, **Ctrl+p** | Each frame averaged with neighbours where the motion puts them, in the warp's `dn_` passes. Off / Light / Full per computer, on/off and Stronger per clip. | `warp.glsl`, `gyroflow/still.lua`, `--denoise` |
| **Translation** | The hand's sideways jerks, measured per clip from accelerometer and picture, cached, and shifted out (`Ctrl+j`). | `gyroflow-export --translation`, `--with-translation` |
| **360 view**, **reframe**, **heading** | A 360 clip shown as a view to look around in; the view recorded as you look and exported flat (reframe); what straight ahead follows (travel, camera, north, world). | `mpv/scripts/view360/`, `vs-gyroflow/src/sphere360.rs`, `--reframe`, `--heading` |

## How it runs

| Word | Meaning | In the code |
|---|---|---|
| **GPU path** | The only player renderer: `gyroflow-gpu-table` computes each frame's warp parameters into a table; `shaders/gyroflow/warp.glsl` warps in libplacebo (vo=gpu-next). The export's `--gpu` runs the same shaders in ffmpeg's libplacebo filter. | `vs-gyroflow/src/gpu_table.rs`, `user_shader.rs`, `warp.glsl` |
| **GPU table** | The per-frame data the warp shader reads (kernel parameters and row rotations as quaternion knots), written into the shader file the player loads, and streamed ahead of playback. | `gpu_table.rs`, `gyroflow/ahead.lua` |
| **Pipes path** | The export's other path: Gyroflow's engine (`lib.rs` `Engine`) on the CPU between an ffmpeg decoding and one encoding. Used where the GPU path can't run. | `vs-gyroflow/src/lib.rs`, `gyroflow-export` |
| **Unsupported**, **as recorded** | What the warp can't do plays as recorded, with a card saying why. | `gpu_table.rs` `unsupported`, `gyroflow/main.lua` `play_unstabilized` |
| **Card** | The player's notification (a small box), and `mp.ask` dialogs, drawn by the side panel. | `side-panel/notify.lua`, `notify_client.lua` |
| **Panel**, **side panel** | The settings beside the video (tabs Source, Transform, Output, Project, 360°…), drawn in ASS. | `mpv/scripts/side-panel/` |
| **Batch** | The player run without a window by the Library, doing one job described in a request file (`AFP_BATCH`): moments, photos, exports, share renders. | `workspace/main.lua` (end), `library/src/main.rs` |
| **Inbox** | `player-<pid>.in`: lines the Library appends for a running player to pick up (open this, reload that). | see [contracts.md](contracts.md) |
| **IPC** | mpv's JSON socket (`--input-ipc-server`), used by the Library, the agent and the live rigs. | `mpv/scripts/agent/`, `tests/*_ipc.py` |
| **Catalog** | What was last seen of a project's files, shared through `.afp/catalog/`: each computer's journal, snapshots folding them, care marks. The files are the truth; the catalog is a cache. | `catalog/` (`afp-catalog`), `library/src/seen.rs` |
| **Journal** (catalog) | A computer's append-only log of what it saw and did, in `.afp/catalog/`. | `catalog/src/journal.rs` |
| **Journal** (responsiveness) | Per-action timings written to the computer's logs folder, ranked by `tools/journal_report.py`. Not the catalog's. | `workspace/journal.lua`, `library/src/journal.rs` |
| **Care marks**, **deep look** | Who last listed every folder of a root, and when, so a change made in place by something else is seen within a day. | `catalog/src/care.rs` |
| **Probe** | `gyroflow-probe --library`: what the Library needs to know of a clip (camera, curve, lens, GPS, clock). Its answer's version is `GEOTAG`. | `bin/gyroflow-probe.rs`, `library/src/index.rs` |
| **NAS Helper** | One Docker container on the NAS: the catalog guardian (collaboration), `afp-stream`, Jellyfin links. Runs only what Anton starts. | `tools/nas-helper/`, `tools/stream/` |
| **Twin** | The same knowledge written in two or more languages (a list of curves in Lua and Rust). Each set has a guard test. | CLAUDE.md, [settings-flow.md](settings-flow.md) |
| **Rig** | A test that needs a live player, a screen or the NAS, run by hand, not in CI. | [../dev/testing.md](../dev/testing.md) |
| **One-off script** | A migration of Anton's own data, run once when he says so. The product never reads old formats. | `tools/one_off/` |
