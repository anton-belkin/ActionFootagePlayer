# Upscaling: an attempt set aside (November 2026)

**Archived:** the upscaling attempt (FSR to the screen #244, photo upscalers #246) was closed unmerged and set aside by Anton; nothing of it is in main. Written 2026-11-17.

Tried on 2026-11-16 and 17, built as two draft PRs, tried by Anton in a trial build, and set aside: he didn't like
what came out. Nothing of it is in main. This page is what was tried and where it lies, in case it's picked up
again.

## Where the work is

| | Branch | Last commit | PR (closed, not merged) |
|---|---|---|---|
| FSR 1 as the GPU path's upscale to the screen, Quality › Sharpen (FSR) | `claude/live-fsr-2c15eb` | 64d1a62 | #244 |
| A photo's Upscale choice (Auto, None, FSR, Real-ESRGAN, SwinIR-L), its preview in Save frame | `claude/photo-upscalers-ca9167` (on top of the first) | 24bd7ec | #246 |

Both were cut from main at 5847beb. Their own docs are on the branches: `gpu-render.md`'s "The upscale to the
screen: FSR 1" and `photo-upscaling.md`. The two Core ML models are assets of the pre-release
`upscaler-models` (Real-ESRGAN x4plus, SwinIR-L x4 GAN); the photo branch fetches them from there.

The probe before the PRs (its tools, the three models as .pth, sample frames and the comparison page) is not in git:
`research_notes/upscaling/` in Anton's checkout, with a README.

## What was tried

**The question:** a good and fast open upscaler, for playback and for rendering; two cases mattered: a photo cropped
with a strong zoom, and stabilized clips with a zoom looking soft.

**The probe**, on a dolphin still (clip 0102) and a coral clip (0021):

- macOS's own: VideoToolbox's super-resolution is x4 only, input up to 1920x1080 (1920x1920 for an image); its
  low-latency variant takes at most 1280 px a side in 8-bit 4:2:0, so not 4K 10-bit clips; MetalFX Spatial takes any
  size, 10-20 ms for a 4K output.
- The still was itself about 1250 source pixels stretched to 3072, so the limit was the source: MetalFX and
  VideoToolbox's x4 cleaned edges and added nothing.
- Open neural models on the photo (realesr-general-x4v3, Real-ESRGAN x4plus, SwinIR-L): much crisper, and drawn:
  portholes and a tree-like shape on a yacht, decks turned to rock.
- For the clip Anton liked "rendered at 0.8 and scaled back by MetalFX" over the full-size warp; FSR 1 on the same
  frame looked nearly the same, and was chosen as it's open and runs everywhere.

**Built:**

- `fsr.glsl`: AMD's EASU + RCAS on the chain's RGB, after the look, running when the render is smaller than the
  picture shown; `resolution=auto` rendering at 0.8 + FSR without the denoise; a step in quality.lua; Quality ›
  Sharpen (FSR).
- Photos: the stretch measured through the lens geometry, Auto (none to 1.1, FSR to 2.4, SwinIR-L past it), the
  neural models through Core ML in `afp-upscale` (no Python), run once on the SDR rendition with the HDR one made
  from it and a smoothly scaled gain; a preview rendered ahead on a paused frame, progress and Cancel in the panel.

## What was found on the way

- In mpv a pass's `MAIN.w` in `WHEN` is the video's own size, not the render's: FSR never ran until it compared
  against `HOOKED.w`.
- FSR is not "about 1 ms": offscreen on the M1 Pro it added 0.4-2.3 ms over mpv's scaler, and 0.8 + FSR cost more
  than the full size without the denoise (9.0 against 7.6 ms), less with it (17.1 against 20.8). Its cost in the
  player was never measured: mpv's timers gave nothing for the warp's and FSR's passes there.
- An unzoomed Action 6 frame above water is already stretched 1.9-2.2 times by the lens correction and the
  stabilization's crop (about 1.4 under water), hence the 2.4.
- Core ML on the GPU: Real-ESRGAN 14 s, SwinIR-L 51 s for 1280x720 source pixels, matching torch; the Neural
  Engine's compiler ran over ten minutes and was dropped. SwinIR-L running during playback dropped most frames.
- `main.lua` went over Lua's 200 locals once and the plugin didn't load; the branch has a test that compiles it.
- `gyroflow-export --photo` on a clip inside a project writes into the project's `_Stills` whatever `--output`
  says.

## Not verified when it stopped

The preview's place and scale over the frame, and its SDR picture inside an HDR look; FSR with the denoise on the
XDR; anything on Windows or Linux; the training data's terms for the SwinIR-L weights (some sets are research-only
or non-commercial).

## Why it was set aside

Anton, 2026-11-17, after trying the second trial build: he doesn't like what came out. What exactly he disliked
isn't recorded here.
