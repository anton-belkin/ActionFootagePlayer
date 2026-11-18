# What the player says, and where

Agreed with Anton on 2026-11-18 (proposal: https://claude.ai/artifact/JW7KkQSSuQbaM3hGoQbG3T): no white-on-black text
over the clip. What the player does is said in **cards** at the top of the video, in the middle, drawn in the side
panel's look; a name is typed in a **dialog** in the middle of the video. Always cards, whether the panel is open or
not (Anton didn't want progress drawn on the panel's buttons).

## Cards

| Kind | Looks | Stays |
| --- | --- | --- |
| working | a turning ring, a bar: the share done where the tool says it (export, clip previews, moments of the marks, the sideways jerks, cutting), else one that moves with the time so far (a moment's picture, reading the clip, the first frame) | until the same card is ended |
| done | a green tick, a full green bar | 3 s (an export 5 s); a click closes it |
| error | red, the reason under it | until it's clicked or Esc |
| info | a short line (a key's toggle, "Renamed to …", "Mark the part first") | 1.5–4 s; the ones of 2 s or less (what the panel shows anyway) not while the panel is open |

Cards stack (at most four shown), and nothing wipes another: each has an id, and only the same id changes it.

## Dialog

Title, a line under it, the field (the suggestion dimmed: Enter takes it, Tab puts it in to change it), the earlier
names as chips (a click or ↑↓ puts one in), Enter / Esc and the two buttons. The keys are the dialog's while it's open.
For: a moment's name, Update or a new moment, an export's name, renaming a moment, tags (K). Not yet: Ctrl+x's choice
of what to cut (still mpv's console list), the quality plugin's prompt, the agent's note.

## How it's built

- `mpv/scripts/side-panel/notify.lua`: the cards and the dialog, on an overlay of their own above the panel and the
  bar, over the video's place (beside the panel, above the docked bar, below the macOS title bar). Script messages
  `notify <json>` and `ask <json>` to `side_panel`.
- `mpv/scripts/side-panel/notify_client.lua`: what each plugin loads (a `pcall(require)` at the top of its main.lua; the
  plugins sit side by side in `scripts/`). It adds `mp.notify(t)` and `mp.ask(opts, done)` and routes the plugin's
  remaining `mp.osd_message` calls to cards (errors by their words: Couldn't…, …failed). Without the side panel, mpv's
  own text and console, as before.
- The app's `mpv/app.conf` has `osd-level=0`: mpv's own lines (A-B loop, volume, "Paused") are gone too. Script
  overlays don't depend on it.
- Dropped outright: the look and the stabilization said as a clip opens (dlogm, gyroflow), the 360 hint on opening,
  "Tags changed in Finder", "Lens type changed in the Library" (all in the log still).
- A moment saved is in the list at once (`preview-done` adds it, `state.fresh` keeps it until the catalog lists it, a
  minute at most), not a few seconds later.

## Checked

`luajit tests/notify_test.lua` (cards, timing, Esc, the client's routing, the dialog's typing in UTF-8, ↑↓, Tab, Esc).
Pictures from a headless mpv (`vo=null`) driven over IPC: `side_panel dump` writes the panel, the bar and the cards as
ASS, drawn over a frame by mpv's screenshot.
