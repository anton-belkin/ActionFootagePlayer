# The Library answers at once

The Library's window used to wait for the disk after every change and then read whole tabs again. On a NAS (a stat
about 5 ms, thousands of clips) that is seconds to tens of seconds for one tag. Two things change that
(Anton, 2026-11-15):

1. **Screen first, disk after.** A change shows at once, is written in the background, and is taken back with a
   message when it couldn't be written.
2. **Pinpoint update.** A command says which items it changed, and the window corrects only those: no tab is read
   again.

This file says where the time went, how it's measured, and the rules of the first. The second has its own part at
the end once it's in.

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

Not through it (they read the views again as before): a new folder (its name is the app's), Take in, Project
settings (locations), Find moved files and Mend, the lens chosen for Stabilize optically (the task needs it on the
disk before it starts), and what tasks make.

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
