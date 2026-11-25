# The catalog against the files: requirements, holes and safeguards

**Status:** built in #250 (v0.14.0, via integration #252); the Helper's short-break rule in #284 (untagged, after v0.14.0); parts not built (by decision): scheduled content re-read, ctime, tolerances, per-read distrust.

Review of 2026-11-17, after the second "a field vanished from the views" incident, then the safeguards agreed with
Anton the same day, built. Evidence: the code (references below), `catalog/tests/safeguards.rs` (`cargo test --test
safeguards` in `catalog/`) and two tests in `library/src/seen.rs`.

## Summary

1. **The construction is sound.** Journals, snapshots and versions do what they promise: nothing tested loses or mixes
   up records, and a damaged file is never used. What could go wrong was narrower: a record saying something old about
   a file that nothing reads again, because the catalog takes "same size + same time" for "same insides".
2. **Requirement (Anton):** every legitimate change to the library's files is taken in — by our programs (noted at
   once) and equally by Finder or a script (moves, renames, deletions, rewritten own files, links). Changes that keep
   size *and* time (a restore that keeps times) are out of scope for the looks: a check on demand covers them.
3. **Built A — one precision of times (100 ns)**, cut where times are read and where records are written. It ends the
   NAS/Mac ping-pong (ns vs 100 ns) that re-read and re-noted every file for ever: the door of both incidents.
4. **Built — looked after within a day.** Each computer says which roots it looked at deep (`.afp/catalog/care/`); the
   NAS Helper says it watches. At its start and every hour the Library looks deep only at roots nobody looked after in
   the last day. With the Helper running, the Library never looks deep; the Helper looks deep only at its start, and
   not after a short break (2026-11-24, below).
5. **Built — a broken own file keeps its record.** A clip's `.afp.json` that isn't JSON (a script's bug, a half-write)
   used to be read as "no tags" and wiped them from every view. Now its record stays, it's listed in Checks, and it's
   taken in as soon as it reads right.
6. **Built — what the Library couldn't note is looked at again**, from the files as they are then (never replayed), soon
   and then less often. **Built — on demand in Checks:** "Look at every folder again" and "Read every file again".
7. **Not built, by decision:** a scheduled content re-read (daily disk load for a case out of scope), ctime, tolerances,
   per-read distrust in the Library (#220).
8. **Not verified:** the 100 ns figure on the real NAS (no NAS access in this work), and the Library's new Checks block
   in the app (logic tested; the page not looked at).

## The requirement

Scenarios the catalog must take in, by who makes the change:

| Change | By our programs | By Finder or a script |
|---|---|---|
| Move, rename, delete (a file, a folder, a whole tree) | noted at once | seen by the next look that follows folders' times |
| Rewrite a clip's own file or a link | noted at once | written through a temporary file: as above; **in place** (the folder's time stays): see below |
| Write it wrong (not JSON) | — | the old record kept, said in Checks, taken in once right |
| Restore keeping size and time | — | **out of scope for the looks**: "Read every file again" |

How soon, where nothing of ours made the change:
- the project's own folder: within 5 minutes (the Library's regular look), in place within a day;
- a location the NAS Helper watches: within seconds (inotify), in place too;
- a location nothing watches (a USB disk, no Helper): moves, renames, deletions at the Library's next look of
  everything (its start, Mend, Take in, a batch); in place **within a day** (the Library's look after);
- anywhere, at once: Checks' "Look at every folder again" (`afp-catalog look <project> --deep`), what a script should
  end with.

## How the catalog decides a file is unchanged

*How the catalog works now, in short: [../reference/architecture.md](../reference/architecture.md); its files:
[../reference/files-and-formats.md](../reference/files-and-formats.md).*

- A look lists a folder only when its time, or the digest of what's on record in it, differs from the folder's record
  (`catalog.rs` `sweep`). The digest hashes each child's name, size and time (`state.rs` `digest`).
- In a listed folder, a file is read again only when its size or time differs from its record. Otherwise its record is
  kept as it is.
- A deep look (`Sweep::deep`) lists every folder, whatever its time; files are still read only when size or time
  differ. A check (`Sweep::reread`, new) reads every file of ours and writes only where the insides differ.
- A folder's time changes only when something in it is made, removed or renamed — not when a file in it is written in
  place. Our writers write a temporary file and rename it (and `note_now` touches the folder), so our writes always
  change it; an editor's or a script's `open('w')` doesn't. Hence "in place" above.

Times are compared exactly, and since A always in whole 100 ns (`store::canon`), on every computer.

## Who looks where (after the change)

| Who | What | When | How |
|---|---|---|---|
| Library | project's own folder | every 5 min | folders' times (`seen.rs`, `project::own_roots`) |
| Library | every root nobody looked after in a day | at its start, then every hour | deep (`care::look_after`), slowly |
| Library | everything | first look, after Mend, Take in, a batch | folders' times |
| Library | folders this Mac changed | ~1 s after FSEvents | listing |
| Library | what it couldn't note | 5 s, doubling to 5 min | listing, files read as they are then |
| Library | on demand (Checks) | at once | deep, or every file read again |
| NAS Helper | its roots | at its start, unless a short break (watched until <15 min ago and a deep look within a day); after the system lost count; after being held up 15 min | deep (`cli.rs` `watch`, `care::to_look_at_start`) |
| NAS Helper | folders inotify names | ~1 s after a change | listing |
| NAS Helper | everything | every 30 min | folders' times |
| Player | its clip's moments | as the clip opens | stat each, read again when size/time differ |

**Who looked after which root** is in `.afp/catalog/care/<computer>/` (`catalog/src/care.rs`):
- `<root key>.deep`: a line appended at each deep look of that root, by whoever made it (Library, Helper, `afp-catalog
  look --deep`, `afp-catalog check`).
- `<root key>.watch`: the Helper's, made new right after its deep look at its start, a line every 5 minutes while it
  runs, removed when it stops on purpose.
- A root is **looked after** when some computer's `.deep` is younger than a day, or some `.watch` is younger than 15
  minutes. Ages are measured against a file made on the share just then: the share's clock only, never one computer's
  against another's.
- A root's key is a hash of its id (its location's path as the project's file has it), the same on every computer.

A short break of the Helper (its container restarted, the NAS rebooted quickly: its last `.watch` line younger than 15
minutes) when some computer looked at the roots deep within a day: no folder is listed in full at its start, only the
look at folders' times, which sees files added, moved, renamed or deleted meanwhile; its `.watch` goes on. What a
script changed in place during that break is the one thing not seen (Anton, 2026-11-24: a restart shouldn't list
everything again). A long break, or no deep look for a day: deep at its start as before. Tests:
`a_short_break_lists_no_folder_in_full`, `a_short_break_after_a_day_without_a_deep_look_looks_deep`.

So: the Helper running, the Library never looks deep. The Helper stopped, within 15 minutes the Library takes over
(at its next hourly turn). Two Macs, no Helper: whichever looks first that day spares the other.

## The holes found, and what closes each

### 1. The same file's time seen with two precisions (was active) — closed by A

Times were kept and compared in nanoseconds, exactly. On the real NAS a Mac over SMB sees `…540500` where the NAS sees
`…540523` (found in PR #249's work): SMB carries 100 ns units. So each computer saw the other's records as changed:

- **the project's folder:** a ping-pong at every NAS 30-minute look and every Library 5-minute look, every moment read
  again and every record rewritten (estimated ~3 minutes of SMB traffic and ~20 000 journal records per half hour at
  10 000 moments);
- **a clip's own file in a location:** the Library wrote and noted it, the NAS's watcher re-read and re-noted it only
  for the extra digits. This was the channel of both incidents: a reader that kept less rewrote a record it had no
  reason to touch, and won by being newer;
- **the player:** re-noted at each opening the moments the NAS had noted last.

**A:** every time is cut to whole 100 ns where it's read (`DiskStore`'s `stat` and `list`, `store::canon`), where a look
compares (`sweep`: the folder's stat and every listed item, whatever the store) and where a record is written
(`Catalog::note`, whoever made it). Still exact equality, and it holds for the folders' digests — which a tolerance
couldn't. Records the NAS wrote in ns differ once more after the update and are read once, then everything settles.
Tests: `two_precisions_settle_as_one`, `the_nas_leaves_alone_a_file_the_mac_just_noted`,
`an_older_reader_never_rewrites_what_the_mac_noted`.

### 2. A change that keeps size and time — out of scope, on demand

A restore or copy that keeps times (Time Machine, `cp -p`, `rsync -t`), `touch -r`. No look reads such a file, not
even the deep one. **"Read every file again"** in Checks (`afp-catalog check <project>`) reads every file of ours,
corrects where the insides differ, says which (`Report::corrected`), and writes nothing where the catalog was right.
Test: `the_check_against_the_files_corrects_a_restore_that_kept_times`.

### 3. A change in place that nobody hears of — closed: within a day

Before: the Library's looks were of the project's folder only, the Helper never looked deep, and no program looked
again at the locations' own files. A script's in-place edit with the Helper down (or no Helper) stayed unseen until
that clip was edited again; a wrong record stayed too.

Now: the looks after (above), the Helper's deep look at its start, and "Look at every folder again". Tests:
`a_script_editing_in_place_is_seen_within_a_day_or_at_once_on_demand`, `one_computers_deep_look_spares_the_others`,
`a_watching_helper_spares_the_library_and_a_stopped_one_doesnt`,
`the_helper_at_its_start_sees_what_changed_while_it_was_down`.

### 4. A clip's own file that isn't JSON — closed

`inspect::read` read such a file as "nothing of ours" (`Ok(None)`): one look while a script wrote it, or a script's
bug, wiped the clip's tags, Accept / Reject and marks from every view, for good. Now it's an error of its own kind
(`InvalidData`): the look keeps the record as it was, says the file (`Report::unreadable`), and lists its folder
again at every look until it reads right (in place too). The Library lists such files in Checks and reads them again
every 5 minutes, noting each once right. Tests: `a_broken_own_file_keeps_its_record_until_it_reads_right`,
`a_sidecar_that_isnt_json_is_an_error_on_the_real_disk`, `seen::tests::a_broken_own_file_is_kept_said_and_taken_once_right`.

Not covered the same way: a moment or photo caught half-written by a foreign writer reads as "no link" until its size
or time changes again (which it does when the writer finishes). Our writers rename whole files.

### 5. A note that failed — looked at again

`seen::note_now` ignored a failed write (`let _ =`). It was already mostly harmless (it touches the folder first, so
a later look lists it), but in a location without a watcher only the next look of everything would. Now what it
couldn't write or read goes to a list the Library's thread looks at again — by **looking**, i.e. reading the files as
they are then, never replaying what was seen before. So a retry can't write something old over something new, can't
ping-pong (nobody writes in reply to another's write, only on a difference from the file), and repeats harmlessly. 5 s,
then doubling to 5 minutes while the share is away; a newer program's project (`TooNew`) is never retried. Test:
`seen::tests::a_note_that_couldnt_be_written_is_kept_to_look_at_again`.

### 6. A coarse disk as a location — bounded by a day

On exFAT (10 ms) or FAT (2 s), a file added in the same tick as the folder's last listing leaves the folder's time as
recorded, and only a deep look lists it: now within a day. Cards are taken in rather than kept as locations, so this
is unlikely. Test: `a_coarse_folder_time_hides_a_new_file_until_a_deep_look`.

### 7. Clocks, journals, folds — nothing found

No rule uses one computer's clock against another's (the looks after measure against the share's own clock). Torn or
damaged journal lines are left out, folds cover each journal up to an offset, tombstones are dropped only below every
computer's highest version; the random test (300 seeds, failing operations, stale listings) ends with every catalog
equal to the files.

### 8. The version gate — holds for a catalog already open

`load`, `note`, `compact` and `clean` each check `Project.afproject` (`catalog.rs` `usable`), so an `afp-catalog
watch` already running stops the moment the project is raised. Test: `the_version_gate_holds_for_a_catalog_already_open`.
It can't stop programs from before the gate; `own_kept` carries fields it doesn't know through, and the test in
`inspect.rs` ties what's kept to `project::VERSION`. The care files are new and need no raise: older programs ignore
them.

## What's in Checks

Under the selection's checks, a block for the whole project ("The catalog"): **Look at every folder again** (after a
script or Finder changed files while nothing watched; the other computers are spared for a day) and **Read every file
again** (after a restore that kept times: says how many records it corrected, or that the catalog was right), and
the clips' own files that aren't JSON, as last seen. Commands `catalog_look`, `catalog_care` (`main.rs`); the page part
`catalogBody` (`ui/app.js`).

## What I would not do

- **Distrust the catalog per read in the Library** (stat each file shown): ~5 ms each over SMB, the slowness the
  catalog was built to remove; closed once already (#220).
- **Re-read contents on a schedule:** daily load on the disks for a case Anton put out of scope; the check is on demand.
- **Hash whole files:** gigabytes read for a few hundred bytes of insides.
- **Tolerances in comparisons:** they don't reach the digests; one precision does the job exactly.
- **ctime as a "changed?" signal:** another precision to agree on, bumped by Finder's tags, absent on FAT.
- **Deep looks by a running Helper on a schedule:** inotify tells it of every change while it runs.
- **Clocks, locks, a central database, or deleting `.afp/catalog` to rebuild:** each brings back what §9 avoided.
  "Read every file again" is the rebuild.

## Tests

`catalog/tests/safeguards.rs` (simulated share, two computers):

| Test | Holds |
|---|---|
| `two_precisions_settle_as_one` | A: ns vs 100 ns settle; nothing read or written after the first looks |
| `the_nas_leaves_alone_a_file_the_mac_just_noted` | A: the Helper doesn't re-read what the Library noted |
| `an_older_reader_never_rewrites_what_the_mac_noted` | A: the 2026-11-17 mechanism can't happen |
| `finder_moves_renames_and_deletes_are_seen_where_the_nas_watches` | moves (with the own file, tags kept), renames, deletions, a folder moved |
| `finder_changes_are_seen_by_the_librarys_look_without_a_watcher` | the same, no watcher |
| `a_script_editing_in_place_is_seen_within_a_day_or_at_once_on_demand` | in place: not by folders' times; within a day; at once on demand |
| `one_computers_deep_look_spares_the_others` | one deep look a day between computers |
| `a_watching_helper_spares_the_library_and_a_stopped_one_doesnt` | two days watched, no Library deep look; stopped: it looks |
| `the_helper_at_its_start_sees_what_changed_while_it_was_down` | the Helper's start |
| `a_broken_own_file_keeps_its_record_until_it_reads_right` | not JSON: kept, said, taken in once right, settles |
| `a_moment_deleted_and_put_back_in_finder` | deletion and put back (#recycle) |
| `the_check_against_the_files_corrects_a_restore_that_kept_times` | out of scope for looks; the check corrects and then writes nothing |
| `a_coarse_folder_time_hides_a_new_file_until_a_deep_look` | the coarse-disk limit |
| `the_version_gate_holds_for_a_catalog_already_open` | the gate, on a real folder |
| `a_sidecar_that_isnt_json_is_an_error_on_the_real_disk` | `inspect::read` on a real file |

Also `store::tests::times_are_given_in_whole_100_ns` (real disk) and in the Library `seen::tests::*` (above).

Not tested: the 100 ns figure on the real NAS. To confirm on the NAS rig: write 1 000 files from a Mac, `stat` them on
both sides, check that every pair agrees after the cut. The Library's thread loop itself (look-after timing, retry
backoff) is tested through its parts, not run end to end; the Checks block is not looked at in the app.
