# User discovery, refreshed: November 2026

Input for the communication strategy. Researched 2026-11-09, read only: nothing was posted, no accounts were made.
It refreshes the October 2026 user-discovery report (Mac only, Osmo Action 6, D-Log M, divers) against the product
as it is now (v0.10.0, the site at <https://actionfootageplayer.ch>).

**How to read it.** Every thread URL below was opened. Pages that were blocked and known only from search results are
marked *not opened*. "Last" is the date of the thread's latest post when it was read. Fit: **H** high, **M** medium,
**L** low, followed by the feature that answers the pain.

## Contents

1. [In short](#in-short)
2. [What changed since October](#what-changed-since-october)
3. [Who it is for now](#who-it-is-for-now)
4. [Audience segments, ranked](#audience-segments-ranked)
5. [Threads to answer, prioritized](#threads-to-answer-prioritized)
6. [Channels and their rules](#channels-and-their-rules)
7. [Search phrases and what ranks for them](#search-phrases-and-what-ranks-for-them)
8. [Check in the product before replying](#check-in-the-product-before-replying)
9. [Method and limits](#method-and-limits)

---

## In short

- **Nothing has been announced yet.** On GitHub: 0 stars, 0 forks, release assets at 0–2 downloads each, and 12
  unique visitors in 14 days. Anton's DJI forum post from 2026-10-22 has 49 views and 1 reply. Every channel below is
  a first contact, and several of them (Homebrew, Scoop, awesome-lists) have star or age thresholds the project
  doesn't meet yet.
- **The strongest pain cluster is still "log is the only way to HDR on the Action 6".** On forum.dji.com, threads
  asking DJI to bring back HLG ran from January to late September 2026. One user asks outright, "Do I need DaVinci
  Studio for that?" The player answers it exactly: D-Log M plays as HDR and exports as HLG, with no grading.
- **The pain has a second, broader form: a backlog of flat clips.** The top comment (45 likes) on a 120K-view Pocket 4
  video is about opening Resolve six months later to "300 flat clips". The same thread asks for a desktop Mimo. That
  is the active-traveler segment, and Moments, Live Photo and the Library answer it.
- **Divers are still the sharpest niche, and their forums tolerate a disclosed author.** ScubaBoard and WaterPixels
  have open threads with exactly this pain: Osmo Action 6 D-Log looking flat until graded, "the learning curve was
  too much", and shaky mask-mounted footage.
- **Every high-intent search shows a gap.** Searches for D-Log M → HDR, stabilizing an Osmo Action under water,
  showing depth on dive video, making a Live Photo from a video on a Mac, and opening .insv on a Mac all rank forums,
  LUT shops or SEO filler first. None of the results offers a player. Guide pages on the site, and a Habr article in
  Russian, can fill these gaps.
- **New since October:**
  - Competitor: GyroGrade, a Mac fork of Gyroflow with LUTs that renders rather than plays.
  - Pocket 4: its "no HLG" hook has weakened, because firmware added an HDR 10-bit mode, and it shoots D-Log, not
    D-Log M.
  - Insta360 X6: launched and not yet verified with the player.

## What changed since October

| | October 2026 | November 2026 |
|---|---|---|
| Platforms | Mac (Apple Silicon), with Homebrew's mpv | Mac with mpv and ffmpeg bundled (no Homebrew); **Windows 10/11 x64 and ARM64** (alpha installer, no admin rights); Linux plugins (untested) |
| Cameras | Osmo Action 6, D-Log M | Plus GoPro GP-Log (**HERO12 verified**), GP-Log2 (MISSION 1), Insta360 10-bit I-Log; **360°: Insta360 X5 raw clips stitched live**, plus stitched 360 MP4s from any maker's app |
| Jobs | Watch log in HDR, stabilize, depth overlay | Plus Moments and the Action Footage Library, clip previews, **Live Photo / Motion Photo** and HDR stills, crop for feeds, temporal denoise (clean still frame), **Dolby Vision exports**, lossless cut, NAS playback and **Jellyfin streaming** (repo only) |
| Site | none | <https://actionfootageplayer.ch> in EN/FR/RU/ZH; two named audiences: *Divers & snorkelers* and *Active travelers* |
| Audience | Divers with an Action 6 on a Mac | Divers, Action 6 log shooters, travelers with a backlog, Gyroflow/log graders, 360 shooters, and later self-hosters |

Changes in the market and the evidence:

- **The Pocket 4 / 4P got an "HDR 10-bit" mode by firmware (Jul–Aug 2026), per threads [351957] and [351582].** The
  "no HLG, so log is the only way to HDR" hook now holds only for the Osmo Action 6 (and the O4 Air Unit Pro). The
  Pocket 4 also records **D-Log**, and the 4P **D-Log 2**, not D-Log M (thread 354285 asks DJI to bring D-Log M
  back). The player takes D-Log by hand (`Ctrl+l`) and doesn't do D-Log 2. Check this before answering Pocket 4
  threads or contacting Pocket 4 creators.
- **A new competitor: [GyroGrade](https://github.com/rsmith4321/gyrograde)**, formerly "Gyroflow Plus". It is a Mac
  fork of Gyroflow with LUT/OpenColorIO, v1.0.1 from October 2026, "stabilizing and finishing log footage in one
  app". Its author promotes it in Gyroflow issues #250 and #1248. It renders and doesn't play, and it has no HDR.
- **Gyroflow isn't closing the gap.** On 2025-06-25 the maintainer wrote in #700 that HDR "is not coming anytime
  soon". Real-time playback (#635) is "planned" but has "not enough interest". Anton commented on #635 today
  (2026-11-09).
- **Insta360 X6** (August 2026, 10-bit I-Log, new sensor) and **GoPro MISSION 1** (GP-Log2) are what people buy now.
  Neither is verified in the player. Divers on WaterPixels say anything other than GP-Log2 is a "waste of time"
  under water.
- **GoPro's desktop tools are failing in public:**
  - GoPro Player 3.3.x won't start on Windows 11 26H2 (Oct–Nov 2026).
  - It crashes on 5K stabilization on macOS.
  - HLG clips look "hazy" in it.
  - There's no MAX2 desktop app ("no serious desktop solution").
  - A thread reports that Quik desktop's cloud login ended on 2026-08-21 (unverified).
- **The October advice still holds:**
  - forum.dji.com bans anything that "may compete with" DJI products. Free tools are tolerated in practice, so answer
    inside threads.
  - ScubaBoard accepts disclosed authors.
  - Reddit is still blocked for automated tools: in the Browser pane, by curl (403) and through the search tool.
- **Anton's own DJI thread [355258](https://forum.dji.com/thread-355258-1-1.html)** links to the old repo name
  `dlogm-gyroflow-player`, and posts there can only be edited for 2 minutes. A reply with the new name, the site and
  the Windows build would update it.

## Who it is for now

What the site and the README promise, job by job:

| Job | Feature | Where |
|---|---|---|
| See log footage in color without grading | D-Log M (detected on Action 6 / Osmo 360, by hand on Action 4/5 Pro, Pocket 3 and drones), D-Log by hand, GP-Log, GP-Log2, 10-bit I-Log by hand → HDR (HLG), the maker's Rec.709 LUT or NBCU SDR | Mac (verified), Windows (alpha), plugins anywhere |
| Watch shaky clips steady, with nothing to render | Gyroflow's engine live, from the camera's gyro (EIS / HyperSmooth / FlowState off); corrected for water; edges filled | Mac, Windows; Linux needs mpv with VapourSynth |
| See what the camera measured | Depth and in-water status (Action 6); GPS and speed (GoPro, Osmo 360); heart rate (Osmo 360) | any mpv |
| Find the good minute | Camera highlights (GoPro HiLight, DJI quick switch) on the seek bar; own marks | any mpv |
| Keep and organize a trip | Moments, clip flip-books, Action Footage Library, lossless cut; marks to FCP / Resolve / Premiere (coming soon) | Mac, Windows |
| Share the same evening | `Shift+E`: the part, stabilized and in color, as HDR HLG (+ Dolby Vision on Mac) or SDR | Mac, Windows (no DV) |
| Get a photo out of a video | Full-resolution still, temporal denoise, crop for feeds, HDR gain map; **Live Photo** (Mac), **Motion Photo** (Android) | Mac, Windows |
| Look around in 360 | Insta360 X5 raw clips stitched live with the horizon held; stitched 360 MP4s from Insta360 Studio, DJI Studio and GoPro Player | Mac, Windows |
| Watch the archive anywhere | NAS clips open in about a second; `afp-stream` feeds Jellyfin stabilized log → HDR | **repo only, not on the site** |

Cameras verified: DJI Osmo Action 6 and GoPro HERO12 Black. *Expected* (built for, not tried): Osmo 360, Action 4/5
Pro, Pocket 3, HERO13, MISSION 1, HERO5–11, Insta360 single-lens and X5. *Not yet*: GoPro MAX2 log, 8-bit I-Log,
drone telemetry, D-Log 2.

## Audience segments, ranked

Ranked by how sharp the pain is × how well the player answers it today × how reachable the people are. Sizes are
proxies: subreddit members (GummySearch, Oct–Nov 2026) and view counts.

| # | Segment | The pain, in their words | What answers it | Where they are | Proof we have |
|---|---|---|---|---|---|
| 1 | **Divers and snorkelers with an action cam** (Action 6, HERO12/13, MISSION 1) | "looks flat until color corrected"; "the learning curve was too much"; "pretty shaky unless you can keep your head very still"; "you don't know until you download the footage at the end of the dive" | Stabilization corrected for water (0.16 → 0.02 px), log in HDR on double-click, depth overlay, clean still frame, `Shift+E` the same evening | ScubaBoard, WaterPixels, r/scuba (199k), r/freediving (64k), underwater YouTubers (8–14k subs, 15–76K views per Action 6 video) | Daily use on dives; verified on the Action 6 and HERO12 |
| 2 | **Action 6 owners who want HDR without grading** (also Action 4/5 Pro, Osmo 360, O4 Air Unit Pro) | "HLG color mode is REALLY missing"; "native HDR matters"; "zero work in post"; "Do I need DaVinci Studio for that?" | D-Log M → HLG playback and export, with the maker's LUT as an option | forum.dji.com (largest cluster), r/djiosmo (17k), r/dji (242k), Action 6 reviewers (134k–654k subs) | Verified on the Action 6 |
| 3 | **Travelers with a backlog** ("hours of clips"), any of the three brands | "300 flat clips" six months later; "wish they'd do a Mimo desktop app"; batch export | Moments, highlights on the seek bar, Library batches, Live Photo / Motion Photo, one-key HDR export | YouTube comments on Mimo/Quik tutorials, r/gopro (196k), r/osmopocket (44k), r/macapps (261k), Product Hunt | Built and in use; Library merged 2026-11-09 |
| 4 | **Log graders and Gyroflow users** (FPV, enthusiasts) who want the step between card and editor | "Add Preview and Export LUTs" (Gyroflow #250); Gyroflow exports lose HLG (#700); Gyroflow "hard for casual users" | Stabilized and in color while playing, one key into Gyroflow, ProRes/HLG batch, marks to the NLE (coming) | Gyroflow GitHub + Discord, r/fpv (188k), r/colorists (70k), Joshua Bardwell / NURK FPV audiences, IINA/mpv issues | Uses Gyroflow's engine, which is both credibility and a dependency |
| 5 | **360 shooters on a computer** (Insta360 X5/X6, Osmo 360, MAX2) | "Can't read insv files"; Studio v6 confusion; "hate having to double edit"; full-res frame export capped at 4K | Raw X5 clips stitched live, horizon held, drag to look; reframe to flat is in progress | r/Insta360 (76k), 360 YouTubers (Ben Claremont 305k), WaterPixels/ScubaBoard 360 threads. **The Insta360 forum bans promoting competing tools** | X5 built for, not yet verified on real footage; X6 untested |
| 6 | **Windows users** of any of the above | Mimo, DJI Studio and GoPro Player are weak or absent on PC; "H.265 plays terrible" in VLC | The Windows app (alpha) | Same communities; winget | x64 tried on one PC, ARM only in CI |
| 7 | **Self-hosters with a NAS** | "NAS support is a must-have" (DC Rainmaker comments) | NAS playback, `afp-stream` → Jellyfin | r/selfhosted (868k), r/jellyfin (139k), awesome-jellyfin | Works on Anton's NAS; not released, not on the site |
| 8 | **mpv / Linux power users** | 360 in mpv (#8283, 👍13); LUTs while playing in IINA (#6508) | The plugins in their own mpv | mpv wiki, awesome-mpv, IINA/mpv issues | Linux untested |

**Compared with October.** Divers stay first because the proof is strongest and nobody else offers stabilization
corrected for water. Segment 2 is the same people as October's "D-Log M to HDR" hook, now with Windows, but the
Pocket 4 has dropped out of it. Segments 3, 5, 6 and 7 are new, opened by Moments / Live Photo, 360, the Windows app
and afp-stream. Segment 3 is the largest but also the most crowded (phone apps). Segments 5–7 need verification or a
release before they can be addressed honestly.

## Threads to answer, prioritized

The reply angle everywhere is to answer the question first, put the player in one line as a complement, and say you
made it. On forum.dji.com and GoPro's forum, avoid comparing against the maker's own app. Don't post on the
Insta360 forum.

### Priority 1: open, fresh, high fit (reply first)

| # | Thread | Start → last | Pain | Fit | Reply angle |
|---|---|---|---|---|---|
| 1 | [forum.dji.com 351957: Please reconsider removing HLG… Native HDR matters](https://forum.dji.com/thread-351957-1-1.html) (Action 6) | 2026-05-18 → 09-01 | The Action 6 has only Normal 10-bit and D-Log M; the Pocket 4 got HDR, the Action 6 didn't | H: D-Log M → HLG playback + export | Until DJI adds HDR, D-Log M can be shown and exported as HLG on a computer with no grading. Present it as one option next to the LUTs |
| 2 | [forum.dji.com 351582: Official reason why HLG (HDR) is removed?](https://forum.dji.com/thread-351582-1-1.html) (1,997 views) | 2026-05-03 → 09-23 | Liked HLG for "zero work in post" | H (Action 6), M (Pocket 4) | Answer the "zero work" need directly; note that the Pocket 4 now has HDR 10-bit |
| 3 | [forum.dji.com 349128: HLG color mode is REALLY missing](https://forum.dji.com/thread-349128-1-1.html) (780 views) | 2026-01-10 → 02-13 | "Do I need DaVinci Studio for that?" (HLG export from D-Log M) | H: HLG export without an editor | Answer that exact question; older, but it ranks |
| 4 | [ScubaBoard: Komodo with Osmo Action 6 and TG-7](https://scubaboard.com/community/threads/komodo-with-osmo-action-6-and-tg-7.669862/) | 2026-07-23 → 08-16 | Shot "10 bit dlog"; flat until graded; learned Resolve for one trip | H: Action 6 D-Log M in color, depth, stabilization | Congratulate him on the edit. For checking clips the same evening, there's a free player; you made it. Ask how grading D-Log M under water went |
| 5 | [WaterPixels: How do you edit your underwater shots? What drives you nuts?](https://waterpixels.net/forums/topic/3987-how-do-you-edit-your-underwater-shots-what-drives-you-nuts/) (55 replies) | 2026-10-08 → 10-22 | Resolve's learning curve, subscriptions, catalogs; the thread is about stills, and video is uncovered | M-H | Answer as a fellow diver-developer (the OP builds a Mac app for stills): the video side is flat log, EIS failing under water, and no quick review on the trip |
| 6 | [ScubaBoard: DJI Action 6 + dive kit (snorkelling), Sardinia](https://scubaboard.com/community/threads/dji-action-6-dive-kit-snorkelling-sardinia-budoni-malara-near-la-maddelena.671026/) | 2026-09-19 → 09-20 | Reviews clips on a phone; shot with EIS on and AWB | M | Share Action 6 snorkel settings from experience (D-Log M, why EIS fails in water); the player in one line |
| 7 | [ScubaBoard: GoPro, what no one tells you!](https://scubaboard.com/community/threads/gopro-what-no-one-tells-you.669752/) | 2026-07-17 → 09-22 | Noise at 15–20 m without lights; shaky mask-mount footage | M: clean still frame, gyro stabilization with HyperSmooth off | Settings advice first (HyperSmooth off, then stabilize from the gyro); mention the tool as how you do it |
| 8 | [ScubaBoard: Insta 360 Advice for Diving](https://scubaboard.com/community/threads/insta-360-advice-for-diving.670346/) | 2026-08-16 → 10-30 | X6 vs Ace Pro 2; "it's the editing that takes time" | M: raw 360 review on a laptop | Practical tip: look through raw 360 dives without exporting. Only after X5 is verified (X6 is untested) |
| 9 | [forum.dji.com 355232: How to enable GPS by default and export GPX telemetry?](https://forum.dji.com/thread-355232-1-1.html) (Action 6, track days) | 2026-10-20 → 10-21 | Wants GPS, GPX and timecode; DJI only logged it as feedback | M-H: GPS overlay from the file | GPS from the remote is in the MP4's metadata track and can be shown live; ask whether GPX export matters (a product signal) |
| 10 | [gyroflow #250: Add Preview and Export LUTs Function](https://github.com/gyroflow/gyroflow/issues/250) | 2022-02-27 → 2026-10-17 | Wants log through a LUT while stabilizing; now a GyroGrade thread | H | "To *watch* or cut log clips stabilized and in color, here's a player that does both live and opens Gyroflow with one key." A complement, not a fork |
| 11 | [gyroflow #700: Add support for HDR](https://github.com/gyroflow/gyroflow/issues/700) (👍2) | 2023-08-07 → 2026-08-31 | Gyroflow's export loses HLG; maintainer: not soon | H: stabilized HLG export (+ DV on Mac) | Short factual note for Action 5 Pro HLG / Action 6 D-Log M users |
| 12 | [iina #6508: Video color presets and LUT intensity controls](https://github.com/iina/iina/issues/6508) | 2026-10-16 | Wants .cube LUTs during playback on a Mac | M-H | "For camera log specifically, an mpv-based player picks the look from the clip's metadata" |
| 13 | [GoPro community: Why are my photos and videos with my Hero13 Black hazy?](https://community.gopro.com/s/question/0D5Uo00000y0RJ4KAM/why-are-my-photos-and-videos-with-my-hero13-black-are-hazy) (11 answers) | 2026-06-15 → ~06-20 | HLG looks hazy in GoPro Player 3.3.4 on a Mac, fine in QuickTime | M-H | Diagnose it (Player's HLG tone mapping; QuickTime is right). The player only as "another player that shows it right". GoPro's own forum, so keep it low-key |
| 14 | YouTube comments: [Scott Bauer Below, USE THESE SETTINGS UNDERWATER for the DJI Action 6](https://www.youtube.com/watch?v=acxTa9GboPQ) (56K views) | 2026-03-05 | "How can I use footage from these settings in the edit later?"; "do you recommend D-Log?"; "seems a bit shaky" | H | Reply to those comments with the D-Log M + EIS-off workflow; better still, contact the creator (see Creators) |
| 15 | YouTube comments: [Benett Graezer, FASTEST Way to Grade Osmo Pocket 4 D-Log in Mimo](https://www.youtube.com/watch?v=t_vY-mml6jk) (120K views) | 2026-05-18 | "300 flat clips"; "wish they'd do a Mimo desktop app"; batch export | M (Pocket 4 D-Log by hand only) | Answer the desktop-Mimo comment, after checking that Pocket 4 D-Log looks right with `Ctrl+l` |

### Priority 2: good fit, but older, slower, or after a product step

| # | Thread | Start → last | Pain | Fit | Note / angle |
|---|---|---|---|---|---|
| 16 | [forum.dji.com 344055: Using highlight function in post](https://forum.dji.com/thread-344055-1-1.html) | 2025-08-05 → 08-28 | Wants QS-button highlights in Resolve; DJI: Mimo only | H once marks-to-editor ships | Before that: "the marks are in the file, and a player can show them" |
| 17 | [forum.dji.com 343352: How to import depth data into editing software?](https://forum.dji.com/thread-343352-1-1.html) (Action 5 Pro) | 2025-07-20 → 07-28 | Depth only in Mimo; 280 videos | M-H | Check depth on the Action 5 Pro first |
| 18 | [forum.dji.com 349103: Custom LUT D-Log M → PQ 1000 nit and HLG](https://forum.dji.com/thread-349103-1-1.html) (Nathan Acre) | 2026-01-08 → 02-16 | Community HDR LUT; users want it outside Resolve | M | Credit his LUT; mention live playback. Don't take over his thread |
| 19 | [forum.dji.com 341930: Is there any LUT D-Log M to HDR?](https://forum.dji.com/thread-341930-1-1.html) (O4 Air Unit Pro) | 2025-06-15 → 06-17 | O4 Pro has only D-Log M | H (old) | D-Log M from FPV/drones plays and exports as HLG (by hand); link #18 |
| 20 | [forum.dji.com 354012: DJI Studio D-Log M recognition is bugged](https://forum.dji.com/thread-354012-1-1.html) (Osmo 360) | 2026-08-19 → 08-27 | Studio tags D-Log M at random (71 clips) | M | Explain where the flag sits in the file, and that a metadata-based check is reliable |
| 21 | [forum.dji.com 351402: Strange gray image in VLC](https://forum.dji.com/thread-351402-1-1.html) (Air 3S) | 2026-04-25 → 05-05 | H.265 plays badly in VLC on Windows | M: the Windows app | An mpv-based player as the VLC alternative |
| 22 | [WaterPixels: GoPro13 GoPro Labs firmware (testing underwater)](https://waterpixels.net/forums/topic/1848-gopro13-gopro-labs-firmware-testing-underwater/page/4/) (94 replies) | 2024-11-04 → 2026-09-10 | GP-Tune in free Resolve "not good enough for GP-LOG2" | M | Share how GP-Log2 → HDR/SDR is converted; mention that it runs live |
| 23 | [WaterPixels: GoPro Mission 1 Series](https://waterpixels.net/forums/topic/3490-gopro-mission-1-series/page/4/) (92 replies) | 2026-04-14 → **11-09** | GP-Log2 color-shift bug after firmware v4.01.70; 300 GB affected | M | Stick to the bug; later, offer to check whether the GP-Log2 transform shows the shift |
| 24 | [ScubaBoard: Insta360 x5?](https://scubaboard.com/community/threads/insta360-x5.671347/) | 2026-10-09, no replies | Hero 13 vs Action 6 vs X5 | M | Answer the buying question honestly; the player at most a footnote |
| 25 | [ScubaBoard: Dive Video Data Overlay](https://scubaboard.com/community/threads/dive-video-data-overlay.576971/) (27 replies) | 2019-04-05 → 2025-10-08 | Overlays still go through DashWare and CSV | M: live depth from the Action 6 | The Action 6 logs depth in the file; a live overlay exists |
| 26 | [thatcherfreeman/dwg-transforms #6](https://github.com/thatcherfreeman/dwg-transforms/issues/6) | 2026-02-15 → 09-09 | Action 4/5 Pro owner wants D-Log M "mainly for an HDR/HLG workflow" | M | Light touch: HLG without Resolve Studio's DCTL |
| 27 | [iina #5116: 360 degree video support](https://github.com/iina/iina/issues/5116) (👍5) | 2024-08-04 → 08-08 | No Mac player for Insta360 | M-H after X5 is verified | "For .insv on a Mac, a player that stitches live with horizon lock" |
| 28 | [mpv #8283: Panorama 360 video support](https://github.com/mpv-player/mpv/issues/8283) (👍13) | 2020-11-18 → 2025-07-03 | 360 in mpv | M | Point mpv users to the plugin set (view360) |
| 29 | [GoPro community: GoPro Player 3.3.1 crashing on macOS 15.7.1, stabilization of 5K fails](https://community.gopro.com/s/question/0D5Uo00000jQPicKAG/gopro-player-331-crashing-on-macos-1571-stabilization-of-5k-footage-always-fails) | 2025-11-29 → ~2026-01 | HyperSmooth Pro crashes on a Mac | M | Gyroflow as the general answer; the player as its live form; disclose |
| 30 | [YouTube: Danny Gan, How to color grade DJI D-Log M, Pocket 3 & Action 6](https://www.youtube.com/watch?v=rvnLI2YV9LI) (34K views) | 2026-06-01 | Which IDT to pick; "bluish tint after applying the Action 6 LUT"; wants HLG without LUTs | M-H | Answer the HLG-without-LUTs comment |

### Priority 3: wait for a product step, or research only

- **After testing the camera:**
  - GoPro community, [8K MISSION 1 PRO stabilizing](https://community.gopro.com/s/question/0D5Uo000011fTd8KAE/8k-gopro-mission-1-pro-why-not-support-gopro-player-for-stabiling)
    (2026-08) and [GoPro Player won't start on Windows 11 26H2](https://community.gopro.com/s/question/0D5Uo000015nKwgKAE/gopro-player-v3300-refuses-to-start-on-windows-11-version-26h2)
    (2026-10-11 → ~11-06, MAX2 export blocked). Both need tests on MISSION 1 8K and MAX2 raw files.
  - WaterPixels, [Insta360 X6, game over](https://waterpixels.net/forums/topic/3896-insta360-x6-game-over/)
    (2026-09-09 → 09-13). Needs the X6.
  - forum.dji.com [354792, Pocket 4P D-Log/D-Log 2 to HLG and PQ](https://forum.dji.com/thread-354792-1-1.html)
    (2026-09-26 → 10-26). Needs D-Log 2.
  - Gyroflow [#1164, Osmo 360 as two fisheyes](https://github.com/gyroflow/gyroflow/issues/1164). Needs raw Osmo 360
    files.
- **Research only (the forum's rules ban competing tools):** on the Insta360 forum,
  [full-resolution still from .insv](https://forums.insta360.com/section/17/post/100048/) (Studio caps it at 4K),
  [Can't read *.insv files](https://forums.insta360.com/section/17/post/100155/), and
  [underwater mode in the dive housing](https://forums.insta360.com/section/17/post/97966/) ("many divers will switch
  to DJI or GoPro").
- **Leave alone:**
  - Gyroflow [#635](https://github.com/gyroflow/gyroflow/issues/635): Anton answered today.
  - Gyroflow #1166: Anton's own.
  - Gyroflow [#1248](https://github.com/gyroflow/gyroflow/issues/1248): a fork discussion.
  - Gyroflow [#1078](https://github.com/gyroflow/gyroflow/issues/1078): closed. It still confirms that Gyroflow
    exports drop DJI telemetry.
  - IINA [#6264](https://github.com/iina/iina/issues/6264): its author has his own PR.
  - [faeton/insta360-quicklook #4](https://github.com/faeton/insta360-quicklook/issues/4): a possible collaborator
    for Quick Look, not a place to pitch.
  - dpreview [CapCut's stability better than Gyroflow](https://www.dpreview.com/forums/threads/capcuts-stability-feature-is-better-than-gyroflow-and-catalyst-for-nonpro-enthusiasts.4838509/)
    and forum.dji.com [Avata 2 clip ruined by Gyroflow stabilisation loss](https://forum.dji.com/thread-355314-1-1.html):
    low fit.

### Reddit: a manual pass

Reddit refused every automated route (Browser pane, curl 403, the search tool). Run these by hand while logged in,
newest first, and read each sidebar's rules before any reply:

- r/djiosmo (the Osmo Action home; r/osmoaction doesn't exist): `D-Log M`, `HLG`, `underwater`, `EIS off`, `Mimo Mac`
- r/dji: `D-Log M view`, `D-Log M HDR`, `Mimo desktop`
- r/osmopocket: `D-Log`, `HDR`, `flat`
- r/gopro: `GP-Log`, `GP-Log2`, `Player Mac`, `HyperSmooth off underwater`, `MAX2 desktop`
- r/Insta360: `insv Mac`, `Studio alternative`, `X5 Mac`, `I-Log`
- r/scuba, r/freediving: `action camera`, `Osmo Action 6`, `stabilization`, `editing`
- r/fpv: `gyroflow preview`, `D-Log M O4`
- r/macapps: `Live Photo from video`, `video player HDR`
- r/jellyfin, r/selfhosted: `GoPro`, `action camera`, `360` (for afp-stream later)

## Channels and their rules

### Communities

| Community | Size / activity | Self-promotion rule | How to show up |
|---|---|---|---|
| **forum.dji.com** | Largest DJI cluster; threads with 1–2K views | [Rules](https://forum.dji.com/thread-71515-1-1.html) §III: no advertising or spam; §VI: no advertising of anything "that may compete with" DJI products; posts editable for 2 minutes only. In practice, free tools stay up (Lapset, Nathan Acre's LUTs, Anton's own thread) | Answer inside threads; one link; "free and open source, I made it"; never compare with Mimo |
| **ScubaBoard** | Diver threads stay open for months | Commercial activity outside the marketplace forums is "expressly forbidden"; no recruiting by PM. Precedents: a disclosed "Unabara" post (May 2026) and an overlay tool's "looking for testers" post (Oct 2025) were left up | Disclosed in-thread answers; at most one "looking for testers" post, after asking staff |
| **WaterPixels** | Active; developers take part | "Do not use the forums as a platform for spamming or self-promotion"; promotional content needs prior authorization; an Industry forum exists | Message a moderator first, then an Industry post; in-thread answers as a diver |
| **Wetpixel, DeeperBlue** | Not reachable (connection refused; 403 and a guest limit) | Not read | Manual check |
| **GoPro community** | Staff answer most threads; older questions get closed | Guidelines not opened (search only); the Terms bar unsolicited advertising; developers mention their tools in practice | Short, diagnostic answers; the player as "another player" |
| **Insta360 forum** (forums.insta360.com) | One support reply per post | Rule IV.1: no advertising of "anything that may compete with any Insta360 products" | **Research only** |
| **dpreview** | XenForo beta | Code of conduct hidden from guests; no link shorteners; new accounts pre-moderated | Build history first |
| **MavicPilots** | Many D-Log M threads (titles only) | Blocked (Cloudflare); rules not read | Manual pass |
| **Reddit** | r/selfhosted 868k, r/videography 462k, r/opensource ~400k, r/macapps 261k, r/dji 242k, r/scuba 199k, r/gopro 196k, r/fpv 188k, r/jellyfin 139k, r/Insta360 76k, r/colorists 70k, r/freediving 64k, r/osmopocket 44k, r/djiosmo 17k | Not opened. From mirrors: r/macapps allows about one app post per developer per 30 days and gives developer flair at ~500 subreddit karma or with a GitHub history; r/selfhosted puts projects under 3 months old in a weekly megathread. October: r/scuba welcomes disclosed free OSS; r/freediving snaps at drive-by pitches | Build comment history; a before/after video post in r/scuba; the Live Photo angle in r/macapps |
| **Gyroflow Discord** ([invite](https://discord.gg/YaUtNpWTUh)) | The support channel for Gyroflow | Rules not opened | **Ask AdrianEddy first**: the player embeds their engine, and his blessing is worth more than any post |

### Launch venues

| Venue | Rules that matter | Fit and timing |
|---|---|---|
| **Show HN** ([guidelines](https://news.ycombinator.com/showhn.html)) | Something people can run; early work is fine; no landing pages or sign-up walls; never ask for upvotes | **Strong fit**: link the repo or the download; a first comment on the why (diver, flat log, EIS fails under water), the how (mpv, libplacebo shaders, Gyroflow core, Dolby Vision RPU written in Rust) and the limits (alpha, unsigned). Real-time Gyroflow on the GPU of a NAS is the HN hook |
| **Product Hunt** | Self-hunting is allowed; launches go live at 12:01 AM PT; "never ask for upvotes"; asking people to try and comment is OK; closed betas fit poorly | Secondary; a weekend launch on Live Photo and HDR from action cams, after HN |
| **AlternativeTo** | No links in descriptions; public betas OK; a queue of thousands (paid priority review takes 1–2 days); a developer can claim the page | List it as an alternative on [Gyroflow](https://alternativeto.net/software/gyroflow/) (only 2 alternatives listed: the best slot), [Insta360 Studio](https://alternativeto.net/software/insta360-studio/) (0 likes, irrelevant alternatives), [GoPro Quik](https://alternativeto.net/software/quik-by-gopro/), [ReelSteady](https://alternativeto.net/software/reelsteady/), [Telemetry Overlay](https://alternativeto.net/software/telemetry-overlay/), [IINA](https://alternativeto.net/software/iina/), [mpv](https://alternativeto.net/software/mpv/). DJI Mimo, GoPro Player and DJI Studio have no page |
| **mpv wiki, [User Scripts](https://github.com/mpv-player/mpv/wiki/User-Scripts)** | Anyone can edit | **Now**: the plugins under Lua scripts, the log → HDR shaders under User Shaders; tag the repo `mpv-script` |
| **[awesome-mpv](https://github.com/stax76/awesome-mpv)** | No rules; last merge 2026-02, PRs waiting since September | Media Player → macOS (only IINA is there); expect a long wait |
| **[awesome-video](https://github.com/krzemienski/awesome-video)** | Edit `contents.json`; 1–2 neutral sentences | `players-clients` or `vendors-hdr`; active |
| **[open-source-mac-os-apps](https://github.com/serhii-londar/open-source-mac-os-apps)** | Edit `applications.json`; 468 PRs open | Player or Video; slow |
| **[awesome-mac](https://github.com/jaywcjlove/awesome-mac)** | Alphabetical; keep the 4 language READMEs in sync | Audio and Video Tools; very active |
| **[awesome-macOS](https://github.com/iCHAIT/awesome-macOS)** | PR template is mandatory; no Electron; a votes queue | Video |
| **[awesome-jellyfin](https://github.com/awesome-jellyfin/awesome-jellyfin)** | Project ≥ 4 weeks old; neutral text | afp-stream under Content Import, once it's released |
| **awesome-selfhosted** | First release more than 4 months old | afp-stream, about 4 months after its first tagged release |
| **Gyroflow docs** ([docs.gyroflow.xyz](https://docs.gyroflow.xyz)) | No third-party page; nearest are "Video editor plugins" and "Gyroflow Core" | A docs PR or a request to the devs, after the Discord conversation |
| **MacUpdate** | Free "Add App" form, human review | Easy, low value |
| **MacRumors forums** | (Search snippets only.) Developers may self-promote in their own thread in *Mac Apps and Mac App Store* | One thread there |

### Package managers

- **Homebrew cask:** out of reach for now.
  - A self-submitted cask needs **≥ 225 stars, or 90 forks, or 90 watchers**.
  - Homebrew 5.0 deprecated casks without code signing; casks that fail Gatekeeper were due to be disabled in
    September 2026; `--no-quarantine` is deprecated.
  - A **personal tap** (`anton-belkin/homebrew-tap`) works today, but users still meet Gatekeeper.
  - **Notarization ($99/year) unlocks the official cask and removes the "Open Anyway" step**, the biggest friction
    for every Mac channel.
- **winget:** **the most realistic Windows channel now.** No popularity bar and no signing requirement. The installer
  must come from the publisher's URL (GitHub releases are fine), install silently, and pass Defender and AV scans.
- **Scoop Extras:** needs ≥ 100 stars or 50 forks; until then, a bucket of your own.
- **Chocolatey:** human moderation; checksums required.
- **Flathub:** builds from source only, which doesn't fit "plugins for your own mpv". Skip for now.

### YouTube creators to contact

All channel pages were opened. Show each the one feature that fits them.

| Segment | Creator (subscribers) | Why | Show |
|---|---|---|---|
| Underwater | [Scott Bauer Below](https://www.youtube.com/@scottbauerbelow) (9.86K) | The most-watched Action 6 underwater videos (56–76K views); his viewers ask about D-Log and shake | Stabilization corrected for water, depth, clean still |
| Underwater | [Dive Wise](https://www.youtube.com/@divewisely) (13.5K) | Action 6 underwater test (2026-10-18) | EIS off + live stabilization, depth |
| Underwater | [Sean Elliott, Underwater Video](https://www.youtube.com/@seanuwvideo) (7.5K) | Budget underwater setups | D-Log M under water in HDR, denoise |
| Action cam | [DC Rainmaker](https://www.youtube.com/@Dcrainmaker) (654K) | Data-minded; his Action 6 NAS video draws "NAS support is a must-have" | Depth/GPS overlay, NAS + Jellyfin |
| Action cam | [PhilSpenceMedia](https://www.youtube.com/@PhilSpenceMedia) (134K) | Action 6 review 2026-10-31 (138K views) | D-Log M auto-HDR + live stabilization |
| Action cam | [The Drone Creative](https://www.youtube.com/@TheDroneCreative) (426K) | Action 6 "12 weeks later" (394K views), Osmo 360 II | Action 6 / Osmo 360 log, Live Photo |
| Action cam | [Capture Guide](https://www.youtube.com/@captureguide) (12.5K) | "Normal vs D-Log M on Action 6" | Log without grading, side by side |
| Action cam | [Benett Graezer](https://www.youtube.com/@BenettGraezer) (367K) | Pocket 4 D-Log; viewers want a desktop Mimo | Only after Pocket 4 D-Log is checked |
| Gyroflow / FPV | [Joshua Bardwell](https://www.youtube.com/@JoshuaBardwell) (447K), [NURK FPV](https://www.youtube.com/@NURKFPV) (147K), [Oscar FPV](https://www.youtube.com/@OscarFPV) (20K; his blog oscarliang.com is bigger) | Gyroflow tutorials with 100–170K views | Gyroflow live while playing, one key into Gyroflow |
| 360 | [Ben Claremont](https://www.youtube.com/@BenClaremont) (305K), [The 360 Guy](https://www.youtube.com/@The360Guy) (68.3K), [Action Cam Guy](https://www.youtube.com/@actioncamguy) (9.79K) | Insta360 Studio tutorials; viewers lost after Studio v6 | Raw X5 live, horizon held; reframe when it ships |
| Color | [Kaur Hendrikson](https://www.youtube.com/@KaurH) (7.22K) | Builds D-Log M / GP-Log transforms with Thatcher Freeman | Ask him to **review the color math**: a credibility partner, not a promoter |
| Color | [Danny Gan](https://www.youtube.com/@DannyGan) (49.3K), [Simon Wyndham](https://www.youtube.com/@simonwyndham) (2.46K), [Yantastic](https://www.youtube.com/@PeterYan) (36.3K) | D-Log M and GP-Log2 grading; confused viewers | D-Log M → HLG with no IDT to choose; GP-Log2 in HDR |

No 2026 video on simply *viewing* log footage turned up; searches return only grading tutorials. A short "you don't
have to grade D-Log M" video of our own would fill that gap.

### Russian-speaking venues

- **Habr** ([rules](https://habr.com/ru/docs/help/rules/)):
  - Advertising is allowed only in company blogs, or **once** in the «Я пиарюсь» hub (32K+ reach in 30 days).
  - A technical article with a plain link counts as knowledge sharing. Good topics: how log becomes HDR, Gyroflow on
    the GPU, and stabilizing under water.
  - New authors go through the Sandbox and need an invite.
  - There is **no Russian Gyroflow guide at all**, so the gap is real.
- **vc.ru:** links to your own non-commercial projects are allowed.
- **Pikabu:** the [Моё] tag allows links to your own pages. Posts that only link out are banned. Post a self-contained
  story or video (a dive before and after).
- **4pda:** blocked, but "osmo action 6 4pda" is a live Google suggestion, so its action-cam threads exist. Manual
  pass.
- **Telegram:** no action-cam or diving channels found automatically (tgstat blocked). Manual pass.

### What the new features open up

| Feature | Opens | Not yet |
|---|---|---|
| **Windows app** | Everyone in segments 1–4 on a PC; winget; the "VLC plays H.265 badly" threads; GoPro Player's Windows 11 failures | A Windows-specific announcement until more testing; SmartScreen warning (unsigned) |
| **360** | r/Insta360, 360 YouTubers, Insta360 rows on AlternativeTo, mpv #8283 / IINA #5116, diver 360 threads | The Insta360 forum (rules); the X6 and MAX2 (untested); any claim before X5 is verified on real footage |
| **Live Photo / Motion Photo** | r/macapps, Product Hunt, the "make Live Photo from video on Mac" search gap (results say it can't be done) | Nothing blocks it; this is the most mainstream hook |
| **Moments + Library** | The backlog segment; "300 flat clips"; Mimo-desktop requests | Marks to the editor (coming soon) for the QS-highlights thread |
| **Dolby Vision exports** | Mac HDR people, r/colorists, Show HN technical depth | Avoid "Dolby" in titles (trademark notice) |
| **NAS + Jellyfin** | r/selfhosted, r/jellyfin, awesome-jellyfin, DC Rainmaker | Not on the site, not released; age thresholds |

## Search phrases and what ranks for them

Google autocomplete was read live on 2026-11-09 (en-US, ru-RU). The ranking check used a US-only search tool, so the
Russian results aren't Yandex. Result pages there were not opened.

### English

- **Log, no grading:**
  - d-log m lut
  - d-log m to rec.709 lut
  - d-log m davinci resolve
  - osmo action 6 lut download
  - osmo action 6 dlog m lut
  - osmo action 6 conversion lut
  - hlg vs d log m
  - hlg or d log m pocket 3
  - log footage vs rec 709
- **GoPro / Insta360 log:**
  - gp-log2 lut
  - gopro gp log lut
  - gp-log to rec.709
  - i log insta360 x5
  - i log insta360 lut
- **Stabilization:**
  - gyroflow osmo action 6 lens profile
  - gyroflow osmo action 5 pro
  - gyroflow alternative
- **Desktop apps:**
  - dji mimo for mac
  - dji mimo for windows
  - dji mimo pc
- **360:**
  - insv file viewer
  - open insv file mac
  - insv file player
  - 360 video player mac
- **Photos:**
  - video to live photo converter
- **Telemetry:**
  - telemetry overlay free
  - gopro telemetry overlay free
  - osmo action 6 depth display

Forum phrasing to mirror in page copy:
- "HLG color mode is missing"
- "native HDR"
- "looks washed out"
- "shoots flat"
- "looks flat until color corrected"
- "no motion data found"
- "import depth data into editing software"
- "highlight marker in DaVinci Resolve"
- "H.265 plays terrible in VLC"
- "Can't read insv files"
- "no serious desktop solution"

### Russian

People often type these in Latin script.

- **Log:**
  - d-log m to rec.709
  - lut osmo action 5
  - dlog m davinci resolve
  - osmo action 6 4pda
- **Gyroflow:**
  - gyroflow как пользоваться
  - gyroflow davinci
  - gyroflow macos
  - стабилизация видео бесплатно
- **Desktop apps:**
  - dji mimo для windows
  - dji mimo для mac
  - gopro quik для mac
- **360 and players:**
  - insv формат
  - hdr плеер для пк
- **Live Photo:**
  - сделать live photo из видео
  - конвертировать видео в живое фото
- **Diving:**
  - видео дайвинг красное море

### What ranks, and where the gaps are

| Query | Top results now | A player answer? |
|---|---|---|
| how to view D-Log M without grading | Forum threads (phantompilots, mavicpilots, forum.dji.com) | **No.** Everyone says "apply a LUT in an editor" |
| Osmo Action 6 D-Log M LUT | Retailers, LUT shops (Leeming, editorskeys), a blog | No; LUT sellers dominate |
| D-Log M to HDR HLG export Osmo Action | An SEO page, confused forum threads | **No, a big gap** |
| stabilize Osmo Action underwater gyroflow | Adobe community threads, aggregators | **No underwater guide at all** |
| gyroflow alternative real-time player | Gyroflow docs, AlternativeTo, oscarliang.com | No real-time player exists |
| DJI Mimo for Mac desktop | SEO pages, App Store Mimo, DJI downloads | No; "there is no desktop Mimo" |
| open insv on Mac without Insta360 Studio | SEO/vendor converters, the Insta360 manual | No; "rename to .mp4 / use VLC" |
| best 360 video player for Mac | Old directories (Kolor Eyes), vendor SEO | Nobody covers raw .insv |
| make Live Photo from video on Mac | watermarkly, Setapp, Movavi, Apple support | **Results say it's not possible on Mac** |
| GoPro telemetry overlay free | A 2016 article, gopro-overlay (PyPI), Telemetry Overlay | Render-only tools; nothing live |
| Osmo Action 6 depth overlay dive video | Retailers, a blog, DJI's own video | **Nothing shows depth on playback** |
| GP-Log LUT HERO13 view log | GoPro Labs, LUT shops | No |
| D-Log M как смотреть без цветокоррекции | A Rutube grading video, editorskeys, mavicpilots | No Russian answer |
| gyroflow стабилизация osmo action инструкция | English and German guides | **No Russian guide** |
| сделать live photo из видео на Mac | Russian SEO pages, Setapp | "Impossible on Mac" |

**The pattern:** every high-intent query ranks forums, LUT shops or SEO filler, and none shows a player. The
cheapest wins are pages on the site titled after the queries:
- "D-Log M to HDR without grading"
- "Stabilize Osmo Action footage under water"
- "Depth on your dive video"
- "Live Photo from a video on a Mac"
- "Open Insta360 .insv on a Mac"

Add one Habr article in Russian on Gyroflow and stabilizing under water.

## Check in the product before replying

The threads above sometimes need things that aren't verified yet. In order of how many replies each one unlocks:

1. **Pocket 4 D-Log by hand** looks right, and whether **D-Log 2** (Pocket 4P) is worth adding: Benett Graezer's
   audience, threads #2 and #15, and Priority 3's thread 354792.
2. **Insta360 X5 on real footage**, then the **X6**: all 360 threads and creators.
3. **GoPro MISSION 1** (GP-Log2, 8K stabilization) and **MAX2** raw files: the WaterPixels and GoPro threads.
4. **Depth on the Action 5 Pro** and GPS on the Action 6 with the remote: threads #9 and #17.
5. **Notarizing the Mac app**: Homebrew, and the "Open Anyway" step on every Mac channel.
6. **Full-resolution stills from 360**: the Insta360 frame-export pain.

## Method and limits

- Four research passes ran in parallel, plus the site, the README and the GitHub data:
  - DJI and drone forums
  - divers, GoPro and Insta360
  - GitHub and YouTube
  - channels and search phrases
- Forums were read by fetch or in the Browser pane, with non-essential cookies rejected. GitHub was read with `gh`.
  YouTube was read with `yt-dlp --skip-download` (metadata and top comments).
- Spot checks: the ScubaBoard Komodo thread, Gyroflow #700 (the maintainer's quote is dated 2025-06-25), and #635
  (Anton's comment today) all matched.
- **Not reachable:**
  - Reddit (every route)
  - MavicPilots (Cloudflare)
  - Wetpixel (connection refused)
  - DeeperBlue (403, then a guest limit)
  - MacRumors, Softpedia, 4pda and tgstat (403)
  - The GoPro community guidelines and the Gyroflow Discord rules (not opened)

  The forum rules for these come from search snippets or mirrors and are marked as such.
- MavicPilots titles from search results, for the manual pass (not opened):
  - [d-log to hdr rec 2020 workflow](https://mavicpilots.com/threads/d-log-to-hdr-rec-2020-workflow.142454/)
  - [technical transforms for easier viewing of flat or log DJI footage](https://mavicpilots.com/threads/technical-transforms-for-easier-colour-correction-or-viewing-of-flat-or-log-dji-footage.100735/)
  - [gyroflow tips](https://mavicpilots.com/threads/gyroflow-tips.132751/)
  - [Mini 5 Pro: which color profile](https://mavicpilots.com/threads/dji-mini-5-pro-which-color-profile-should-you-actually-use.155374/)
- The October report itself is gone from the scratchpad. "October" here means the summary in the task brief.
- The raw notes from the four passes (with more threads, comment dumps and rule excerpts) are in the shared notes
  folder of the session group, not in the repo.

[351957]: https://forum.dji.com/thread-351957-1-1.html
[351582]: https://forum.dji.com/thread-351582-1-1.html
