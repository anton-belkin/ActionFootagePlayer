# Gotchas
Checked against 295101c (2026-11-25).

Traps that cost a session time, grouped by area. Each: what you see, why, what to do, and where in the code. The
short list every session must know is in [CLAUDE.md](../../CLAUDE.md), "Traps". Rigs are in [testing.md](testing.md),
builds in [release.md](release.md).

## libplacebo user shaders

These apply to `shaders/gyroflow/warp.glsl`, `shaders/*.glsl`, `mpv/scripts/*/*.glsl` and anything mpv loads with
`glsl-shaders`. `vs-gyroflow/src/user_shader.rs` parses them the way libplacebo does, for the export, and its test
`every_shader_of_ours_parses` reads every `.glsl` of ours.

- **`//!` anywhere is a directive, comments too.**
  - You see: the shader fails, for example "'hook' : no matching overloaded function found".
  - Why: libplacebo reads `//!` as a directive even inside a comment, and the pass ends there.
  - Do: never write `//!` in a comment. Gyroflow's lens GLSL (`shaders/gyroflow/lens/`, gyroflow-core's
    `distortion_models/`) has some; `gpu_table.rs` `lens_code` rewrites them to `// !`.
- **A comment after `//!TEXTURE` or `//!BUFFER` is read as the hex data.**
  - You see: "must be a valid hexadecimal sequence".
  - Do: put comments inside a HOOK pass, never right after a texture or buffer block.
- **A comment after `//!PARAM` is read as its value.**
  - You see: the player hangs on a black window, and the log grows by hundreds of MB.
  - Why: libplacebo takes everything up to the next directive as the value.
  - Do: no comments between `//!PARAM` and the next directive. `user_shader.rs` now refuses such a file ("its value
    … isn't a number (a comment after it?)").
- **`frame` counts every pass.**
  - You see: a ping-pong on `frame & 1` never flips in a two-pass file.
  - Why: `frame` counts executions of the whole file, once per pass.
  - Do: keep your own counter in a `//!STORAGE` buffer, with a "ready" magic value (buffers start as garbage).
  - ffmpeg's libplacebo filter feeds no `PTS`, so the export's GPU path derives it from `frame` as
    `(frame - 1) / npass / FPS` (`gpu_table::for_libplacebo`, `passes_per_frame`). Every pass must then run every
    frame (`WHEN gyroflow_gpu_on`), or the count drifts.
- **`r16f` storage textures read back 0** through MoltenVK. Use `rgba16f`.
- **`keepaspect=no` drops the side panel's margins.**
  - You see: the video fills the whole window under the panel.
  - Why: mpv ignores `video-margin-ratio-*` with `keepaspect=no`.
  - Do: set `video-aspect-override` to the area's W:H instead (`mpv/scripts/view360/main.lua`).
- **`MAIN.w` in a pass's `WHEN`/`WIDTH` is the video's own size**, even after the warp replaced MAIN (libplacebo
  7.387). A pass gated on it never ran (the FSR work, PR #244). Use `HOOKED.w`.
- **PARAMs work in `WIDTH`, `HEIGHT` and `WHEN`**, DYNAMIC ones too. `OUTPUT` there is libplacebo's target rectangle.
- **`HOOKED_raw` × `HOOKED_mul` is not always code / (2^depth − 1).** 10-bit limited range reads ×65472/65535. It is
  linear, so read known black and white codes and scale by them (the 360 stamp does).
- **Per-clip values in the shader's text cost seconds** (next section). Pass them as `//!PARAM` (DYNAMIC) or a texture.
- **A DYNAMIC parameter change doesn't redraw the frame** in a stock mpv: libplacebo's frame cache hashes the hooks,
  not their values, so the view moves only with the next video frame. Our mpv has patch 0011. A shader that must react
  while paused needs our mpv, or a new frame.
- **Dolby Vision 8.4 exports decode as PQ.** `gyroflow-export`'s HLG files carry DV 8.4 metadata, so mpv's
  `video-dec-params` say `gamma=pq`, `colormatrix=dolbyvision`, and a MAIN shader sees PQ. To see the HLG signal, set
  `dolbyvision=no` and `colormatrix=bt.2020-ncl` on the frame (`mpv/scripts/dlogm/main.lua`, `SDR_TAGS_NO_DOVI`).

## Shader pipelines and caches

docs/design/gpu-render.md, "Pipeline builds", has the numbers.

- **New shader text costs 1.5-8 s; text seen before costs ~16 ms.**
  - You see: a clip, a moment or a toggle opens slowly the first time, fast after.
  - Why: every pass is SPIR-V (cached by its GLSL text) plus a Metal pipeline (cached by the SPIR-V). A `#define` with a
    per-clip value makes every pass new.
  - Do: keep per-clip values out of the text. `gyroflow_time_offset` (#219) and `gyroflow_fps` are DYNAMIC params in
    `warp.glsl` for this reason. Check with `tools/pipebench` before a live run.
- **The underwater colour rebuilt every pass.** libplacebo put linearize helpers for the source's transfer into every
  user pass. `tools/libplacebo-patches/0001` adds them only where used. A stock libplacebo still has the cost.
- **A new build is new text.** The first start of each kind of clip after an install pays once. Don't judge speed
  on the first open after a trial install.
- **Caches follow `HOME`.** `gyroflow-gpu-table` and `gyroflow-export` keep shaders, motion and probes in
  `~/Library/Caches/ActionFootagePlayer/{gpu,motion,probe}` (`vs-gyroflow/src/lib.rs`, `file_cache`). A rig run
  with your `HOME` fills them and later runs hit them. Set `HOME` to a scratch folder.
- **"Spent N ms" is logged only above 10 ms.** Use `--msg-level=vo/gpu-next/libplacebo=debug` for every pass's
  source, and diff two starts.
- **Metal's real cache** is `com.apple.metal/<gpu>/` in the user's cache folder, not the near-empty
  `com.apple.metalfe`.

## mpv and its Lua plugins

- **Lua's 200 top-level locals.** `gyroflow/main.lua` is near the limit. `workspace/main.lua` is its plugin's wiring
  only (see the next point).
  - You see: "main function has more than 200 local variables" and the plugin doesn't load.
  - Do: put new code in a module of its own (`transform.lua`, `sidecar.lua`, `ahead.lua`, `measure.lua`), required
    without a new local where needed (`require('measure').setup(...)` in `gyroflow/main.lua`). Add the module to the
    packagers; `tests/packaging_test.py` checks it.
- **The workspace plugin's parts share `W`** (`workspace/main.lua` makes it; each part is a module with
  `install(W)`).
  - A part calls another's function through `W` as it runs (`local function refresh(...) return W.refresh(...) end`
    at its top), so parts install in any order; a value it keeps from `W` at install (`fs`, `catalog`, `cache`,
    `markers`, `batch`) must have been put there by a part installed before it (the order in `main.lua`).
  - `W.state` is one table for the plugin's life (published as `user-data/workspace`): a new file refills it
    (`W.replace_state`), never `state = …`. "Another file by now" is `W.generation`, not a table compared.
  - Every event, key and script message is registered in `main.lua`, in the order they were when it was one file:
    a `file-loaded` handler's place there is the order it runs in.
- **Script messages and key bindings share one namespace.**
  - You see: a message does nothing (the Lens drop-down, #225).
  - Why: `mp.add_key_binding` registers its name as a script message too, and takes over a message of that name
    unless the message is registered after it.
  - Do: name bindings apart from messages (for example `crop-key-*`). `tests/script_messages_test.py` checks.
- **`screenshot-to-file`:** use an absolute path (a relative one lands in mpv's working folder). `mp.command_native`
  returns nil even on success, and a 4K PNG takes seconds to write: wait for the file. A screenshot at another size
  than the window renders the frame again at that size, which resets size-keyed state (the denoise ring).
- **The app has `autocreate-playlist=filter`** (`mpv/app.conf`). With `--length` or at a clip's end, mpv goes on to the
  next clip in the folder, and unload hooks fire again. Pass `--autocreate-playlist=no` for one-clip runs. A moment
  opened from `_Moments` plays the whole folder.
- **Under `lavfi-complex`, mpv gives the tracks' decoders no hardware device.** `hwdec=videotoolbox` silently becomes
  software decoding. Use `videotoolbox-copy` / `auto-copy` there (`gyroflow/main.lua`, 360 without `--video-join`).
  mpv also decodes the tracks one after another in its playback thread: `vd-queue-enable=yes` gives each its own.
- **A warp whose output shape isn't the source's** needs `file-local-options/video-aspect-override` set to the output's
  W:H (`gyroflow/main.lua`), or mpv shows it in the source's shape.
- **In the grid (RGB) layout, `cscale=bilinear`.** mpv converts the whole source to RGB first; the app's Lanczos chroma
  upscale took 10 ms of a 5.3K frame, and the warp resamples it anyway (`gyroflow/main.lua`).
- **IPC socket paths over ~104 characters fail silently** on macOS. Use `/tmp/<short>.sock`.
- **`display-names` adds a number:** "Built-in Retina Display (4251086178)". `--screen-name` wants the plain name.
  Strip `%s*%(%d+%)$` (`mpv/scripts/dlogm/main.lua`).
- **`video-params` is the decoder's size.** For what is shown, read `video-out-params`.
- **IPC mouse events sent at once coalesce.** Space them ~80 ms. Click with `mouse x y` then `mouse x y 0 single`.
- **`--window-minimized` stops rendering.** Screenshots then fall back to the raw frame. Use `--focus-on=never`.
- **LuaJIT's table order changes between runs.** Sort keys before hashing or comparing.
- **Killing the app's shell wrapper leaves its mpv running.** Kill `Contents/Helpers/mpv` too.
- **A stock mpv keeps every user shader it has loaded** until its window closes (~450 MB each with Ctrl+p). Our mpv's
  patch 0015 drops unlisted ones. With another mpv, `gpu_flush` in `gyroflow/main.lua` remakes the window past
  1.5 GB (4 GB: at once). For "slower over a session" reports, measure `footprint -p <pid>` across settings changes.
- **A batch player (`AFP_BATCH`) must not take the agent's socket or `session.json`.** It doesn't now
  (`tests/agent_test.lua`); keep it so when changing `mpv/scripts/agent/main.lua` or `macos/player.sh`.

## The display and the GPU

- **mpv with `vo=gpu-next` hangs while the Mac is locked or the display sleeps.**
  - You see: the log stops after "VO: [gpu-next] …", with "no Screen or DisplayLink available". It looks like a
    shader hang.
  - Do: check `ioreg -n Root -d1 | grep CGSSessionScreenIsLocked` first. Test headless (`--vo=null`) when locked.
    The display sleeps after a few minutes without input; IPC commands don't count as input.
- **An mpv older than 0.42 (Homebrew's 0.41.0) has no frame PTS for shaders**, so the GPU path is off and clips play
  as recorded with a card (`mpv_has_pts` in `gyroflow/main.lua`). A trial app built without `tools/build_mpv.sh`
  carries that mpv.
- **The screen's HDR headroom:** `NSScreen.maximumPotentialExtendedDynamicRangeColorComponentValue` through
  `osascript -l JavaScript` (~0.3 s). The XDR says 16, an SDR screen 1.
- **`CGWindowList` returns no windows** without the screen recording right, so `screencapture -l` can't target the
  player. Use mpv's own screenshots.

## Exports and photos

- **`gyroflow-export --photo` on a clip inside a project writes into the project's `_Stills`**, whatever `--output`
  says. With `--force` it replaces Anton's own still of that clip and time.
  - Why: photos go to `--out <folder>`, else `<project>/_Stills`, else `stabilized/` beside the clip
    (`out_dir_for` in `gyroflow-export.rs`). `--output` is for one video only.
  - Do: pass `--out <scratch folder>`, never `--force` against his Library, and read the `-> path` it prints on the
    first run before looping. Or copy the clip outside any project.
- **Keep the export's shader to iterate without rebuilding:** `AFP_GPU_KEEP=1 AFP_TIMES=1 gyroflow-export …` keeps
  `.<output>.partial.glsl` (a dotfile beside the output) and prints the ffmpeg command. Edit, rerun the command.
- **Comparing an export's frame with ffmpeg's `-ss`:** put the time a quarter frame before the frame
  (`tests/uw_export_check.py`). The exporter rounds `t * fps`; `-ss` takes the first frame at or after.
- **Old GoPro H.264 is `yuvj420p` in a stream tagged `tv`.** It is full range; read it so, as mpv does
  (`gyroflow-export.rs`, `full_range`).
- **AVAssetWriter's default timescale is 600.** Video and metadata times get rounded (a Live Photo's still time lands
  mid-frame). Set `mediaTimeScale` to the track's `naturalTimeScale` (`macos/photo.swift`). Validate Live Photo pairs
  with `PHLivePhoto.request(withResourceFileURLs:)`.
- **`afp-cut` refuses a track with sample tables beyond** `stsd stts stss stsc stsz stco co64 sdtp sgpd sbgp`
  (`ctts`, for one: Lumix HEVC) (`cut.rs` `read_track`). Share then copies the part with the player's ffmpeg
  (`-c copy`).
- **`tools/one_off/photos_to_links.py --move-renders`** puts the old photos in `_Renders/Photos/<path>`, which Clear
  the render cache keeps (not `<hash>.<profile>`) but Share never finds. Change that step before it's run.
- **A shell variable holding arguments:** zsh doesn't split `$o` into words. Use `${=o}`, or pass arguments one by one.
- **Comparing two exports frame by frame:** the HEVC export (VideoToolbox's real-time mode) isn't reproducible: the
  same command twice gives other frames (PSNR ~50 dB), and so do the pipes. Compare with `--prores` on the GPU path,
  which is (every frame's hash equal). gyroflow-export outside the app finds Homebrew's ffmpeg, which has no
  libplacebo, and takes the pipes: point it at the app's with `GYROFLOW_TOOLS=<app>/Contents/Helpers`.
- **A setting for an export is a key of the settings JSON, never a flag** (`--settings`; registry.json
  `settings_json`; [contracts](../reference/contracts.md#the-settings)). `tests/settings_json_test.py` fails on a flag
  a key stands for in the player's Lua; `gyroflow-export`'s test `every_key_the_export_takes_is_read` fails on a key
  the registry lists and `apply_settings` doesn't read. mpv's `utils.format_json` writes an empty table as `[]`:
  `--settings []` is read as `{}`.
- **The player's request is the video itself as `project` when there's no project.** gyroflow-export takes a
  `.gyroflow` only from the settings, and finds the clip's own project otherwise, as it always did.

## Motion measured from the picture (.afp-motion)

- **Gyroflow's frame rate differs from the MP4's in its last bits.**
  - You see: the rolling-shutter correction silently drops; the motion looks almost right.
  - Why: the correction's context includes the frames' times. Gyroflow reads fps from the decoder.
  - Do: carry the project's `video_info` fps, frame count and duration in the motion file, and use them when they
    are within a hair of the clip's (`vs-gyroflow/src/afp_motion.rs`, `from_project` and `apply`).
- **A lens profile with an empty `calibrator_version` makes gyroflow-core refuse it**, and the readout is then 0. Our
  built-in lenses carry one; a lens kept by the player writes one.

## The catalog, shares and the NAS

docs/design/library-management.md §9 and docs/design/catalog-safeguards.md have the design.

- **A stat over SMB costs ~5 ms; listing 100 entries ~80 ms uncached.** Never stat per file. List a folder with
  `getattrlistbulk` (`catalog/src/store.rs`).
- **`fsync` on smbfs:** std's `sync_data` is `F_FULLFSYNC`, which smbfs refuses (ENOTSUP). Use libc `fsync` and accept
  ENOTSUP, EOPNOTSUPP and EINVAL (`store.rs`).
- **Times differ by precision.** SMB carries 100 ns units; the NAS's own disk has ns. Compared exactly, each side saw
  the other's records as changed and re-noted every file. Times are cut to 100 ns everywhere (`store::canon`).
- **A Mac sees another computer's new files ~30 s late,** and may give a folder's new mtime with its old listing.
  Clean-ups wait 10 minutes (`grace`); a changed folder is listed again after 60 s (`recheck.json`).
- **In-place rewrites leave the folder's mtime alone,** and FSEvents on a share is unreliable. Whatever writes in a
  project must note it to the catalog at once (`seen::note_now` in the Library, `catalog-note` in the player).
- **A share's `#recycle` holds a copy of every overwritten file**, "Copy #N of Project.afproject" for each save, in a
  folder named like the project. Opening that folder looks like a project that lost its changes. Tell them apart by
  modification time; check the Library's recent list for a `#recycle` path. The walks skip `#` names.
- **Older programs must refuse a newer project, never rewrite it.** A NAS Helper built before a change re-noted
  records without the new fields, and the Library lost them seconds later (#220, #249). Raise
  `afp_catalog::project::VERSION` and `projectfile.lua` `M.VERSION` together; update the NAS Helper before using the
  new version on the NAS.
- **Reading a whole clip over the share starves everything else on that connection** (stat 6 → 225 ms during a 1.7 GB
  read). The table is read ahead in steps (`ahead.lua`, docs/design/gpu-table-ahead.md).
- **Drop a file from this Mac's cache without sudo:** mmap it and `msync(MS_INVALIDATE)` (`tests/gpu_ahead_e2e.py
  --cold`). The NAS keeps its own cache.

## Builds, installs and checkouts

- **Many registered copies of the app.** A double click opens a worktree's `build/` app, a `-previous.app` or a
  rig's clone. Unregister all but `~/Applications` ([release.md](release.md), "Launch Services").
- **The app in `~/Applications` changes under you.** Anton and other sessions install trial builds at any time. Test
  the app you built (`open -a <worktree>/build/ActionFootagePlayer.app`, or pass it to the rig).
- **`install.sh --apps` is not the release's app.** It runs Homebrew's mpv. Build trials with `tools/dev_app.sh`.
- **Windows checkouts may have CRLF line ends.**
  - You see: the GPU path doesn't start on Windows (fa28a24).
  - Why: `gpu_table.rs` finds `warp.glsl`'s sections by `\n`, and Git for Windows' `core.autocrlf` wrote `\r\n`.
  - Do: `.gitattributes` keeps `*.glsl`, `*.glsl.in` and `*.wgsl` LF. Parsers and shaders must accept `\r\n` anyway.
- **`tools/build_mpv.sh` refuses a changed mpv clone.** Change patches in `tools/mpv-patches/`, not in
  `build/mpv-afp/src/mpv`.
- **ffmpeg's configure breaks on spaces in paths.** `FFPL_DIR` must have none (the default under `~/Library/Caches`
  is fine; a worktree path under "VSCode workspaces" is not).
- **Homebrew's ffmpeg has no libass**, so it can't burn subtitles in. Use mpv's `screenshot-to-file … subtitles`.
- **Cargo isn't on PATH** in a fresh shell: `$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin` or
  `/opt/homebrew/opt/rustup/bin`.
- **Apple's `strip` and `install_name_tool`,** not another one earlier in PATH (Anaconda's cctools): those leave a
  broken signature. `tools/dev_app.sh` takes them from `xcrun`.

## The Library page

- **`lenses.js` loads before `app.js`** (`library/ui/index.html`), so it must not touch `S` at load time.
- **In jsdom, run the page's scripts with `vm.Script(...).runInContext(dom.getInternalVMContext())`,** not
  `window.eval`: `app.js`'s top-level `const`s must be shared between scripts. Stub `Element.prototype.scrollIntoView`
  and `CSS.escape` (`library/ui-tests/page.js` does).
- **jsdom runs the page's scripts as one,** so a script that uses a later script's name at load time passes there and
  fails in the real window. Only headless Chrome (the shot scripts) catches it. The order is `ui/index.html`'s.
- **WKWebView's `position: sticky`** works only on a direct child of the `overflow:auto` scroller (`#main`), with an
  opaque background, and no box between them that clips overflow (an `overflow:hidden` ancestor becomes the scroll box
  and nothing sticks). `.main>.mbar`, `.tlday header` (`style.css`).
- **Leaflet's panes have z-indexes up to 1000.** `.mapbox{z-index:0}` keeps the map under the pinned bar.
- **The Filters panel is frozen while used** (`S.ffreeze`, keyed by `filtersKey()`). The key includes
  `probesVersion` and the items' identity: frozen before the probes came, the panel stayed empty.
- **Timeline keeps its model** (`TL.model`, keyed by `sig`). A switch that changes what's shown (Hide rejected) must be
  in the key. Its tile size is in two places: `TL` in `timeline.js` and `.card.tl` in `style.css`.
- **Buttons with their own listeners:** `culling.js` uses `data-c`, `atstart.js` `data-s`, not `data-a`.
- **`shot.js` and the other shot scripts make a fresh Chrome profile per run.** State a shot needs must come from its
  own script lines, not from an earlier shot.
