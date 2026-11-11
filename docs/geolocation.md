# Where clips were recorded

Action Footage Library, 2026-11-10. Anton shoots a lot, mostly under water, where cameras have no GPS; his iPhone on
the same trip has. The Library puts clips on the map from that, shows where each was recorded, and filters by place.

## What a location is, and where it's kept

A location is an attribute of the **original** (Anton: "location is a property of the source original, written into
the original's sidecar; not into clips, not into moments"). It's kept in the original's own file, `<clip>.afp.json`
beside it (the main one when the clip is in several places, [library-management.md §8](library-management.md#8-the-project-is-its-files)):

```json
"location": { "lat": 27.733312, "lon": 34.25, "alt": 2.0, "source": "photos",
              "place": { "site": "Shark Reef", "locality": "Sharm El Sheikh", "region": "South Sinai", "country": "Egypt", "cc": "EG" },
              "photo_gap_s": -240, "set": "2026-11-10T18:02:11Z" }
```

- `source`: `photos` (suggested from Apple Photos and accepted) or `manual` (typed). `photo_gap_s`: how far in time the
  photo it came from was (negative: before the clip).
- `place` is written with it, so every computer shows and filters the same names without looking anything up.
- The camera's own GPS isn't written: it's in the clip, read by `gyroflow-probe --library` (`gps`).
- Nothing is written into the original itself, nor into clip previews or moments. A photo saved from the clip carries
  its place in its own Exif ([Photos made from a clip](#photos-made-from-a-clip)): there it is the photo's, for apps
  that know nothing of the clip.
- The player keeps the key as it saves marks or a view (`sidecar.lua`), and a file holding only a location isn't
  removed as empty (`sidecar.lua`, the Library's `tags.rs`). The catalog reads it with the rest of the sidecar
  (`afp-catalog`, `inspect.rs`), so the Location filter costs no file reads.

## When a clip was recorded

To match photos, a clip's start must be a moment in UTC. What each camera writes as the MP4's own time
(`moov/mvhd`), checked on Anton's clips (`vs-gyroflow/src/geotag.rs`):

| Camera | mvhd | Seen on |
|---|---|---|
| DJI (Osmo Action 6, Mini 4 Pro, Mavic) | UTC. The file's name is the camera's local time | DJI_20261022152937 → 13:29:38 inside (UTC+2); a drone in the Maldives 16:38 in the name, 11:38 inside (UTC+5) |
| Insta360 X5 | UTC; `first_gps_timestamp` agrees to 7 ms | 13:36:28 inside, 14:36:28 in the name (BST) |
| GoPro that writes `TZON` (HERO12…) | UTC; `CDAT` is its local clock, `TZON` its zone | Egypt HERO12: mvhd 09:07:26, CDAT 11:07:25, TZON +120 (left on Swiss time; its UTC is right) |
| GoPro without `TZON` (HERO7, older) | the camera's local time, no zone | HERO7 often never set: 2016-01-01 |
| GoPro with a GPS lock | GPSU / GPS9 time: the real UTC, whatever the clock | |
| DJI Mavic Pro (camera FC220), 2017-18 .MOV | its **local** time (its photos' EXIF, its card's file times and its videos agree) | DCIM MAVIC May: mvhd 19:10:59 + 153 s = the file's 19:13:42 local |
| DJI Mavic Air 2 (FC3170), 2020-23 (only "Lavf56.15.102" inside) | not known: UTC assumed, and said so | |
| anything else | UTC assumed, as MP4 has it, and said so | |

**Drones** (Anton, 2026-11-10: "you don't read the Mavic's location, nor its model"), what each writes:

| Drone | Its name | Its place |
|---|---|---|
| Mavic Pro (FC220), 2017-18 .MOV | `moov/udta/©mdl` "FC220-Se" (plain text) → "DJI Mavic Pro" | `©xyz` "+47.116829+9.255399+14.600" (ISO 6709): one point a clip |
| Mavic Air 2 (FC3170), 2020-23 | nothing in the video: its own photos beside it (DJI_*.JPG, EXIF Model) say FC3170 → "DJI Mavic Air 2"; none: "DJI drone" | none in the video, no .SRT |
| Mini 4 Pro (FC8482), 2024-26 .MP4 | its djmd track's header (1.1.10) "DJI Mini4 Pro" → "DJI Mini 4 Pro" | each frame's drone block 3.3.4 = GpsBasic, radians (not in telemetry-parser: read here); its .SRT when the app made one |

The first DJI metadata samples are read straight from the MP4's sample table (a few KB), not by telemetry-parser's open
(15 MB at each end of the file: 7 s a clip from the NAS, now ~1 s). The drone's name is what `gyroflow-probe --lens`
says, so the player's side panel shows it too. Any other camera's QuickTime location (`©xyz`,
`com.apple.quicktime.location.ISO6709`: phones) is read as its place.

The probe reports it as `clock`: `{ utc, basis: gps | utc | local | unset | assumed, zone_min, why }`.

- **local**: the start is taken as the place's local time. The zone is the one at the photos near it (Apple's
  geocoder gives each place's time zone, summer time included): two rounds, the nearest photo to the naive time,
  then to the time it gives.
- **unset** (before 2005, or on the first day of a year cameras start their clock at: 2000, 2010, 2015, 2016-01-01):
  not suggested; set by hand. (It was "before 2017" first, which left out Anton's HERO4 clips from the USA in May 2016:
  their clock was right, their photos' EXIF agree; with it, all 21 found their photos, San Francisco to Niagara Falls.)
- A camera whose clock was simply wrong (set an hour off) shows as a larger gap; the slider takes it in. (A per-group
  clock shift was tried in the first version and dropped for the compact sheet; the Library still takes `shifts`.)

## Suggesting from Apple Photos

*Commands › Location › Suggest from Apple Photos*, on the selection (clips or folders, in Footage or Sources),
on a Mac only (elsewhere the section says so and offers *by hand*).

1. Each clip's start in UTC and length (above). Clips with their own GPS are left out (said); ones with no usable time
   too (said: set them by hand).
2. Clips in the same folder whose recordings are at most 60 min apart are taken together for the clock (a local-time
   camera's zone is found once for them).
3. Photos with a place taken from 5 days and 14 h before the first clip to as long after the last (`afp-photos near`).
4. Each clip's **nearest photo in time**: 0 when taken during it, else the gap to its start or end; its place looked
   up (by 0.01°, once each).
5. **One compact sheet for the whole selection** (Anton, 2026-11-10):
   - a slider, **nearest photo at most** 3 h … 5 days (3, 6, 9, 12, 18 h, 1, 1.5, 2, 3, 4, 5 days; 12 h first);
   - **the places found, by name** ("Sharm el-Sheikh, South Sinai, Egypt"), each with how many clips get it at that
     limit, the range of their gaps to their photos, and how many spots (photo points) it covers; a Map button;
   - **too far**: the clips beyond the limit (and those without a usable time), listed with why;
   - one radio among the places (and "None") for what the too-far clips get; the place with most clips first;
   - "Keep the N locations already set" when some have one;
   - **Apply to N clips** / **Cancel**. Clips within the limit get their own nearest photo's point and place
     (`source: photos`, `photo_gap_s`); the too-far ones the chosen place's most common point (`source: manual`).

   On the Egypt trip (131 HERO12 clips, 25 photos with a place): one place, Sharm el-Sheikh (2 spots); at 3 h 17 clips
   in and 114 too far, at 12 h 75 / 56, at 1 day 115 / 16, at 2 days 120 / 11, at 5 days all.

How far off it can be: a diver's iPhone stays on the boat, so the nearest photos are those before and after the dive,
typically 10–60 min from a clip; the boat is moored at the site, so the place is right to a few hundred metres. A
transfer between sites with no photo taken shows as a large gap. The camera's clock matters as much: a GoPro's or
DJI's own UTC is good to seconds when the camera got its time from the phone app; one set by hand may be minutes off.

### Reading Photos: PhotoKit, not Photos.sqlite

`macos/photos.swift` → `afp-photos` in the Library's `Contents/MacOS`, through PhotoKit:

- It only fetches (`PHAsset.fetchAssets`, `PHAssetResource` for a file name); no change request exists in it, so the
  library can't be changed by it.
- macOS asks once, attributing it to Action Footage Library (its `NSPhotoLibraryUsageDescription` says why). Its prompt
  says "access": macOS has no read-only level for apps (`.readWrite` is the one that can read). Revocable in System
  Settings › Privacy & Security › Photos.
- Reading `Photos.sqlite` directly (osxphotos' way) was rejected: it needs Full Disk Access for the whole app (far more
  than Photos), and its schema changes with macOS versions (this Mac runs 27). What it would add (each photo's own
  time zone, Photos' own place names) comes from Apple's geocoder here.
- PhotoKit gives each photo's time as a moment (UTC) and its place; iCloud photos not downloaded are included (their
  metadata is local).
- A test build with its own bundle id, or a new ad-hoc-signed build, is a new app to macOS: it asks again.

## Place names

Reverse geocoding, **cached on this computer** (`<cache>/places.json`, by the point rounded to 0.01°, ~1 km), each
place looked up once, at most one lookup a second:

- **Mac: Apple's geocoder** (`CLGeocoder` in `afp-photos place`), in English. Free, no key, rate-limited by Apple per
  app; nothing sent to third parties. Gives the time zone too, and `areasOfInterest` (a national park, a reef), taken as
  the site.
- **Windows, Linux: OpenStreetMap's Nominatim**, as its usage policy asks: one request a second at most, the app named
  in the User-Agent, results cached, no bulk lookups.
- At sea: the country and region where territorial waters count, the sea's name otherwise ("Red Sea"); a dive site has
  no name in either, so it's typed when a location is set (kept in `place.site`).

## In Info

A clip: *Location*: a small map with its dot, the place (site, locality, region, country), the point and height,
where it came from (Apple Photos with the photo's gap, by hand, or the camera's GPS), *Show on a map* (Apple Maps;
OpenStreetMap elsewhere) and *Remove*. Camera GPS: the place looked up as the clip's probe comes in.

A folder or several clips: *Location*: a small map with a dot per spot, the places by name with how many clips each,
and how many have none.

The small maps are Apple Maps pictures made on the Mac (`afp-photos map`: MapKit's `MKMapSnapshotter`, no key, nothing
but the map's area asked of Apple), dots drawn on them, in the page's light or dark look, kept in the cache
(`<cache>/maps/`) by their points, size and look. Elsewhere there's no map, only the names.

Access to Photos is asked only from *Suggest from Apple Photos*, after the Library's own explanation; opening the
Library only looks whether it's allowed (`afp-photos status`, which never asks).

## The Location filter

In the left panel, Footage and Sources: **Country** (with **No location**), **Region**, **Place** (site or locality),
nested by their parent, and **From** (Apple Photos, by hand, the camera's GPS). Counts and narrowing work as in the
other lists: ticking a country leaves its regions and places.

## Photos made from a clip

Issue #50: a photo saved from a clip (`gyroflow-export --photo`: Save frame, the Project tab's photos of marks) carries
where it was taken, so Apple Photos and Google Photos put it on their maps (`geotag::photo_place`):

1. **The camera's own GPS at the frame's time** (`geotag::fix_at`): the fix nearest it, at most 30 s away (a GPS that
   found its satellites a little after the start, or lost them for a while). GoPro: the GPS5 / GPS9 rows of the GPMF
   samples around it, each at its time in its sample. DJI drones: the frame's own GpsBasic in the `djmd` track (a
   sample a frame, read from the sample table: the frame's, then one a second further each way), else the .SRT beside
   the clip (its block at that time). Insta360: its GPS track (nav.rs). Else QuickTime's location (`©xyz`, one place
   a clip: the Mavic Pro, phones).
2. Else **the location the Library keeps for the clip** in its own file (the main one over the project's locations,
   then the other copies', then `<clip>.afp.json` beside it).
3. Else none, and gyroflow-export says so.

Altitude goes with it when known (GoPro's GPS altitude, a drone's `abs_alt` or GpsBasic altitude, the location's
`alt`). Written as:

| Photo | Where |
|---|---|
| JPEG / Ultra HDR JPEG (photo.rs) | Exif GPS IFD: GPSVersionID 2.3, latitude and longitude (degrees, minutes, seconds to 1/10000″) with their N/S, E/W, altitude and its reference (1: below sea level) |
| HEIC (afp-photo) | the same, as ImageIO's GPS dictionary |
| Live Photo | the HEIC's GPS, and its .mov's `com.apple.quicktime.location.ISO6709` (as an iPhone writes it) |
| Motion Photo | the JPEG's GPS, and its MP4's `com.apple.quicktime.location.ISO6709` (mdta keys, mp4meta.rs) |

The capture time stays as it was (the clip's start, local, plus the frame's time).

## Not yet

- A map of the whole project (Map tab), clickable dots; the ⌖ badge on rows; maps on Windows and Linux.
- Writing the location into moment MP4s and video exports (`©xyz` / ISO 6709): the original's is the one source; they
  can read it from there when made. (Photos carry theirs: above.)
- mpv's own "Save frame as image" (its `screenshot`) writes no metadata at all, place nor time.
- Dive-site names from a dive log or a list of sites.
