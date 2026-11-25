# Settings: where each one is stored and who reads it
Covers: the player (mpv plugins, `gyroflow-gpu-table`, `warp.glsl`), the export (`export.lua` → `gyroflow-export`), photos and Live Photos, moments, the Library (`renders.rs`, Source settings, Share) and the NAS stream (`afp_stream.py`).

Checked against 295101c (2026-11-25), with the settings JSON (the export takes the player's settings, 2026-11-25).

This is a map, not a design. When a row and the code disagree, the code is right: fix the row.
Rows cite symbols, not line numbers. One row per setting, so a change touches one row.

The rule behind it is in [CLAUDE.md](../../CLAUDE.md), "A feature works on every path, or says where it
doesn't". A cell marked **gap** is a setting that one path leaves out. Some gaps are on purpose; most are worth an
issue.

## How to read the tables

Where a setting is kept:

- `conf:x`: `gyroflow.conf` script-opt `x` (this computer's; the gyroflow plugin's `o` table). `dlogm.conf:x` and
  `uwcolor.conf:x` the same for those plugins.
- `view.x`: a key of the transform bundle ([transform.lua](../../mpv/scripts/workspace/transform.lua) `KEYS`). A
  bundle is kept in the clip's own file (`<clip>.afp.json` `view`), in a moment's or clip preview's link (`stab`), or
  is the session's (`user-data/gyroflow/session`). With none saved, the app's defaults (`defaults_bundle`) play.
- `source.x`: the clip's input, in `<clip>.afp.json` `source` (`transform.lua` `INPUT_KEYS`, `UW_KEYS`, and a few
  more the Library writes). Every moment and clip preview of the clip reads the original's.
- `ud:x`: `user-data/gyroflow/x`, what the gyroflow plugin publishes (`publish`).
- `req.x`: a key of `gyroflow-gpu-table --player`'s JSON (`player_args`, `gpu_user_data`, `gpu_table`).
- `PARAM x`: a `//!PARAM` of `shaders/gyroflow/warp.glsl`, set through `glsl-shader-opts`.
- `set.x`: a key of [the settings JSON](contracts.md#the-settings), which the export takes as it is (`--settings`). A
  flag in an Export cell is the one the key stands for on gyroflow-export's command line; nothing of ours passes it
  (`tests/settings_json_test.py`).

The paths:

- **Player**: what reads it and how it reaches the picture.
- **Export**: `export.lua` `export_settings`, one function for every route: the clip playing (Shift+E) sends the
  gyroflow plugin's own settings (`gpu_settings`, `user-data/gyroflow/settings`: what its warp is made of) with the
  look and colour as they play (`as_it_looks`); a saved view (another clip, a Library batch, `photo-at`) sends
  `settings.lua` `of_view`'s. The export's choices go over both (`exportsettings.lua` `apply`).
- **Photo**: `photo_full` (Alt+Shift+s: the clip playing) and `photo-at` (a saved view), into `gyroflow-export --photo`
  (`still.rs`, `photo.rs`). A Live Photo or Motion Photo adds `--live` and renders its motion with the same settings.
  A photo link (Alt+s) keeps only `t`, `crop`, `camera`; the Library renders it later.
- **Moment**: what a moment's link keeps, and what its preview (`preview` message) renders with.
- **Library**: `renders.rs` `recipe_with` (what is hashed), Source settings (`app.js`, written by `tags.rs`
  `set_source`), Share (`share.js`). The player renders the recipe through `export-batch` and `photo-at`.
- **NAS stream**: `afp_stream.py` asks a windowless player (`conf:render_args`) and renders `render.user_data`
  with `afp-render`.

## Stabilization (the transform bundle)

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| Stabilization on, wider, off | `view.view`; default `conf:view` (normal, wide) | `view`, `stab_off` → `req.wide`, `req.lens` (`filter_lens`) | `--wide`, `--no-stabilize`, `--lens-only` | same flags | `stab.view` | `settings.view`; shown read-only in the list filters (`items.js` `deriveTransform`) | `user_data.wide`, `lens` |
| Level (weak, medium, strong) | `view.level`; `conf:level` | `req.level` (`levels.rs`) | `--level` | same | `stab.level` | `settings.level` | `user_data.level` |
| Horizon lock | `view.horizon_lock` (yes, no; "project" read as the preset's); `conf:horizon_lock` | `req.horizon_lock` 0 or 100 | `--horizon-lock 0\|100` | same | `stab.horizon_lock` | `settings.horizon_lock` (`preset_horizon` "no" in `defaults_of`) | `user_data.horizon_lock` |
| Lens corrected or as recorded | `view.lens`; `conf:lens` | `lens_mode` → `req.lens` (stabilized, recorded, only) | `--lens-recorded`, `--lens-only` | same | `stab.lens` | `settings.lens` | `user_data.lens` |
| Output projection | `view.projection` (linear, stereographic, equidistant, equisolid, orthographic, project); `conf:projection` | `frame.projection` → `req.projection` (`lens_projection`) | `--projection` (only for the four non-linear ones, lens corrected) | same | `stab.projection` | `settings.projection` | `user_data.projection` |
| Frame aspect | `view.aspect` (W:H landscape-wise or original); `conf:aspect` | `shown_aspect()` (`transform.concrete`) → `req.aspect` | `set.aspect`: the clip playing's `shown_aspect()`; a saved view's `frame_aspect`, worked out by the workspace plugin (`made_view_of`) | same | `stab.aspect` | `settings.aspect`; `frame_aspect` added by the player at render time | `user_data.aspect` |
| Frame orientation | `view.orientation` (as-is, landscape, portrait); `conf:orientation` | folded into `shown_aspect()` | inside `--aspect` | same | `stab.orientation` | `settings.orientation` | inside `user_data.aspect` |
| Water's colour fringes | `view.chromatic`; `conf:chromatic` | `set.chromatic` → `req.gpu_chromatic` (off when `conf:chromatic_skip`) | `set.chromatic` (`--chromatic`); never skipped | same; the screen grab (`gpu_frame`) is not used when skipped | `stab.chromatic` | `settings.chromatic` | **gap**: only with `AFP_CHROMATIC=1` (default off) |
| Temporal denoise on or off | `view.denoise`; `conf:still` | `still.lua` → `set.denoise`; `req.gpu_denoise`, `PARAM gyroflow_dn_radius` | `set.denoise` with `set.temporal_denoise` (`--denoise <amount>`) | still: cleaned up where the settings' denoise is on (`still_radius` 0 otherwise); motion: as export | `stab.denoise` | `settings.denoise` | with `AFP_DENOISE=player` (default off: slower than real time on the N100): `render.settings.denoise` → `user_data.gpu_denoise`, `afp-render -behind`; off, said in the log |
| Denoise Stronger | `view.denoise_strong`; `conf:denoise_strong` | `PARAM gyroflow_dn_strong`; `set.denoise_strong` | `set.denoise_strong` (`--denoise-strong`) | same | `stab.denoise_strong` | `settings.denoise_strong` | `render.settings.denoise_strong` → `-set gyroflow_dn_strong` (with `AFP_DENOISE=player`) |
| Gyroflow project | `<clip>.gyroflow` or `<clip.ext>.gyroflow`; a moment's own `<link>.gyroflow` | `req.project` | `--project` (`ud:project`) | same | own project beside the link; `--project`, `--alone`/`--whole` | `gyroflow` fingerprint in the recipe, else `preset` fingerprint | `user_data.project`; a dep of the cached answer |
| Preset (no project) | `default.gyroflow` in `$GYROFLOW_DATA` or the plugin folder | `req.preset` | gyroflow-export finds it itself | same | same | `preset` fingerprint (from `render-defaults.json`) | the NAS player's own preset |

The project owns level, horizon and frame (`transform.lua` `PROJECT_KEYS`). Over a project, those bundle keys are
kept but not used.

## Computer's settings (gyroflow.conf, not per clip)

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| Temporal denoise amount | `conf:temporal_denoise` (full, light, off) | `DENOISE_FRAMES` → `PARAM gyroflow_dn_radius`; off: none whatever `view.denoise`; `set.temporal_denoise` | `set.temporal_denoise` (a recipe's `opts.temporal_denoise` for a Library render; the quality preset may override: `exportsettings.lua` `apply`) | motion as export; the still is cleaned up as the settings say, a Library photo by its recipe's amount (#319) | this computer's at render time | `opts.temporal_denoise` (from `render-defaults.json` or `defaults_of`) | the NAS's own `/config/player` gyroflow.conf, in `render.settings.temporal_denoise` → `-behind`, `-set gyroflow_dn_radius` (with `AFP_DENOISE=player`) |
| Grain kept | `conf:still_grain` (0.25) | `PARAM gyroflow_dn_keep`; `set.denoise_grain` | `set.denoise_grain` (`--grain`, #314) | same | this computer's at render time | this computer's at render time; **gap**: not in the recipe, so a changed grain doesn't make a new render | `render.settings.denoise_grain` → `-set gyroflow_dn_keep` (with `AFP_DENOISE=player`) |
| Edge fill | `conf:edges` (sphere, smooth, off) | `edge_mode` → `req.edges` | `--edges` (quality preset may change it) | same | this computer's | `opts.edges` | **gap**: sphere becomes smooth |
| Edge blur | `conf:edges_blur` | `req.edges_blur` | `--edges-blur` | same | this computer's | `opts.edges_blur` | `user_data.edges_blur` |
| Sideways jerks (translation) | `conf:translation` (allowed); computed per clip by Ctrl+j into `translation.rs`'s cache; on/off per session (`trans.on`) | `req.gpu_translation`, `PARAM gyroflow_trans`; `set.translation` (allowed and on) | `set.translation`: taken where computed for the clip, on the GPU path (#313) | same, the clip playing | the clip playing's preview | **gap**: a session's toggle, not in bundle or recipe | **gap** |
| Lens under water default | `conf:underwater` (auto, yes, no, project: auto) | `default_underwater`, used where `source.underwater` is unset; `defaults.underwater` | the clip playing: `set.underwater` as it plays. A saved view: `source.underwater`, else the defaults' (`transform.render_settings`) | same | from the original's source, else the defaults' | `defaults_of` and `render-defaults.json`'s `defaults.underwater`; in the recipe's settings where the source doesn't say and it's yes or no (#317; auto left out: hashes kept) | the NAS player's own conf |
| Playback only | `conf:resolution` (→ `PARAM gyroflow_render_scale`), `ask_when_slow`, `chromatic_skip`, `lookahead`, `hwdec`, `gpu_flush`, `still_delay`; `dlogm.conf:denoise` | yes | n/a: a file has every frame at its own size | n/a | n/a | n/a | `AFP_HEIGHT`, `AFP_INTERP` instead |
| Warp interpolation | none: by frame rate (`INTERPOLATION_SLOW`/`_FAST`: Lanczos up to 31 fps, `SLOW_MAX_FPS`, bicubic above) | `req.interpolation` | export default; quality preset sets `--gpu-interpolation`, `--interpolation` | same | n/a | through `export.quality` | `AFP_INTERP` (bilinear) |

Read and ignored: `still_radius`, `still_tolerance`, and the VapourSynth path's keys (`gpu`, `workers`, `max_fps`, …).

## Input (the source's, per original)

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| Log curve | `source.curve` (dlogm, dlog, gplog, gplog2, ilog); else the metadata | dlogm `CURVES` → `<curve>_hlg.glsl`; `ud`-like state in `user-data/dlogm/curve` | `--curve` (`none` when the look is off) | same | the original's | Source settings `SRC_CURVES`; `settings.curve` | `dlogm.curve` → the dlogm plugin's `<curve>_hlg.glsl` (registry.json's `hdr`, every log curve baked into the image by `make_looks.py`) |
| Recorded turned | `source.rotation` (0, 90, 180, 270) | `frame.rotation` → `req.rotation` | `--rotation` | same | the original's | Source settings `SRC_TURNS`; `settings.rotation` | `user_data.rotation` |
| Lens under water | `source.underwater` (auto, yes, no) | `underwater` → `req.underwater` (`water.rs`) | `--underwater` | same | the original's | Source settings `SRC_WATER`; `settings.underwater` | `user_data.underwater` |
| Lens type | `source.lens_type` | read by `lens_pick.rs` wherever vs-gyroflow loads the clip | same (gyroflow-export reads the file) | same | same | Source settings "Lens type"; in `settings` (the whole `source` is copied) | read if the NAS sees the `.afp.json` |
| Lens for one clip | `source.lens` (a profile) | `lens_pick.rs` | same | same | same | in `settings` (copied with `source`) | as above |
| Lens for a camera mode | `curated_lenses.json` in the app's data folder (`lens_pick.rs` `user_file`) | `lens_pick.rs` | same | same | same | **gap**: not in the recipe, so a changed mode lens doesn't make a new render | **gap**: the NAS has its own data folder |
| Optical motion | `<clip>.afp-motion` beside the clip | gyroflow-probe "measured" | gyroflow-export reads it | same | same | **gap**: not in the recipe | read if beside the clip; not a dep of the cached answer |

### Underwater colour (input too)

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| On | `source.uw`; default `uwcolor.conf:on` | `uwcolor.lua`; dlogm curve becomes `uw` (`underwater_hlg.glsl`) | the clip playing: `set.curve` uw; a saved view: `set.uw` | same | the original's | Source settings `SRC_UW`; `settings.uw` only when on; HDR kept (`resolve`) | **gap**: curve `uw` has no look ("no look for the curve") |
| Brighter highlights | `source.uw_highlights`; `uwcolor.conf:highlights` | shader `uw_*` opts | `--uw-highlights` | same | the original's | `SRC_UW` "bright"; `settings.uw_highlights` | **gap** |
| Strength | `source.uw_strength` (0–1); `uwcolor.conf:strength` | eases the shader's numbers | `--uw-strength` | same, plus `--uw-params` (the numbers on screen) | the original's | Source settings "Strength"; `settings.uw_strength` | **gap** |

The defaults reach the Library through `render-defaults.json` (`publish` merges `user-data/uwcolor/defaults`).
**Gap**: `defaults_of`, the fallback when that file is missing, reads only `gyroflow.conf`, so `uwcolor.conf` is
left out there.

## Look and colour (this computer's, per file)

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| Look (hdr, dv, dvsdr, tonemap, nbcu, maker, off) | session; start look `dlogm.conf:hdr_screen`, `sdr_screen` | dlogm shaders and target | `--sdr` (maker), `--nbcu`, `--dv-rpu`; `--curve none` for off | still: `--sdr-image maker\|macos\|nbcu`; motion as export | preview is SDR | profile `look` (HDR, SDR, Photo); SDR → `req.sdr` → `--nbcu` | look variants by URL (hlg, sdr, pq), every log curve |
| SDR look for SDR exports | `user-data/dlogm/sdr_look` | n/a | Export SDR, `--youtube <look>` | n/a | n/a | always nbcu | n/a |
| Dolby Vision trims | `$DLOGM_DATA/dv` (the user's), else `dv/nbcu_fit.rpu` | `macos-dovi-rpu-file` | `--dv-rpu` with the same file | motion only | n/a | **gap**: `export-batch` never passes `--dv-rpu`, though `renders.rs` `PROFILES` says the HDR look is HLG + Dolby Vision 8.4 | n/a |
| Live L1 | `dlogm.conf:dv_live_l1` | `macos-dovi-live-l1` | `--dv-live-l1` | motion only | n/a | **gap**, as above | n/a |

## Frame, photo and 360

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| Save frame camera (crop mode) | session (`ud:crop`); a photo link's `camera` | `PARAM gyroflow_cam_*` (`gpu_camera`) | n/a | `--camera yaw,pitch,roll,zoom` | n/a: a moment keeps neither camera nor frame | recipe `camera` (photo) | n/a |
| Save frame crop | session; a photo link's `crop` | mpv `video-crop` | n/a | `--crop x,y,w,h` | n/a | recipe `crop` (photo) | n/a |
| Photo kind (still, Live, Motion) | `conf:live_photo` | n/a | n/a | `--live`, `--format jpeg` | n/a | always a still (`photo-at` `still = true`) | n/a |
| 360 view (yaw, pitch, roll, fov, projection) | session (`user-data/view360`) | `view360.glsl` `PARAM view360_*` | `--360` exports the whole sphere; the view itself is not passed | `--camera` from crop mode | **gap**: not kept | **gap**: no 360 fields in the recipe | **gap** |
| 360 heading | session | `PARAM view360_heading` | `set.heading` (`start_360`) | n/a | **gap** | **gap** | **gap** |
| 360 recording, its frame, size, tidy | `<clip>.afp.json` `view360.recording` | `recording.lua` | flat: `--reframe recording --size` | n/a | **gap** | **gap** | **gap** |
| Sphere width, stitching distance | `conf:sphere_width` (0: 5760, `sphere360::DEFAULT_WIDTH`), `conf:stitching_distance` | `gyroflow-gpu-table --sphere360` arguments; `set.sphere_width`, `set.stitching_distance` | `start_360`: both in its settings (#320) | n/a | n/a | n/a | n/a |

## Export choices

| Setting | Stored | Player | Export | Photo | Moment | Library | NAS stream |
|---|---|---|---|---|---|---|---|
| Size | `conf:export_size`, `conf:photo_size` (`export-setting` message) | n/a | `--resolution` (`exportsettings.lua` `apply`) | `--resolution` | n/a | `share.js` (kept per computer in local storage `afl.share.<kind>.<key>`); recipe `export.size` when not the default | `AFP_HEIGHT` |
| Encode | `conf:export_encode` | n/a | `--encode quality` | n/a: no encoder | n/a | `export.encode` | `AFP_BITRATE` |
| Quality preset | `conf:export_quality`, `conf:photo_quality` | n/a | removes or sets `--denoise`, `--edges`, `--chromatic`, `--gpu-interpolation`, `--interpolation` | also `--radius 0` or `7` (`PHOTO_RADIUS_MAX` = `still.rs` `MAX_RADIUS`); highest drops `--image` | n/a | `export.quality` | n/a |
| Not for 360 | `exportsettings.lua` `unavailable` | | the 360 renderer sizes itself | | | | |

The defaults (original, fast, displayed) are left out of the recipe, so renders made before these settings keep
their hash.

## Moments: what the link keeps

A moment's link (`.afplink.mp4`) keeps `clip`, `in`, `out`, `alone`, `snap` and, when it has settings of its own,
the bundle in `stab`. It never keeps the input (read from the original's `source`) or the computer's settings (edges,
denoise amount): those are read when it plays or renders. Its own Gyroflow project lies beside the link.

The preview (`preview` message) is the frame on screen when k was pressed (`--picture`), else rendered by
gyroflow-export with a saved view's settings (another clip) or the clip playing's.

## Twins and their guards

The same logic in more than one language, from CLAUDE.md, with the test that fails when the copies drift.

| Twin | Files | Guard |
|---|---|---|
| Workspace and pointer lookup | `workspace.lua`, `vs-gyroflow/src/workspace.rs`, `catalog/src/locations.rs` | [workspace_cases.txt](../../tests/workspace_cases.txt), run by [workspace_test.lua](../../tests/workspace_test.lua), `workspace.rs` tests, `locations.rs` `workspace_cases` |
| Render settings | `transform.lua` `render_settings`, `renders.rs` `settings` | [render_settings.json](../../tests/render_settings.json): [render_settings_test.lua](../../tests/render_settings_test.lua) and `renders.rs` `settings_are_worked_out_as_the_player_does` |
| Bundle keys and valid values | `transform.lua` `KEYS`, `UW_KEYS`, `INPUT_KEYS`, `VALID`; `renders.rs` `BUNDLE_KEYS`, `UW_KEYS`, `valid` | [render_keys_test.py](../../tests/render_keys_test.py) (allow-list `PLAYER_ONLY`) |
| gyroflow.conf defaults | `gyroflow/bundle.lua` `defaults_bundle`, `renders.rs` `defaults_of` | [gyroflow_conf_cases.json](../../tests/gyroflow_conf_cases.json): [gyroflow_conf_test.lua](../../tests/gyroflow_conf_test.lua) and `renders.rs` `defaults_are_read_as_the_player_does` |
| Render recipe version | workspace `RENDER_VERSION` (1) and `index.rs` `RENDER_VERSION` (1): the old per-moment recipe; `renders.rs` `VERSION` (4): the hashed one | **none**. CLAUDE.md pairs workspace `RENDER_VERSION` with `renders.rs` `VERSION`, but its real twin is `index.rs` `RENDER_VERSION` (`render_recipe_of`) |
| Curves | `dlogm/main.lua` `CURVES`, `gyroflow-export/options.rs` `--curve`, `make_hdr_lut.py`, `source.js` `SRC_CURVES`, `tags.rs`, `albums.js` `ALBUM_CURVES` | [curves_twin_test.lua](../../tests/curves_twin_test.lua) |
| Export choices | `exportsettings.lua` `CHOICES`, `share.js` `EXPORT_CHOICES`, `renders.rs` `EXPORT_CHOICES`, gyroflow-export `--resolution`/`--encode`, `still.rs` `MAX_RADIUS` | [export_choices_test.py](../../tests/export_choices_test.py) |
| Underwater colour rule | `dlogm/uw_estimate.lua`, `vs-gyroflow/src/uwcolor.rs` (not in CLAUDE.md's list) | [uwcolor_cases.json](../../tests/uwcolor_cases.json), written by [uwcolor_test.lua](../../tests/uwcolor_test.lua), read by `uwcolor.rs` tests |
| Project's own folders | `index.rs` `OWN_FOLDERS`, `inspect.rs` `OWN_FOLDERS`, `checks.rs` `OWN`, `moments.lua` `OWN_FOLDERS`, `afp_stream.py` `OWN_FOLDERS` (registry.json's, `walked`) | [project_folders_test.py](../../tests/project_folders_test.py) |
| Script messages and bindings | every `script-message-to` and its `register_script_message` | [script_messages_test.py](../../tests/script_messages_test.py) |
| user-data | every `user-data/<key>` read has a writer | [user_data_test.py](../../tests/user_data_test.py) |
| The probe's answer | `gyroflow-probe --library`, `index.rs` `GEOTAG` | `index.rs` `geotag_goes_with_what_the_probe_answers` |
| What the project keeps | `afp_catalog::project::VERSION`, `projectfile.lua` `M.VERSION` | `inspect.rs` `what_is_kept_goes_with_the_projects_version` |
| Tags | `tags.lua`, `library/src/tags.rs`, `catalog/src/finder.rs` | each side's own tests ([tags_test.lua](../../tests/tags_test.lua), `tags.rs`); **no shared cases** |
| The clip's own file | `sidecar.lua`, `workspace.rs`, `tags.rs` `clip_own_in`, `reframe.rs` | **no guard test** |
| 360 view | `view360.glsl`, `view360.wgsl`, `view.lua` | `view360.rs` `the_wgsl_draws_what_the_players_glsl_does`; [view360_test.lua](../../tests/view360_test.lua) for `view.lua` |
| Camera sidecar names | `index.rs` `low_res_names`, `scrub.lua` `sidecar_names` | [scrub_sidecar_cases.txt](../../tests/scrub_sidecar_cases.txt) |

## How to add a setting

First decide what it is. That decides where it's kept and which twins it touches.

- **A matter of taste per clip** (how it's shown): a bundle key. Kept whole with the others.
- **How the source must be read** (set once, true for every moment): an input key in `source`.
- **This computer's** (how much work it can do): a `gyroflow.conf` option, carried into renders as recipe `opts`.
- **How a file is made**: an export choice.

Then go through each step. Mark each one in the PR as *done*, *n/a because…*, or *not done → issue #N*.

1. **Stored.**
   - Bundle key: `transform.lua` `KEYS`, `VALID` and, for old bundles, `NEUTRAL`. Then `renders.rs` `BUNDLE_KEYS`,
     `valid`, and `complete`'s neutral. Add cases to `render_settings.json`.
   - Input key: `transform.lua` `INPUT_KEYS` (or a list like `UW_KEYS`) and `migrate_source`; `renders.rs`
     `migrated_source`; `tags.rs` `set_source` if the Library sets it.
   - Default in `gyroflow.conf`: the plugin's `o` table, `defaults_bundle`, `renders.rs` `defaults_of`, and cases in
     `gyroflow_conf_cases.json`. Read the conf as mpv's `options.lua` does: nothing trimmed, booleans only yes/no.
   - Computer's option: also into `publish`'s `render-defaults.json`, and `renders.rs` `recipe_with` `opts`.
   - Never in `Project.afproject`.
2. **Player.** A key of the settings JSON: registry.json `settings_json` (with `table`, `export`, the flag it stands
   for, an example), set in `gpu_settings` (or `player_args`, which it starts from) and read by `vs-gyroflow`'s
   `PlayerClip` if the warp needs it. If it changes the geometry, add it to `gpu_table.rs` `PLAYER_GEOMETRY_KEYS`. A
   shader value is a `//!PARAM` in `warp.glsl`. Run `python3 tools/gen_reference.py`.
3. **Export.** `gyroflow-export/options.rs` `apply_settings` reads the key (its test sends the registry's example and fails
   until it does), working on both its GPU path and its pipes path (`lib.rs`'s engine); a flag for the command line if
   you like, parsed into the same `Options`. The clip playing's settings reach it as they are; a saved view's need
   `gyroflow/settings.lua` `of_view` to set it (a case in `tests/settings_cases.json`). If a quality preset should
   change it, `exportsettings.lua` `apply`. Never a flag from `export.lua` (`tests/settings_json_test.py`).
4. **Photo, Live Photo, Motion Photo.** Check `photo_full` and `photo-at`; the still in `still.rs` may need its own
   flag (as the denoise has `--radius`). Check `gpu_frame`: the screen grab must show the same thing, or be skipped.
5. **Moment.** A bundle key travels in the link's `stab` by itself. Anything else: decide whether a moment keeps it.
   Check the preview (a saved view's settings) and the Save frame view.
6. **Library.** The recipe must change when the pixels change. A bundle or input key is in `settings` already. A
   computer's option goes in `opts`. Keep the default out of the recipe (as `uw` and the export choices are), or
   every render's hash changes. Show or set it in Source settings or Share if the user needs it there. Add it to
   `render_keys_test.py` checks, or to its `PLAYER_ONLY` with why.
7. **NAS stream.** `render_args` passes `user_data` through, and `settings` beside it. Check `afp_stream.py`'s pops
   and overrides (`shader_for`), what it takes of the settings (`denoise_of`), what makes a render again
   (`fingerprint`: add to it only where the pixels change, or every cached render on the NAS goes), and that a file it
   depends on is in `deps`.
8. **Twins.** Run every guard in the table above that the setting touches.

### Versions to raise

- **`renders.rs` `VERSION`**: the renderer gives other pixels for the same recipe. Every cached render becomes
  another one.
- **Workspace `RENDER_VERSION` and `index.rs` `RENDER_VERSION`**: the old per-moment recipe (`render_recipe`)
  changes. Raise both together.
- **`afp_catalog::project::VERSION` and `projectfile.lua` `M.VERSION`**: what the catalog or project keeps changes.
  Older programs then refuse the project instead of wiping it.
- **`index.rs` `GEOTAG`**: what `gyroflow-probe --library` answers changes (for example a new field Source settings
  reads). Else old answers stay cached.
- A new bundle or input key needs no version of its own: the recipe holds it, so its hash changes where it is set.

### Then test

Run everything CI runs (CLAUDE.md, "Tests"). Run the live rig for the paths you changed, and say which you ran.
