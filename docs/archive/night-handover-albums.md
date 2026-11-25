# Albums, step 1: handover (2026-11-16)

**Archived:** night handover for albums step 1 (clip links, Footage → Albums), merged as #235 via #241 (v0.14.0). Written 2026-11-16.

Branch `claude/night-albums-3c6687`, on `origin/main` 18ee53e. What was built: a clip in an album is a light link the
Library writes itself; the Footage tab is Albums. This note is for the sessions that build on it (photos as links,
moments and photos inside albums, Timeline, Map, Share, the render cache): `docs/design/albums-photos-share-concept.md` is
the design.

## The link's file

`<name>.afplink.mp4`, written by `library/src/albums.rs` (`link_file`), about 20 KB:

- `ftyp` (`qt  `), `mdat` holding one JPEG as it is, `moov` with one video track of one Photo JPEG sample (`jpeg`
  sample entry, held one second) and `moov/meta` (hdlr `mdta`, `keys`, `ilst`) with the one key
  `ch.actionfootageplayer.link`.
- The link: `{ "v": 1, "kind": "clip", "clip": <path in the tree>, "size": <bytes>, "roots": [<project folder>],
  "created": <ISO time, when the probe knew it> }`: the fields a clip preview's link had, plus `created`.

Why this form:
- **Nothing changes in how links are found.** The name, the key and its place are those of every clip preview and
  moment, so the player (`mpv/scripts/workspace/link.lua`, which redirects to the original before mpv opens the
  file), the catalog (`catalog/src/inspect.rs`) and the Library (`index::link`) read it as they are. Checked: the
  Library's reader, the catalog's reader, `link.lua` under luajit, ffprobe.
- **One picture, nothing else.** The thumbnail is the file's only frame, so Finder and Quick Look draw it (checked
  with `qlmanage -t`) and no cover art is kept beside it. `albums::picture` reads that frame back for the cards
  (`index::thumb_bytes` asks it for an `.afplink.mp4` without cover art).
- **No tool run.** The file is built in memory and written once, so it needs neither ffmpeg nor the player.

Not `draft`: the player's "finish drafts" batch leaves these alone. A flip-book preview made before is the same kind
of link with an H.264 track and cover art; both show as album items.

## The thumbnail

`albums::thumbnail`: the camera's `.THM` as it is, else `index::thumb_bytes` of the camera's low-resolution file or
of the clip (the picture the cards show: the NAS's own, Quick Look's `afp-thumbs`, Explorer's cache, ffmpeg's first
keyframe; kept in the Library's thumbs cache, so a clip seen in the window costs nothing more). No picture at all (an
offline clip never shown): a dark frame.

Colour: when the clip's curve is known, the JPEG goes through a LUT (`albums::recolour`, crates `zune-jpeg` and
`jpeg-encoder`, MIT/Apache). The curve is what the page already has: `source.curve` of the clip's own file, else the
probe's `color` when it's a log curve (`clipCurve` in `albums.js`). The LUT (`albums::lut_for`) is the player's:
`dji-dlogm-rec709.cube` for D-Log M when installed, else `<curve>_hlg.cube` then `3-NBCU_HLG2SDR_DL_v1.2.cube` as
one, as `gpu_lut::compose_nbcu` does. Found in the player's `plugins/dlogm/luts` (beside the probe tool), or
`AFL_LUTS`. No LUT or no curve: the thumbnail as it is. No clip is probed for it.

## Where albums live

`<project>/_Clips/`: its folders are the user's albums, subfolders are albums in albums. `_Clips` is not renamed
(the concept: "shown as albums, as they are: nothing moves"), so there is no migration script. A link anywhere else in
the project's folder (`Generated/…`) shows under **Other folders**, as before.

## What the Albums view is built from

Rust:
- `index::footage` (the `footage` scan) now also returns `links`: every clip link of the project's folder,
  `{ path, rel, clip, draft, size, mtime }` (`albums::clip_links`). `clips_dirs` (the folders with links, and empty
  folders) is as before. `items[].preview` / `pdir` (the first link of a clip) are still there; the page uses
  `preview` only for a card's picture in Originals.
- `albums::album_add { album, clips: [{ tree, path, thumb, size, created, curve }] }` →
  `{ made: [paths], already, album, failed: [[clip, why]], ms }`; `albums::album_move { album, files }` →
  `{ moved, album, problems }`. `album` is a path in the project's folder (`_Clips/Egypt/Dives`). Both tell the
  catalog (`seen::note_now`). Removing is `delete_items` with `kind: "preview"`, renaming an album is `rename_item`
  on a folder, a new album is `new_folder` with view `footage`: all as before.

Page (`library/ui/albums.js`, loaded before `app.js`; `app.js` calls it at lines marked `albums.js`):
- The view's id is still `footage` (`S.view`, `DATA.footage`, `VIEW_ROOT.footage = "_Clips"`); only its label is
  Albums. `arrange()` in `app.js` sets `DATA.footage = albumItems()`.
- `ALBUM_SOURCES`: functions, each returning items with `kind`, `key`, `rdir` (the folder of the item's file, in the
  project's folder) and whatever the cards need. Today one, `albumClips()`: a clip item copied per link, with
  `key: "a:" + rel`, `link_file`, `preview` = the link. **A new kind of item** (moments, photo links) is another
  function pushed here; its items then lie in the same folders, and `albumFacets` shows the filter by kind
  (`ALBUM_KINDS`) once an album holds two kinds. Selection, `ctx()`, the menu and the side panel take the items by
  their `kind`, as the other views do.
- `ALBUM_VIEWS` is an empty list reserved for **views inside Albums that aren't folders** (Timeline, Map): nothing
  reads it yet. *(Now: Timeline and Map push themselves into `ALBUM_VIEWS`, #239)* The place to draw them is `renderMain()` in `app.js`, where the view's top (`S.path` empty) lists
  `childDirs("")`.
- Add and move: `askAlbum(mode)`, `renderAlbumSheet()` (`S.sheet === "album"`, state in `S.album`), `albumGo()`;
  `albumMenu` (the context menu) and `albumBar` (the bar above the list).
- Originals' items carry `albums` (how many links point at the clip): a chip and the filter "In an album".
- "Go to the footage" and the clip's own view (`goFootage`, `buildClip`) now use Originals, where every clip is.
- The app opens on Originals (`S.view: "sources"`): Albums is empty until something is put in one.

## Tests and screenshots

- `cd library && cargo test` (42 pass). `albums::tests`: the file read back by the Library's and the catalog's
  readers, colour through a LUT, add / add again / move, the scan's `links`.
- Timing: `AFL_ALBUM_DIR=<folder of clips> AFL_THUMBS_HELPER=<built afp-thumbs> AFL_LUTS=$PWD/../luts
  AFL_ALBUM_CURVE=gplog cargo test links_timing -- --ignored --nocapture`.
- The page: `JSDOM=<node_modules/jsdom> node library/tests/albums.js library/ui` (41 checks) against
  `library/tests/mock.js`.
- Screenshots, headless Chrome, no window: `PUPPETEER=<node_modules/puppeteer-core> node library/tests/shots.js
  library/ui <out> [dark]`. For "before" pictures, point it at a checkout of the older `library/ui`.
- Windows: `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` compiles.

## Measured

300 local clips (10 GoPro samples cloned 30 times, APFS), debug build, 8 threads, each thumbnail through the GP-Log
LUT: 4.5 s with no thumbnail kept (15 ms a clip), 2.6 s with the thumbnails kept (8.6 ms a clip); 20.7 KB a link.

## Left unfinished, not verified

- Albums hold clip links only. Moments and photos still have their tabs; Exports too.
- `Generated` is under Other folders, not beside the albums at the top. *(Now: moments, photos and Generated came in #238)*
- Whole albums aren't moved into other albums from the Library (items are); Finder does it.
- The player still has its preview batch (`make_clips`, drafts, `clips-finish`; the Library's `add_job` still takes
  `clips`): unreachable from the Library's UI, not removed. `X` now only takes a clip out of its album. *(Now: `X` is Reject, #233; taking out is the binding `workspace/not-interesting`, with no key)*
- Not seen: the app's real window; a project on the NAS (speed over SMB, the catalog picking the links up); a real
  D-Log M clip's thumbnail after the LUT (the pipeline is tested with a synthetic LUT and run with the real GP-Log
  one); Explorer's thumbnail of a link on Windows (the Library itself reads the frame on every system).
