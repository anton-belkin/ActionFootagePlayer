---
name: file-bug
description: Turn Anton's "заведи баг" / "file a bug" (a one-line description, an approximate time or "the last marker") into a GitHub issue built from the usage trace of ActionFootagePlayer and its Library - steps, what he saw, warnings and errors, files, version - for another session to investigate. Use whenever Anton asks to file, open or log a bug about something he just did in the player, the side panel or the Library.
---

# File a bug from the usage trace

Anton tests by hand. The player, its side panel and the Library write what he did and what they showed into the usage
trace (docs/design/usage-trace.md): the journal in `~/Library/Logs/ActionFootagePlayer/journal/*.jsonl`, with markers
("Something's wrong here": Alt+b in the player, the flag on the panel's rail, ⇧⌘B or the flag in the Library) and
their pictures (`mark-*.png`) beside it. This skill turns a stretch of it into an issue someone else can investigate
without asking him again.

Everything here stays on this computer except the issue text, and the issue text is shown to Anton before it goes.

## 1. What he said

Take his words as they are (he dictates; "Jira Flow" means Gyroflow). Note:

- the description (one line is enough);
- when: "the last marker" / "the marker" (the default when he says nothing about time), a marker's number, "just now",
  "about 14:20", "10 minutes ago".

## 2. The stretch of trace

```bash
python3 tools/trace_report.py --marks                     # the markers, newest first: 1 = the last
python3 tools/trace_report.py --mark last                 # 5 min before it, 1 min after
python3 tools/trace_report.py --mark 2 --before 10m       # another, a longer run-up
python3 tools/trace_report.py --since 2026-11-26T14:10 --until 2026-11-26T14:25
python3 tools/trace_report.py --last 15m --actor anton    # his own actions only (claude: a session's tests; ipc)
python3 tools/trace_report.py --mark last --json /tmp/stretch.json   # every record, for the details
```

- If the report says the trace is off, or finds nothing: tell him (journal.conf `trace=yes` turns it on,
  `~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/journal.conf`), and ask him for the steps instead.
- Read it all, then keep only what bears on his description: the actions that led there, what the app showed (texts,
  cards, dialogs, toasts), the settings changed, the warnings and errors near it. Leave out the rest (other clips,
  unrelated clicks). Actions by `claude` or `ipc` are a session's or a rig's, not his: say so if they're in the way.
- If it's not clear which stretch he means (no marker near the time, two candidates), ask him one short question.

## 3. The picture

A player marker has `picture: <path>` (the window at the moment, with the panel and the cards). Read it with the Read
tool and say in the issue what it shows that matters (one or two sentences). Library markers have no picture (it would
need the Screen Recording permission): their record keeps the dialogs and toast that were showing.

The picture stays on this computer: name its path in the issue, never upload it.

## 4. The issue

Draft it in this shape:

```markdown
**Anton:** <his words, as he said them>

**Version:** player <version from the run line> (<mpv>), Library <version> · <date and time of the marker>
**Marker:** <id> — “<his note>” · picture (on Anton's Mac): `<path>`

### Steps (from the trace)
1. Opened `<file>` …
2. Pressed Ctrl+l (gyroflow/level) at 1:23.5 …
3. …

### Expected / what he saw
- Expected: <from his words; say when it's inferred>
- Saw: “<the texts shown, from the trace>” <what the picture shows>

### Warnings and errors around it
- 14:12:03 warn [gyroflow] …

### Files
- `<path>` (on the NAS / local)

<details><summary>Trace excerpt (tools/trace_report.py --mark <n>)</summary>

```
<the trimmed timeline: the relevant lines only, a few dozen at most>
```
</details>

From the usage trace (docs/design/usage-trace.md). The journal and the picture are on Anton's Mac.
```

- Keep his words first and unchanged. Steps are what the trace shows, numbered, in plain words, with keys and panel
  buttons named as he'd see them (the action's name and its label).
- Don't diagnose in the issue beyond one line of "where to look first" if the trace makes it obvious (an error naming a
  file or a command). The investigating session will do the rest.
- Title: what's wrong, in his terms, short ("Level change tilts the horizon on DJI_0042").

## 5. Show it, then create it

Show Anton the title and the body, and wait for his go-ahead (he may correct the steps or the expectation). Then:

```bash
gh label list --search from-trace | grep -q from-trace || \
  gh label create from-trace --color 5319E7 --description "Filed from the usage trace (docs/design/usage-trace.md)"
gh issue create --title "<title>" --label from-trace --label bug --body-file <the body in a scratch file>
```

Give him the issue's link. If he wants it investigated now, that's a new session's task: offer one, with the issue's
number as its brief.
