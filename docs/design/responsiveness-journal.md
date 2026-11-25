# The responsiveness journal

**Status:** built in #266 (v0.14.0); the page's command times in the real window fixed in #278 (untagged, after v0.14.0); parts not built: shader-pipeline builds and #256's log not read by the report, Windows process start/launcher not instrumented.

Anton, 2026-11-19: the interface not answering at once irritates, wherever it is — Open in the player, the time from
the window opening to the clip playing, a button. So both apps keep a journal while they're used: every action, when
it was asked for, the steps that prepared it and when its result was on screen. Later `tools/journal_report.py` says
where the waiting is, weighted by how often each action is done, and why. This is the instrument only: what to make
faster comes from what it records.

Nothing leaves the computer. The journal is a folder of text files on this Mac.

## Where it is, what it costs on disk

`~/Library/Logs/ActionFootagePlayer/journal/` (Windows: `%LOCALAPPDATA%\ActionFootagePlayer\Logs\journal`, Linux:
`~/.local/state/ActionFootagePlayer/journal`; any: `AFP_JOURNAL_DIR`). One file a process:

- `player-<when it started>-<pid>.jsonl`: a player (all its plugins write into it);
- `batch-…-<pid>.jsonl`: a headless player the Library started for a batch;
- `library-…-<pid>.jsonl`: the Library (its page's records and the app's own).

Kept 45 days and at most 200 MB together (the oldest go first, checked 20 s into each run); a file stops at 20 MB.
Measured: a busy scripted minute in the player wrote 20 KB, the Library's page about 10 KB: a day of heavy use is
a few MB, so the cap is weeks to months. `AFP_JOURNAL=0` turns it off (the page: `localStorage["afl.journal"]="off"`).

The player's log of #256 (`~/Library/Logs/ActionFootagePlayer/player-*.log`, the last five) stays as it is: it has
what mpv says (shader builds, the commands run); the journal has the timings.

## The format

*The files, kinds and caps in short: [../reference/files-and-formats.md](../reference/files-and-formats.md).*

JSON lines. Every record has `k` (kind), `w` (when, ms) and `s` (who wrote it: the plugin, `page` or `app`).

| k | what | fields |
|---|------|--------|
| `run` | a process's header | `app` (player, library), `pid`, `start` (when the process began), `parent` (batch: the Library's job), `version`, `mpv`, `platform` |
| `load` | a plugin (or the page) loaded | `precise`: the clock is to the µs |
| `act` | an action asked for | `a` its id, `n` its name, `via` (key, click, core…), `key`, `v` (the page's data-v), `d` the handler's own time, `delay` (page: from the event to its first listener) |
| `step` | a step of an action | `a`, `n`, `d` (when it has a length; `w` is then its start), context: `where` (nas/local), `size`, `nth`, `again`, `status`… |
| `show` | something on screen for it | `a`, `n`: frame, picture, pause, osd, `overlay:<plugin>`, paint, `weak` (see below) |
| `done` | the page's summary of an action | `paint`, `settled`, `images`, `first_image`, `last_image`, `parts` {step: [count, total ms, max ms]} |
| `io` | file operations of an action, counted | `nas_n`, `nas_ms`, `local_n`, `local_ms`, the slowest one |
| `cost` | what the journal itself cost | `records`, `bytes`, `rec_ms`, `flush_ms`, `flush_max_ms` |

**The clock.** `w` is each process's monotonic clock (mpv's `mp.get_time`, Rust's `Instant`, the page's
`performance.now()`) plus one reading of the wall clock when it starts, in Unix milliseconds. It never jumps, and
records of different processes line up within about a millisecond — enough to measure "double-click in the Library →
first frame in the player" across the two.

**Ids.** `P<pid>.<plugin>.<n>` (player), `L<pid>.p<n>` (the page), `L<pid>.start`, `L<pid>.job<id>` (a batch).
A Library action that opens files in the player writes `handoff.json` (its id, when, the paths) beside the journal;
the player takes it when it opens one of those files (also when it was already running and macOS hands it the file
with no environment) and writes a `handoff` step with `parent`. A batch gets `AFP_JOURNAL_PARENT`.

**Which action a record belongs to.** In the player, a key binding's handler runs *as* its action, and so do the
timers (`mp.add_timeout`) and subprocess callbacks it starts: their steps and shows carry its id. Something drawn by
other code (another plugin, mpv's events) goes to the last action asked in the player within 15 s, marked `weak`;
the analysis counts a weak show as the result only within 0.5 s of the action. On the page, the commands, renders and
pictures of the newest action going on are its; the polls (`snapshot`, `probes_since`, anything an interval starts)
never are. The app's own steps (Rust) have no id: the analysis gives each to the page action whose time it falls in.

## What is journaled (by default, through the shared helpers)

The helpers: `mpv/scripts/workspace/journal.lua` (required by every plugin near its top), `library/ui/journal.js`
(the page's first script), `library/src/journal.rs` (the app's writer). They hook the layers every action goes
through, so a new key, button or command is covered without a line of its own:

**Player**
- The start: from the process starting (macOS: the kernel's start time, Linux `/proc`; Windows: from the first plugin
  loading) through each plugin loading, the first file (`start-file`, `file-loaded` with size, duration and NAS or
  local, `video-reconfig`) to the first frame (`playback-restart`), and then whether it plays without dropped frames
  (`smooth`/`not-smooth`: frame-drop and delayed-frame counts sampled once a second for up to 15 s).
- Every key binding of every plugin (`mp.add_key_binding`, `mp.add_forced_key_binding`): `<plugin>/<binding>`, with
  the key and the handler's own time. The side panel's clicks and list rows are named by their button
  (`panel/<id>`, `panel/list/<row>`).
- Files opened later (next/previous, a moment, the Library's open): `file/open`, the same steps. Seeks (whatever asked
  for them) to the frame they land on; pause and play by mpv's own keys.
- Every subprocess (`gyroflow-probe`, `gyroflow-gpu-table`, `gyroflow-export`, `afp-*`, `sdr-check`, …) with its
  duration; every `io.open`, `utils.file_info`, `utils.readdir` counted and timed per action, on a NAS share or local
  (mount's list of smbfs/nfs/afpfs shares), a single one ≥ 50 ms as a step of its own with its path.
- The stabilization applied: `warp-set` (the GPU warp in mpv's shader list), `gpu-table` reused, `unhold` (the GPU
  path's black hold over), each with a `picture` show. (`vf-set`, the VapourSynth filter, went with it in #304; old
  journals still have it.)
- On screen: any OSD overlay drawn (`overlay:<plugin>`: the panel, the bar, the 360 view), any OSD message. The side
  panel's cards and name dialog (`mp.notify`, `mp.ask`, docs/design/notifications.md) say whose they are: notify_client.lua
  sends the asking plugin's action id with each, and notify.lua records `card:working`, `card:done`, `card:error`,
  `card:info` or `dialog` for that action once it's drawn — so a moment saved is "k → dialog", then "Enter → card:done".

**The rule for the player's hooks: never in a plugin's way.** In the first trial build the overlay hook put its timed
`update` on the overlay as a field; mpv's `update` sends every field of the overlay as the `osd-overlay` command and
refuses a function ("disallowed Lua type found: function"), so every plugin that drew (the side panel, the telemetry)
died on its first draw — and the headless tests never drew. So:
- a hook hands mpv exactly what the plugin gave (the same command table, flags, overlay; the overlay's `update` is
  timed through a copy of mpv's metatable, nothing set on the overlay itself) and returns all mpv returned;
- the journal's own part runs protected (`pcall`): its callbacks, its functions (`step`, `shown`, …) and its part of
  each hook swallow their faults (the first three said in mpv's log, all counted as `troubles` in the `cost` record);
  a plugin's own errors pass as they would without the journal; every plugin requires it through `pcall`;
- `tests/journal_mpv_test.sh` runs the hooks in a real mpv (`--vo=null`: overlays, commands, subprocesses, a key, the
  files, what the journal wrote), and `tests/journal_test.lua` with mpv's overlay code stood in for and with the
  journal broken on purpose.

**Library**
- The start: the process starting → the page starting → DOM ready → the first screen settled (`library/start`).
- Every click, double-click, key, keystroke in a box, choice and right-click on the page, named by what it was on
  (`click:<data-a>`, `dblclick:pick`, `key:Meta+a`, `input:#q`, `change:<data-a>`), with the delay before its first
  listener and the page's handlers' time.
- Every command the page asks of the app, per action: count, total and longest time; a failing one; one ≥ 50 ms as a
  record of its own. The page's scripts ask through `Journal.invoke` (app.js's `invoke`): the real window's
  `window.__TAURI__.core` is frozen, so swapping its `invoke` (all journal.js did before 2026-11-24) worked only in the
  tests, and every run's `load` said `"invokes": false`: no command times, no handoff to the player. Since then `load`
  says `"invokes": true, "core": <whether the app's own object could be wrapped too>` (page version 2). The page's `render*`/`load*`/`derive*`/`apply*`/`show*` functions the same.
- Pictures (`<img>` loads) per action: the first, the last, how many. `paint`: the frame after the action's handlers;
  `settled`: the frame after its last command answered and was handled.
- Double-click → the Library's list as the player's playlist (`open_list`, its `.afplist.m3u8`) or one item
  (`open_items`) → the player's first frame, as one action across both (`dblclick:pick → player`): the handoff names
  the list file and the item, the player takes it at whichever it opens first.
- Batches: `job-added` (with the asking action), `job-start`, and the batch player's own journal.
- The app's steps: `catalog-wait`, `scan` (items, NAS or local), `open_project`, `project_info`, a picture made or
  read ≥ 20 ms (`thumb`).

## What isn't (yet), and the gaps that remain

- **The pixel on the glass is never seen.** The closest honest points: the player's `frame` is mpv's
  playback-restart (the frame handed to the video output, presented with the next vsync: ~8–17 ms more on the XDR
  screen, up to ~33 ms on a 60 Hz one with a render in between); an overlay's show is its text handed to mpv, drawn
  with the next frame mpv makes (the same gap); `picture` is the warp shader in mpv's list, used from the next
  frame on. The page's `paint` is after WebKit made the frame (rAF, then a task); the compositor's present isn't seen.
- **Before the process.** Finder's double-click to the process starting (Launch Services, the app's launch) isn't
  visible; nor, for a player already running, macOS delivering the file (it shows as the gap between the Library's
  `handoff` and the player's `start-file`).
- mpv's own key bindings other than seeks, pause/play and file changes (volume, frame-step, screenshots…) are not
  actions in the journal; what was pressed is in the #256 log.
- Messages between plugins (`script-message`) and the agent's IPC commands are not actions of their own; their
  effects go to the last action asked.
- Shader pipeline builds (libplacebo's "Spent N ms creating pipeline", the cause of slow cold moments in #219) are in
  the #256 log, not in the journal: the analysis doesn't read that log yet.
- Per-frame render times (the ~16 ms budget) aren't journaled on purpose (nothing per frame); `smooth` sums up the
  first seconds.
- The app's (Rust) steps carry no action id: given by time, so two actions at once can blur.
- Windows: the player's process start (from its first plugin instead) and the launcher (`windows/launcher`) aren't
  instrumented; the Library's start counts from its `main()` there.

## What it costs at run time

Measured in this branch (2026-11-19, M1 Pro):

- Player, inside mpv (a benchmark script, `--vo=null`): a step 7–8 µs, an action 25 µs (it also tells the other
  plugins, `user-data/afp/journal/act`), a show already recorded for its action 1 µs. A record is put in a list; the
  list is written every 2 s from a timer in one append: 0.6 ms for 4,000 records. In a scripted minute of use the 11
  plugins together made 135 records (5.8 ms) and wrote 57 times (50 ms, the longest 3 ms) — each on its own plugin's
  thread, never on mpv's render thread, and only when it had something to write. Nothing is done per frame: the only
  periodic work is the 2 s flush and, for 15 s after a file opens, one property read a second.
- Library page: a record 1.1 µs (jsdom); the records go to the app every 2 s (`journal_write`), the app's journal
  thread writes them: the page and the app's commands never wait on the disk.
- What it cost each run is in its `cost` records, and the report's last line sums them.

Not measured: the same costs in the real Library window (WebKit) and in a player with a window; frame drops with the
journal on vs off.

## Reading it

```bash
tools/journal_report.py                        # the last 7 days
tools/journal_report.py --since 2026-11-19 --json /tmp/report.json
tools/journal_report.py --action "dblclick:pick → player"
```

Per action: how often, median / p90 / worst time to its result, "first thing shown" (the median time to anything on
screen), the waiting (each time's result past 100 ms, added up: frequency × delay), ranked by that. For the slow
cases: the steps and what share of the wait each took (steps overlap: shares can add up past 100 %), the points
(`file-loaded came at 1.2 s`), and what the slow ones have in common (on the NAS, the first of its kind in the run,
the file opened before, a moment, a file over 1 GB, pictures loaded): the median with and without. The JSON has every
action with its steps and context, for a later session to dig into.

The result of an action: the player's file opening and seeks — the first frame (the stabilized `picture` when it
comes later); a key — the latest of the first shows of each kind from its own code (or within 0.5 s
of it); the page — `settled`; a double-click opening the player — the player's first frame.

## Adding an action

Usually nothing to do: a new key binding, panel button, page click or app command is journaled as it is. When there
is something only the code knows:

- Player: `J.step("name", { d = ms, where = J.where(path) })` for a step, `local done = J.span("name"); …; done()`
  for a timed one, `J.shown("what")` for a result on screen the hooks don't see, `J.label("name")` to name the action
  going on, `J.act("name")` for one started without a key. In a plugin at Lua's 200-locals limit (workspace, gyroflow)
  use `package.loaded.journal` instead of a local.
- Page: `Journal.act(name)`, `Journal.step(name, { d })`; a command that must tell another process which action it
  serves goes in `HANDS_ON` (journal.js: open_items, open_list, add_job) and takes `journal: Option<String>`.
- App (Rust): `journal::step("name", json!({ "d": ms, "where": journal::where_(path) }))` or
  `let _s = journal::span("name", json!({...}));`.

Tests: `luajit tests/journal_test.lua`, `python3 tests/journal_report_test.py`, `cargo test journal` (in `library/`),
`node --test journal_test.js` (in `library/ui-tests`).
