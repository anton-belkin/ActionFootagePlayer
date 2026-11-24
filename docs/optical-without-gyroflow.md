# Stabilization without a Gyroflow project: design

Goal (Anton, 2026-11-24): a clip plays stabilized in the player's default mode (levels, horizon, frame, projection all
the player's own) without a `.gyroflow` project beside it. That holds for clips without gyro, and for clips with gyro
that need a project today only because their lens isn't known. The project stays possible, but it is no longer a
precondition. Later, on his word only, a one-off script removes the projects that are no longer needed.

This is analysis and a proposal. Nothing here is built yet.

## 0. Decided (Anton, 2026-11-24, after the first version of this document)

1. **The point** is that a lens can be chosen, and optical motion recorded, **without a Gyroflow project**. That holds for
   every GoPro and every clip without gyro. Finding lenses is secondary, and only for the simplest case.
2. **Storage:** option (b). `<stem>.afp-motion` sits beside the clip's own `.afp.json`, wherever the main sidecar rule
   puts that. The player reads it **in portions** (streaming), so the clip doesn't wait for the whole file; measure what
   is fastest. Places with no `.afp.json` handling (read-only) are ignored for now.
3. **Horizon:** for clips on optical motion, the horizon lock is **off and not available**. There is no horizon in the
   data, so locking would give a random one.
4. **Lens layers:**
   - the file's own lens;
   - **our local lens database** (profiles we estimated or accepted, as files, searched like Gyroflow's even before
     Gyroflow has them);
   - Gyroflow's database.

   A lens is taken by itself when the camera's metadata names the mode unambiguously and a reliable profile with
   readout exists for that mode. Otherwise it is found or estimated from the picture.
5. **No archive automation in the product.** Big archive workflows are done by a separate Claude session using these
   tools. The product gets three plain commands on the selected clips, run by hand and iterated:
   - **Find lens:** tries the known profiles, local first, and says which clips one fits and which have none. The user
     assigns.
   - **Estimate lens:** one clip at a time. The new profile goes into the local database; Find is then run again on the
     rest.
   - **Stabilize optically:** the existing batch, writing `.afp-motion` instead of a project.
6. **Water** is a property of the source, not a sorting mark. It is set in the Library's existing **Batch set Source
   settings** panel ("Lens under water": Auto / Yes / No, for one clip or many, written to `source.underwater`), not as a
   flag beside pick and stars. Anton corrected his first idea the same day.
7. **Reuse Gyroflow's code, don't write it twice.** One path, the same code as Gyroflow's (Anton has improved it with
   Adrian's review, #1296). If that means a PR to Gyroflow to move code from the app to its core, so be it.
8. **Testing:** on a small sample of clips from different cameras, in and out of water, from the NAS. Writing there is
   allowed for the sample. No big runs: Anton runs the whole archive himself once it works.

## 1. How it works today

### Where the motion comes from

`mpv/scripts/gyroflow/main.lua` `find_source()` decides once per file, in this order:

1. **A `.gyroflow` project beside the clip** (`<stem>.gyroflow` or `<name>.gyroflow`, or the workspace plugin's paths):
   `kind = "project"`. Everything comes from the project: motion (gyro or optical), lens, readout time, sync, smoothing,
   horizon, zoom, output size and rotation, water. In the settings model (#136) the project **owns** level, horizon,
   frame and orientation (`transform.PROJECT_KEYS`). The panel shows them in "In the Gyroflow project", and only
   On / Wider / Off is the player's. The water and turn inputs in Source are greyed out ("As defined in the Gyroflow project").
2. **`gyroflow-probe <clip>`** (`vs-gyroflow/src/probe.rs`) says `mode: "default"` only when all of these hold:
   - the file has motion data;
   - the camera recorded that it did **not** stabilize the picture itself;
   - for non-DJI cameras, a lens is known (`has_lens_profile`).

   Then the clip plays with the player's levels (`kind = "default"`), with the lens picked by `lens_pick::apply`.
3. **No motion, but a lens:** `kind = "lens"`. The lens is corrected and nothing is smoothed (LensMode::Only).
4. Otherwise nothing: the clip plays as recorded.

Both render paths (VapourSynth and the GPU warp table) get their motion from one gyroflow-core `StabilizationManager`
built in `vs-gyroflow/src/lib.rs` `load_manager()`. A project goes through `import_gyroflow_data`. A bare clip goes
through `load_video_file` + `lens_pick::apply` + optional preset. Whatever motion ends up in that manager plays on both
paths.

### The lens today

`vs-gyroflow/src/lens_pick.rs` and `probe.rs`, in this order:

1. The clip's own choice: `source.lens_type` in `<clip>.afp.json`, set in the Library for selected clips.
2. The lens in the file: DJI with EIS off, GoPro HERO12+ POLY, Insta360. With `file_first`, the file's lens beats the
   type unless the type was chosen by hand.
3. A curated type for the camera's recorded lens mode: built-in `curated_lenses.json` plus the user's list in
   App Support. #231 saves a Gyroflow project's lens there as the camera + mode + size default.
4. Gyroflow's DB pick: same camera, same lens mode, same aspect, then ranked by identifier, EIS, resolution, fps,
   official, stars. Profiles mostly voted down are taken last.

What a user can choose today is limited to the **curated types of cameras that have them** (Action 6, HERO4 Black,
HERO3 White, drones). A HERO7 has no types, so the only way to give it another lens is a Gyroflow project. Nothing in
the player or the Library picks a profile from the whole DB, or an estimated one.

The bundled DB comes from gyroflow/lens_profiles *latest*, downloaded into gyroflow-core's cargo checkout once. Ours
(rev 990772f) was fetched on **2026-11-11, before v43** (2026-11-13). v43 holds Anton's 11 estimated-from-footage
profiles (lens_profiles#41, all with readout):
- HERO4 Black: 1080 Wide, 2.7K Wide, 2.7K Medium at 24/30/60, 4K Wide;
- HERO3 White 1080;
- HERO7 Black: 2.7K Linear and Wide HS EIS, 4K Wide HS EIS;
- Action 6 Standard FOV 4K.

The installed player therefore doesn't have them. The local database (§5) holds them in any case, and the next build
should fetch v43.

### "Stabilize optically" today

- Library → `gyroflow-optical` → `vs-gyroflow/src/optical.rs` → **the Gyroflow app's command line** (1.7.0+):
  `gyroflow <clip> --preset <p> --export-project 1`.
- The analysis is asked for in the preset's `synchronization` (`do_optical_correction`, `ignore_file_motion`,
  `optical_tracking` klt|dis).
- The result is a full `.gyroflow` beside the clip, which the player then plays as `kind = "project"`.
- Clips are left out when the lens isn't known, when they already have a project (unless Redo), or when they're 360 or
  have usable gyro.
- Measured speed: about real time at 4K30 (HERO12), about 1.4× clip length at 4K60 (HERO7); 45 s per 2000 frames of 4K60
  on the M1 Pro.
- `research_notes/lens_refine/archive/stabilize_all.py` did the same over the archive (2026-11-12/13):
  - **1911 clips got a `.gyroflow` on the NAS**, ~18 h of compute in 2 jobs;
  - each clip got confidence tags in its `.afp.json` ("Lens: fits / fits loosely / close / off / nearest known / from
    camera", "Lens: water / air", "dynamic crop", "medium unsure");
  - Anton marked 252 of the first 690 "wrong water".

### What a Gyroflow 1.7.0 project stores as "optical motion"

`gyro_source.optical_correction` (gyroflow-core `gyro_source/optical_correction.rs`, base91 CBOR):

- `video_base`: for a file without motion data, the rotation measured between frames, chained (one orientation per
  frame). This is the motion itself.
- `coeffs`: a correction spline δ(t), six control points per frame, rolling shutter resolution. It is composed onto the
  quaternions.
- `quats_checksum` and `context_checksum`: what it was measured **against**. The context covers frame timing, sync
  offsets, **rolling-shutter readout**, and **the lens rays, refraction included**.
- When the lens, readout or water change, `video_base` still plays (it carries no check). But it was turned into angles
  with the old lens, so the amplitude is wrong. The δ spline is silently dropped ("measured on other … not applying
  it"). This matches what was seen: ticking "underwater" in a finished project changes nothing until the analysis
  runs again.
- The tracks themselves (≈4 bytes per point per frame, ~200 MB per 10 min at 60 fps) are kept **in memory only**. A
  project never stores them.

**The analysis is in gyroflow-core, not the app:** `OpticalMotionAnalysis::from_manager(stab)`,
`feed_frame(ts, w, h, stride, gray8)`, `finish()`, `solve()`. Only frame decoding is in the app (`src/rendering`,
ffmpeg at ~960 px). vs-gyroflow already decodes frames itself (gyroflow-export, the #170 translation measure). So we can
run the same analysis in our own binary, **with no Gyroflow app installed**.

### What Gyroflow 1.7.0 does *not* do

- No lens from footage.
- No estimate of the rolling-shutter time: readout comes from the lens profile, or is 0. Most DB profiles have none;
  no profile of the modes Anton estimated had one.
- No horizon from the picture. Optical motion is relative: it starts at identity and carries no gravity.

### Our lens from the picture (`research_notes/lens_refine`, Rust port)

- **Works:** own bundle adjustment with rolling shutter (poses, points, lens, readout), started from a rough scene.
  - `fast.py --known`: candidate profiles tried first on ~100 frames, ~9 s when one fits. Otherwise an estimate on 300
    frames, ~40–70 s.
  - Right lenses move 2–6 px, wrong ones 41–87.
  - Catches a focal error of 2–3 % and a wrong readout.
- **Didn't work:**
  - pairwise epipolar (sees no readout, hides radial error);
  - COLMAP with the lens free (diverges or drifts, no rolling shutter);
  - single-clip free estimates on a weak scene (HERO3 White scattered 1159–1669);
  - tracking at 960 px (7 px error; it needs ≥ 1920 px);
  - constant-lens fits on dynamic-crop EIS (AutoBoost, RockSteady+);
  - rotation-only clips shot standing still (no model).
- **Reliability comes from agreement between clips of one mode**, not from one clip's own firmness. The per-mode archive
  pass gave 30 modes from 157 clips. Anton watched three clips per profile: "excellent".
- **Water:**
  - The medium must be given; it cannot be fitted (n and f trade off).
  - The check decides air/water on 91 % of clips with a good lens. That rises to 95–96 % when the vote is clear. With
    only a nearest-known lens it is a coin toss.
  - Three fragments add coverage, not accuracy.
  - Neighbouring clips help partly; a plain folder vote fails (GX011574 and GX011577 sit side by side, one in air, one
    in water).
- **Rust port:**
  - `src/core/lens_from_video/` on the Gyroflow fork, branch `claude/lens-from-video-644a92` (ff833143). Lands 0.4–1.8 px
    from the Python answer; readout 31.1 vs 30.6 ms.
  - nalgebra only, so it builds everywhere gyroflow-core builds.
  - **Not pushed.** Its GUI dialogs were never clicked through.

### Footage in the Library today

From the Library's probe cache, 5042 files; hours are clip durations.

| What the probe says | Clips | Hours | Plays stabilized today |
|---|---:|---:|---|
| Gyro, stabilizes in the player | 504 | 5.3 | yes |
| No motion data (HERO4, HERO3 White, Action 6 EIS/Normal, drones) | 1973 | 32.8 | only with a project |
| Gyro, but the camera stabilized (HERO7 HyperSmooth 834, HERO12 HyperSmooth/AutoBoost 444, Action 6 RockSteady) | 1278 | 14.7 | as recorded, or with an optical project |
| "Unsupported file format" (HERO3 without GPMF, Mavic Pro MOV) | 954 | 10.4 | only with a project |

So the player's default mode serves about a tenth of the footage today. The rest depends on projects: the ~1911 from
`stabilize_all` plus Anton's hand-made ones.

## 2. What the idea misses or contradicts

1. **Gyro clips without a lens are the same problem: the lens layer is shared.** (Anton's correction, 2026-11-24.)
   - Motion is useless without a lens, whether it comes from gyro or from the picture.
   - So the lens layer is step one for both kinds of clip, and it needs a **picker over all profiles**, not only the
     curated types. Today a HERO7 cannot be given a lens at all without a project.
   - By today's probe cache, few gyro clips are refused *only* for the lens. The curated list, DB v43 and #231's saves
     cover his cameras.
   - What remains:
     - a DB "nearest" pick that is simply wrong;
     - DJI, which skips the lens check entirely;
     - new cameras and modes;
     - in-camera-stabilized clips, where the gyro describes the picture *before* the camera's crop.

     Anton should name the clips he means (question 1).
2. **The motion is tied to the lens, readout and water it was measured with.** Anton's option (a), "take only the
   motion from the project and set everything else in the player", breaks this.
   - When the player's lens differs from the project's, `video_base` still plays, but with the old lens's angles. The
     rolling-shutter correction is dropped silently. The result looks almost right and isn't.
   - So the order is fixed: **lens + readout + water first, motion second.** Any later change of these three marks the
     motion stale and needs a new measurement (≈ real time), unless tracks are kept (§3.3).
3. **Water is a third input, as important as the lens.**
   - Anton marked 252 of 690 stabilized clips "wrong water", and a motion measured in the wrong medium is wrong.
   - The lens command must decide the medium too, or ask. It is not a player setting to flip afterwards.
4. **Horizon.**
   - Optical motion has no gravity: it starts at identity.
   - The player's default horizon lock (`horizon_lock=yes`) spins optical-only clips. That was the PR #88 issue with
     projects.
   - **Decided:** for optical clips the horizon lock is off and not offered (panel, key, levels' presets).
   - Gyro clips with HyperSmooth have accelerometer data and keep it.
5. **Rolling shutter is only half solved.**
   - The readout time is now estimated per mode, and the optical analysis uses it (band rows).
   - Jelly of moving objects and of translation stays. That is the separate per-pixel repair idea
     ([rolling-shutter-repair-by-flow]), not part of this.
6. **Some modes have no constant lens.**
   - Dynamic-crop EIS (HERO12 AutoBoost, Action 6 RockSteady+): "nearest static known lens" is the only answer today.
   - Optical motion on top still helps, as `stabilize_all` did, but the lens there stays "approximate" in any wording.
7. **"Unambiguous" needs a definition.** Proposal:
   - Unambiguous: the lens in the file; a hand choice; a curated or saved type for the recorded mode and size; a DB
     profile whose identifier matches exactly and isn't voted down; an estimated profile of that mode.
   - Not unambiguous:
     - "Nearest" DB picks (other resolution, frame rate or EIS);
     - cameras that record no mode (HERO3 White's SETT is unreliable; HERO4's mode codes are guessed);
     - dynamic crop.

   For ambiguous lenses the per-clip check (§5) decides, or the clip is marked "lens unsure".
8. **Gyroflow app dependency.**
   - Today the batch needs the Gyroflow app installed (not the App Store copy), and its preset quirks:
     `synchronization` replaces the command line's settings.
   - With the analysis run from gyroflow-core inside vs-gyroflow, the dependency goes away. Windows and Linux behave the
     same, and #208 (a key in the player) becomes a small task.

## 3. Where the optical motion lives

### (a) Keep the `.gyroflow`, take only its motion

Pros:
- no new format;
- the Gyroflow app opens and edits it (remove tracks on a fish, refine);
- the ~1911 existing projects keep working as they are;
- the CLI path stays.

Cons:
- One file type with two meanings: "only motion" or "Anton's hand work that owns its settings". Every reader (player,
  Library, NAS helper, moments' project copies, export) has to tell them apart by some marker, and opening it once in
  Gyroflow and saving turns one into the other.
- The project still holds a lens, readout, water, smoothing, zoom and output size that the player would have to ignore,
  while its motion stays tied to its lens (§2.2). A player-side lens change is silently degraded.
- Writing it still needs the Gyroflow app, or our own writer of Gyroflow's full project format.
- #136's rule "the project owns its keys" gets an exception that applies only to some projects.

### (b) Our own motion file beside the clip

Pros:
- Matches what the motion is: an **input** property of the source, like the gyro in the file. It is not a transform. It
  sits in Source next to lens and water; Transform (levels, horizon, frame, projection) stays wholly the player's.
- The file records what it was measured with (lens id and checksum, readout, water, analysis settings, clip identity),
  so the player can say "measured with another lens: measure again" instead of degrading silently.
- Written by our own command (gyroflow-core analysis), with no Gyroflow app.
- "The project is its files" fits: a file beside the clip found by name, like `.afp.json`; several copies of a clip
  follow the existing main-sidecar rule.
- Moments need no project copies: like the gyro in the file, the motion is the source's.

Cons:
- A new format and a converter to maintain.
- Gyroflow can't open it directly; Ctrl+e must write the motion into the project it makes.
- On read-only locations (SD card) there is no place beside the clip (question 7).

### Recommendation: (b), with Gyroflow's own data inside

- **File:** `<stem>.afp-motion`, a small binary next to the clip.
  - **Header (JSON):** clip identity (size, mvhd `created`, fingerprint, as links use); lens (DB key, file checksum, or
    the profile inline); readout; medium and refraction coefficient; analysis settings (tracking, strength); date and
    tool version; quality (`measured_frames / frames`, `rms_deg`, the lens check's verdict).
  - **Body:** `OpticalCorrection` exactly as gyroflow-core serializes it (base91 CBOR, as in projects).
- **Why that body:**
  - converting an existing `.gyroflow` takes seconds and needs no new analysis, when its lens and water are still the
    right ones;
  - writing a project from it for Gyroflow (Ctrl+e) is a copy.
- **Size:** about 60 B per frame, so ~2 MB for 10 min at 60 fps. That is too large and too binary for `.afp.json`, which
  is small, JSON and rewritten often.
- **Where:** beside the clip's main `.afp.json`, as chosen by the main sidecar rule (`library/src/tags.rs` `clip_own`,
  `workspace.lua` `own_path`): the first copy over the project's locations that has one. Same stem, other extension.
- **Read whole, measured (step 1, 2026-11-24):** Anton asked for reading in portions if it pays. It doesn't:
  - the file is ~60 bytes a frame, so even the longest of the 1911 batch projects is 2-4 MB;
  - read cold from the NAS, those took 11-38 ms;
  - by contrast, the gyro in an MP4 is scattered through the file, which is why it is streamed.

  The file is two lines of text: the header as JSON (`head -1` shows it), then the correction as Gyroflow's projects
  keep it (base91 of zlib'd CBOR). `vs-gyroflow/src/afp_motion.rs`.
- **Player:**
  - In `load_manager`, the "own file" branch: after `load_video_file` and the lens, if `<stem>.afp-motion` exists and its
    clip identity matches, call `stab.set_optical_correction(...)` (and set `ignore_file_motion` for in-camera-stabilized
    gyro). Then `kind = "default"` with levels, as for gyro clips.
  - If the header's lens, readout or water ≠ the current ones: still play (the base is close), and Source says "stale".
  - Priority: `.gyroflow` (hand work wins) > usable gyro in the file > `.afp-motion` > lens only > nothing.
- **Optional, later: keep the tracks** in the local cache (≈4 B per point per frame). A lens or water change could then
  re-solve in seconds without decoding again. That needs a small gyroflow-core addition, rebuilding the analysis'
  setup with new lens parameters over old tracks. Gyroflow has `remeasure_against` for a new gyro, but not for a new
  lens.

## 4. How it fits the existing model

- **Rule "the project is its files":**
  - `.afp-motion` beside the clip, found by name, carrying the clip's identity.
  - Nothing new in `Project.afproject`.
  - Copies of a clip: the main sidecar's rule.
  - On the NAS, saving into the recycle bin makes "Copy #N" (see nas-recycle notes): write atomically, as `.afp.json` is
    written.
- **`<clip>.afp.json`:**
  - `source.lens_type` (exists); new `source.lens` for a specific profile (DB key, file, or estimated) chosen by hand or
    by the lens command, with `source.lens_confidence`.
  - `source.underwater` yes/no/auto (exists: the player's Source tab and the Library's Batch set Source settings, §5.4).
  - The "Lens: …" tags from `stabilize_all` stay as tags for review. The motion itself is not in `.afp.json`.
- **Panel, Source:**
  - Lens block: Type / Profile (picker over curated + saved + DB + estimated, with stars or "estimated from footage"),
    confidence, "Find from the picture…".
  - Under water: Auto/Yes/No.
  - New **Motion** line: "Gyro in the file" / "Measured from the picture (date, lens)" / "None — Measure (≈ clip length)"
    / "Stale: measured with another lens or medium — Measure again".
  - Source's own Save writes lens and water. Motion is written by the command.
- **Panel, Transform:**
  - Unchanged for gyro clips.
  - For optical clips, the "In the Gyroflow project" block disappears: levels, frame and projection become the
    player's, and the horizon lock is off and greyed out ("No horizon in motion measured from the picture").
- **Output:** unchanged.
- **Library:**
  - Info: Lens (with confidence), Water, Motion.
  - Commands on the selected clips: **Find lens**, **Estimate lens**, and **Stabilize optically**, which now writes
    `.afp-motion` instead of a project (§5). "Redo" re-measures.
  - Filters "Motion: from the picture / stale / none".
  - #231's "Save as the default for <camera> · <mode>" also takes a lens found from the picture.
- **Translation (#170), denoise, export, GPU table:** they read the same manager, so nothing changes. The GPU table's
  disk-cache key must include the motion file's stamp, as it does the project's.


## 5. The lens: layers, commands, water

### 5.1 Our local lens database

- **What it is:** a folder of profile files in Gyroflow's own JSON format, `<App Support>/ActionFootagePlayer/lenses/`.
  The folder #231 already writes `lenses/<name>-<crc>.json` into.
- **How it is searched:** like Gyroflow's database, by camera, lens mode, size, fps and EIS. `lens_pick`'s ranking runs
  over local ∪ Gyroflow, and **local comes first**.
- **What every file records:** where it came from (estimated from footage on clip X; accepted by hand; saved from a
  Gyroflow project), the medium it was estimated in (always stored as the air lens), and its readout.
- **Its role:** a profile Gyroflow already has works the same either way. One it doesn't have yet (not submitted,
  under review, or not in this build's DB) still counts.
- **The curated list** (`curated_lenses.json`, built-in + user) stays what it is: the map from a camera's recorded mode
  codes to lens types, for cameras whose modes Gyroflow doesn't name (HERO4 SETT, DJI fov_type). A type can now name a
  local profile.
- **Seed:** only Anton's 11 profiles from lens_profiles#41, which this build's Gyroflow DB lacks. They are known to be
  good. The other archive profiles (drones, Action 6 1080 EIS) stay out; their quality wasn't checked.

### 5.2 When the lens is taken by itself

Anton's rule (2026-11-24): **a lens is reliable only when it matches the clip in every parameter, the rolling-shutter
readout included.** In every other case the player says "no ready lens", and Find lens / Estimate lens are the way.

Taken without any command, in this order:

1. **A hand choice:** `source.lens` (a profile, kept with its readout) or `source.lens_type`.
2. **The lens in the file**, where the camera wrote it (DJI with EIS off, GoPro HERO12+, Insta360). DJI writes the
   readout with it (Action 6: 9.74 ms). Whether every GoPro does (GPMF `SROT`) is checked in step 3. Where none is
   written, it counts as not reliable.
3. **A profile from the local database or Gyroflow's that matches exactly:**
   - same camera, same recorded lens mode, same in-camera stabilization, same resolution, same fps (readout goes with
     the sensor mode and the frame rate: HERO4 2.7K Medium reads out in 31, 24 and 12 ms at 24, 30 and 60 fps);
   - and it **has a readout**;
   - official, ours or community alike, provided nobody voted it down.
   - This only applies to a camera whose metadata names its lens mode unambiguously (§5.2.1).
4. Otherwise **no ready lens.** The clip plays as it does without a lens (lens-only or as recorded), and Source says so,
   with Find lens and Estimate lens.

A profile without a readout, or one that matches only nearly (other fps, resolution or EIS), is not taken by itself. It
is a **starting point**: Find lens tries it and measures the readout on the clip itself (#1296 does this already:
`readout_measured`, "Rolling shutter readout time measured"). That takes tens of seconds instead of a full estimate. When
it fits, Assign keeps the profile with the measured readout in the local database for that exact camera, mode, size and
fps, so the next clip of that mode takes it by itself.

How much this changes, by Gyroflow's DB v41 (the one in our build):
- 9810 profiles, 1094 with a readout, 76 official with a readout;
- HERO7 Black: 54 profiles, 5 with a readout;
- HERO4 Black: 26 and 0;
- Osmo Action: 26 and 0;
- no drone of Anton's has a profile with a readout.

So most of Anton's clips will start as "no ready lens". Find lens and Estimate lens fill the local database mode by
mode; the 11 profiles of #41 cover the commonest old modes from day one.

This also changes today's behaviour: gyro clips stabilized now with a "nearest" DB pick (HERO7) show "no ready lens"
until Find lens has run on one clip of their mode.

#### 5.2.1 Which cameras name their lens mode unambiguously (working hypothesis, from the code)

What the metadata gives, as gyroflow-core's `camera_identifier.rs` and our `probe.rs` / `markers.rs` read it:

| Camera | Lens mode read from | Also read | Hypothesis |
|---|---|---|---|
| GoPro HERO5–HERO12 (GPMF) | `VFOV` (Wide, Linear, SuperView, Narrow, Medium, Max…), `PRJT` (Max Wide) | `EISA`/`EISE` (HyperSmooth on/off, Boost, AutoBoost), `ZFOV` (only to tell Narrow from Linear), resolution, fps | **unambiguous**, except AutoBoost (the crop changes during the clip) and a digital zoom other than Linear's, which isn't read |
| GoPro HERO4 Black (no GPMF) | udta `SETT` byte 7 low nibble (`markers::hero4_lens_mode`): 0, 2, 4 | resolution, fps | **unambiguous for 0 (Wide) and 2 (Medium)**: the archive estimates confirm them. 4 (Narrow?) unconfirmed |
| GoPro HERO3 White (no GPMF, "unsupported file format") | its `SETT` codes proved untrustworthy | resolution, fps | **one lens per resolution** (the archive found its two codes to be one lens); no mode used |
| DJI Osmo Action 6 | StreamMeta `fov_type` (1 Wide, 2 Normal; 6, 0, 3, 7 not named) | `eis_status`, resolution, fps; lens + readout in the file for Wide with EIS off | **unambiguous with EIS off** for named fov types; with EIS on (RockSteady, RockSteady+, Horizon) not: the camera warped the picture |
| DJI drones (Mini 4 Pro, Mavic Air 2, Mavic Pro) | the model (from the file or a sibling photo); one fixed lens | resolution, fps | **unambiguous by resolution + fps**, except digital zoom, which isn't read |
| Insta360 | lens in the file | | the file's |

If a lens marked "fits" stabilizes badly in Anton's viewing, something the metadata doesn't tell is missing for that
camera. Its row gets fixed then.

### 5.3 The three commands (Library, on the selected clips; no automation beyond them)

**Find lens** (also for gyro clips; for them it's the only step):

- Per clip: candidates of the same camera, local first, then Gyroflow's, nearest first, up to 8.
- Checked on three fragments (start, middle, ~80 %), in air and in water. This is #1296's `find_known`.
- About 10–30 s a clip.
- A candidate without a readout, or with one for another fps, gets its readout measured on the clip with the lens held
  as it is.
- The result is a list: clip → the profile that fits (Good / Loose / none), the medium it saw, and its readout (the
  profile's own, or measured).
- **Assign** writes `source.lens` for the clips the user ticks. When the profile needed a measured readout, or was a
  near match, it is also kept in the local database with that readout for the clip's exact camera, mode, size and fps.
  Then other clips of that mode take it by itself (§5.2). Clips with none stay selected for the next step.
- The medium found never overwrites a water flag set by hand; it only shows where it disagrees.

**Estimate lens** (one clip):

- From a stretch of the clip: three automatic fragments, or the player's current position when started from there.
- #1296's `estimate` gives lens + readout in under a minute, checked on the fragments it wasn't made from, with the
  same Good / Loose / Poor card.
- **Keep** saves it into the local database (camera, mode, size; air lens) and assigns it to the clip.
- Then Find lens again on the rest: clips it fits drop out of the list.

**Stabilize optically** (the existing batch):

- Same selection, same Redo.
- It needs each clip's lens to be known (by itself or assigned), and its water flag.
- It writes `.afp-motion` beside the main `.afp.json` instead of a project. No Gyroflow app is needed.

Archive-wide workflows (grouping by mode, picking good clips, voting on water) are **not** product features. A separate
Claude session does them with these commands and `gyroflow-probe`.

### 5.4 Water and lens in the source settings panel

- **Water is already there.** The Library's **Batch set Source settings** panel (`library/ui/app.js` `sourceBody`) sets
  "Lens under water" (Auto / Yes / No) for the selected clips. It writes `source.underwater` into each clip's own file
  through `src/tags.rs` `set_source`, and the player's Source tab shows the same value. Find lens and Stabilize
  optically read it from there. Nothing new is needed for water.
- **The lens goes into the same panel.** Its "Lens type" field (curated types only today) grows into a lens field over
  the local database and Gyroflow's: "As found / taken by itself" or a chosen profile, written as `source.lens`.
  Find lens' Assign writes the same key.
- The old "Lens: water" / "Lens: air" custom tags from `stabilize_all` stay as they are. A one-off script can copy them
  into `source.underwater` on Anton's word.

### 5.5 Reusing Gyroflow's code

The math is already in **gyroflow-core**, which the player links:

- the optical analysis: `OpticalMotionAnalysis` in v1.7.0;
- the lens from the picture: `lens_from_video::{find_known, check_lens, estimate, profile}` in #1296, on the fork branch
  `lens-from-video`, with Adrian's review addressed.

Only **decoding the frames** sits in Gyroflow's app (`src/rendering`, ffmpeg). Core is ffmpeg-free on purpose: it builds
for Android and iOS, so decoding can't move there.

**Proposal: link core, decode ourselves.**
- The tracking, solving, candidate search, verdicts and file formats are Gyroflow's own code, and Anton's upstream fixes
  reach us with the next rev bump.
- Ours is a thin frame loop: gray frames at 960 px for motion and ≥ 1920 px for the lens, real PTS as the app uses.
  vs-gyroflow already decodes (gyroflow-export, the #170 measure).
- Our gyroflow-core goes to a fork branch `afp-v1.7.0` + #1296's three commits, until upstream merges and releases them.

The alternative is Gyroflow's command line, new flags upstream (`--find-lens`, `--estimate-lens`, motion export). It
needs the Gyroflow app installed, an upstream release, and the preset quirks. Results still arrive as a project to
convert. It is one more dependency for the same code, so not recommended.

## 6. Cost and time (M1 Pro, measured earlier)

| Step | Per clip |
|---|---|
| Find lens (≤ 8 candidates × 3 fragments × air/water) | 10–30 s |
| Estimate lens | under a minute |
| Stabilize optically (decode + KLT at 960 px) | through Gyroflow's app: ≈ clip length at 4K30, ≈ 1.4× at 4K60; measured by the player (step 2): 3–6 s for 9–18 s clips (4K30/60, HERO7, Action 6, HERO3/4), about a third of the clip |
| Converting an existing `.gyroflow` to `.afp-motion` | seconds |
| Opening a clip: reading `.afp-motion` (~2 MB per 10 min at 60 fps) | to be measured cold from the NAS (§3) |

The test sample (§8) is about a dozen clips, an hour or two of compute.

## 7. Existing `.gyroflow` projects

- Nothing is removed by the product. A `.gyroflow` beside a clip keeps winning over `.afp-motion` (hand work).
- A converter (`gyroflow-probe --motion-from-project` or similar) writes `.afp-motion` from a project's
  `optical_correction`, with the project's lens, readout and water in its header. Then the clip plays in the default
  mode once the project is gone.
- Which projects to convert, re-measure or delete is a one-off decision on Anton's word, later. The ~1911 from
  `stabilize_all` are recognised by their preset; hand-made ones by trims, keyframes, own smoothing or removed tracks.

## 8. Order of steps

Each step is its own PR, UI ones with before/after screenshots.

1. **`.afp-motion` + the player reads it:**
   - the format, a reader in portions (measured), and a `load_manager` branch;
   - the Source "Motion" line, the horizon lock unavailable for optical clips;
   - the converter from `.gyroflow`.
   - Test on the sample: the same clip from its project and from the converted file gives the same frames (SSIM).
2. **Stabilize optically writes `.afp-motion`:** gyroflow-core's analysis in vs-gyroflow, our decode loop, the Gyroflow
   app no longer needed.
   - **Done (2026-11-24):** `vs-gyroflow/src/measure.rs`. The Library batch and the player's Ctrl+m / "Measure motion from
     the picture" both run it.
   - Agreement with the motion Gyroflow's app measured on the same clips, same lens: the turn between frames differs
     by a median of 0.004–0.04° against turns of 0.3–0.5° a frame.
   - The exception is a clip shot from inside a car. There the still dashboard and the street outside disagree, and the
     two measurements part ways after a fast turn; neither is trustworthy there.
3. **The lens layer:**
   - the local database, the "taken by itself" rule (§5.2) and `source.lens`;
   - a lens picker over local + Gyroflow profiles in panel Source and Library Info;
   - Gyroflow DB refreshed to v43.
   - **Done (2026-11-24), PR on top of step 2:**
     - `vs-gyroflow/src/lens_db.rs` holds the player's own database: 11 built-in profiles in `resources/lenses` and the
       user's `lenses/`, each with an `"afp"` mode block.
     - The order is §5.2's.
     - DB picks need an exact identifier and a readout.
     - The lens picker in panel Source and Library Info assigns to the mode (`gyroflow-probe --assign-lens` /
       `--forget-lens`).
     - HERO4 SETT 0/2 are named.
     - DJI's EIS goes into the mode, from its metadata.
     - Library probe answers are asked again (GEOTAG 5).
4. **Find lens / Estimate lens:**
   - **Done (2026-11-24), PR on top of step 3:**
     - Our gyroflow-core is `afp-v1.7.0` + #1296's three commits (fork branch `afp-v1.7.0-lens-from-video`, 2c15a9cc), and
       the bundled lens database is now v43.
     - `vs-gyroflow/src/lens_find.rs` decodes the fragments and runs lens_from_video's candidates, find_known, estimate
       and check_lens.
     - What was found is kept in the cache: each lens a profile of the clip's frame, with the readout measured.
       `gyroflow-optical --find-lens`, `--estimate-lens` and `--keep-lens`, plus batch actions.
     - Library, Stabilize part: Find lens / Estimate lens, and the results with "Keep for this mode".
   - On GOPR1585 (HERO4 2.7K Medium 24p):
     - Find accepts our Medium profiles: readout 30.6 (the profile's own) and 30.2 measured.
     - Estimate gives f 1763 and readout 31.1 ms, in 20 s.
     - Find took 14 s a clip. #1296's core on our fork branch, Library commands, result list with Assign / Keep.

**Test sample** (NAS, writes allowed for these clips only), about a dozen clips:
- HERO4 Black Wide and Medium, in air and in water;
- HERO3 White;
- HERO7 with HyperSmooth, in air and in water;
- HERO12 with AutoBoost;
- Action 6 Normal and Action 6 EIS on;
- at least two clips Anton marked "wrong water", and two with a hand-made project, to check they still win.

The exact list goes into the first PR.

## 9. Questions for Anton

Answered 2026-11-24:
- "Reliable" means an exact match in every parameter, readout included (§5.2). Everything else is a starting point for
  Find lens, which measures the readout, or for Estimate lens.
- The local database is seeded with the 11 profiles of lens_profiles#41 only.
- Which cameras name their mode: the working hypothesis of §5.2.1, corrected when a "fitting" lens stabilizes badly.

- Assign keeps the lens, with its readout, as the default of that camera, mode, size and fps in the local database. An
  estimated lens becomes a known one, and the next clips of the mode take it by themselves.

- **Gyroflow reuse: link gyroflow-core and decode the frames ourselves** (Anton: the result goes straight into
  `.afp-motion`, with no project in between; the rest is cosmetic). The comparison it was chosen on:

   | | Link gyroflow-core, decode ourselves | Gyroflow's command line |
   |---|---|---|
   | Same math as Gyroflow | yes: the same code, compiled in | yes |
   | Same frames as Gyroflow | nearly: our decode loop (scaling, timestamps, 10-bit to gray) may differ by a little; checked on the sample against Gyroflow's own project | exactly |
   | Gyroflow app needed | no; also on Windows and Linux, and no App Store sandbox trouble | yes, the right version, on every computer |
   | Lens from the picture available | now, from our fork branch with #1296 | only after #1296 is merged **and** released, plus new CLI flags for find and estimate, reviewed upstream: Adrian's timetable |
   | Result | written straight into `.afp-motion` / the local database | a project to convert; errors read from its log; preset quirks |
   | Progress, cancel, low priority, a key in the player (#208) | ours to control | through a child process |
   | Upkeep | keeping our fork branch in step until upstream merges (a rebase now and then) | following CLI changes upstream |

   The risk of the first is a small difference in frames from Gyroflow's own decoding; the sample comparison shows it.
   The risk of the second is waiting for upstream before anything works. #1296 stays offered upstream either way, for
   Gyroflow's users.
