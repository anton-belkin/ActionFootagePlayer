# The usage trace, and filing a bug from it

**Status:** built in this PR (untagged): the player, its side panel and the Library page and app; the Library's markers
have no picture; mpv's own keys other than those journaled already (volume, frame-step…) are not recorded.

Anton, 2026-11-26: his own testing by hand is the bottleneck. After clicking around he has to remember and retell what
happened. So the apps keep a factual record of what he did and what they showed him, with their warnings and errors;
later he says "заведи баг" with one line, and a session reads the stretch, picks what matters and opens an issue another
session can investigate (`.claude/skills/file-bug/SKILL.md`).

It is not a second log. It is [the responsiveness journal](responsiveness-journal.md) with content: the same files
(`~/Library/Logs/ActionFootagePlayer/journal/*.jsonl`), the same clock, the same action ids and writers, the same
retention (45 days, 200 MB). The journal says when and how long; the trace adds what it was about.

## Who sees it: a setting, off by default

The app is public. The trace holds file paths, typed names and notes, so it's off unless this computer says so:

```
~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/journal.conf
trace=yes
actor=anton
```

(Windows `%LOCALAPPDATA%\ActionFootagePlayer\mpv\script-opts\`, Linux `$XDG_CONFIG_HOME/ActionFootagePlayer/mpv/script-opts/`;
the plugins in your own mpv: its `script-opts/journal.conf`.) It's a computer's setting beside `gyroflow.conf`, read
the way mpv reads every plugin's options (`journal.lua` `trace_options`; nothing trimmed, a boolean only `yes`/`no`).
The Library reads the same file (`journal.rs` `trace_options`, the same rules) and tells its page (`journal_hello`'s
`trace`). `AFP_TRACE=1`/`0` overrides it for one run. Nothing leaves the computer; the journal with the trace off is
as before (timings only).

## Who acted

Every run's header (`run`) says `actor`: `AFP_ACTOR` from the environment, else journal.conf's `actor`, else `user`
(Anton's journal.conf says `anton`). **Sessions set `AFP_ACTOR=claude` when they start the player or the Library for a
test** (CLAUDE.md), so their clicks never pass for his.

mpv can't tell where a command came from: a key in input.conf, the menu, the Library and an IPC client all look the
same. Where it can be told apart: an IPC client says who it is by setting `user-data/afp/journal/ipc` (`claude`,
`ipc`) while it drives the player; with the trace on, every action not from a key or a click meanwhile carries that
`actor` (pause, seeks, files opened). A session or rig driving Anton's own player over the agent plugin's socket
sets it first (`{"command": ["set_property", "user-data/afp/journal/ipc", "claude"]}`, docs/dev/testing.md).

`tools/trace_report.py --actor anton` keeps his.

## What it adds (with the trace on)

In the same records, as fields or new kinds (the formats: [../reference/files-and-formats.md](../reference/files-and-formats.md#responsiveness-journal)):

| What | Where it's recorded | Record |
|---|---|---|
| The file playing, the position, paused, at each action | `journal.lua` `act_begin` (every plugin) | `act`: `file`, `pos`, `paused` |
| A file opened | `journal.lua` `start-file` | `step` `start-file`: `path` |
| An OSD message's words | `journal.lua`'s `mp.osd_message` hook | `text` `n: osd` |
| A card's words (each new title, detail or kind; not a bar moving) | `side-panel/notify.lua` `notify` | `text` `n: card`, `kind`, `detail`, `card` (its id), `quiet` |
| The name dialog, and the answer | `notify.lua` `ask`, `answer` | `text` `n: dialog` (`subtitle`, `value`, `suggestion`); `answer` (`n` the dialog's title, `text`, `cancelled`) |
| A setting changed, old and new | `journal.lua` `watch_settings`: registry.json's settings as the plugins publish them (`user-data/gyroflow/<key>`, `user-data/dlogm/<key>`, `user-data/uwcolor`'s uw keys) and the lens mode, translation, resolution, look and screen | `set`: `n` (`gyroflow.level`), `old` (absent the first time it's seen), `new` |
| The side panel's tab | `side-panel/trace.lua` `tab` | `tab`: `n` (a tab, `closed`, `hidden`) |
| mpv's warnings and errors, every plugin's and its own | `journal.lua` `watch_log` (`mp.enable_messages("warn")`, the workspace plugin's) | `log`: `level`, `prefix`, `text`; the same line within 10 s once, at most 120 a minute |
| The Library: what was clicked, in its words | `journal.js` `traceFields` | `act`: `label` (a button's, a tab's, a box's name), `value` (a choice's, a box's) |
| The Library: dialogs, the toast, the banners | `journal.js` `check` (a MutationObserver, a task after the page changed) | `text` `n: dialog` (`body`), `toast`, `banner` (`id`) |
| The Library: a command's error, the page's own | `journal.js` `failed`, `error`/`unhandledrejection` | `log`: `prefix` `invoke:<cmd>` or `page` |
| The Library: Source settings changed | `tags.rs` `set_source` | `set`: `n` `source.<key>`, `clip`, `old`, `new` |
| The Library app: a panic | `journal.rs` `init`'s panic hook | `log` `prefix: panic` |
| Who acted | `run` `actor`, `trace`; `act` `actor` (IPC) | |

Small on purpose: it records a person using the apps, never per frame. Texts are cut at 500 characters, a setting's
table at 300 (longer ones are left out).

## "Something's wrong here"

A marker where it went wrong, so the stretch is found later without remembering the time:

- **Player:** `Alt+b` (free in every plugin and the menu; `side_panel/mark`), the flag on the side panel's rail (above
  Support, only with the trace on), or the menu's ActionFootagePlayer → Something's wrong here…. It writes a `mark`
  record at once (its id, the file and position, the actor, the picture's path) and flushes, saves the window as it is
  (`screenshot-to-file <journal>/mark-<when>-<pid>-<n>.png window`: the video, the panel, the cards), and only then
  asks a line about it in the name dialog. Enter keeps the note (`mark-note` `note`); Esc or nothing: the marker stays
  without one. With the trace off, the key says so in a card.
- **Library:** ⇧⌘B (Ctrl+Shift+B elsewhere) or the flag at the window's bottom left (only with the trace on): a `mark`
  record with what the page showed (the dialogs open, the toast, the focused element's words), then a small box for the
  line. **No picture:** capturing the Library's window would need macOS's Screen Recording permission for the app
  (`screencapture`), and WKWebView's own snapshot needs native code the Library doesn't have yet. Not done; the record
  says so (`no_shot`).

The pictures sit beside the journal and go with it (45 days, the 200 MB cap: `prune` in `journal.lua` and
`journal.rs`). Anton's agent notes (Ctrl+r, `agent/reports/`) stay what they were: a marker is the quick one.

## Reading it: tools/trace_report.py

```bash
tools/trace_report.py                      # the last 30 minutes
tools/trace_report.py --marks              # the markers, newest first (1 = the last), with notes and pictures
tools/trace_report.py --mark last          # 5 min before the last marker, 1 min after (--before, --after)
tools/trace_report.py --since 2026-11-26T14:00 --until 2026-11-26T14:20 --actor anton --app player
tools/trace_report.py --last 2h --json stretch.json
```

A timeline, one line a record: the time, the actor, the app, then the action (`▶ gyroflow/level [Ctrl+l] on
DJI_0042.MP4 at 1:23.5`), what was said (`says (card:done): “Exported”`), settings (`set gyroflow.level: medium →
strong`), warnings (`⚠ warn [gyroflow] …`), markers (`⚑ MARKER … picture: …`); then the files, the warnings and
errors, the markers. The runs' headers (app, version, actor, whether the trace was on) come first. The timings stay
`tools/journal_report.py`'s.

## Filing a bug

`.claude/skills/file-bug/SKILL.md`, for any session: Anton's words and a time (or "the last marker") → the report on
that stretch → the relevant part kept → the marker's picture looked at → an issue (`gh issue create`, label
`from-trace`) with his words, the steps from the trace, expected vs what he saw, the warnings and errors, a trimmed
timeline in `<details>`, the files and the versions. Pictures stay on the Mac (their path in the issue). He sees the
text before it's created.

## Not done, and why

- The Library's markers have no picture (above).
- mpv's own keys that aren't journaled actions (volume, frame-step, screenshots) aren't in the trace either; what
  they show on the OSD isn't (mpv's own OSD, not `mp.osd_message`).
- Messages between plugins and from IPC (`script-message`) aren't actions; their effects go to the last action asked
  (as in the journal). An IPC client that doesn't say who it is passes for the run's actor.
- The NAS stream and exports record nothing of it (no person there). A Library batch player (`AFP_BATCH`) reads the
  same journal.conf, so its run has the trace too: its warnings and errors are useful; it has no actions.
