# The catalog against the files: holes and safeguards

Review of 2026-11-17, after the second "a field vanished from the views" incident. Evidence: the code (references
below) and `catalog/tests/safeguards.rs` (run: `cargo test --test safeguards` in `catalog/`). Nothing in the catalog's
behaviour was changed.

## Summary

1. **The construction is sound.** Journals, snapshots and versions do what they promise. Nothing tested loses or mixes
   up records, and a damaged file is never used. What can go wrong is narrower: **a record can say something old about
   a file, and nothing ever reads that file again.** The catalog trusts "same size + same time = same insides". Nothing
   re-reads contents on a schedule.
2. **Hole 1, the likeliest, active today: times seen with two precisions.** The NAS sees a file's time in ns, a Mac in
   100 ns steps. Each sees the other's records as changed, so both re-read and re-note every file of ours **for ever**:
   the project's folder at every NAS 30‑minute look, and each file the Mac writes. This is the door both incidents
   came through: an older or slightly different reader gets to rewrite a record it had no reason to touch.
3. **Hole 2, rare but lasting: a change that keeps size and time** (a restore or copy that keeps times, `touch -r`, a
   rewrite inside a coarse disk's tick). It is never seen, not even by the deep look.
4. **Hole 3, rare but lasting: clips' own files in the locations are never looked at again** except through the NAS's
   inotify, our own notes, or a "look at everything". A change in place made while the watcher is down, or a record
   wiped by a bad re-note, stays until someone edits that clip again.
5. Minor: a coarse disk (exFAT/FAT) as a location can hide a new file until a deep look. A lost `note_now` isn't
   retried. The version gate only protects against programs new enough to have it.
6. **Recommended: (a) one precision for times in records** (100 ns, what SMB carries), set where times are read. This
   ends hole 1 and the churn behind both incidents. **(b) A content check on a schedule ("scrub")**, run by whoever has
   the disk locally (the NAS Helper; the Library's daily deep look where there's no Helper). It re-reads the small
   insides of our files, notes only real differences, and **reports how many it corrected**. This one mechanism closes
   holes 2–3, mends whatever an older program did, and doubles as "rebuild" and "self-check".
7. Cost on the NAS: about seconds (NVMe) to a few minutes (HDD, cold) a day, at low priority, for 10 000 clips + 10 000
   moments. On a Mac over SMB: about 3 minutes a day in the background, or a seventh of the files each day.
8. Not recommended: distrusting the catalog on each read in the Library (Anton's rule, #220), hashing whole videos,
   tolerances in comparisons, anything that uses clocks or locks.

## How the catalog decides a file is unchanged

- A look lists a folder only when its time, or the digest of what's on record in it, differs from the folder's record
  (`catalog/src/catalog.rs:626-629`). The digest hashes each child's name, size and exact time
  (`state.rs`, `digest`).
- In a listed folder, a file is read again only when its size or time differs from its record (`catalog.rs:658-660`,
  `:696`). Otherwise the record is kept as it is.
- `--deep` (`Sweep::deep`) only forces every folder to be **listed** (`catalog.rs:626`). Files are still read only
  when size or time differ. Test `today_a_change_keeping_size_and_time_is_never_seen`: the deep look lists 6+ folders
  and reads 0 files.
- `afp-catalog note <project> <files>` and the Library's `seen::note_now` read the named files whatever is on record
  (`cli.rs` `observe`, `seen.rs:188-250`). This is the only way a file is re-read without its size or time changing.
- A record replaces another only with a higher version (Lamport clock, `state.rs` `apply`), whatever its content. The
  newest observation wins, not the truest. Every write first reads what's new (`catalog.rs:361`), so "newest" is
  well-defined. But the winner is only as good as the reader that made it.
- Who looks at what, and how often:

| Who | What | How often | By |
|---|---|---|---|
| Library | project's own folder | every 5 min | folder times (`seen.rs:521`, `project::own_roots`) |
| Library | project's own folder | once a day | deep: every folder listed (`seen.rs:533`) |
| Library | everything (locations too) | first look, after Mend, Take in, a batch | folder times (`seen.rs:517`) |
| Library | folders this Mac changed | ~1 s after FSEvents | listing (`seen.rs:508`) |
| NAS watcher | folders inotify names | ~1 s after a change | listing (`cli.rs` `watch`) |
| NAS watcher | everything | at start, and every 30 min (`--every 1800`, `cli.rs:402`) | folder times; never deep |
| Player | its clip's moments | as the clip opens | stat each, re-read when size/time differ (`cli.rs:243`) |

So **no program ever deep-looks at the locations**: `own_roots` is the project's folder only, and the footage folders
inside it are left as well (`inspect.rs` `leave`). And **no program ever re-reads a file whose size and time match its
record.**

## Every way a record can differ from its file, and stay so

### 1. The same file's time seen with two precisions (active today)

**How.** Times are kept and compared in nanoseconds, exactly (`store.rs` `Stat::mtime`, the comparisons above). On the
real NAS a Mac over SMB sees `…540500` where the NAS sees `…540523` (found in PR #249's work). SMB carries times in
100 ns units, and the Mac's figure is the NAS's cut down to them. Folders are affected too, not only files.

**What happens** (`today_two_precisions_reread_everything_for_ever`). A Mac and a NAS that look at the same project
never settle. In every round each reads every moment again and writes every folder's and file's record again. With one
precision, the second round reads and writes nothing. In the real system:

- **The project's own folder:** a ping-pong at the NAS's 30-minute look. The NAS lists every folder the Mac recorded
  last and re-reads every file. The Library's next 5-minute look does the same the other way. At 10 000 moments in
  2 000 folders, from §9's measured unit costs, this is about 2 000 listings × ~80 ms plus 10 000 reads × 11–24 ms
  (8 at a time) on the Mac, about 3 minutes of SMB traffic per half hour. It also adds ~20 000 records to the journals
  per half hour, and so a fold about as often. (Estimate, not measured.)
- **A clip's own file in a location** (`today_the_nas_renotes_each_file_a_mac_just_noted`). The Library writes and
  notes it. The NAS's watcher, told by inotify, lists the folder and re-reads the file only because of the extra
  digits, then notes it again. With one precision it reads nothing. This was the channel of both incidents. The
  Helper re-read a file it had no reason to read, with a reader that kept less, and its record won by being newer.
- **The player:** opening a clip, `derived` compares each moment's time exactly (`cli.rs:243`). For moments last noted
  by the NAS it re-reads and re-notes each one at every opening. That changes the folder's digest, so the NAS's next
  look lists and re-reads them again.

**Does it settle?** Never in the project's folder. In a location it settles on the NAS's record after one extra note
per write, since no Mac looks there regularly. When two records of one file carry the same insides, last writer wins
harmlessly: only the time differs, though the Library's page counts it as a change and reads its view again
(`catalog.rs` `take`). When they carry **different insides** (an older reader, a read mid-write), the newer one wins
and nothing questions it, because size and time now match. See 2 and 3.

**Why the 1 µs tolerance wasn't enough anyway.** Times are compared in three places: the folder's own time, the files'
times, and the folder digest, which hashes exact times. A tolerance can't go into a hash. With the tolerance on files
only, folders would still be listed at every look, just without the reads. A canonical form, every time cut to 100 ns
where it's read (`DiskStore`'s `stat` and `list`), is one change in one place. It is still exact equality. It holds
for the digest too.

### 2. A change that keeps size and time

**How.** A restore from a backup or a copy that keeps times (Finder's Duplicate keeps the time, `cp -p`, `rsync -t`,
Time Machine). `touch -r`. A tool that rewrites a sidecar's text to the same length within one tick of a coarse disk
(exFAT 10 ms, FAT 2 s). A moment's MP4 key rewritten in place to the same length by a tool that then restores the
time.

**Heals?** Never: `today_a_change_keeping_size_and_time_is_never_seen`. The folder is listed (its time changed when the
file was renamed in), and the deep look lists everything, but neither reads the file. Only an explicit `note` of that
file, or a later change of its size or time, does.

**Likelihood.** Low for sidecars, since our writers always change the time. It is realistic for "I restored my
`_Moments` from the backup". Worst case: a moment pointing to the wrong clip, or a clip's old tags, until it's next
edited.

### 3. A change outside our programs that nobody hears of

**How.** A text editor, a sync tool or a script changes a file in place: its folder's time stays. If the NAS's watcher
is down then, or the change was made before it started, or the disk isn't the NAS's own (a USB disk on a Mac), the
change is never heard of.

**Heals?**
- In the project's folder: the Library's daily deep look finds it, if size or time changed.
- In a location (clips' own files beside the footage): **never**, until a "look at everything" (Mend, Take in, a batch)
  or the next edit of that clip (`today_an_in_place_change_in_a_location_waits_for_ever_without_inotify`). The NAS's
  30-minute look goes by folder times. The Library's looks are of the project's folder only. A deep look over the
  locations would see it, and the test shows it does, but no program runs one.
- Same lasting hole when the record, not the file, went wrong: a bad re-note (1) in a location stays wrong.
  `today_an_older_program_wipes_a_field_and_nothing_heals_it` reproduces 2026-11-17 in the simulator. With an older
  reader on the NAS and two precisions, Accept / Reject goes from the record and no look of any computer brings it
  back. With one precision the older reader never touches the record.

### 4. A folder's time not changing on an in-place rewrite

By design a folder's time changes only when something in it is made, removed or renamed (`sim.rs` header, as real
disks do). Our writers write through a temporary file renamed over the old one (`library/src/index.rs:792`,
`mpv/scripts/workspace/sidecar.lua` `write`), and notes also touch the folder (`Catalog::touch`). So our own writes
always change the folder's time. Other tools' in-place writes don't: see 3.

### 5. A coarse disk as a location

On exFAT (10 ms) or FAT (2 s), a file added in the same tick as the folder's last listing leaves the folder's recorded
time as it was. Its digest is computed from the records, which don't have the file yet, so the folder isn't listed
again. `today_a_coarse_folder_time_hides_a_new_file_until_a_deep_look`: hidden from every regular look, seen by a deep
one. The recheck a minute later (`catalog.rs:726`) helps only when the folder was listed because it changed. Cards are
taken in rather than kept as locations, so this is unlikely. And no deep look covers locations (3).

### 6. Clocks

No rule uses one computer's clock against another's. Versions are Lamport counters. Times of files are compared only
for equality, never ordered. The clean-up's 10-minute grace and the recheck delay use each computer's own clock
against itself (`catalog.rs` `clean`, `rechecks`). A computer whose clock is wrong changes nothing in the catalog. The
file times it causes are set by the NAS anyway.

### 7. A journal line lost, a snapshot folded while another writes

Covered by the design and its tests, nothing found:
- A torn or damaged line is left out (`journal.rs` `parse`; `journal_lines_cut_off_or_damaged_are_left_out`). The next
  line starts a new segment.
- A line lost after the file was written leaves the folder's time changed (the touch comes first in `note`/`note_now`).
  The next look lists it. In the project's folder that is within 5 min. In a location, only through inotify or
  FSEvents, else as in 3. The Library doesn't retry a failed `note_now` (`seen.rs:250`, `let _ =`).
- A fold covers each journal up to an offset (`Manifest::at`, `covers`). Lines after it are read from there. A reader
  whose segments were cleaned up gets `Gap` and reads again from the newer snapshot. Two computers folding at once both
  produce good snapshots. The random test (300 seeds, 5–20 % of operations failing, stale listings) ends with every
  catalog equal to the files.
- Tombstones are dropped at a fold only below every computer's highest version (`compact`, `min_l`). A computer can't
  bring back a deleted file with a lower version than the deletion.

### 8. Records written by an older or newer program

- **The gate is asked at every write**, not once. `load`, `note`, `compact` and `clean` each read `Project.afproject`
  (`catalog.rs:105` `usable`). A look writes through `note`. So the Library's `seen::note_now` and looks, the player's
  `afp-catalog note`, and an `afp-catalog watch` already running all stop the moment the project is raised.
  `the_version_gate_holds_for_a_catalog_already_open` checks this on a real folder: note, look and fold are all
  refused after the raise. The watcher exits (3), and Docker's restart brings it back only to stop again.
- **What it doesn't cover:** programs from before the gate existed. A window of milliseconds between the check and the
  append. And fields added without raising the version, which is exactly 2026-11-17. That last one is now held by the
  test tying `OWN_KEPT` and `VERSION` together (`inspect.rs` tests) and by `own_kept` carrying unknown fields through.
  Photos' and moments' insides (`link`, `tags`) aren't carried through that way. A new key there needs a version raise,
  which the same test enforces.
- Any `afp-catalog` run raises the project to its own version (`cli.rs:221`). So the first newer player to note
  something makes every older Library and Helper refuse the project. That's intended, but worth knowing when a Mac
  lags behind.
- A newer program's records read by an older one: the older one refuses to load at all (`TooNew`). Its views read the
  disks.

### 9. Insides read while being written

A clip's own file that isn't valid JSON is read as "nothing of ours" (`inspect.rs:166`, `Ok(None)`), not as an error.
A reader catching a half-written file would record "no tags, no pick". Our writers rename a whole file into place, and
the watcher waits 1 s for quiet, so this needs a foreign non-atomic writer, and it heals when the size or time differs.
Low. The scrub covers it.

## Backstops today, and what's missing

| Backstop | Catches | Misses |
|---|---|---|
| Notes by our programs (`note_now`, `afp-catalog note`) | everything we write | changes not made by us; a note that fails (no retry) |
| Folder times + digest, every look | files added, removed, renamed; our in-place writes (touch) | in-place writes by others; coarse times |
| NAS inotify | anything changed on the NAS's disks while it runs | while it's down; disks not the NAS's |
| Recheck a minute later | a Mac's stale listing | — |
| Daily deep look (Library, project's folder) | in-place writes by others there, if size/time changed | **locations**; size+time kept |
| Version gate + `own_kept` | older programs of the gate's era; fields added later to clips' own files | programs before the gate; new fields elsewhere without a raise |
| The views' walk of a folder never listed | a moment just noted in a new folder | — |

**Missing:** (i) anything that ever reads a file's insides again when its size and time match. (ii) Any regular look
at the locations except the NAS's inotify. (iii) Any count of how often the catalog was wrong. Today an incident is
found by Anton noticing a field gone.

## Safeguards proposed

Costs from §9's measurements: a stat over SMB ~5 ms, opening and reading a small file 11–24 ms, listing a folder of
100 ~80 ms. On the NAS's own disk, reading is cheap.

**A. One precision for times in records: 100 ns.** Cut every time to whole 100 ns where it's read: `DiskStore::stat`
and `list` (`store.rs`), and `observe` through them. That's what SMB carries and what a Mac sees. Exact equality stays
everywhere, the digests included. Cost: none. On the first look after the change, records the NAS wrote in ns differ
once more and are re-read once, then everything settles. To confirm first, on the NAS rig: write 1 000 files from a Mac,
`stat` them on both sides, and check that every pair agrees after the cut (Samba builds the SMB time as
`ns / 100`). Then turn round the assertions in `today_two_precisions_reread_everything_for_ever` and
`today_the_nas_renotes_each_file_a_mac_just_noted` (sim: the Prec store cut to 100 ns).

**B. A content check on a schedule ("scrub"), with a report.** Over every file whose insides count (clips' own files
everywhere; the project's moments, links, photos):
- Read its insides with the same reader as a look (`inspect::read`).
- Note a new record **only when the insides differ** from the record's, or when size or time do. An equal-content
  record is never rewritten, so the scrub can't cause churn or race the other writers.
- Write a count and the paths it corrected to `.afp/catalog/check/<computer>.json`, and one line to the log. The
  Library's Checks part shows the latest ("The catalog was corrected for 3 files on 17 Nov"). This is the self-check.
  A non-zero count is a bug to look into, not just something healed.

Who runs it:
- **The NAS Helper** (`afp-catalog watch`): once a day at low priority, over all roots. It is the one computer that
  reads locally. A clip's own file is ~1–4 KB. A moment's key needs an open, a few box headers and its `moov`. For
  10 000 + 10 000 files: seconds on NVMe, about 2–4 min on the HDD mirror cold (≈10 ms a seek). It also catches what it
  missed while down, which inotify can't.
- **The Library, where there's no Helper** (a USB disk, a share without Docker): its daily deep look covers the
  locations too and re-reads contents. Over SMB that's ~17 ms a file, 8 at a time: ~45 s per 10 000 own files, ~3 min
  for 20 000, in the background. Or a seventh each day. When the Helper's report is younger than a day, the Library
  skips its own.
- **On demand**, as "Check the catalog against the files" (`afp-catalog check <project>`, and in Checks): the same
  run, at once, printing the differences. This is also the **rebuild**. It corrects every record from the files
  without deleting `.afp/catalog` under other computers' feet.

**C. Retry a failed note.** `seen::note_now` keeps entries it couldn't write and tries again at the next 3-s tick. It
is small, and the scrub would cover it within a day anyway.

**Recommended design: A + B.** A ends the churn that let both incidents happen and costs nothing. B gives every
remaining hole (2, 3, 5, 9, a lost note, a bad re-note, an older program's damage) a bound of one day. It also makes
the catalog's error rate visible. Together they leave the Library free to trust the catalog on every read, as Anton's
rule wants.

## What I would not do

- **Distrust the catalog per read in the Library** (stat each file shown): ~5 ms each over SMB, the slowness the
  catalog was built to remove, and closed once already (#220).
- **A fingerprint of whole files** (SHA of each MP4): reading gigabytes for a few hundred bytes of insides. The insides
  themselves (`meta`) are the fingerprint that matters, and B compares them.
- **Tolerances in comparisons**: they don't reach the digests (see 1). A canonical precision does the same job exactly.
- **ctime as an extra "changed?" signal**: it would catch kept-time restores without a scrub. But Finder tags and
  xattrs bump it too, it's another precision to agree on, and FAT has none. B covers it within a day.
- **Deep-listing everything every 30 min on the NAS**: listing doesn't read insides, so it still misses 2. B is the
  same cost once a day and catches more.
- **Clocks, locks, a central database, or deleting `.afp/catalog` to rebuild**: each brings back a problem the design
  was made to avoid (§9).

## Tests added

`catalog/tests/safeguards.rs`. Each `today_*` test asserts a hole as it is today, and says which assertion to turn
round when its safeguard lands:

| Test | Shows |
|---|---|
| `today_two_precisions_reread_everything_for_ever` | 1: never settles with two precisions; settles with one |
| `today_the_nas_renotes_each_file_a_mac_just_noted` | 1: the NAS re-reads a file the Mac just noted only because of the precision |
| `today_an_older_program_wipes_a_field_and_nothing_heals_it` | 1+3: the 2026-11-17 mechanism; no look heals it; one precision prevents it |
| `today_a_change_keeping_size_and_time_is_never_seen` | 2: not even the deep look reads it |
| `today_an_in_place_change_in_a_location_waits_for_ever_without_inotify` | 3: the regular looks miss it; a deep look over locations would catch it |
| `today_a_coarse_folder_time_hides_a_new_file_until_a_deep_look` | 5 |
| `the_version_gate_holds_for_a_catalog_already_open` | 8: note, look and fold refused after the raise, on a real folder |

Not tested: the 100 ns figure itself. It comes from the NAS observation in #249's work, and I had no NAS access by
design. Unicode name forms (§9 lists them as untested). Two writers' read-modify-write of one clip's own file, which
is a question about the files themselves, not the catalog.
