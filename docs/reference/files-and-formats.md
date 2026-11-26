# Files and formats
Covers: `mpv/scripts/workspace/` (sidecar, workspace, projectfile, moments, link, tags, librarylist, journal,
projectcache), `mpv/scripts/gyroflow/`, `mpv/scripts/dlogm/`, `vs-gyroflow/src/` (workspace, mp4meta, xmp,
afp_motion, reframe, lens_pick, lens_db, lib.rs `file_cache`), `catalog/src/` (project, locations, journal, snapshot,
catalog, inspect, care, live, finder), `library/src/` (main, tags, renders, index, geo, journal, albums),
`macos/player.sh`, `windows/launcher/`, `tools/nas-helper/`, `tools/stream/`.

Checked against 295101c (2026-11-25).

Every file the programs read or write, as the code has it. Where a design doc says otherwise, the code is right and
this file follows it; where this file and the code disagree, fix this file. Words are in [glossary.md](glossary.md).

| Format | Where | Owner (writes it) |
|---|---|---|
| [The clip's own file](#the-clips-own-file-clipafpjson) | `<clip>.afp.json` beside the clip | player (sidecar.lua), Library (tags.rs, geo.rs) |
| [Project.afproject](#projectafproject) | the project's folder | Library (project.rs `update_doc`), player (projectfile.lua) |
| [.afplocation pointer](#afplocation-pointers) | top of each location | Library (catalog `write_pointers`) |
| [This computer's project record](#this-computers-project-record) | `<data>/ActionFootage/projects/<id>.json` | Library |
| [The catalog](#the-catalog-afpcatalog) | `<project>/.afp/catalog/` | afp-catalog (Library, player via `afp-catalog note`, NAS Helper) |
| [Links in MP4s](#links-in-mp4s) | `ch.actionfootageplayer.link` in `moov/meta` | gyroflow-export, afp-cut, afp-meta, Library (albums) |
| [Links and tags in photos](#links-and-tags-in-photos-xmp) | XMP `afp:link`, `dc:subject` | gyroflow-export (photo.rs), afp-photo, afp-meta |
| [The project's own folders](#the-projects-own-folders) | `_Moments`, `_Clips`, `_Stills`, `_Exports`, `_Renders`, `Generated` | player, Library |
| [Measured motion](#measured-motion-afp-motion) | `<clip>.afp-motion` beside the clip | gyroflow-optical, gyroflow-export |
| [Gyroflow projects](#gyroflow-projects-and-presets) | `<clip>.gyroflow`, `<moment>.gyroflow`, `default.gyroflow` | Gyroflow, player (Ctrl+e) |
| [Tags](#tags) | own file, link, XMP, Finder xattr | player (tags.lua), Library (tags.rs), catalog (finder.rs) |
| [Library lists](#library-lists-afplistm3u8) | `<Library cache>/lists/<ms>.afplist.m3u8` | Library |
| [Batch requests](#batch-requests-afp_batch) | `$TMPDIR/ActionFootageLibrary-<pid>/<job>/` | Library, player |
| [Settings files](#settings-script-opts-and-render-defaults) | `<mpv config>/script-opts/*.conf`, `render-defaults.json` | player, user |
| [LUTs and looks](#luts-and-looks) | `luts/*.cube`, `$DLOGM_DATA` | packagers, setup |
| [Lens lists](#lens-lists) | `curated_lenses.json`, `lenses/` | user, Library |
| [Per-computer caches](#per-computer-caches) | `~/Library/Caches/…`, mpv `~~cache` | each program |
| [Journal logs](#responsiveness-journal) | `~/Library/Logs/ActionFootagePlayer/journal/` | player, Library |
| [Data folders](#data-folders-on-each-system) | App Support and the like | each program |
| [NAS Helper](#nas-helper-and-stream) | `/data`, `/config`, the Jellyfin folder | NAS Helper |

## The rules

- **The project is its files.** What a clip is, how it plays, its marks, tags and place live in the clip's own file,
  in links inside the files made from it, or in a per-computer cache. `Project.afproject` holds where the footage is.
  Never add new state to it (CLAUDE.md, [library-management.md](../design/library-management.md) §8). See the note under
  [Project.afproject](#projectafproject) for what it holds today beyond that.
- **Writes are whole.** Every JSON file of ours is written to `<file>.tmp` and renamed over the file. A reader never
  sees half of one. Each writer reads the file again just before, changes its part and keeps every key it doesn't
  know.
- **Raise the version when what's kept changes.** `afp_catalog::project::VERSION` and `projectfile.lua` `M.VERSION`
  together (test `what_is_kept_goes_with_the_projects_version`); the probe's answer with `index.rs` `GEOTAG`; the
  render recipe with `renders.rs` `VERSION`, and the per-moment one with workspace and `index.rs` `RENDER_VERSION`.
- **Older programs refuse newer data, never wipe it** (#220, #249). Each format below says how.
- **Migrations are one-off scripts** in `tools/one_off/` (`photos_to_links.py`, `uw_to_source.py`), run once on
  Anton's library. A few old forms are still read by the product; they are listed in [Old forms](#old-forms-still-read).

## The clip's own file (`<clip>.afp.json`)

**What:** everything kept about one original: its marks, its input (how to read it), its saved view, its 360
recording, its place, its tags and comment. Not made until something is saved. Removed again when nothing is left in
it (`format`, `version`, `clip` and `synced` alone don't keep it).

**Path:** the clip's path with its extension replaced: `DJI_0042.MP4` → `DJI_0042.afp.json`. In a project with
locations, one clip may have copies in several locations; its own file is the **main one**, found by `own_file`:
beside the first copy (library locations in order, then copies-only and this computer's own) that has one, else
beside the first copy that isn't copies-only. The others are read, never written. Two files found: nothing is saved
until one is removed.

```json
{
  "format": "actionfootageplayer-clip", "version": 1,
  "clip": { "file": "DJI_0042.MP4", "size": 123456789, "created": "2026-10-22T13:28:42Z", "hash": "<sha256 hex>" },
  "marks": [ { "t": 12.4, "camera": "dji" }, { "t": 40.1, "name": "Turtle" } ],
  "source": { "curve": "dlogm", "rotation": 180, "underwater": "auto", "uw": true, "uw_highlights": false,
              "uw_strength": 0.8, "lens_type": "normal", "lens": { "name": "…", "how": "chosen", "profile": {} } },
  "view": { "view": "normal", "level": "medium", "horizon_lock": "yes", "chromatic": true, "aspect": "original",
            "orientation": "as-is", "projection": "linear", "lens": "corrected", "denoise": true, "denoise_strong": false },
  "view360": { "recording": { "follows": "travel", "parts": [ { "samples": [[0.0, 10, 0, 0, 90]] } ], "frame": "16:9" } },
  "location": { "lat": 27.91, "lon": 34.33, "source": "photos", "place": { "site": "…", "country": "…" },
                "set": "2026-11-10T10:00:00Z" },
  "pick": "accept", "rating": 4, "tags": ["reef", "Turtles"],
  "synced": { "finder": ["Accept\n2", "★★★★", "reef"] },
  "comment": "free text"
}
```

| Key | Meaning |
|---|---|
| `format`, `version` | Always `actionfootageplayer-clip`, 1. A file with another `format` is "not one of ours" and left alone. |
| `clip` | Which clip the file is for: name, size, creation time, fingerprint (`workspace.rs` `fingerprint`). The highlights plugin renames a file whose `clip.size` differs to `.afp.json.old` rather than apply it. |
| `marks` | Absent: the camera's marks count. Present (even `[]`): these are the marks. `camera` flags one copied from the camera. |
| `source` | The input: log curve, turn, lens under water, underwater colour (`transform.lua` `INPUT_KEYS`, `UW_KEYS`), the lens type and a lens for this clip alone (`lens_pick.rs`). |
| `view` | The saved transform bundle, all keys at once (`transform.lua` `KEYS`). Absent: the defaults play. |
| `view360.recording` | A 360 clip's recorded view (`view360/recording.lua`, `reframe.rs`). |
| `location` | The original's place ([geolocation.md](../design/geolocation.md), `geo.rs`): `lat`, `lon`, `alt`?, `source` (`photos`, `manual`), `place` (`site`, `locality`, `region`, `country`, `cc`, `water`), `photo_gap_s`?, `set`. The camera's own GPS is never written here. |
| `pick`, `rating`, `tags`, `synced` | Culling ([Tags](#tags)). |
| `comment` | The Library's alone. The player keeps it as any unknown key. |

**Version rule:** `version` stays 1; fields are added, never changed in meaning. The catalog keeps
`marks view source tags rating pick location` and passes every other key through as it is (`inspect.rs`
`OWN_KEPT`, `own_kept`), so a program older than a field doesn't wipe it. A change to that list raises the project's
VERSION.

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/sidecar.lua` `read`; `workspace/workspace.lua` `own_file`, `sidecar_file`; `workspace/places.lua` `own_path`; `highlights/main.lua`; `view360/main.lua`; `workspace/transform.lua` `migrate_source` | `sidecar.lua` `update` (each plugin through its own `sidecar.new`) |
| Rust (vs-gyroflow) | `workspace.rs` `own_file`, `sidecar_file`; `reframe.rs` `sidecar_path`, `Recording::read`; `lens_pick.rs` `clip_lens_value`, chosen lens type; `geotag.rs` | — |
| Rust (catalog) | `inspect.rs` `read` (`is_sidecar`), `own_kept` | — |
| Rust (Library) | `tags.rs` `clip_own`, `clip_own_in`; `index.rs`; `checks.rs`; `changed.rs` | `tags.rs` `write_json` (tags, pick, rating, comment, `source.lens_type`), `geo.rs` (`location`), `lens.rs` (`source.lens`, set_source) |
| Python | — | `tools/one_off/uw_to_source.py` (once) |

## Project.afproject

**What:** marks a folder as a project and says where its footage is. A double click opens it in the Library. Not
hidden: the user sees it.

**Path:** `<project>/Project.afproject`. Before 0.9 a project was the folder `_ActionFootagePlayer/` instead.

```json
{
  "format": "actionfootage-project", "version": 4,
  "id": "1a2b3c4d-5e6f-4a12-a345-0123456789ab",
  "locations": [
    { "name": "Archive", "at": "", "path": "//nas/video/Footage/2026", "copies": false },
    { "name": "Action 6", "at": "Action 6", "path": "../Cards/A6", "copies": false }
  ],
  "hosts": { "nas": ["nas.local", "nas-tailnet"] },
  "settings": { "clips_flat": false, "clips_layout": "tree", "moments_alone": false },
  "generated_runs": true
}
```

| Key | Meaning |
|---|---|
| `id` | The project's identity: what pointers, this computer's record and the catalog's cache folder go by. Made by the Library (`settings.rs` `new_id`) at the first save of locations. Without one, the catalog uses a hash of the path. |
| `locations` | Folders of footage, each placed at `at` in the project's tree, in order (the first that has a clip wins). `path` is a network path `//host/share/…` (the same on every computer), else relative to the project folder, else absolute. `copies`: a working copy, never where a clip's own file goes first. None: the project folder is the one location. |
| `hosts` | Other names of a host (`host_key` folds case, `user@`, `._smb._tcp.local`, `.local`): a `//host/share` path matches a share mounted by any of them. |
| `settings` | Library settings: `clips_flat`, `clips_layout` (`tree`, `flat`, `trips`), `moments_alone`. `update_doc` always writes `settings` (`{}` at least). |
| `generated_runs` | A folder per run in `Generated/` (Library's Project settings; the player's `generated_runs`). Written only when on. |

**Version:** `afp_catalog::project::VERSION` = `projectfile.lua` `M.VERSION` = 4.

| Version | What it says |
|---|---|
| 1 | before locations (up to 0.10) |
| 2 | with its locations (the Library, 0.11) |
| 3 | the catalog keeps a clip's location (0.12) |
| 4 | the catalog keeps Accept / Reject, the stars and whatever else a clip's own file holds (0.13) |

**Rule:** a program refuses a project whose version is above its own: the catalog returns `Error::TooNew` for
load and note (`project.rs` `too_new`), the Library's `update_doc` refuses, the player's `pf.update` refuses ("update
it"). Every writer raises a lower version to its own and never lowers it (`project.rs` `raise`, `projectfile.lua`
`complete`). A file with another `format` is not touched.

> **Note (code vs rule):** CLAUDE.md says the file keeps only `locations`, `hosts` and `id`. The code also writes
> `settings` and `generated_runs` (Library `project.rs` `set_setting`, `settings.rs` `save_locations`), and leaves old keys
> (`clips_made`, `clips_failed`, `stills_made`, `moment_names`, `photo_tags`) in place when a file still has them.
> `update_doc` sets the version after the change and never lowers it (test
> `project_version::writing_the_project_never_lowers_its_version`). The player's `moments_alone` is kept in
> its own cache, not here, so the two apps don't share that switch.

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/projectfile.lua` `read`; `workspace/workspace.lua` `locations`, `place`; `workspace/opening.lua` `generated_runs` | `projectfile.lua` `update`, `migrate` (from `_ActionFootagePlayer/`) |
| Rust (vs-gyroflow) | `workspace.rs` `locations` | — |
| Rust (catalog) | `project.rs` `project_id`, `version`, `too_new`; `locations.rs` `locations`, `read_json` | `project.rs` `raise` |
| Rust (Library) | `project.rs` `read_doc`, `project_info`; `changed.rs` | `project.rs` `update_doc` (via `project_id`, `save_locations`, `set_setting`, the project move) |
| Python | `tools/stream/afp_stream.py` (project of a moment) | — |

## .afplocation pointers

**What:** a small file at the top of a location that says which project uses it, so a clip opened from Finder finds
its project. One per project that uses the location.

**Path:** `<location folder>/<project folder's name>.afplocation`. A name starting with `.` is ignored.

```json
{ "format": "actionfootage-location", "version": 1, "project": "//nas/ssd/Trips/Egypt",
  "id": "1a2b3c4d-…", "at": "Action 6", "name": "Action 6", "hosts": { "nas": ["nas.local"] } }
```

`project` is where the project is, written as `path_for` makes it (network path, relative, or absolute). `hosts` is
the project's `hosts`, copied into every pointer.

**Rule (#299):** a pointer counts only for the project of its `id`. A pointer without an id counts for none. Its path
is placed (`place`, with its `hosts`), and the project there must have that id. When it doesn't, this computer's
record for that id (`projects/<id>.json` `project`) is tried, checked the same way. Shares are never searched by name.
The lookup stops at the volume's top folder or the home folder. Keep the three copies in step (shared cases:
`tests/workspace_cases.txt`).

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/workspace.lua` `projects_in`, `pointer_id`, `known`; cached by `projectcache.lua` | — |
| Rust (vs-gyroflow) | `workspace.rs` `pointer_id`, `known` | — |
| Rust (catalog) | `locations.rs` `pointer_project`, `pointer_name` | `locations.rs` `write_pointers` (removes them from locations taken out) |
| Rust (Library) | through the catalog | `settings.rs` `save_locations`, `moving.rs` project move (calls `write_pointers`) |

## This computer's project record

**What:** what one computer keeps about a project: where the project is here, its own extra locations (copies on its
own disk), and its own choices. Never in `Project.afproject`: another computer may differ.

**Path:** `<data>/ActionFootage/projects/<id>.json` (`<data>`: macOS `~/Library/Application Support`, Windows
`%APPDATA%`, else `$XDG_CONFIG_HOME` or `~/.config`). `id` must be letters, digits and `-`.

```json
{ "project": "/Volumes/ssd/Trips/Egypt",
  "locations": [ { "name": "On this computer", "at": "", "path": "~/Work/Egypt" } ],
  "never_rename": true, "tag_stores": ["finder"] }
```

These locations come first and are always copies-only (`local`). `never_rename`: a clip's name counts when a link's
clip is looked for. `tag_stores`: the tag stores synced on this computer (only `finder` is known). No version.

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/places.lua` `locations_of`, `fs.projects_dir`; `workspace.lua` `known` | — |
| Rust (vs-gyroflow) | `workspace.rs` `projects_dir`, `known` | — |
| Rust (catalog) | `locations.rs` `local_file`, `locations`, `never_rename`, `tag_stores`, `pointer_project` | `locations.rs` `note_here` (`project`, as a project opens) |
| Rust (Library) | through the catalog | `settings.rs` `save_local`, `tagging.rs` the tag stores' save |

## The catalog (`.afp/catalog`)

**What:** a shared record of the project's files (and what's inside the ones that count) so that no program has to
list a share's folders to know them. Each computer appends to its own journal; snapshots fold the journals now and
then (the Library only). Design: [library-management.md](../design/library-management.md) §9,
[catalog-safeguards.md](../design/catalog-safeguards.md).

**Path:** `<project>/.afp/catalog/`. On a share, only these operations are used: list, stat, read, append by the one
writer, write under a new name (temp then rename to a name nobody else uses), remove (`store.rs`). No locks on the
share.

```
.afp/catalog/
  journal/<computer>/<first line, 12 digits>.log   this computer's lines, a segment rotated at 1 MiB
  snap/<generation, 8 digits>-<sha, 32 hex>.json    a snapshot's manifest, named by its content
  parts/<sha, 32 hex>.gz                            a part of a snapshot: gzipped JSON lines, named by its content
  care/<computer>/<root key>.deep | .watch          who looked after each root, and when
```

**Journal line:** `<8 hex of the text's SHA-256> <JSON>\n`. A line with no newline or the wrong sum is left out; after
a failed write the next line starts a new segment. The first line of a segment names it:
`{"computer": "…", "segment": N, "format": 1}`. Each other line is a batch: `{"q": seq, "l": lamport, "e": [entries]}`.
An entry (`state.rs` `entry_json`):

```json
{ "r": "p", "p": "_Moments/Turtle.afplink.mp4", "s": 81234, "m": 1732540000123456789,
  "x": { "link": { "v": 1, "kind": "moment" } }, "h": "<digest>" }
```

`r` is the root (`p` the project folder, `l:<location path>` a location), `p` the path in it, `s` size, `m` mtime in
ns, `d: 1` a folder, `g: 1` gone, `x` what's inside (`{"link": …}`, `{"link": …, "tags": […]}` for a photo,
`{"own": …}` for a clip's own file: `inspect.rs` `read`).

**Snapshot manifest:** `{"format": 1, "gen", "by", "covers": {computer: last line}, "lamports", "l", "bits",
"parts": [2^bits names], "count", "at": {computer: [segment, byte]}}`. Records go into buckets by what a reader looks
for: a link's original (`c:<clip>`, else `h:<hash>`), else the folder (`d:<root>/<folder>`), so the player reads only
its clip's part. Three snapshots are kept (`KEEP`); a file read is checked against its name.

**Care files:** `.deep` gets a line at each deep look of a root; `.watch` is the NAS watcher's heartbeat (a line every
5 minutes, fresh for 15). Root key: first 16 hex of the root id's SHA. Only the share's own file times are compared.

**Version:** the manifest and the segment header carry `format: 1`; a reader refuses another manifest format. What the
records hold is versioned by the project's VERSION: the catalog refuses to load or note a newer project
(`catalog.rs` `usable`).

**This computer's side of it** (in `<cache>/catalog/<project id>/`, see [Data folders](#data-folders-on-each-system)):
`writer.json` (`{seg, size, seq, l, broken}`: where its journal is), `writer.lock`, `recheck.json`, `clean.json`,
`look.lock`, cached snapshot files and parts. `<data>/computer.json` (`{id, machine, host}`) is this computer's name in
the journals, made once and tied to the machine's id. `<cache>/live/` holds `library-<pid>.json` (`{pid, port}`, a
Library listening on 127.0.0.1) and `player-<pid>.in` (a player's inbox of `{project, files}` lines).

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/catalogtool.lua` `catalog.ask`, `catalog.now`, `catalog.derived` (through `afp-catalog`) | `catalog.note`, `catalog.look` (runs `afp-catalog note`/`look`) |
| Rust (catalog) | `catalog.rs` `load`, `journal.rs` `parse`, `snapshot.rs` `Manifest::from_bytes`, `decode_part`, `care.rs` | `catalog.rs` `note`, fold, clean-up; `journal.rs` `header`, `line`; `snapshot.rs`; `care.rs`; `live.rs` |
| Rust (Library) | `seen.rs`, `index.rs`, `checks.rs` (through `afp_catalog`) | through `afp_catalog` (looks, folds) |
| Python | — | NAS Helper runs `afp-catalog` with `AFP_HOME=/data/catalog` |

## Links in MP4s

**What:** a moment, clip preview, album item, export or render keeps a link to its original inside itself: QuickTime
`mdta` key `ch.actionfootageplayer.link`, a JSON text. It is how everything made is found again, wherever it was moved
([moments-and-workspaces.md](../design/moments-and-workspaces.md), "The link inside").

**Where in the file:** `moov/meta` (hdlr `mdta`, `keys`, `ilst`, no version/flags: Apple's form, which AVFoundation
reads). Readers also accept `moov/udta/meta` (ffmpeg's `-movflags use_metadata_tags`); `moov/meta` wins. Beside it
gyroflow-export may write `com.apple.quicktime.creationdate` and `com.apple.quicktime.location.ISO6709`. Other keys
in the meta are kept byte for byte (`mp4meta.rs` `merged_meta`).

```json
{ "v": 1, "kind": "moment", "clip": "Day 1/DJI_0042.MP4", "size": 123456789,
  "created": "2026-10-22T13:28:42Z", "hash": "<sha256 hex of first+last MiB>",
  "in": 12.345, "out": 20.5, "name": "Turtle", "roots": ["/Volumes/ssd/Trips/Egypt"],
  "snap": 1, "alone": false, "stab": { "view": "normal", "level": "medium" },
  "tags": ["reef"], "pick": "accept", "rating": 5, "comment": "…" }
```

| Key | Meaning |
|---|---|
| `v` | 1. Readers take only `v == 1`. |
| `kind` | `moment`, `clip` (clip preview, album item), `export`, `photo`, `render`. |
| `clip` | The original's path in the project's tree, `/` separated, where it was last seen. Rewritten in place when the clip is found elsewhere. |
| `size`, `created`, `hash` | The original's identity: size, its MP4's creation time, fingerprint (`workspace.rs` `fingerprint`: SHA-256 of its first and last MiB). gyroflow-export adds `hash` when missing (`with_fingerprint`). |
| `in`, `out` | Seconds in the original, rounded to ms. |
| `name`, `roots` | The moment's name; the project folders it was made in (a hint). |
| `snap`, `alone`, `stab` | Settings of its own: the transform bundle fixed when it was made, and whether its part is stabilized on its own. |
| `look`, `stabilized` | An export's. |
| `t`, `crop`, `camera` | A photo link's frame time, crop, camera. |
| `draft` | A clip preview still being made. |
| `moment`, `profile`, `recipe` | A render's: its moment's path, `HDR`/`SDR`/`Photo`, and everything that decides its pixels. |
| `tags`, `pick`, `rating`, `comment` | Culling of a moment ([Tags](#tags)); carried when a moment is saved again (`moments.lua` `CARRIED`). |

The original is found by `moments.lua` `original`: the hint at `clip` through the locations (size must match), then
the place this computer last found it, then same name and size, then the fingerprint and creation time.

**A clip in an album** (`_Clips/<album>/<clip>.afplink.mp4`, ~20 KB) is built in memory by `albums.rs` `link_file`,
no ffmpeg: `ftyp`, an `mdat` holding one JPEG as it is, and a `moov` with one Photo-JPEG video sample held a second
and the link in `moov/meta`. Its link is `{ v: 1, kind: "clip", clip, size, roots, created? }`. Its thumbnail is the
camera's `.THM`, else the card's picture, in normal colour when the clip's curve is known (`albums.rs` `thumbnail`,
`lut_for`).

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/link.lua` `read`, `read_keys`, `created`; `workspace/moments.lua` `scan`, `original` | builds the JSON in `workspace/` `momentsave.lua` (moment, export, photo), `clippreviews.lua` (clip), `renders.lua` (render), passes it as `--link` |
| Rust (vs-gyroflow) | `mp4meta.rs` `read_keys` | `mp4meta.rs` `write_keys`, `merged_meta`; `gyroflow-export --link`; `afp-cut --set-key` (in place); `afp-meta --link/--tags/--key` |
| Rust (catalog) | `inspect.rs` `mp4_keys`, `read` | — |
| Rust (Library) | `index.rs`, `renders.rs` | `albums.rs` (album items, written whole with their thumbnail frame) |
| Python | `tools/stream/afp_stream.py` (moments and clip previews) | — |

## Links and tags in photos (XMP)

**What:** a photo made from a clip carries the same link JSON as `afp:link` (namespace
`https://actionfootageplayer.ch/ns/1.0/`) and its custom tags as `dc:subject` (which Photos and Lightroom read as
keywords), in its XMP packet. A photo's pick and rating are in its link.

**Files:** `.heic`/`.jpg` photos, and photo links `<clip stem>_<t>s.afplink.jpg` (a cropped frame, at most 1280 px,
visibly a link). Readers look only in the first 64 KiB. `afp:link` may be an element or an attribute (ImageIO's form).

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/link.lua` `photo`, `photo_tags` | (asks afp-meta) |
| Rust (vs-gyroflow) | `xmp.rs` `link`, `tags`, `read` | `photo.rs` (JPEG at export), `xmp.rs` (JPEG in place), `afp-meta`; a HEIC through `afp-photo` (macOS, ImageIO) |
| Rust (catalog) | `inspect.rs` `photo_meta` | — |
| Rust (Library) | `index.rs` `photo_tags` | `tags.rs` (through afp-meta) |
| Python | — | `tools/one_off/photos_to_links.py` (once) |

## The project's own folders

**What:** folders in the project that hold what the programs made, never footage. Their names start with `_`, so they
sort first.

| Folder | Holds |
|---|---|
| `_Moments/` | Moments: `<name>.afplink.mp4`, and a moment's own Gyroflow project `<name>.gyroflow` beside it. |
| `_Clips/` | Clip previews and the user's albums: `<the clip's path>.afplink.mp4` (layout `tree`, `flat` or `trips`). An album is a folder (subfolders: albums in it); one album name stands for `_Clips/<name>`, `_Moments/<name>` and `_Stills/<name>`. |
| `_Stills/` | Photos (`.heic`/`.jpg` with `afp:link`) and photo links (`.afplink.jpg`). |
| `_Exports/` | Exports: `<name>[ (SDR)].mov`, `.mkv` for YouTube. |
| `_Renders/` | The render cache: `<recipe hash>.<profile>.<ext>` (Library, `renders.rs`); the hash is the first 24 hex of the SHA-256 of the canonical, trimmed recipe (`renders::hash`); `HDR`/`SDR` are `.mov` (`.mp4`: a part cut as recorded), `Photo` `.heic`, else `.jpg`. Before 0.11 `<the moment's path>.<profile>.mov`. Cleared only by the user's Clear, which takes only files named so (or carrying a render link) and `.making` leftovers (`renders::clear`, `ours`); anything else there is kept. |
| `<kind folder>/Generated/` | Where a run puts what it makes (`_Moments/Generated`, …); with `generated_runs` a folder per run (`YYYY-MM-DD HH-MM`, one a day for what's saved by hand). Projects of before have `Generated/` at the top, still read. |
| `.afp/` | The catalog. |

Files may be moved anywhere in the project folder; they are found by the link inside. The lists that leave these out of
the footage must agree (`tests/project_folders_test.py`): `index.rs` `OWN_FOLDERS`, `inspect.rs` `OWN_FOLDERS`,
`checks.rs` `OWN`, `moments.lua` `OWN_FOLDERS`, and `afp_stream.py`. Today they hold `_moments _clips _exports _stills
_renders _cache moments clips exports stabilized _actionfootageplayer generated`, plus `#recycle` in the two Rust
`OWN_FOLDERS` (the catalog also skips names starting with `.`, `#`, `~~`, `@` and `Thumbs.db`). `takein.rs` has its own
`OWN` of the six folders it moves.

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/moments.lua` `scan`, `generated_rel`, `clip_preview_relative`; `workspace/renders.lua` `render_file` | `workspace/` `momentsave.lua`, `clippreviews.lua`, `marksjob.lua`, `renders.lua` (moments, previews, photos, exports, renders through gyroflow-export) |
| Rust | `catalog/src/inspect.rs` `Ours`; `library/src/index.rs`, `renders.rs`, `checks.rs` | `gyroflow-export`; `library/src/albums.rs`, `renders.rs`, `takein.rs` |
| Python | `tools/stream/afp_stream.py`, `tools/stream/jellyfin.py` | — |

## Measured motion (`.afp-motion`)

**What:** a clip's motion measured from its picture (Gyroflow's optical correction), for clips without usable gyro
data. With it the clip plays stabilized without a Gyroflow project ([optical-without-gyroflow.md](../design/optical-without-gyroflow.md)).
A HERO7 Black's with HyperSmooth on is HyperSmooth's turn measured on top of its gyro instead (`ignore_file_motion`
false, `in_camera_turn` in the header; [hero7-hypersmooth-iori.md](../design/hero7-hypersmooth-iori.md)): played, the
same turn goes to gyroflow-core as the clip's image orientations, as a HERO12's IORI.

**Path:** `<clip stem>.afp-motion` beside the clip (`afp_motion.rs` `path_for`).

**Structure:** two lines. Line 1, the header (JSON, readable with `head -1`); line 2, the correction as base91 of
zlib'd CBOR, as Gyroflow's projects keep it.

```json
{ "format": "actionfootageplayer-motion", "version": 1,
  "clip": { "name": "GOPR0001.MP4", "size": 4000000000, "created": 1731000000, "width": 2704, "height": 1520,
            "fps": 59.94, "duration_ms": 61000, "frames": 3656 },
  "lens": {}, "lens_name": "…", "readout_ms": 15.5, "readout_direction": "TopToBottom", "refraction": 1.33,
  "frame_offset": 0, "offsets": {}, "ignore_file_motion": false,
  "measured": { "how": "measured", "from": "", "at": 1732540000, "tool": "…", "tracking": "klt", "strength": 1.0,
                "per_frame": true, "frames": 3656, "measured_frames": 3600, "rms_deg": 0.2 },
  "in_camera_turn": { "stabilizer": "HyperSmooth (HS EIS)", "integration": 2, "model_sigma_s": 0.5, "model_delay_s": 0.2,
                      "rms_deg": 1.1, "p95_deg": 2.3, "from_picture_rms_deg": 0.6, "rounds_deg": [2.1, 0.08, 0.01],
                      "measured_share": 0.97 } }
```

`in_camera_turn` only in a HERO7's (`in_camera_turn.rs` `Turn`), which is then version 2.

**Version rule:** `version` above 2 is refused ("update the player"); a player before 2026-11-26 refuses 2 as well.
Written: 2 with `in_camera_turn`, else 1. A file whose `clip.size` (and `created`, when
both have one) differs is "measured on another clip" and not used.

| Language | Read | Written |
|---|---|---|
| Lua | `gyroflow/sources.lua`, `gyroflow/measure.lua`, `side-panel/main.lua` (whether there is one) | runs gyroflow-optical / gyroflow-export (Ctrl+m) |
| Rust (vs-gyroflow) | `afp_motion.rs` `header_for`, `read_for`, `apply` (with `in_camera_turn.rs`); `gyroflow-gpu-table`, `gyroflow-probe`, `lib.rs` | `afp_motion.rs` `MotionFile::write`; `gyroflow-optical`, `measure.rs` (`in_camera_turn::fit`), `optical.rs` |
| Rust (Library) | `index.rs`, `changed.rs` (a change re-probes), `renders.rs` (its fingerprint is in a render's recipe: `motion`) | through `gyroflow-optical` jobs |

## Gyroflow projects and presets

- **A clip's project:** `<clip stem>.gyroflow`, then `<clip name>.gyroflow` (`A.gyroflow`, then `A.MP4.gyroflow`),
  beside the clip. Ctrl+e writes a new one at the first name. Gyroflow's own format; the player only reads it, except
  for that new one. (`workspace.lua` / `workspace.rs` `gyroflow_projects`, `gyroflow_project`, `gyroflow_new`.)
- **A moment's or clip preview's own project:** `<name>.gyroflow` beside its `.afplink.mp4` (a copy made with the
  moment, so a later change to the clip's project doesn't change it). `workspace/places.lua` `own_project`.
- **The default preset** for clips without a project: Gyroflow's own, then the first of `$GYROFLOW_DATA/default.gyroflow`
  and `<plugin folder>/default.gyroflow`. The plugin also ships `underwater.gyroflow`.
- A project's fingerprint goes into a render's recipe (`gyroflow`), so editing it makes the render stale. Without
  one, the clip's lens (`lens`: what the probe says it plays with) and its `.afp-motion`'s fingerprint (`motion`) go
  in instead, with the preset's (`preset`).

## Tags

One model on originals, moments and photos (never clip previews or exports): `pick` (`accept`/`reject`), `rating`
(1–5), `tags` (custom words, compared without case, first spelling kept). Design: [tags-and-culling.md](../design/tags-and-culling.md).

| Item | The library's own store (the truth) |
|---|---|
| Original | its own file: `pick`, `rating`, `tags`, `synced` |
| Moment | its link: `tags`, `pick`, `rating` (rewritten in place by `afp-cut --set-key`) |
| Photo | XMP `dc:subject` (custom tags); pick and rating in its `afp:link` |

**Other stores**, synced both ways when this computer's record says so (`tag_stores`): macOS Finder tags, xattr
`com.apple.metadata:_kMDItemUserTags`, a binary plist of strings (`"Name"` or `"Name\n<colour>"`). Accept is
`Accept\n2` (green), Reject `Reject\n6` (red), a rating `★★★`. The sync is three-way: the library now, the store now,
and what they last agreed on (`synced.<store>` in the item's record).

**Recent tags:** `<data>/ActionFootage/tags-recent.json`, `{ "recent": [...] }` (12 at most), shared by both apps.

| Language | Read / written |
|---|---|
| Lua | `workspace/tags.lua` (model, merge, Finder through `/usr/bin/xattr`); `workspace/tagging.lua` (`recent_file`, `remember_tags`); `side-panel/tagchips.lua` |
| Rust (catalog) | `finder.rs` `plist`, read/write of the xattr |
| Rust (Library) | `tags.rs` (`set`, sync, `holds_nothing`), `index.rs` `photo_tags`; `ui/culling.js` |

## Library lists (`.afplist.m3u8`)

**What:** what the Library showed when an item was double-clicked, handed to the player as its playlist. A snapshot.

**Path:** `<Library cache>/lists/<unix ms>[-n].afplist.m3u8`; lists older than a day are removed at the next write.

```
#EXTM3U
#PLAYLIST:Originals › Egypt
#AFP-START:17
#EXTINF:-1,Turtle at the wreck
/Volumes/ssd/…/_Moments/Turtle at the wreck.afplink.mp4
```

`#AFP-START` is the item opened, from 0. Paths with a newline, leading spaces or `#` are left out.

| Language | Read | Written |
|---|---|---|
| Lua | `workspace/librarylist.lua` `is_list`, `parse` (mpv reads the M3U itself) | — |
| Rust (Library) | — | `playing.rs` `list_m3u`, `write_list`, `open_list` |

## Batch requests (`AFP_BATCH`)

The Library runs the player without a window with `AFP_BATCH=<request.json>`. Per job, in
`$TMPDIR/ActionFootageLibrary-<Library pid>/<job id>/`: `request.json`, `status.json`, `stop` (its appearing stops
the run), `log.txt`; and `focus.json` beside the jobs (the folders the Library shows now, made first).

- Request: `{ kind, project, status, stop, focus, photo, sdr, only, layout, alone, profiles, items }`. `kind`: `clips`,
  `clips-again`, `moments`, `photos`, `export`, `render`, `share`.
- Status (written whole): `{ kind, state, done, total, failed, fraction, current, message, failures }`, `state` one of
  `starting looking running done stopped failed`.

Read/written by `workspace/batch.lua` (and `renders.lua` `render_moments`) and `library/src/batch.rs` (`start_job`,
`start_player`). Optical and lens jobs run `gyroflow-optical` with the same request file.

## Settings: script-opts and render-defaults

- **Plugin options:** `<mpv config>/script-opts/<plugin>.conf`: `gyroflow.conf`, `dlogm.conf`, `uwcolor.conf`,
  `highlights.conf`, `dji_telemetry.conf`, `sdr_check.conf`, `side_panel.conf`, `footage_menu.conf`, `agent.conf`.
  mpv's `key=value` form, read by `mp.options` (nothing trimmed, booleans only `yes`/`no`). The app's config folder is
  `<player data>/mpv/` (`macos/player.sh`, `windows/launcher`); the first time, these files are copied from the user's
  own mpv. `platform.lua` `save_options` rewrites keys in place and keeps the rest. Old VapourSynth keys in
  `gyroflow.conf` (`gpu`, `workers`, `max_fps`, …) are read and ignored.
- **`render-defaults.json`:** `$GYROFLOW_DATA/render-defaults.json`, written by the gyroflow plugin each start when it
  changed: `{ v: 1, defaults: {…}, edges, edges_blur, temporal_denoise, preset }` (`defaults.underwater`: gyroflow.conf's
  lens under water for a source that doesn't say, yes, no or auto). The Library reads it for a render's
  recipe (`renders.rs` `player_defaults`; `AFL_PLAYER_DEFAULTS` overrides), else works the defaults out of
  `gyroflow.conf` as the player would (`defaults_of`; shared cases `tests/gyroflow_conf_cases.json`).
- **`setup-offered`:** an empty marker in `<player data>/` once the setup was offered (`macos/player.sh`).

## LUTs and looks

- **Shipped:** the dlogm plugin's `luts/`: `<curve>_hlg.cube` per log curve (made by `tools/make_hdr_lut.py`; every
  export curve needs one), `3-NBCU_HLG2SDR_DL_v1.2.cube`, `5-NBCU_PQ2SDR_DL_v1.2.cube`. Shaders in `shaders/`
  (`dlogm_hlg.glsl`, `nbcu_hlg2sdr.glsl`, …). Dolby Vision looks in `dv/` (`nbcu_fit.json`, its `.rpu` files made by
  `tools/make_dv_rpu.sh`). See [luts.md](../design/luts.md).
- **Fetched at setup:** DJI's `dji-dlogm-rec709.cube` and the shader made from it, `shaders/dlogm_lut3d.glsl`, in
  `$DLOGM_DATA` (it can't be shipped). Files are looked for in `$DLOGM_DATA` first, then the plugin's folder.
- **The user's DV trims:** `$DLOGM_DATA/dv/` (else `~~/dlogm-dv`): the look as JSON and its RPU, two files used in turn.
- **Temporary:** gyroflow-export writes `.<stem>.uw.cube`, `.sdr.cube`, `.pq.cube`, `.nbcu.cube` beside its output
  and removes them when it ends.

## Lens lists

- **The user's curated list:** `curated_lenses.json` in the player's data folder (`$AFP_CURATED_LENSES` overrides),
  layered over the built-in `vs-gyroflow/resources/curated_lenses.json`. A broken one is left out with an error.
  Read by `lens_pick.rs` `user_file`; the Library's `index.rs` `curated_lenses_file`.
- **The player's own lens database:** `lenses/` beside it (`lens_db.rs` `user_dir`): Gyroflow lens JSON files.
- Both are stamped (`user_file_stamp`) into the probe's cache key, so a change re-probes.

## Per-computer caches

All can be deleted; they are rebuilt.

| Cache | Path (macOS) | Key / format | Owner |
|---|---|---|---|
| Motion data, probes, Library probes, GPU tables, translation, lens find, 360 hold | `~/Library/Caches/ActionFootagePlayer/<kind>/<hash>` (`motion`, `probe`, `library`, `gpu`, `translation`, `lensfind`, `reframe-hold`) | SHA-256 of path, size, mtime and a version (`lib.rs` `file_cache`) | vs-gyroflow binaries |
| HDR stills | `~/Library/Caches/ActionFootagePlayer/hdr-stills` | — | `live-photo/main.lua` |
| Project lookup | `~~cache/actionfootageplayer-projects.json` | `{ v: 2, folders }`; holds while folders, files and mounts are as they were | `workspace/projectcache.lua` |
| Moment scan, this computer's choices | `~~cache/actionfootageplayer-moments.json` | each link file's size, mtime, raw link; `found`; `moments_alone`, `clips_layout` | `workspace/momentlist.lua` |
| Catalog | `~/Library/Caches/ActionFootage/catalog/<project id>/`, `…/live/` | see [the catalog](#the-catalog-afpcatalog) | afp-catalog |
| Library | `~/Library/Caches/<Library app id>/`: `thumbs/`, `views/<hash>-<view>.json`, `lists/`, `maps/`, `probes.json`, `places.json` | — | `library/src/browsing.rs`, `playing.rs`, `settings.rs`, `albums.rs`, `geo.rs` |
| GPU path prefetch | `$TMPDIR/vs-gyroflow-prefetch-*.bin` | — | `prefetch.rs` |

The Library's probe answers are kept only while `index.rs` `GEOTAG` (11) is the same; raise it when what the probe
answers changes (#227). The Library page's `localStorage` keeps only this window's choices (`afl.*` keys: tab, side
panel, layout, hide rejected, the Share look, …).

## Responsiveness journal

**Path:** `~/Library/Logs/ActionFootagePlayer/journal/` (Windows `%LOCALAPPDATA%\ActionFootagePlayer\Logs\journal`,
Linux `$XDG_STATE_HOME/ActionFootagePlayer/journal`; `AFP_JOURNAL_DIR` overrides). One JSON-lines file per process:
`player-<when>-<pid>.jsonl`, `library-<when>-<pid>.jsonl`, and `handoff.json` (which Library action opened which
files). Records: `{ k: "act"|"step"|"show"|"io"|"run"|"load"|"cost"|"capped", w: ms, a, n, d, … }`. Capped per file
and folder (20 MB a file, 200 MB the folder), kept 45 days; `AFP_JOURNAL=0` turns it off. Nothing is sent anywhere. Written by `workspace/journal.lua` and `library/src/journal.rs`;
read by `tools/journal_report.py`. See [responsiveness-journal.md](../design/responsiveness-journal.md).

## Data folders on each system

Two names are in use: the player's `ActionFootagePlayer` and the shared `ActionFootage` (catalog, project records,
recent tags).

| What | macOS | Windows | Linux |
|---|---|---|---|
| Player data (`<player data>`) | `~/Library/Application Support/ActionFootagePlayer/` | `%LOCALAPPDATA%\ActionFootagePlayer\` | none: the plugins use `~~` |
| … mpv config (`~~`) | `<player data>/mpv/` (`script-opts/`) | `<player data>\mpv\` | the user's mpv |
| … `$GYROFLOW_DATA` | `<player data>/gyroflow/` (`default.gyroflow`, `render-defaults.json`) | same | — |
| … `$DLOGM_DATA` | `<player data>/dlogm/` (DJI LUT, shaders, `dv/`) | same | — |
| … `$AGENT_DATA` | `<player data>/agent/` (`reports/<…>/report.json`, `screen.png`, …; `session.json`, `mpv.sock`, `state.json`) | same | `~~/agent` |
| Lens list | `~/Library/Application Support/ActionFootagePlayer/curated_lenses.json`, `lenses/` | `%APPDATA%\ActionFootagePlayer\` | `~/.config/ActionFootagePlayer/` |
| Shared data (`<data>/ActionFootage`) | `~/Library/Application Support/ActionFootage/` (`projects/`, `tags-recent.json`, `computer.json`) | `%APPDATA%\ActionFootage\` | `~/.config/ActionFootage/` |
| Shared cache | `~/Library/Caches/ActionFootage/` (`catalog/`, `live/`) | `%LOCALAPPDATA%\ActionFootage\` | `~/.cache/ActionFootage/` |
| Player cache | `~/Library/Caches/ActionFootagePlayer/` | `%LOCALAPPDATA%\ActionFootagePlayer\cache\` | `~/.cache/ActionFootagePlayer/` |
| Library app | `~/Library/Application Support/<Library app id>/recent.json`; cache as above | Tauri's | Tauri's |
| Logs | `~/Library/Logs/ActionFootagePlayer/journal/` | see above | see above |

`recent.json` (the Library's projects opened) is a list of `{ path, share?, rel?, hosts?, at_start? }`. `AFP_HOME`
moves the catalog's data and cache to `<AFP_HOME>/data` and `<AFP_HOME>/cache` (tests, NAS Helper). Windows paths are
from the code; they were not checked on a Windows machine for this file.

## NAS Helper and stream

The container keeps its own in `/data` (read-write): `helper.json` (what each part is doing), `catalog/` (its
`AFP_HOME`: its name in the journals, its catalog caches), `stream/cache/` (renders, up to `AFP_CACHE_GB`),
`stream/args`. `/config/player/script-opts/` is the player's settings for the stream. The Jellyfin folder gets a
`.strm` (the stream's URL), `.nfo` and cover per item, listed in `.afp-jellyfin.json` (only those are ever removed)
and `.afp-jellyfin-posters.json`. Nothing is written beside the footage. Code: `tools/nas-helper/helper.py`,
`tools/stream/afp_stream.py`, `tools/stream/jellyfin.py`.

## Old forms still read

CLAUDE.md says the product has no dual-format reader. These remain in the code today:

| Old form | Read by | What happens |
|---|---|---|
| `<clip>.marks.json` (marks only, format `actionfootageplayer-marks`) | `sidecar.lua`, `workspace.lua`/`workspace.rs` `own_file`, `inspect.rs`, `index.rs`, `checks.rs` | read when there's no `.afp.json`; moved over at the first write. `tags.rs` `clip_own_in` ignores it. |
| `_ActionFootagePlayer/` project folder (up to 0.8) | `workspace.lua`, `projectfile.lua` | makes a project; `migrate` moves its settings into `Project.afproject` |
| `Project.afproject` `photo_tags` | `library/src/tags.rs` | taken into the photo at its next change |
| `view` holding input keys | `transform.lua` `migrate_source` | moved into `source` |
| `_Renders/<moment path>.<profile>.mov` (v1 recipe) | `renders.rs`, `workspace/renders.lua` | still counts as current while its recipe matches |
| `Generated/` at the project's top | `workspace/opening.lua` `moments_home`, `index.rs` | read where it is |

## Where the twins disagree

- `reframe.rs` `sidecar_path` and `lens_pick.rs` read the own file beside the clip only, not the main one over the
  locations (`own_file`). For a clip whose own file is beside another copy, the 360 recording and a clip's own lens
  are not found by the export and the probe.
- The render recipe's versions differ: the player's `RENDER_VERSION` is 1 (its path-named renders), `renders.rs`
  `VERSION` is 4 (hash-named renders).
- On Windows, `render-defaults.json` is under `%LOCALAPPDATA%` (`renders.rs`) while the lens list is under
  `%APPDATA%` (`lens_pick.rs`).
