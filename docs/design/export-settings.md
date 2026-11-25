# An export's settings: size, encoding, quality

**Status:** built in #254 (v0.14.0). The model in short, with the settings' keys: [../reference/settings-flow.md](../reference/settings-flow.md).

Asked on 2026-11-17: neither the player nor the Library let you choose a video export's resolution, its codec mode
(fast or quality), or its picture quality ("all the same ones as for display"), nor a photo's. This page is the
model, what each choice does on each of the export's two paths, and where it is in each app.

## One description, both apps

A video export has three settings, a photo two. The names are the same in the player (Output tab, Save frame) and in
the Library (Share's dialog):

| Setting | Choices (the first is the default) | Video | Photo |
|---|---|---|---|
| **Size** | Original, 4K, 1080p, 720p | yes | yes |
| **Encoding** | Fast, Quality | yes | no (a photo has no encoder to choose) |
| **Quality** | As displayed, Quickest, Highest | yes | yes |

- **The defaults are what was exported before** these settings existed: Original size, Fast, As displayed. Someone
  who doesn't touch them gets the same files as before, byte for byte in their command lines (the tests check that
  nothing is added). Renders in the Library's cache made before stay found (below).
- **Size** is the output's short side at most (2160, 1080, 720 pixels), the shape kept, never enlarged. A 9:16 or
  turned output is capped on its width. Original is the clip's size, or its Gyroflow project's output size.
- **Encoding.** Fast: the encoder's real-time mode, the source's bit rate for the output's size (as before).
  Quality: the encoder's quality mode and 1.5 times that bit rate.
- **Quality** is the picture-quality settings playback has, as presets. Of Performance & Quality's settings, those a
  file can have: the temporal denoise (how much of it), the edge fill, the color-fringe correction, and the warp's
  interpolation (playback picks it by frame rate). Its frame-rate limit and the size it renders on screen don't apply:
  a file has every frame, at its own size (that's Size). For photos, the clean-up from the frames around it.
  - **As displayed**: as this computer plays the clip now. That is what every export did before, so it is the
    default. Performance & Quality's "Skip color aberration correction" stays screen-only, as its caption has always
    said ("exports and photos have it").
  - **Quickest**: each at its lightest.
  - **Highest**: each at its best.

Where it lives:

- **Player:** `mpv/scripts/gyroflow/exportsettings.lua` (the choices, the defaults, the mapping), kept per computer
  in `gyroflow.conf` (`export_size`, `export_encode`, `export_quality`, `photo_size`, `photo_quality`), published as
  `user-data/gyroflow/export_settings` with the choices; the panel draws from that. `script-message-to gyroflow
  export-setting <video|photo> <key> <value>` changes one.
- **Library:** `library/ui/share.js` (`EXPORT_CHOICES`, the same lists), remembered per computer like the look
  (`afl.share.video.size`…). The `share` command takes `settings: { video, photo }`.

The Library doesn't render: it asks the player's batch (`export-batch`, `photo-at`), each item's request carrying
`export`. So both apps' settings become `gyroflow-export` options in one place: `exportsettings.lua`'s `apply`.

## What each preset is, on each path

`gyroflow-export` has two paths (docs/design/export-gpu-path.md): the GPU path (one ffmpeg, the player's own shaders in the
libplacebo filter) and the pipes (decoder, Gyroflow's engine, encoder). Every setting exists on both: none of them is
silently dropped.

| | As displayed (default) | Quickest | Highest |
|---|---|---|---|
| Temporal denoise | the clip's (Transform), as much as this computer does (Performance & Quality: Full, Light or off) | none | Full, where the clip has it on (even if this computer plays it lighter or not at all) |
| Its Stronger | the clip's | none | the clip's |
| Edge fill | as set (trail, spread out, or the background) | the frame's edge spread out instead of the trail | the trail instead of spread out (the background stays the background) |
| Color fringes under water | the clip's | left as recorded | the clip's |
| Warp interpolation, GPU path | the player's: Lanczos up to 31 fps, bicubic above | bilinear | Lanczos at any frame rate |
| Interpolation, pipes | Lanczos | bilinear | Lanczos |
| Photo clean-up | the clean still frame (Ctrl+p) on: 5 frames each side; off: none | none | 7 each side (the most), rendered by gyroflow-export rather than taken from the frame on screen |

Size and encoding:

| | GPU path | Pipes | Photo |
|---|---|---|---|
| Size | the warp renders at that size directly (libplacebo's `w`/`h`: warp.glsl's RT is the output's): no extra scale | `scale=W:H` after the engine, by the interpolation's filter (bilinear for Quickest, Lanczos else) | the image scaled at the end (`photo::Image::downscaled`, a tent filter as wide as the step), its HDR and SDR renditions alike |
| Encoding: Fast | VideoToolbox `-realtime 1 -prio_speed 1` (as before) | the encoder's default (as before) | — |
| Encoding: Quality | VideoToolbox `-realtime 0 -prio_speed 0`; NVENC `-preset p7 -tune hq`; x265 `-preset slow`; QSV `veryslow`; AMF `-quality quality`; Vulkan's encoder has no mode: the bit rate alone | the same | — |
| Bit rate | the source's for the output's pixels; ×1.5 for Quality | the same | — |

The new `gyroflow-export` options: `--resolution original|2160|1080|720|<n>` and `--encode fast|quality`. The quality
presets are the options that already existed (`--denoise`, `--edges`, `--chromatic`, `--gpu-interpolation`,
`--interpolation`, `--radius`, `--image`).

A clip with nothing to change (not log, nothing to stabilize it with) was skipped ("nothing to change") and Share then
copied the original or cut its part losslessly. With a smaller Size it is now re-encoded at that size.

## Where a choice can't be had

- **360° exports** (the whole sphere, Flat along the recording) are their own renderer with their own size (the 360°
  tab). With one of them chosen on the Export button, the panel greys the three rows and says so.
- **Photos have no encoding** choice: the row isn't there (Save frame, and the Library's Photos part).
- In a **project**, the player's photo key (Alt+s) keeps a photo link and renders nothing: the Library's Share renders
  it later, with the settings chosen in its dialog. Save frame says so; Alt+Shift+s renders now with the player's photo
  settings.
- ProRes (`--prores`, not offered in either app) keeps its own profile: Encoding doesn't apply to it.

## The render cache

A render made with other settings is another render. The recipe (`renders.rs`) gains `export`: the settings that
aren't the defaults (`renders::export_settings`). With the defaults the recipe, its hash and its file name are what
they were, so every render already in a `_Renders` folder is still found. The views' "rendered already" counts are of
the default settings' renders; with other settings chosen, the dialog counts those items as to render.

## The panel: short lists are buttons again

PR #199 (issue #196, b305076) made every single choice a drop-down. Now `ui.lua`'s `select_row` draws a list of two or three
choices as a row of buttons, the chosen one lit, as `segmented` drew them before (#196 kept the widget): the label
above, the buttons across the row. Four or more stay drop-downs, and so does a short list whose labels don't fit a
third of the row (the edge fill's "Trail by direction / Frame's edge spread out / Background color") or that shows a
value off its list (the speed's 1.5×). The buttons' places are named as the drop-down's rows are (`level:strong`).

## The photo button

The photo command was Alt+s (a photo link in a project, else a photo rendered now) and Save frame's "Save the frame as
a photo", which showed only while framing. Now:

- the Output tab has a **Photo** part under Export: "Save a photo of this frame" (Alt+s);
- Save frame shows the photo's kind, size and quality and the same button whether framing or not.

## Measured (2026-11-18, M1 Pro, headless)

The clip: D-Log M, 4K60, 15.5 s, under water (`DJI_…0069_D`, a local copy). The player was a scratch copy of the
installed app (built from main 5956481) with this branch's scripts and `gyroflow-export`, a scratch HOME; no window,
nothing installed, Anton's library not touched.

**Through both apps** (`share_e2e::a_share_with_the_player`, `AFL_SHARE_SETTINGS`): the Library plans a share of a
moment (0.5–2.5 s), a photo link (1 s) and the whole clip, the player's batch renders what the cache lacks, the Library
copies it out. All three saved in each run; the player's command lines carried the settings (below).

| Settings | Whole run | Clip (15.5 s) | Moment (2 s) | Photo |
|---|---|---|---|---|
| Defaults (as before) | 51 s (85 s cold, the first run) | 3840×2160, 93 Mbit/s, 181 MB | 25 MB | 3840×2160 HEIC, 4.7 MB |
| Quickest, 1080p | 16 s | 1920×1080, 24 Mbit/s, 47 MB | 7 MB | 1920×1080 HEIC, 1.6 MB |
| Highest, quality encoding | 75 s | 3840×2160, 137 Mbit/s, 267 MB | 36 MB | 3840×2160 HEIC, 4.6 MB |

**The whole clip alone**, `gyroflow-export` with the flags the player passed (all three on the GPU path):

| | Time | Speed | Size | Bit rate |
|---|---|---|---|---|
| As displayed (default) | 42.9 s | 22 fps | 3840×2160, 180 MB | 93 Mbit/s |
| Quickest, 1080p | 10.0 s | 108 fps | 1920×1080, 47 MB | 24 Mbit/s |
| Highest, quality encoding | 51.1 s | 19 fps | 3840×2160, 266 MB | 137 Mbit/s |

The pipes, forced (`--gpu off`, 2 s): Quickest 1080p 3.3 s (1920×1080), Highest 12.8 s (4K, 143 Mbit/s), defaults
12.6 s. The same frame (6 s, a part at 100 %, the 1080p one doubled), HLG shown as is:
`docs/img/export-settings/frames-displayed-quickest-highest.png`.

## Tests

- `luajit tests/exportsettings_test.lua`: the choices, the defaults adding nothing, each preset's options.
- `luajit tests/export_args_test.lua`: the player's own export, the Library's batch and photo requests with and
  without settings, `export-setting` keeping them in gyroflow.conf.
- `luajit tests/sidepanel_test.lua`: the buttons-for-short-lists rule, the photo buttons, the settings rows, greyed
  for 360.
- `cargo test --bin gyroflow-export export_settings`: `--resolution`, `--encode`, the bit rate and the pipes' scale on
  the command line; `cargo test --lib an_image_is_made_smaller`: the photo's scale.
- `library`: `renders::tests::the_export_settings_are_part_of_a_render`, `share::tests` (settings asked of the player,
  other renders); `library/tests/share.js` (the dialog).
