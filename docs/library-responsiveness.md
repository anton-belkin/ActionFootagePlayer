# The Library answers at once

The Library's window used to wait for the disk after every change and then read whole tabs again. On a NAS (a stat
about 5 ms, thousands of clips) that is seconds to tens of seconds for one tag. Two things change that
(Anton, 2026-11-15):

1. **Screen first, disk after.** A change shows at once, is written in the background, and is taken back with a
   message when it couldn't be written.
2. **Pinpoint update.** A command says which items it changed, and the window corrects only those: no tab is read
   again.

This file says where the time went, how it's measured, the rules of the first, and how the second works.

## Where the time went

After any changing command the page ran `refresh()`: `project`, `scan moments`, `scan <the view shown>`, then drew
everything again. The catalog told the page of the same change too (`catalog-changed`), which started a second
`refresh()` beside the first. So one tag cost one write and **two readings of two views**.

What a reading costs, on a project of 10,000 clips (2,000 moments, 1,000 photos, 200 exports), on this computer's
disk and with a NAS's slowness stood in for (5 ms a stat, 5 ms a read, 15 ms a write):

| What the page asks for | This disk | As a NAS | Questions to the disk |
|---|---:|---:|---|
| `project` | 63 ms | 3.0 s | 467 folders listed (the counts of clip previews, moments, photos, exports) |
| `scan footage` | 0.43 s | 3.6 s | the same 467 (it asks for the project's settings), 15.9 MB sent to the page |
| `scan moments` | 1.4 s | 14.5 s | those, and for each of the 250 moments with a render its clip's own file found and read |
| `scan stills` | 0.11 s | 3.3 s | |
| `scan exports` | 0.10 s | 3.5 s | |

With 1,000 clips the same readings take 0.4 s, 0.5 s, 1.8 s, 0.4 s, 0.4 s as a NAS: they grow with the archive.

The commands themselves don't (the same on 1,000 and 10,000 clips):

| Command | This disk | As a NAS |
|---|---:|---:|
| `set_tags`, 1 clip / 20 / 200 | 8 ms / 0.1 s / 1.0 s | 0.2 s / 1.5 s / 14 s |
| `set_tags`, 1 moment / 20 moments / 20 photos | 2 / 6 / 5 ms | 0.17 / 0.85 / 0.79 s |
| `set_locations`, 1 clip / 20 / 200 (8 at a time) | 6 / 9 / 40 ms | 0.18 / 0.49 / 3.6 s |
| `set_source`, 1 clip / 20 / 200 | 8 / 13 / 77 ms | 0.2 / 1.2 / 10.4 s |
| `rename_item`, a moment / a folder | 7 ms | 0.26 / 0.29 s |
| `delete_items`, 1 moment / 20 | 6 / 8 ms | 0.18 / 0.83 s |
| `delete_renders`, 20 moments | 95 ms | 1.7 s |
| `new_folder` | 1 ms | 0.13 s |

(`set_tags` on clips finds the project's locations again for every clip, which runs `mount` each time: the 5 ms a
clip on this disk.)

And the window, from the click to the change showing in the list (the page run in jsdom, the app's answers taking
as long as measured above):

| Change, 10,000 clips | Before: shown after | | | Screen first: shown after | | |
|---|---:|---:|---:|---:|---:|---:|
| | page's own work | this disk | as a NAS | page's own work | this disk | as a NAS |
| A tag on 1 clip | 0.60 s | 2.2 s | 21.6 s | 0.11 s | 0.11 s | 0.11 s |
| A tag on 200 clips | 0.61 s | 3.2 s | 35.7 s | 0.10 s | 0.10 s | 0.19 s |
| A location for 20 clips | 0.59 s | 2.3 s | 22.0 s | 0.10 s | 0.10 s | 0.12 s |
| Source settings for 20 clips | 0.61 s | 2.2 s | 22.6 s | 0.10 s | 0.10 s | 0.13 s |
| A moment renamed | 0.52 s | 1.6 s | 18.0 s | 0.10 s | 0.09 s | 0.11 s |
| A moment deleted | 0.52 s | 1.6 s | 17.8 s | 0.16 s | 0.17 s | 0.31 s |

"Page's own work": the app answering at once, so what's left is the page taking in two views and drawing. With
screen first the readings still happen (one instead of two), in the background: the page is done after 0.4 s, 2.1 to
3.2 s, 22 to 36 s in the three columns, as before; that is what the pinpoint update removes.

## Measuring it again

- `library/src/perf.rs`: the project made on this computer's disk, its catalog read as the Library's thread reads it,
  every changing command and every reading timed twice (as the disk is; as a NAS). The slowness is
  `afp_catalog::store::nas`: each stat, read and write of the Library's and the catalog's waits. Off as shipped.

  ```
  cd library && AFL_PERF_DIR=/tmp/afl-perf cargo test --release -- --ignored where_the_time_goes --nocapture
  ```
  `AFL_PERF_CLIPS` (10000), `AFL_PERF_NAS` ("5,5,15"). It leaves `timings.json` and the views as the page gets them.
- `library/ui-tests/page_perf.js`: the page itself, in jsdom, on those views, the mock app answering as slowly as
  `timings.json` says.

  ```
  cd library/ui-tests && npm install && AFL_PERF_DIR=/tmp/afl-perf node page_perf.js <label>
  ```

Neither is the real window or the real NAS: jsdom's drawing is slower than the window's, a Mac caches a share's
listings for a while, and the tools that write a link (`afp-cut`, `afp-meta`) take time to start that isn't counted.

## Screen first, disk after: the rules

`library/ui/writes.js` is the mechanism, `library/ui/changes.js` ties it to the page and holds the changes that go
through it. Tags, locations (by hand, from Apple Photos, taken off), Source settings, renaming (a moment, a photo, an
export, a folder), deleting, and deleting renders.

- **Two things are kept apart**: a view as the app last read it from the disk (`CONFIRMED`), and what's shown, which
  is that with the changes still being written put over it, in the order they were made. Nothing is ever undone by
  hand: a change that failed is no longer put over, so what shows is the disk's state and the later changes.
- **Several quick changes to one item**: written one at a time in the order made (two commands on one clip's own
  file, or on the project's file, would otherwise write over each other); the last one wins because it's written
  last. An earlier one that fails takes back only itself.
- **A change while another is being written**: queued; the window never waits for the queue.
- **A change that can't be written** (the command fails): what it showed goes back, a strip under the top bar says
  which change, on what, and why ("Not saved: the tag “reef” on A.MP4, B.MP4. …"), and stays until OK is pressed.
  The views it touched are read again, in case it failed halfway.
- **Written only in part** (the command lists problems, without saying for which items): "Not all saved: …" with the
  problems; the change stays shown until the views are read again, then the disk decides item by item.
- **A change to something an earlier change made** (a moment renamed and tagged at once), when the earlier one
  fails: it can't be written either, isn't sent, and is said with it.
- **A change from the watcher** (the player, another computer) for an item with a change of ours waiting: ours stays
  on top until it lands, since it's put over whatever the disk says meanwhile; once written, the disk has both.
- **A reading that started before a change landed and arrives after it** would show the item as it was: the change
  stays put over every reading that started before it landed.
- **Closing the window with changes waiting**: the app keeps the window (the page tells it how many are waiting:
  `writes_pending`), the page says "Saving your last changes…", and the window closes once they're written. "Close
  now, without them" closes at once; so does closing a second time. With tasks queued the question about them comes
  first, and quitting then waits for the changes too.
- **Another project opened from the window**: waits for the changes. Opened from outside (a double click in Finder):
  the change being written goes on, the ones not sent yet can't be (they were the other project's) and are said so.
- **The sign that changes are waiting**: Tasks' count, and "Saving changes · N to go" in Tasks, once a change has
  waited over half a second. A quick one shows nothing.

What a patch must be: the command's own rule (what it does to the item on disk), and doing it twice must be as doing
it once, since it's put over fresh readings again and again.

Not through it (they wait for the app and read the views again as before): a new folder (its name is the app's),
Take in, Project settings (locations), Find moved files and Mend, the lens chosen for Stabilize optically (the task
needs it on the disk before it starts), and the end of a task.

## A command joins with a few lines

In `changes.js` (or any file loaded after `writes.js`):

```js
Writes.define("rating", {
  what: (a, op) => "the rating of " + namesText(op.targets.map((t) => t.name)),   // for "Not saved: …"
  item(it, a) { it.rating = a.rating; },          // the item as the views have it, changed as the command changes it
  send: (a) => ["set_rating", { items: a.items, rating: a.rating }],
  problems: (r) => r,                             // what the command says it couldn't do
});
```

and where the command was called:

```js
Writes.run("rating", items, { items: items.map((it) => ({ kind: it.kind, id: it.id, path: it.path })), rating: 3 });
```

`items` are the items as shown (`kind`, `id`, `name`); `Writes.run` gives a promise of the change once it's settled
(`ok`, `answer`). A change that isn't one item's field (a rename, a delete) gives `patch(draft, args)` instead of
`item`: see `rename` and `delete` in `changes.js`.

## The pinpoint update

A changing command answers with what it changed (`library/src/changed.rs`, `Changed`): the items as the views have
them now, each exactly as a reading of the whole view would give it (`index::scan_only`, the same code with only
those items looked up), the ids that are gone, and in words what couldn't be done. The page takes those items as
what the disk has, whatever it had shown, and makes only them again in what's shown. It reads no view.

- **The app's own commands**: `set_tags`, `set_locations`, `set_source`, `rename_item`, `delete_items`,
  `delete_renders`.
- **What the catalog says changed elsewhere** (the player, another computer, a task's files as they land): the
  catalog's thread keeps which files changed at each of the page's counts of changes (`seen.rs`, `since`), and
  `changes_since` turns them into items the same way (`changed::of_files`: a clip's own file is its clip's, a render
  its moment's, a Live Photo's video its photo's, a clip preview its clip's). The page asks for that instead of
  reading its views when it's told `catalog-changed`.
- **When the app can't say which items** it says so and the page reads the views again, as before: the project's
  file changed (its locations), a folder was made, renamed or removed, a clip came or went, more than a few hundred
  items at once, a first look at a location, or no catalog. A folder renamed or deleted from the window reads the
  views once too (what's in it shows at once all the same).
- **Written in part**: every item shows as it is (the command gives all of them, written or not), the strip says
  what couldn't be done, and nothing is read.
- **Filters and the search**: an item they no longer take leaves the list at once, as in Lightroom and Capture One,
  and when everything selected left, the item that took its place is selected, so the next one is under the hand.
  (A filter on a value nothing has any more stops filtering, as before.) A renamed item goes where its name puts it
  among its folder's, as a reading would have it.
- **What's on screen** (the list, the side panel, the filters' counts, the folders' pictures) is drawn again from
  memory, as on every click; only the data is corrected item by item.

The window, 10,000 clips, from the click (the page in jsdom, the mock app answering as slowly as measured, each
command now also taking the time to say what it changed):

| Change | Shown after | | | Page done after | | |
|---|---:|---:|---:|---:|---:|---:|
| | before | screen first | pinpoint | before | screen first | pinpoint |
| A tag on 1 clip, as a NAS | 21.6 s | 0.11 s | 0.03 s | 21.8 s | 21.8 s | 0.45 s |
| A tag on 200 clips, as a NAS | 35.7 s | 0.19 s | 0.04 s | 35.9 s | 35.8 s | 16.3 s |
| A location for 20 clips, as a NAS | 22.0 s | 0.12 s | 0.03 s | 22.2 s | 22.1 s | 0.71 s |
| Source settings for 20 clips, as a NAS | 22.6 s | 0.13 s | 0.04 s | 22.8 s | 22.7 s | 1.45 s |
| A moment renamed, as a NAS | 18.0 s | 0.11 s | 0.02 s | 21.7 s | 21.8 s | 0.44 s |
| A moment deleted, as a NAS | 17.8 s | 0.31 s | 0.02 s | 21.6 s | 21.8 s | 0.39 s |
| A tag on 1 clip, this disk | 2.2 s | 0.11 s | 0.03 s | 2.4 s | 2.2 s | 0.12 s |
| A tag on 1 clip, the page's own work | 0.60 s | 0.11 s | 0.04 s | 0.61 s | 0.40 s | 0.07 s |

"Page done" is now the write itself (the 16.3 s are 200 clips' own files written one after another on a NAS, in the
background) and one small question, `changes_since`. Asked of the app for a tag: `set_tags`, `changes_since`; no
`project`, no `scan`.

Saying what changed costs the command 40 ms on this disk (90 ms as a NAS: six small reads) for clips at 10,000
clips, 11 ms at 1,000: it still looks through the project's clip previews for the clip's own (in memory, from the
catalog), which grows with the archive; for moments, photos and exports it's 9 ms at either size. It's after the
write, in the background.

That it ends where a reading ends is tested both ways:
- `changed.rs`'s test: on the measured project (smaller), after every command the items it says are, field for
  field, the items of the whole view read again; the ones it says gone aren't there.
- `library/ui-tests/pinpoint_test.js`: a sequence of changes (tags on and off clips, moments, a photo, an export;
  a location set and taken off; Source settings; a moment and a photo renamed; a moment and a clip's preview deleted;
  a moment the player made, another computer's tag and marks) on two pages, one corrected item by item, the other
  reading every view again after each step: after every step the views as shown, what the disk has, the clips' counts
  of moments, and the list, side panel and filters drawn at nine places are the same.

A command joins in Rust with its last line: `changed::of_items(&root, &items, problems).with_probes(&probes)` (or
`of_clips`). One that doesn't yet (it answers with its list of problems) still works through `writes.js`: the page
then reads the views again only when it lists problems.

## Albums (2026-11-17)

Every album action now goes the same way: shown at once, written after, the command saying what it made.

| Action | Change (`writes.js` kind, in `albums.js`) | Command | Answers with |
|---|---|---|---|
| Add to an album (Originals, an album, Timeline, a Map place, a media type) | `albumadd` | `album_add` | the new links (`links`), the clips they're of |
| …a moment or a photo link (a copy of its link) | `albumcopy` | `album_copy` | the new moments / photos |
| Move to an album; Take out of the album (moments, photos to Generated) | `albummove` | `album_move` | the links moved (`links_gone` + `links`), the moments / photos gone and made |
| Photos of the marks, Make a photo of the mark | `photoadd` | `photo_add` | the new photo links |
| New album | `newfolder` | `new_folder` (now takes the name the window shows) | its path |
| An album renamed (one to three folders: `_Clips/X`, `_Moments/X`, `_Stills/X`) | `renamealbum` | `rename_album` (new) | the folders' new paths; the views read again, as for a folder |
| Taking a clip's link out, deleting an album | `delete` | `delete_items` | as before |

- **What shows before the answer** is worked out by the command's own rules (`src/albums.rs`): a name taken in the
  album gets " 2", " 3"…; a clip already in the album, a photo of a time already there, is left (the command's
  "already": two adds of the same clip are one link); the album appears among My albums. A clip's link shows with the
  picture the page has of the clip where it was selected, a copy with its file's picture, a photo link with its clip's
  picture: each until the command answers, then its own (the link's thumbnail, the frame). Such an item is drawn faded
  with a small pulsing dot, its tooltip "Being written…".
- **The answer** (`changed.rs` `of_album`): the links as the Footage scan lists them (`links`, `links_gone`), the
  moments and photos as their views' items, the clips whose links changed (Originals' picture is a clip's first link).
  The page puts them into what the disk has (`changes.js` `linksInto`, `mergedInto`) and draws from memory: no `scan`,
  no `project`, no `refresh()`.
- **The catalog's notice of the command's own files** (it's told at once, so the views read the new links) no longer
  makes the page read Footage again: `seen::ours` keeps the files the window's own album commands wrote (a minute at
  most, each once), and `changes_since` leaves them out (`seen::not_ours`). Before, the album's folder was among them,
  which made `changes_since` give up and the page read every view. The folder is told only when the command made it.
- **An album of three folders renamed** is one change: `rename_album` renames each, and when one can't be, names back
  the ones done and fails; the window then shows the album as it was, whole (before: three writes, a failure in the
  second left two albums on screen until the views were read).
- **An item still being written** can be selected, tagged, culled, and added to another album: those are changes too,
  queued behind its write and taken back with it when it fails ("it follows a change that wasn't saved"). Moving it,
  taking it out, renaming, deleting it waits: refused with "… is still being written: in a moment" (they need its
  file, and its name is only the page's guess until the answer). Share isn't guarded (`share.js` is another PR's).
- **Closing the window** waits for album writes as for the others (they're `Writes` changes: `writes_pending`).
- **Where it shows**: an album, My albums, the media-type albums, Timeline, a Map place's list and Originals'
  "In an album" are all made from the same lists (`RAW_FOOTAGE.links`, `DATA.moments`, `DATA.stills`), so a change
  put over them shows in all.

Not changed: renaming an album still reads its views once after it's written (in the background, as a folder
renamed does: what's in it shows at once all the same). If another computer takes a name between the page's guess and
the write, the item lands under the command's name: the pending one is replaced by it (a selection on it is lost).

Pictures: `docs/img/albums-screen-first/` (headless Chrome against the mock, album commands slowed to 4 s; before: the
sheet waits with its button greyed; after: the album open 1 s after the click, the two items faded and dotted).

Tests: `library/ui-tests/albums_writes_test.js` (11: add, add failing, add twice and a name taken, copy with a tag
queued, a copy failing with its tag, move, take out, photo links, three-folder rename failing then whole, new album,
closing waits); `changed.rs` `album_commands_say_what_a_reading_shows` (each command's answer is what a reading of the
views shows after it, links included; the window's own files; the rename named back).
