# The Library against other media managers, once the concept is built

**What this is:** a companion to `docs/albums-photos-share-concept.md`. It asks how the Library would compare with
the products in the comparative study of 2026-11-11 once that concept is built.

**What counts as built:**
- tags and culling (PR #197, with Anton's decisions);
- Albums (#215);
- photos as objects (#216);
- Share (#214);
- renders as decided, including pinned ones;
- telemetry search (#213);
- the card check (#211).

**What doesn't:** photos as originals (#217, an intention), hand-off to the editors (#57, deferred), and the study's
other proposals.

**Sources:** the competitors' facts are the study's, checked then; nothing new was researched. The study is a local
report, `reports/library-comparison-2026-11-11.md`, not in the repo.

## In one paragraph

The study found three gaps: getting footage in, getting moments out, and culling and finding.
- **Getting moments out:** this one closes, as one Share for everything.
- **Culling and finding:** closes, and goes beyond the others with telemetry.
- **Getting footage in:** stays open on purpose. The Library doesn't import, and the card check takes its place.

The Library would then be the only product that is **browse, cull, organize and share in one place, on a NAS, with the
picture right**. The consumer libraries have the organizing but not the picture; the makers' apps have the picture for
one brand on a phone; the NLEs have both but need a project and an edit. What would still be missing is mostly what
needs heavy machinery: a phone app, visual search, faces, editor hand-off.

## What changes, row by row

✓ yes · ◐ partial · ✗ no · ✓✓ best in class. "Before" is v0.11, as in the study.

| Feature | Before | After | Who else has it, and how |
|---|---|---|---|
| Pick / reject, stars | ✗ | ✓ Accept/Reject + stars, in the player while watching the real picture | Lightroom ✓✓, Resolve 21, FCP, Photos 27, Immich |
| Keywords / tags | ◐ | ✓ own tags, synced two-way with Finder tags; originals never written | Lightroom, FCP ✓✓ (range keywords), Immich; Photos' are in its database |
| Smart / saved albums | ◐ facets | ✓ smart albums as `.afpsearch` files, by kind, culling, telemetry | Photos, Lightroom, FCP, Resolve Smart Bins |
| Your albums | ◐ clip previews in `_Clips` | ✓ folders of links, mixing clips, moments and photos | everyone; ours are files you can see in Finder |
| Timeline | ✗ | ✓ years → months → trips named from the place | Photos (Library, Trips), Immich, Google |
| Map | ◐ mini map in Info | ◐→✓ a map of everything with a place; fills in as places come from GPS (#213) and Photos (#188) | Photos, Lightroom Map, Immich, Google ✓ |
| Search by telemetry | ✗ (shown only) | ✓ depth, height, speed, in water, GPS; ranges and smart albums | **nobody** (Quik and Mimo draw stickers only) |
| Share | ◐ export, then AirDrop by hand | ✓ one Share: what, then where; system services + Add to Photos + a folder | Photos, Google, Quik, Mimo, Insta360: one tap |
| Sharing format by destination | ✗ | ✓ HDR for Apple, SDR / `Light` for messengers, Live vs Motion Photo | Photos and Google convert silently; nobody lets a desktop library choose Live Photo for iPhones and Motion Photo for Android |
| Photos from video | ✓ rendered at once (stalls playback) | ✓✓ frame + crop, marked while watching; rendered when shared, in the format needed | Quik frame grab, Photos 27 (iOS), Insta360; none defer the render or keep the crop editable |
| Export | ✓ batch into `_Exports` | ✓ Share → Save to a folder, from cached renders | Lightroom presets, the NLEs' deliver |
| Heavy clips on weak devices | ✗ | ✓ *Keep rendered*: rendered once on the Mac, played as is by the phone and TV | Lightroom Smart Previews (photos only); Immich and Plex transcode on the server |
| Render cache | ◐ moments only, by path | ✓ clips, moments, photos; by recipe hash; local by default, pinned in the library | NLEs' render caches, per project and per app |
| Card ingest | ✗ | ✗ by design; ◐ *Is this card safe to format?* | Resolve Clone ✓✓, OffShoot, Photos, Quik, Mimo |
| Duplicates | ◐ | ◐ the same, plus the card check | Photos ✓ |
| Apple Photos | ✗ | ✓ albums synced (renders made for new items only); Live Photos as Live Photos | n/a: the others either are Photos or ignore it |
| Editor hand-off | ✗ | ✗ deferred (#57), possibly a paid add-on | Kyno is built for it; NLEs import |
| Visual search ("manta") | ✗ | ✗ not in the concept | Photos, Google, Immich (CLIP), FCP 12, Premiere, Resolve 21 |
| Faces | ✗ | ✗ not planned (fish and masks) | Photos, Google, Lightroom, Immich |
| Photo originals | ✗ | ✗ intention only (#217) | every library; Lightroom ✓✓ for RAW |
| Mobile app | ✗ | ✗ (phone sees pinned renders through Jellyfin) | Photos, Google, Quik, Mimo, Insta360, Immich |
| Gyro, log, 360, HDR at playback | ✓ | ✓ unchanged, now also on weak devices through pinned renders | nobody else in a library |

## Against each kind of product

**Apple Photos.**
- **Where they'd be close:** albums, smart albums, timeline, map, stars, Share.
- **What Photos keeps:** visual search, faces, Memories, the phone, iCloud.
- **What we keep:**
  - the NAS, where Photos is unsupported;
  - the picture right at once: log, gyro, 360;
  - moments as files;
  - telemetry.
- **They also become partners:** an album syncs into Photos, so the family sees the trip on their iPhones. This is the
  study's "the turtle on everyone's phone the same evening", closed.

**Google Photos.** The same story as Photos, minus the sync: Share hands over an SDR or Motion Photo file, which is what
Android friends need. Google keeps the cloud, search and the phone.

**Lightroom.**
- **What Lightroom keeps:** RAW, the best cull of all (flags, stars, colour labels, survey view), lens profiles for
  photos.
- **What we match:** pick/reject + stars + tags + smart albums, for video, which Lightroom treats as second class (no XMP
  for video).
- **What we add:**
  - a catalog on the NAS, which Lightroom forbids;
  - the picture right at once;
  - the moment and the crop.
- **What #217 would add later:** RAW and underwater lens fringes, which Lightroom doesn't do under water.

**Immich** (the closest in spirit: self-hosted, on a NAS).
- **What Immich keeps:** CLIP search, faces, a phone app with backup, public links.
- **What we keep:**
  - no server stack;
  - albums that are files, where Immich loses an external library's albums when a file moves;
  - the picture right at once (Immich transcodes to 720p and its HDR "may look pale");
  - telemetry, moments, photos from frames.
- **Pinned renders:** these give what Immich's transcodes give (playback on weak devices) without a second copy of
  everything.

**GoPro Quik, Insta360, DJI Mimo.**
- **Where we'd be level:** the "end of the day" flow: cull, pick photos, share.
- **What we have beyond them:** a desktop, several brands, and the NAS.
- **What they keep:** the phone, the camera's Wi-Fi, auto-edits to music, live stickers.
- **Telemetry:** they show it as stickers; we let you search it, which none of them does.

**Resolve, Final Cut Pro, Premiere.**
- **Where we'd match them:** their organizing for footage (ratings, keywords, smart bins), on files any app can see,
  without a project.
- **What they keep:**
  - verified ingest;
  - visual and transcript search;
  - the edit itself.
- **Hand-off:** the line between us stays at #57, deferred. Until then, Save to a folder plus the editors' own import
  is the bridge.

## What would still be missing (the next gaps)

| Gap | Why it matters | Where it stands |
|---|---|---|
| A phone app | everyone else's day ends on the phone | `docs/mobile-app.md`; the phone sees pinned renders through Jellyfin meanwhile |
| Visual search | "manta" without tagging; now standard in Photos, Google, Immich and the NLEs | study P10; not in the concept |
| Editor hand-off | the "I grade in Resolve" user | #57, deferred, perhaps paid |
| Photo originals | cameras' own photos invisible | #217, intention |
| Split recordings as one | GoPro 4 GB chapters, long dives | study P8; not in the concept |
| A reel of moments | "the trip in 90 seconds" | study P9; Share could gain a "reel" kind of *what* later |
| Browse away from the NAS | travel | study P11 |

## Where it would leave the Library's position

- **Unique, before and after:**
  - the picture right as you browse;
  - reference-in-place on a NAS with files as the truth;
  - moments as tiny link files.
- **Unique, added:**
  - search by telemetry;
  - pinned renders that let any device play what only a Mac can process;
  - photos as frame + crop, rendered per destination.
- **At parity, added:** culling, tags, albums, smart albums, timeline, map, Share, Apple Photos.
- **Behind, on purpose:** import, faces, a full editor, auto-edits to music.
- **Behind, to do:** phone, visual search, hand-off, photo originals.

The study called the Library "more batch runner + browser than library". After this concept it is a library: it
organizes, culls and shares, and it keeps the three things nobody else does.
