# HERO7 HyperSmooth clips stabilized on top: the camera's turn measured from the picture

**Status:** 2026-11-26. Step 1 built (the PR "Under water, HyperSmooth's turn inside the housing"). Steps 2–4 built as a
draft ("HERO7 Black clips stabilized on top of HyperSmooth"): the file, the loading and the player work end to end, but
the estimator isn't better than Stabilize optically yet (see "How it measured"), so step 5 (the run over Anton's clips)
waits. The research behind it is in
`research_notes/dynamic_lens/` (untracked in the shared checkout; README parts 1–5).

## Goal (Anton, 2026-11-25)

A HERO7 Black clip shot with HyperSmooth plays stabilized on top of HyperSmooth, the way a HERO12+ clip does (#295):
- the camera's own gyro is the body's turn;
- HyperSmooth's turn of the picture is measured from the video and kept beside the clip;
- the player hands both to gyroflow-core in the form a HERO12's telemetry has (CORI·IORI, and IORI);
- everything downstream (smoothing, levels, crop, horizon, export) stays what it is.

Gyroflow itself won't carry a HERO7-only module: Adrian, gyroflow/gyroflow#1301, "very niche". So this lives in the
player. Anton has about 840 HERO7 HyperSmooth clips (~10.6 h), most of them under water.

## What the research found

1. **HyperSmooth turns the picture inside the housing.** It renders each frame from the sensor turned by its own rotation,
   through its own model of the lens in air. Behind a flat port the ray of a picture pixel is refracted about the
   camera body's axis.
   - If the turn is applied on the water's side instead (what Gyroflow does today with CORI·IORI), the picture bends by
     `(κ(θ) − 1/n)` of HyperSmooth's tilt, with `κ(θ) = cos θ / √(n² − sin² θ)`.
   - The bend is small in the middle, largest at the edges, and changes from frame to frame. That is the edges'
     "breathing" Anton saw.
2. **HERO12 without GoPro Labs DIVE, flat port:**
   - Applying IORI inside the housing reduces the jitter of tracked points' world rays by 40–80 %.
   - A tilt estimated only from the picture's bending matches the file's IORI (correlation 0.99, scale ≈ 1).
   - **With Labs DIVE (`DIVE=1`)** the camera stabilizes for the water itself, and today's handling is right.
   - Opened upstream as gyroflow/gyroflow#1302, which also fixes the IORI axes: the core read IORI without the x
     negation telemetry-parser applies to the CORI·IORI product.
   - This also explains why #201's "IORI × 1/1.33" try (in_camera.rs) shook more: its clips were DIVE, and the axes
     were off.
3. **HERO7, which has no IORI.** The tilt per frame was estimated from tracks: a bundle adjustment with the lens and port
   fixed, a pose per frame and two tilt unknowns per frame.
   - Held-out edge error 4.1–4.8 → 2.2–2.5 px (4K Wide).
   - Stabilized edge jitter 15–20 % lower (4K Wide) and 8–18 % lower (2.7K Linear, rigid stretches). Anton saw both
     by eye.
   - On one clip a gyro model of HyperSmooth (path smoothed 0.5 s, 0.2 s late) matched the tracks with gain 1. On the
     other (Linear 60p) it didn't.
4. **Unreliable stretches** show up clearly: a fish or a person filling half the frame, the water surface, open water.

## Design

### 1. The core and the GPU path apply IORI inside the housing (independent of HERO7)

- **Our gyroflow fork:** gyroflow#1302's two commits on the pinned branch (`afp-v1.7.0-iori-housing`, on top of
  `afp-v1.7.0-lens-from-video`). This alone improves stock-firmware HERO12 clips under water (Maldives 2024).
- **The player's warp** (`gpu_table.rs`, `shaders/gyroflow/warp.glsl`) does its own refraction, so it has the same
  change: the table carries the frame's `kernel_params.image_orientation` (texel 16, `PARAM_TEXELS` 17), and
  `rotate_and_distort` turns the ray into the body's axes, refracts it, and turns it back.
- **Parity:** `the_shader_maps_pixels_as_gyroflow_does` (`AFP_GPU_CLIP`) compares the warp with gyroflow-core under water,
  with the turn and with Labs DIVE (none). Without the turn in the shader a HERO12 frame is 28 px off on average.
- **Every path:** the player and the export's GPU path render `warp.glsl`; the export's pipes path, photos and moments
  get it from the core.

### 2. The file beside the clip: its `.afp-motion`

HyperSmooth's turn is what Gyroflow's optical correction measures when the motion data is the body's: the small rotation
`δ(t)` that takes the gyro's orientation to the picture's, `q' = q · exp(δ)`. So it's kept as one, in the clip's own
`<stem>.afp-motion` (afp_motion.rs), not a format of its own:
- the gyro is kept (`ignore_file_motion: false`), integrated as it was measured against (VQF);
- `in_camera_turn` in the header says the correction is the camera's own stabilizer's turn, with how it was measured
  and how sure it is (`in_camera_turn::Turn`: the model, the turn's size, how far the picture took it from the model,
  each round's change, the share of frames measured);
- such a file is **version 2**: a player before it refuses it (it would play the turn without the refraction inside
  the housing and the crop share). Other files stay version 1.

Everything that follows a clip's `.afp-motion` follows this one: the Library's Stabilize optically and Measure again,
the render recipe's fingerprint (#318), the probe, the player's Ctrl+m.

### 3. Loading: a HERO7 clip becomes a HERO12-like clip

- **`load_manager`:** with such a file all of the gyro is read before the clip plays (the correction is of the whole
  clip's orientations). `afp_motion::apply` sets the integration, then the turn as the clip's image orientations
  (`in_camera_turn::set_image_orientations`, before the correction's context is taken), then the correction.
- **gyroflow-core** then composes the turn onto the gyro (the picture's orientation, as CORI·IORI) and refracts inside the
  housing with the same turn (`kernel_image_orientation`).
- **`in_camera::stabilized`** answers yes (`in_camera_turn::measured_here`): HyperSmooth's crop share applies.
- **The horizon** can be locked: the gyro has the gravity in it (unlike motion measured from the picture alone).
- **The probe** says `measured.in_camera_turn` (`GEOTAG` 11); the Library's Motion data line and the side panel say
  "on top of HyperSmooth: its turn from the picture".

### 4. The estimator (`in_camera_turn::fit`, run by measure.rs)

Stabilize optically (the Library's batch, gyroflow-optical, the player's Ctrl+m) measures a HERO7 Black with HyperSmooth
on this way (`in_camera_turn::stabilizer_of`); every other clip as before.
- **Tracks:** gyroflow-core's KLT tracking, as measure.rs drives it: 960 px luma, every frame.
- **A model first:** HyperSmooth as the body's path smoothed (σ 0.5 s) and 0.2 s late (`model`). The fit is done on top
  of it: the solver pulls δ to zero where the picture doesn't pin it down, and there the model stands in. A stretch with
  nothing to track then plays about as recorded, instead of getting HyperSmooth's correction back the other way.
- **The fit:** gyroflow-core's own (`optical_motion::solve_with`), all of it taken (strength 1), with two robust passes
  (a fish crossing the frame).
- **Under water:** measured with the turn inside the housing, which depends on what's measured: measured again with the
  turn found, three rounds.
- **The result** as an optical correction of the gyro's quaternions: control points on the solver's spacing, quantized as
  they're stored, its context taken with its own turn as the image orientations, exactly as it will play.
- **Validation with ground truth:** `the_turn_measured_is_the_one_a_hero12_recorded` (`AFP_TURN_CLIP`) measures a HERO12
  clip as if it had no IORI and compares the turn with the IORI it recorded.

### 5. The run over Anton's HERO7 clips, and the report

- Clips: HERO7 Black, HyperSmooth on, under water, from the Library's inventory.
- **First a sample** of about 50 across modes and trips (4K Wide 30/60p, 2.7K Linear, 2.7K Superview, 4:3), then the
  rest.
- The reads come from the NAS; the `.afp-motion` files are written beside the clips on Anton's word.
- **Report:** per clip, the measured share, the turn's size and how far the picture took it from the model; videos of the
  most telling clips for Anton to look at.

## How it measured (2026-11-26)

The measure gyroflow#1302 was judged by: KLT tracks' world rays (the clip's orientation times each point's ray, the
refraction and all), their second difference over 4 frames, median px at the centre / middle / edges
(`in_camera_turn.rs` `how_still_the_world_stays`, `AFP_TURN_STEADY`).

| clip | its own motion data as it plays | the turn measured here (10 rounds) | the picture alone (Stabilize optically) |
|---|---|---|---|
| HERO12 GX010010, stock firmware, flat port | 5.57 / 6.15 / 6.53 (CORI·IORI) | 1.99 / 2.45 / 3.06 (3 rounds) | 2.54 / 3.04 / 3.75 |
| HERO7 GX010564, 4K Wide | 12.55 / 12.86 / 13.11 (the body's gyro: HyperSmooth undone) | 1.59 / 1.94 / 3.11 | 1.46 / 2.03 / 2.93 |

The picture-based two are fitted to the same scene the measure looks at, which flatters them; between them, on the
HERO7, the turn measured here is no better.

What it can't see: **gyroflow-core's optical correction measures only the fast part of a turn.** Each track's residuals
are high-passed (a local quadratic over at most 61 frames, never longer than the track) to take out the parallax of
the camera's translation, so below about 1 Hz the picture says nothing, and the turn there is the model's. On the HERO12
the model is 3–4° from the turn the camera recorded between 0.2 and 1 Hz (`the_turn_measured_is_the_one_a_hero12_recorded`,
`AFP_TURN_CLIP`), whatever its σ and delay; the picture's residual can't choose between them. Under water a turn that's
off there bends the edges the wrong way, which is what the turn inside the housing was for. The picture's own rotation
chained from frame to frame (gyroflow-core's `video_base`, with translation) was further off still (5–8°).

**Next, if it's worth it:** the research's per-frame tilt from the picture's bending (`hs_ba.py`: the lens and port
fixed, a pose and two tilt unknowns per frame) measures the turn's absolute tilt at every frequency, under water, and
matched a HERO12's IORI with correlation 0.99. Ported to Rust, it would give the turn's slow part where the model gives
it now.

## Open questions

1. **Long stretches with nothing to track** play on the model (about as recorded). A fade to "as recorded" isn't built.
2. **Superview and 4:3 modes:** their lenses and HyperSmooth crop need checking. The sample covers them.
3. **HERO8–11** record IORI, but the player leaves them as recorded by Anton's call on #201. With #1302 in our fork they
   could be stabilized on top like the HERO12. That is a separate decision.
4. **HERO5 and HERO6 Black** (EIS, gyro, no IORI) could be measured the same way; not tried.
