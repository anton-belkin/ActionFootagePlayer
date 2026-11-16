# Export on the GPU with the player's own shaders (ffmpeg's libplacebo filter)

Question (2026-11-16): can `gyroflow-export` run the player's GPU path, the same warp and LUT shaders, through ffmpeg's
`libplacebo` filter, with the frame staying on the GPU up to the encoder? This is stage 1, the test. Stage 2, the
build, is **not done**: the reason is in the verdict.

## Built: the GPU export path (`gyroflow-export --gpu`, default `auto`)

`gyroflow-export` renders on the GPU wherever the clip and the ffmpeg allow it. It uses **one ffmpeg**: hardware
decode → the libplacebo filter running **the player's own shaders** → hardware encode, with **no copy of a frame**
on a Mac. The pipes (decoder → Gyroflow's engine → encoder) remain the fallback, and the export says why it fell back,
e.g. `through the pipes, not the GPU path: …`. `--gpu on` takes only the GPU path; `--gpu off` never.

**The shaders.** They are `gpu_table`'s, as the player loads them, plus the look:

- the warp at the view, level, FOV, camera and frame turn (`Options::player`, the same code as
  `gyroflow-gpu-table --player`);
- the fringes under water, the edge fill (smooth or sphere) and the temporal denoise (`--denoise`);
- the translation compensation (`--with-translation`, Ctrl+j's cache);
- then the look: the export's own `.cube` (log → HLG, DJI's Rec.709, NBCU composed), baked as the player's look
  shaders are (`gpu_table::look_shader`, the `log_hlg.glsl.in` template).
- **Interpolation:** the player's for the clip's frame rate: Lanczos up to 31 fps, bicubic (9 fetches) above
  (`--gpu-interpolation` overrides).
- **Frame time:** the filter feeds no `PTS`, so each pass counts its frame with libplacebo's pass counter
  (`gpu_table::for_libplacebo`). The parameters the player sets at run time are written as their defaults
  (`set_param_default`): the denoise's radius, strength and grain.
- **Resolution:** the export's, which the warp renders at directly: it is its own scaler.
- **fps:** the clip's.

**Mac, no copies:** `tools/export-gpu/ffmpeg-patches/0001` (our ffmpeg build,
`tools/export-gpu/build_ffmpeg_libplacebo.sh`).

- The libplacebo filter takes VideoToolbox frames and gives them out. Each IOSurface plane becomes an MTLTexture and
  is imported into Vulkan (MoltenVK, `VK_EXT_metal_objects`), kept per surface.
- The output comes from a VideoToolbox frames pool, its colour attachments set for the encoder. Without them
  `hevc_videotoolbox` rejects the buffer (-12902).
- The encoder runs in VideoToolbox's real-time mode: 82 fps for 4K Main10, the same PSNR at the same bit rate.

**Windows and Linux** (not run here):

- Vulkan Video's decoder where the driver has it (`-hwaccel vulkan -hwaccel_output_format vulkan`), else decoded and
  uploaded.
- The filter on a Vulkan device.
- `hevc_vulkan` where the driver encodes HEVC, with no copies; else a download to NVENC / AMF / QSV / x265 (one copy).
- The Windows app now carries BtbN's GPL build of ffmpeg 9.0.2 for x64 too, not Gyan's essentials, which has no
  libplacebo and no Vulkan (`tools/make_windows_app.sh`; its check requires `hevc_vulkan` and the libplacebo filter).

**Measured** (D-Log M 4K60 under water, 15.5 s, 926 frames, M1 Pro, quiet machine):

| Export | Pipes (before) | GPU path |
|---|---|---|
| Whole clip, HDR (HLG), fringes, default | 53.8 s | **15.6 s** in all, engine load included (13 s of frames, 74 fps) |
| Moment 3-5 s, fringes off | 4 s (28 fps) | 3 s (45 fps); each frame is its own index (MSE 7 against 200-400 for its neighbours) |
| Moment 3-5 s, temporal denoise full | 13 s (9 fps) | 8 s (15 fps), frames aligned |
| Whole clip as Share has it (wide, horizon lock, edges sphere, fringes, denoise light) | 68 s (14 fps) | **35 s** (26 fps) |
| **Share's headless test** (#242's `share_e2e`: clip + moment + photo, the player batch-rendering) | 80 s | **46.8 s**, passes |
| SDR (DJI's LUT) | | 42 fps |
| Wide view + edges smooth / sphere | | 34 / 33 fps; edges filled (dark border share 33 % → 0 %) |
| ProRes 422 HQ | | 57 fps |
| Dolby Vision 8.4 (`--dv-rpu`) | | 52 fps, 120 RPUs, dvvC written |

**The denoise up to the clip's end:** the last decoded frame is repeated as the frames after it (`loop`), where the
player shows its last frames with fewer after them: only the clip's last `radius` frames differ.

**Pictures.** Against the pipes, Y is 38-39 dB. That is the player's warp (8-px grid, its interpolation) against
Gyroflow's per-pixel engine: what the player shows, not what the pipes rendered.

**Still on the pipes** (with the reason printed):

- the preview, photos and their Live Photo / Motion Photo motion;
- `--youtube`, `--crop`, `--dv-live-l1` (each frame's L1 is measured by the pipes' LUT);
- clips the GPU warp can't render: lens correction below 100 % (both GoPro test projects), Sony / mesh lenses,
  IBIS/OIS, equirect;
- a turned clip without stabilization;
- 360 and reframe exports;
- an ffmpeg without the libplacebo filter, or a Mac ffmpeg without the patch (Homebrew's).

**The test.** `vs-gyroflow/tests/export_gpu.rs` (`AFP_GPU_CLIP`, `AFP_GPU_FFMPEG`) renders 2 s of a real clip both ways:
the same frame count, each frame closest to its own index, PSNR above 33 dB. It passes on the D-Log M clip.

**Shipping.**

- **macOS:** `tools/dev_app.sh` bundles the patched ffmpeg (`FFMPEG_BIN`, from `build_ffmpeg_libplacebo.sh`'s default
  folder `~/Library/Caches/afp-ffmpeg-build`). `bundle_runtime.py` lists its sources. **The release workflow doesn't
  build it yet:** it needs a step running `build_ffmpeg_libplacebo.sh` (about 10 min on the runner) before
  `bundle_runtime.py` with `FFMPEG_BIN`.
- **Windows:** the BtbN build above.
- **Share's recipe:** version 4 (`renders::VERSION`), since the GPU path's pixels differ from the pipes'.

## Verdict

**Update, the zero-copy spike** (see "The zero-copy spike" below): libplacebo called directly, with VideoToolbox
IOSurfaces imported into Vulkan, renders the player's shader with its real `PTS` at the encoder's speed. The
D-Log M clip takes **18.9-19.1 s, against 18.1 s for the encoder alone, 24.6 s for the filter and 53.8 s today**. It is the
same picture as the filter path. That is the route to "render what you see, no longer than encoding takes". The
filter findings below stand as the comparison.

**It works on a Mac, with limits.** The speed gain is real but smaller than hoped, because on a Mac the frame does
**not** stay on the GPU.

- **It runs.** ffmpeg 9.0.2's `libplacebo` filter runs the player's generated shaders unchanged: the 8-px grid compute
  pass, the sampling passes, the 2-D `rgba32f` table texture of 16 384 × n texels, `DYNAMIC` parameters at their
  defaults, and the LUT shader's 3-D `rgba16` texture. It does this on MoltenVK with the player's pinned libplacebo
  v7.387.0.
  - One substitution is needed. Nothing feeds `PTS` (mpv does: `gpu_get_auto_param`). Each pass therefore takes its
    frame from libplacebo's own pass counter, `frame`; see (b).
  - The frame index was checked against the exporter: frame *n* matches frame *n* best, out of all 120.
- **The colour matches** today's exporter at about 60 dB, with a mean difference under 1 code of 1023, for both the HLG
  and the Rec.709 LUT.
- **The stabilized picture is the player's, not today's export's.** Against the exporter it scores Y 39 dB, with a mean
  difference of 7 codes of 1023. That difference is the player's warp itself: its 8-px grid and its own Lanczos,
  against Gyroflow's per-pixel engine.
  - With the colour fringes taken out, the two also differ by a **0.33 % zoom**. That is the player's in-warp fringe
    correction against the exporter's chromatic.glsl. It is a player ≠ export difference that exists today and has
    nothing to do with this path; worth its own look.
- **Speed, D-Log M 4K60, 15.5 s, 926 frames, HEVC Main10 HLG, M1 Pro:**

  | Fringes | Today | libplacebo path |
  |---|---|---|
  | On | 53.8 s | 24.6 s (2.2× faster) |
  | Off | 29.1 s | 24.8 s in the RGB grid mode (1.17×); 35.6 s in the player's planes mode, which is slower here |

  - **The path's floor:** libplacebo with no shader, just the upload, convert and download, already takes 13.8 s.
  - **The encoder's floor:** VT encode fed from a decode takes 18.6 s.
- **The frame does not stay on the GPU on a Mac.** ffmpeg 9.0.2 cannot map VideoToolbox frames into Vulkan, or Vulkan
  frames into VideoToolbox: there is no IOSurface / `VK_EXT_metal_objects` code in `hwcontext_vulkan.c`. Each frame is
  therefore copied four times: VT download, libplacebo upload, download, and VT encoder upload.
- **Limits:**
  - **Clips the GPU warp can't render** fall back to today's path, exactly as the player plays them through
    VapourSynth: lens correction below 100 %, Sony / mesh lenses, IBIS/OIS, equirect.
    - **Both GoPro test clips are such clips** (HERO12 Egypt and the 2019 dive: their projects have "lens correction
      0 %").
  - **Not tested here:**
    - the denoise passes (ring of frames over the counter)
    - the edge fill and sphere trail
    - moments (`--from/--to` through `time_offset`)
    - Dolby Vision
    - Windows and Linux

What I recommend, and why stage 2 wasn't built: the gain is large only for clips with fringes. Without fringes it is
15 %, and the path is capped by four copies. Building the full `gyroflow-export` path now would buy:

- **Correctness:** export = the player's pixels.
- **Fringe clips:** 2×.

It would also add a second ffmpeg to ship (about 25 MB plus MoltenVK). **Decide first:**

1. **Is "export = what the player shows" worth a new default?** It changes everyone's exports by up to about 7
   codes of 1023: sharper or softer by the warp's kernel, and on fringe clips by the 0.33 % zoom.
2. **If speed is the goal,** the bigger win is **libplacebo called from the exporter** (below). It removes the copies.

## 1. Where today's time goes (D-Log M, `DJI_20261022152937_0069_D.MP4`, 3840×2160 59.94p, 926 frames)

`gyroflow-export` from this branch, with timers added (`AFP_TIMES=1`: the engine's and the LUT's time, summed over
the three GPU workers). The clip is copied locally to the scratch folder, on an M1 Pro.

| Step | Time (926 frames) | Per frame |
|---|---|---|
| Whole export, fringes on (default, under water) | **53.8 s** (17 fps) | the player's shaders on wgpu (chromatic.glsl) alone 56.4 ms, serial: the bottleneck |
| Whole export, `--chromatic no` | **29.1 s** (32 fps) | engine 72.0 ms ÷ 3 workers ≈ 24 ms; GPU LUT 17.5 ms ÷ 3 ≈ 6 ms |
| Decode (VT) → null | 3.7 s | 4 ms |
| Decode (VT) → `yuv420p10le` over a pipe | 7.9 s | 8.5 ms |
| Decode → CPU `lut3d` (tetrahedral) → null | 49.3 s | 53 ms (today's exporter does the LUT on the GPU instead, `gpu_lut.rs`; CPU only as a fallback) |
| Decode → pipe → `hevc_videotoolbox` Main10 at the source's bit rate | 18.6 s | about 20 ms: the encoder's floor (≈ 50 fps) |

**Earlier figure:** "about 90 s" was measured earlier from another setup, probably the NAS or the denoise.

**Locally, the two cases split like this:**

- **Fringes off:** the engine (about 24 ms wall) and the VT encoder (about 20 ms) set the pace.
- **Fringes on:** the serial wgpu post shader (56 ms) does.

## 2. ffmpeg with libplacebo

### Homebrew's ffmpeg won't do

Homebrew's ffmpeg 9.0.2 has neither `--enable-libplacebo` nor Vulkan (`ffmpeg -buildconf`; `-hwaccels`: videotoolbox
only). So I built one. Recipe: `tools/export-gpu/build_ffmpeg_libplacebo.sh`.

- **What it builds:** ffmpeg n9.0.2 with `--enable-vulkan --enable-libplacebo --enable-videotoolbox --enable-gpl
  --enable-version3`.
- **libplacebo:** v7.387.0, the player's pin (`tools/mpv.lock`), built static with shaderc, lcms2, vulkan and dovi.
- **Build gotchas:**
  - It needs `--extra-libs=-lc++`, because a static libplacebo has a C++ object.
  - Without that, configure silently falls back to Homebrew's libplacebo 7.360 dylib.
  - The work folder can have no spaces in its path.
- **At run time:** Vulkan is MoltenVK 1.4 from Homebrew, through the loader (`DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib`,
  as ffmpeg dlopens `libvulkan`). The device found is "Apple M1 Pro".
- **Built size:** 24.9 MB.

### (a) The player's shaders, as they are

What I ran:

```
AFP_GPU_SHADER=warp.glsl gyroflow-export clip.MP4        # writes warp.glsl and warp.glsl.lp.glsl, skips the export
cat warp.glsl.lp.glsl build/dlogm_hlg.glsl > full.glsl   # + the player's LUT shader (install.sh's)
ffmpeg -hwaccel videotoolbox -noautorotate -i clip.MP4 -map 0:v:0 -map '0:a?' -c:a copy -map_metadata 0 \
  -vf "setparams=color_primaries=bt2020:color_trc=arib-std-b67,\
       libplacebo=w=3840:h=2160:format=p010le:colorspace=bt2020nc:color_primaries=bt2020:color_trc=arib-std-b67:range=tv:\
       dithering=none:custom_shader_path=full.glsl" \
  -c:v hevc_videotoolbox -profile:v main10 -b:v 89874482 -tag:v hvc1 \
  -color_primaries bt2020 -color_trc arib-std-b67 -colorspace bt2020nc -color_range tv out.mov
```

- **Where the shader comes from.** The exporter writes the warp from its own engine's view, through
  `gpu_table::Options::player`, the same options `gyroflow-gpu-table --player` uses. The code is shared, not copied:
  `player_shader` now calls it too.
- **Tags.** The `setparams` tags the input as HLG / BT.2020, as the dlogm plugin's `vf=format` does. Its matrix stays
  the file's BT.709. libplacebo then maps HLG → HLG, which is an identity, exactly as mpv's target does.
  - For SDR (DJI's LUT, `dlogm_lut3d.glsl`) nothing needs tagging: BT.709 → BT.709.
- **What ran without change:**
  - `//!HOOK MAIN` (grid mode: the `COMPUTE 8 8` grid pass, `SAVE`/`BIND` between the passes)
  - `//!HOOK LUMA` and `//!HOOK CHROMA` (planes mode)
  - `//!TEXTURE … rgba32f` (the table)
  - the LUT's `//!TEXTURE … rgba16` 65³ (3-D)
  - `//!PARAM … DYNAMIC` at their defaults
  - `WIDTH OUTPUT.w gyroflow_render_scale *` (RPN)
  - `WHEN gyroflow_gpu_on`
  - the lens-model code
- **The mpv patches.** No shader relies on one of the 16 mpv patches. They are about presentation, Dolby Vision,
  gestures, and redrawing or unloading shaders.
  - What mpv *upstream* gives that the filter doesn't is `PTS`: `vo_gpu_next.c` `update_hook_opts_dynamic` fills it
    from the frame.
  - The `glsl-shader-opts` values come from mpv too. The filter has no option for shader parameters (`extra_opts`
    sets only `pl_options`). In the export, then, everything is the file's default:
    - `gyroflow_time_offset`, which the generator writes;
    - the camera (crop mode's), which would have to be written as the parameter's default value;
    - `gyroflow_render_scale` 1.

### (b) Per-frame inputs without a script

**Motion table.** It is already baked into the shader for the whole clip: one 16 384-wide `rgba32f` texture,
334 frames a row (49 texels a frame, 33 knots).

- The texture's height is no limit. It is 1 row per 334 frames: a 1-hour 60 fps clip is 647 rows; the cap is 16 384
  rows = 5.4 M frames.
- **The text is the limit.** The table goes in as hex, about 1.6 KB a frame: 1.6 MB for this clip, about 340 MB for an
  hour at 60 fps.
  - libplacebo parses that, and ffmpeg reads the file whole.
  - So a long export should build the table for its own range only (`Options::range`), as moments do.

**Frame / time.** The filter feeds no `PTS`. libplacebo defines `frame` in every pass, but it counts **every pass it
runs**, not frames (`custom_mpv.c`: `p->frame_count++` per pass). So the export shader replaces the `PTS` parameter
with:

```
#define PTS (float((frame - 1) / NPASS) / FPS)
```

(`gpu_table::for_libplacebo`)

- `NPASS` is the passes a frame (`gpu_table::passes_per_frame`):
  - grid mode is 2 (+1 for the LUT pass in the same file);
  - planes mode is 3 with P010 input, because `CHROMA` is hooked once per chroma plane: twice for planar 4:2:0 input.
- **Every pass must run every frame.** All the warp's passes have `WHEN gyroflow_gpu_on`, on by default, so the count
  is fixed.
- **Checked.** Frame 90 of the filter's output matches frame 90 of the exporter's best of all 120: MSE 6.5, against
  138 and 214 for 89 and 91.
- **Fragility.**
  - It breaks if libplacebo renders a hook more than once a frame. That doesn't happen in the filter: one
    `pl_render_image` a frame, no frame mixing.
  - It also breaks if a pass's `WHEN` turned off mid-clip.
- **The sturdier alternative** is a ~15-line ffmpeg patch: `vf_libplacebo` writes a parameter named `PTS` from each
  frame's time, as mpv does. That is easy in our own ffmpeg build, but then it isn't "any ffmpeg with libplacebo"
  anymore.

**Moment start (`TIME_OFFSET`).** A parameter whose default the generator writes. The counter starts at the first
frame the filter renders, so the export writes `time_offset = first_frame / fps` (the exporter's `-ss` seek, a quarter
frame early, as today). Not run here.

**Denoise.** The `dn_` passes keep a ring of earlier frames in storage textures, which persist in the filter's hook,
and take `PTS` for the ages. That works in principle with the counter. Not run: the denoise needs the frames after
the last one (`gyroflow_dn_after`), and the filter has no way to flush with a changed parameter at the end.

### (c) Does the frame stay on the GPU?

**Mac: no.**

- ffmpeg 9.0.2's Vulkan hwcontext maps only to and from DRM PRIME and VAAPI (`hwcontext_vulkan.c` `vulkan_map_to`
  and `vulkan_map_from`, under `CONFIG_LIBDRM`). There is no VideoToolbox, IOSurface or `VK_EXT_metal_objects` path.
- `hwcontext_videotoolbox` maps only to its own software format.
- MoltenVK has no Vulkan Video, so neither `-hwaccel vulkan` nor `hevc_vulkan` works.
- The best route is the one above:
  - VT decode to software P010;
  - the filter uploads it;
  - `format=p010le` downloads it again;
  - `hevc_videotoolbox` wraps it in a CVPixelBuffer.
- Measured: no shader, 13.8 s for 926 frames, about 15 ms a frame, against decode alone at 3.7 s.
- P010 output is 2-3 s faster than `yuv420p10le` (24.6 vs 27.4 s).

**Windows, on paper** (source refs from the n9.0.2 tree):

- **True zero copy:** `-hwaccel vulkan -hwaccel_output_format vulkan` → `libplacebo=format=p010` → `hevc_vulkan`.
  `hevc_vulkan` takes `AV_PIX_FMT_VULKAN` (`vulkan_encode_h265.c`), and frame pools get `ENCODE_SRC` usage by
  themselves (`hwcontext_vulkan.c`). It needs NVIDIA/AMD/Intel drivers with `VK_KHR_video_encode_h265`.
- **NVENC** takes only CUDA or D3D11 (`nvenc.c`). Vulkan → CUDA is a GPU-to-GPU transfer through external memory
  (`hwcontext_cuda.c` lists Vulkan as a transfer format): one copy in VRAM.
- **AMF, QSV and D3D12VA** don't connect to Vulkan: download and upload.
- **D3D11VA decode** can't be mapped into Vulkan.

**Linux, on paper:**

- **VAAPI → Vulkan** via DRM modifiers: `vulkan_map_from_vaapi`, which needs `VK_EXT_image_drm_format_modifier`.
- **Back again** via `vulkan_map_to_vaapi` → `hevc_vaapi`.
- The libplacebo filter also imports VAAPI and DRM PRIME frames itself. The documented example is in `doc/filters.texi`:
  `-init_hw_device vulkan -hwaccel vaapi -hwaccel_output_format vaapi … -vf libplacebo`. Users report that pipeline
  with `hevc_vulkan`.
- **NVIDIA:** the CUDA route, or Vulkan decode and encode.

None of the Windows or Linux command lines were run.

### (d) What survives this path

- **10-bit:** yes. P010 in and out, Main10 out.
- **HLG tagging:** yes. The output frames carry what the filter is told (`color_*`), plus `-color_*` on the encoder.
- **Audio copy and metadata:** yes. The same ffmpeg takes `-map 0:a? -c:a copy -map_metadata 0`, now in one process
  instead of two.
- **Dates:** the exporter sets them after the file is written; unchanged.
- **Dolby Vision RPUs** (`dv_mux.rs`): the exporter's own mux needs the encoder's raw HEVC on a pipe (`-f hevc -`). The
  one ffmpeg can write that just as today's encoder does. The RPU injector and the muxer stay as they are. Not run.
- **`--dv-live-l1`:** each frame's L1 is measured by `gpu_lut.rs` on the exporter's side. The filter can't hand that
  back, so this would need a measuring pass of its own. **Lost** on this path.
- **ProRes:** `prores_videotoolbox` takes P210/P010-class input. Not run.

## 3. Pictures compared

**Method.** The same frames from (ii) today's exporter (`--encoder ffv1`, lossless, added for this) and (iii) the
filter path (FFV1). Per plane, at 10 bits: PSNR, mean, p99.9 and max of |Δ|, using `tools/export-gpu/cmp_frames.py`.

| What | Y | U | V | Notes |
|---|---|---|---|---|
| Colour alone, HLG LUT (no warp, 30 frames) | 60.3 dB, mean 0.6, max 24 | 57.0 dB | 60.5 dB | the player's LUT is trilinear on a 65³ texture; the exporter's is tetrahedral |
| Colour alone, DJI Rec.709 LUT | 60.0 dB, mean 0.7, max 22 | 55.9 dB | 53.8 dB | 33³ |
| Warp + HLG, fringes off, planes (120 frames) | 39.2 dB, mean 7.0, p99.9 53-59, max 89-100 | 48.6 dB | 52.1 dB | the warp's sampling: the 8-px grid, the player's Lanczos, against Gyroflow's engine |
| Warp + HLG, fringes off, grid vs planes (both filter) | 65.8 dB, mean 0.24, max 9 | 56.0 dB | 59.2 dB | the export can use the faster grid mode |
| Warp + HLG, fringes on (grid, 120 frames) | 26-28 dB | 31 dB | 39 dB | a 0.33 % zoom: corners ±6 px across, ±3-4 px down, centre 0 (phase correlation at 5 spots) |

**Levels match:** frame 60's mean Y is 417.2 against 416.9.

**Where the warp difference comes from.** The warp shader's coordinates are unit-tested against Gyroflow's kernel for
the same parameters (`the_shader_maps_pixels_as_gyroflow_does`, within 0.17 px through the grid). The difference is
therefore sampling, not geometry.

**The fringe zoom** is between the player's in-warp chromatic correction (`CHROMATIC 1`) and the exporter's engine plus
chromatic.glsl. **The player and today's export already differ there.**

**(i) The player's own frame (an mpv screenshot) was not taken.**

- `--vo=null` has no gpu-next. mpv needs a window for the photo-link path's `screenshot-to-file … video` on gpu-next,
  and taking the screen was ruled out.
- The filter path runs the player's shader file through the player's libplacebo version. The only differences left
  from mpv are its input conversion and dither: `dithering=none` here, and mpv's default dither on a 10-bit target.
- **A windowed check is needed** to put a number on (i) against (iii).

## 4. Speed of the filter path end to end (D-Log M, HLG, HEVC Main10 via VT, audio copied)

| Shader | Output format | Time | fps |
|---|---|---|---|
| Grid, fringes on (+LUT) | p010le | **24.6 s** | 37.6 |
| Grid, fringes on (+LUT) | yuv420p10le | 27.4 s | 33.8 |
| Grid, fringes off (+LUT) | p010le | **24.8 s** | 37.3 |
| Planes, fringes off (+LUT) | p010le | 35.6 s | 26.0 |
| Planes, fringes off (+LUT) | yuv420p10le | 37.6 s | 24.6 |
| The same as null output, no encoder: | | | |
| No shader | p010le | 13.8 s | 67 |
| LUT only | p010le | 15.3 s | 61 |
| Planes + LUT | p010le | 33.7 s | 27 |
| Grid, fringes on + LUT | p010le | 23.4 s | 40 |

- **The encoder runs alongside.** Encoding adds 1-2 s over null.
- **Planes mode is slow under the filter.** It hooks `LUMA`/`CHROMA`, so libplacebo works plane by plane. The player
  picks planes on dry clips; the export should use grid.
- **SDR** (DJI's LUT, 33³) costs the same as HLG.
- **GoPro:** couldn't be timed on this path; both test projects need the VapourSynth fallback.

## What calling libplacebo directly from the exporter would add

- **Zero copy on the Mac**, the main thing:
  1. VT decode to IOSurface-backed CVPixelBuffers.
  2. Wrap each plane as an `MTLTexture`, then import it into Vulkan with `VK_EXT_metal_objects` (MoltenVK supports
     importing an `MTLTexture` into a `VkImage`).
  3. `pl_vulkan_wrap` it and `pl_render_image` into a VkImage exported the same way, as an IOSurface.
  4. Hand that CVPixelBuffer to VTCompressionSession.

  That would remove the 13.8 s floor. The ceiling would then be the encoder (≈ 50 fps, 18-19 s for this clip): about
  1.3× faster than the filter, and 1.6-2.9× faster than today.
- **Parameters set per frame:** `PTS`, the camera, `gyroflow_dn_after` for the denoise's last frames. No pass counter.
- **The L1 measure** (`--dv-live-l1`) as a pass whose result is read back.
- **One process instead of three,** and no second ffmpeg to ship: the exporter links libplacebo (LGPL) and MoltenVK,
  as mpv does.
- **The cost:** our own Swift/ObjC or Rust FFI for the IOSurface ↔ Vulkan import and VT encode, and a Windows/Linux
  story of its own: D3D11 or VAAPI interop.

## The zero-copy spike: libplacebo called directly (2026-11-16, evening)

`tools/export-gpu/afp_render_spike.m` is Objective-C, about 250 lines, linking the pinned static libplacebo 7.387 and
MoltenVK. It is the whole chain with the frame never leaving the GPU:

1. **Decode.** AVAssetReader (VideoToolbox) decodes into IOSurface-backed P010 pixel buffers.
2. **Import.** Each plane becomes an `MTLTexture` on the IOSurface (`newTextureWithDescriptor:iosurface:plane:`),
   then a `VkImage` through `VkImportMetalTextureInfoEXT` (MoltenVK's `VK_EXT_metal_objects`), then
   `pl_vulkan_wrap`.
3. **Render.** `pl_render_image` runs the player's shader file **with its real `PTS` parameter set per frame**, as
   mpv's `vo_gpu_next` does, so no pass counter.
4. **Output.** The result goes into the planes of a pixel buffer taken from the encoder's own pool
   (`AVAssetWriterInputPixelBufferAdaptor`), imported the same way.
5. **Encode.** AVAssetWriter encodes HEVC Main10 HLG. MOV tags: bt2020nc / arib-std-b67 / bt2020 / tv.

Imports are kept per IOSurface, since both pools recycle about 25-30 buffers. One frame is in flight: frame *n*
renders while *n − 1* finishes on the GPU (`pl_tex_poll`) and goes to the encoder.

**Build:**
```
clang -O2 -fobjc-arc -o afp_render_spike tools/export-gpu/afp_render_spike.m $(pkg-config --static --cflags --libs libplacebo) \
  -I/opt/homebrew/include -lc++ -framework AVFoundation -framework CoreMedia -framework CoreVideo -framework Metal \
  -framework IOSurface -framework VideoToolbox -framework Foundation
```
The pkg-config path is the build recipe's prefix.

**Results** (D-Log M 4K60, 926 frames, the same shader as above: grid + fringes + HLG LUT, at the source's bit rate,
M1 Pro):

| Run | Time | fps | Limited by |
|---|---|---|---|
| LUT shader only (≈ the encoder's floor) | **18.1 s** | 51 | the encoder: waits 18.4 ms a frame on it, 0.1 ms on the GPU |
| Player's warp + fringes + LUT, warm | **19.3-19.6 s** | 47-48 | the GPU: about 19.7 ms a frame; the encoder waits 0.05 ms |
| The same, on a quiet machine (load < 5), 3 runs in a row, first one included | **18.9-19.05 s** | 48.6-49 | the GPU: 17.2 ms a frame |
| The same, while another session's `rustc` loaded the machine (load 13-50) | 27.0-27.4 s | 34 | the load, not a cold start: on a quiet machine the first run is as fast |
| The same, without the overlap (`pl_gpu_finish` each frame) | 20.1 s | 46 | |

**Against the earlier paths** for the same clip and settings:

| Path | Time |
|---|---|
| Today's exporter | 53.8 s |
| ffmpeg libplacebo filter | 24.6 s |
| **Spike** | **18.9-19.1 s** |
| Real time | 15.5 s |

So with fringes it is **within 5 % of the encoder alone**, 2.8× faster than today. Without fringes (29.1 s today)
it should land at the same encoder-bound ~18-19 s; not run.

**Faster than real time: the encoder's real-time mode and the player's interpolation** (quiet machine, same clip):

- **The encoder's mode.** VideoToolbox's HEVC encoder runs at about 50 fps only in its default mode. Its real-time mode
  (`kVTCompressionPropertyKey_RealTime` + `PrioritizeEncodingSpeedOverQuality`, ffmpeg's `-realtime 1` /
  `-prio_speed 1`) runs the same 4K60 Main10 at about 82 fps. Decode → encode of this clip takes 11.3 s, against
  18.7 s.
- **What it costs.** At the same bit rate, PSNR against the source moves by 0.01-0.1 dB (Y 42.44 / 42.36 against
  42.45, first 300 frames), and the size is the same.
- **Not the decoder.** Decoding is a separate block on the chip: 4.8 s for the clip, about 190 fps.

| Spike, `--realtime 1` | Time | fps | Limited by |
|---|---|---|---|
| LUT only | 10.8 s | 86 | the encoder |
| Warp + fringes + LUT, Lanczos4 (the exporter's default interpolation) | 19.1 s | 48.5 | the GPU: 18 ms a frame (64 taps, the fringes read red and blue at their own coordinates) |
| Warp + fringes + LUT, **bicubic** (16 taps) | **11.1 s** | **83** | the encoder; the GPU waits 0.4 ms |
| Warp + fringes + LUT, bilinear | 11.0 s | 84 | the encoder |

**So the whole export runs 1.4× faster than real time:** decode, the player's warp with the fringes, the LUT and the
encode, with the frame on the GPU throughout, as long as the warp interpolates bicubic. The player's own default is
bicubic from 9 bilinear fetches. Lanczos4 at full 4K takes about 18 ms a frame on an M1 Pro. While playing, the player
renders at the window's size (`gyroflow_render_scale`), not 4K.

**Pictures.**

- **Against the filter path:** 120 frames, the spike encoded at 600 Mbps against the filter path's lossless FFV1.
  Y 55.3 dB (mean 1.0-1.5 codes, max 15-21), U 53.3 dB, V 62.2 dB. That is the HEVC encode's own loss: the same
  shader through the same libplacebo, frame for frame by the real `PTS`.
- **Against the player:** the closest thing to mpv's own render. Only the scalers and the dither differ: libplacebo's
  defaults here, with dither off.

**Open in the spike:**

- **Cold start.** On a quiet machine, a first run costs only 0.25 s more setup (0.51 s against 0.26 s); no slow first run.
  A `pl_cache` saved to disk, as mpv keeps its shader cache, would save that much. Not tried.
- **The GPU's ~19.7 ms a frame:** the shader's own ~10 ms plus libplacebo's conversion to and from YUV and the output's
  chroma downsampling. The LUT-only render shows that part is about 10 ms with `pl_gpu_finish`. The player pays the
  same per frame.
- **Not in it:**
  - audio and metadata (AVAssetWriter can pass them through);
  - SDR tags, ProRes, Dolby Vision (the HEVC goes to `dv_mux` as today);
  - moments (`time_offset` and a reader time range), progress and stop;
  - layout and ownership hand-over (`pl_vulkan_hold_ex`/`release_ex`). It relies on MoltenVK ignoring layouts.
- **Not verified:** the MTLTexture import with an AVAssetWriter running on a non-default GPU (Intel Macs with two GPUs).

**What it would take to become the export path:** a helper next to `afp-photo` (`macos/`), fed the shader file and
the export's settings by `gyroflow-export` (which already writes the shader). It would replace the decoder → engine →
encoder pipes when the GPU warp can render the clip, falling back as described above. The app already ships
libplacebo (linked into mpv) and MoltenVK, so **no second ffmpeg**.

## Shipping an ffmpeg with libplacebo (proposal; release workflow not changed)

- **macOS:** bundle the ffmpeg built as above next to mpv in `Helpers/`.
  - Static libplacebo, shaderc, SPIRV-Tools and glslang; dynamic `libvulkan` + `libMoltenVK`. mpv's bundle already
    ships MoltenVK and the loader, so they can be shared.
  - Size: ffmpeg 24.9 MB as built (mostly libavcodec), and not much more once shaderc is static. MoltenVK 5.3 MB is
    already in the app.
  - The rpath or an `icd.d` manifest for MoltenVK has to point inside the app; today it's `DYLD_FALLBACK_LIBRARY_PATH`.
  - Today's app runs the system/Homebrew ffmpeg or its own? Check `tools/bundle_runtime.py` before adding a second one.
- **Licences:**
  - ffmpeg with `--enable-gpl --enable-version3` is GPL-3, fine for this GPL-3 repo.
  - libplacebo is LGPL-2.1+; shaderc, SPIRV-Tools, MoltenVK and the Vulkan loader are Apache-2.0 (GPL-3-compatible);
    glslang is BSD/Apache; lcms2 is MIT.
  - Add their licence files to `ThirdParty/`, as `build_mpv.sh` does for mpv and libplacebo.
- **Windows and Linux:** the ffmpeg there would need the same build. Distro ffmpegs often have libplacebo
  (Arch, Fedora). Fall back to the pipe path when `ffmpeg -h filter=libplacebo` fails.

## The new path, if built (stage 2 sketch)

`gyroflow-export --gpu-filter`, made the default on macOS once proven. It would:

- write `gpu_table::for_libplacebo(shader_with(clip, Options::player(..) with Mode::Grid, table))` plus the look's
  `.glsl` into one file next to the output;
- run one ffmpeg with the graph above;
- keep the naming, progress (`-progress pipe:`), stop and background signals and the dates step.

**Falls back to the pipe path with a line saying why when there is:**

- no libplacebo in the ffmpeg;
- `gpu_table::unsupported`, e.g. lens correction below 100 % (both GoPro test clips);
- `--denoise`;
- `--dv-live-l1`;
- `--photo`, preview, 360/reframe, `--translation` unless its table is passed;
- crop mode's camera, unless written as defaults;
- `--youtube` (its PQ LUT and measure);
- non-macOS until tried.

**Share's recipe** (`renders.rs`) would add the path (`"gpu": "libplacebo"` or `"engine"`), since the two give
different pixels (Y 39 dB). `tests/render_settings.json` gets the key.

## What is in this branch

- `gpu_table::Options::player`, now used by `gyroflow-gpu-table --player` too.
- `gpu_table::passes_per_frame` and `gpu_table::for_libplacebo`: the `PTS` swap.
- In gyroflow-export, for experiments only:
  - `AFP_GPU_SHADER=<file>` writes the player's shader for the export and skips it (`AFP_CHROMA_PLANES`,
    `AFP_GPU_GRID`);
  - `--encoder ffv1` writes lossless MOVs;
  - `AFP_TIMES` prints the engine's and the LUT's time a frame.
- `tools/export-gpu/build_ffmpeg_libplacebo.sh` and `tools/export-gpu/cmp_frames.py`.
