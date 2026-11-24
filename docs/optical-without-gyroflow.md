# Stabilization without a Gyroflow project: design

Goal (Anton, 2026-11-24): a clip plays stabilized in the player's default mode (levels, horizon, frame, projection all
the player's own) without a `.gyroflow` project beside it. That holds for clips without gyro, and for clips with gyro
that need a project today only because their lens isn't known. The project stays possible, but it is no longer a
precondition. Later, on his word only, a one-off script removes the projects that are no longer needed.

This is analysis and a proposal. Nothing here is built yet.

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

The bundled DB comes from gyroflow/lens_profiles *latest* at build time. v43 (2026-11-13) already holds Anton's
estimated-from-footage profiles (lens_profiles#41: HERO4 Black modes, HERO3 White, HERO7 EIS modes, Action 6).

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
   - In the default mode an optical clip needs horizon **off**, or "level as at the first frame", or a horizon from the
     picture (not available).
   - Gyro clips with HyperSmooth have accelerometer data and could level.
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
  - `source.water` with Auto/Yes/No (exists in the panel).
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
    player's, and horizon defaults to off (§2.4).
- **Output:** unchanged.
- **Library:**
  - Info: Lens (with confidence), Water, Motion.
  - The batch "Stabilize optically" becomes **"Prepare for stabilization"**: lens and water check first, then the motion
    measurement, writing `.afp-motion` instead of a project. "Redo" re-measures.
  - Filters "Motion: from the picture / stale / none".
  - #231's "Save as the default for <camera> · <mode>" also takes a lens found from the picture.
- **Translation (#170), denoise, export, GPU table:** they read the same manager, so nothing changes. The GPU table's
  disk-cache key must include the motion file's stamp, as it does the project's.

## 5. The lens and rolling shutter from the picture

What the earlier work showed, turned into a command (`gyroflow-probe --find-lens` or a new `afp-lens`; batch over
selected clips):

1. **Per mode, not per clip.**
   - Group clips by camera + recorded mode + size + EIS.
   - For each group, fit a few clips with enough motion (3–5, from different days) and keep the profile only when they
     agree (the "consensus" filter of the archive pass).
   - Save it to the user's list as that mode's lens, marked "estimated from footage", readout included.
   - Cost: ~1–2 min a clip at nice 15. The 30 modes of the archive are already done (`archive/profiles`), and most are in
     DB v43.
2. **Per clip, a check, not an estimate.**
   - Try the mode's profile and its neighbours on three fragments (start, middle, ~80 %), in air and in water.
   - This gives "lens fits / loosely / off" and the medium with a margin. ~10–30 s a clip.
   - The clip gets `source.lens` and `source.water`, or "unsure" when fragments disagree or the margin is small.
3. **A full per-clip estimate only for cameras that don't record a trustworthy mode** (HERO3 White), then clustered.
4. **Dynamic-crop modes:** no estimate. The nearest static lens, labelled "approximate".
5. **Gyro clips (idea, not tried):** with the gyro synced, the rotation is known, so a lens + readout fit needs no scene
   reconstruction at all. Gyroflow's rs_sync already fits pixel motion against gyro. This could be the cheap check for
   "gyro but lens unsure" clips.
6. **Engine:** the Rust `lens_from_video` port, put on our gyroflow-core rev as a fork branch on top of `afp-v1.7.0`
   (no Python in the product).
   - Tracking at ≥ 1920 px; the optical analysis' own 960 px is too coarse.
   - Offering it upstream to Adrian is Anton's call.

## 6. Cost and time

M1 Pro, measured earlier:

| Step | Per clip | Archive (no-gyro + in-camera-stabilized, minus drones) |
|---|---|---|
| Lens per mode (consensus) | 1–2 min × 3–5 clips per mode | done for 30 modes; new modes only |
| Lens + water check (3 fragments) | 10–30 s | ~3000 clips → ~10–25 h at nice 15, 3 jobs |
| Motion measurement (decode + KLT at 960 px) | ≈ clip length (4K30), ≈ 1.4× (4K60) | ~45–55 h of footage → ~50–70 h compute, 2 jobs → ~1.5–2 days wall |
| Converting an existing `.gyroflow` | seconds | 1911 projects → minutes |

- Reads come from the NAS: run during idle time and check that the share is mounted.
- Writing `.afp-motion` on the NAS needs Anton's word.
- Converting instead of re-measuring saves most of the motion step, but only for clips whose lens and water were right.
  Clips marked "wrong water", "Lens: off" or "nearest known" must be re-measured.

## 7. Existing `.gyroflow` projects

Three kinds, to tell apart by a read-only scan first:

1. **Made by `stabilize_all` or the Library batch (2026-11-10 and later):** recognised by the batch preset's settings,
   and the "Lens: …" tags in `.afp.json`.
   - Convert to `.afp-motion` when lens and water are still right.
   - Re-measure the rest, after the lens and water check.
   - Then the project is redundant.
2. **Gyro clips with a project made only for the lens:** the lens is now in the user's list (#231). Check that the list
   gives the same lens; then the project is redundant.
3. **Hand-made** (older than 2026-10-29, or edited after the batch: trims, keyframes, own smoothing, removed tracks):
   keep. They keep winning over everything.

The one-off script lists all three with what it would do. It runs only on Anton's word. Deleted projects go to the NAS
recycle bin anyway. Never open a project from `#recycle`.

## 8. Order of steps

1. **Motion file + player reads it + converter.**
   - Format, `load_manager` branch, Source "Motion" line, horizon default for optical clips.
   - Check: the same clip played from its `.gyroflow` and from the converted `.afp-motion` gives the same frames (SSIM, as
     done for gyroflow-core vs Gyroflow).
   - Biggest effect soonest: the 1911 batch clips play in the default mode.
2. **The lens layer:**
   - the "unambiguous" rule and its confidence;
   - a picker over all profiles (panel Source + Library), including estimated ones;
   - `source.lens`;
   - gyro clips stop needing a project for the lens.
3. **Own measuring command:**
   - gyroflow-core's `OpticalMotionAnalysis` inside vs-gyroflow, decoding with our ffmpeg, writing `.afp-motion`;
   - the Library batch switches to it;
   - a player key "Measure motion" (#208);
   - the Gyroflow app is no longer needed.
4. **Lens + readout + water from the picture:** the `lens_from_video` port on our gyroflow-core branch; per-mode
   consensus, then the per-clip 3-fragment check.
5. **Batch over the archive** (NAS writes on Anton's word): lens check, then convert or re-measure.
6. **One-off cleanup of redundant projects** (Anton's word).

Later, apart:
- keeping tracks for instant re-solve;
- horizon from the picture;
- the gyro-based lens check;
- rolling-shutter jelly repair.

## 9. Questions for Anton

1. **Gyro but no lens:**
   - Which clips did you mean?
   - By the probe cache almost no gyro clip is refused only for the lens now. The candidates are in-camera-stabilized
     HERO7/HERO12/Action 6, wrong "nearest" DB picks, and new modes.
   - A folder or two to look at would settle it.
2. **Storage:** agree with (b), our own `<stem>.afp-motion` beside the clip with Gyroflow's data inside, and `.gyroflow`
   (hand work) still winning over it?
3. **Horizon for optical clips in the default mode:** off, or "level as at the first frame" (needs a first frame with a
   level horizon), or something else?
4. **"Unambiguous" lens:** auto-apply only exact, curated, saved and estimated profiles, and treat DB "nearest" picks as
   "unsure"? For an unsure lens:
   - play with the best guess, marked; or
   - play lens-only until checked?
5. **Water:** let the lens check set it per clip when it is sure (~95 %) and mark the rest "unsure" for you, or always
   decide by hand or by dive?
6. **Read-only places** (SD card, someone else's share): keep the motion in the local cache there, or not measure at all
   until the clip is taken in?
7. **Gyroflow app:** once our own command measures, drop the CLI path, or keep it as a fallback?
8. **Archive run:** which folders first? And may it write `.afp-motion` files on the NAS (photo, DJI_Inbox) when it
   runs? Converting the 1911 batch projects is minutes; re-measuring the doubtful ones is days.
