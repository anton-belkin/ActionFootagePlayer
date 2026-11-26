# The docs

What's here, and how far to trust it. The code is the truth; these say what it does and why.

- **[reference/](#reference-how-it-is)** says how the system is now. Each file names the code it covers and the
  commit it was checked against. Change it in the same PR as the code.
- **[dev/](#dev-how-to-work-on-it)** says how to work on it: tests, releases, traps.
- **[design/](#design-what-was-decided-and-why)** keeps the designs, each with a status line at its top. A design
  explains why; where it and reference/ disagree, reference/ (and the code) win.
- **[archive/](#archive-history)** keeps handovers, spikes and attempts as they were written. Not maintained.

The rules for every change are in [../CLAUDE.md](../CLAUDE.md); what the user gets is in [../README.md](../README.md).

## Reference: how it is

| Doc | What |
|---|---|
| [reference/architecture.md](reference/architecture.md) | The parts, the processes and who talks to whom, how a frame is drawn and an export runs, log curves, the repository's layout |
| [reference/files-and-formats.md](reference/files-and-formats.md) | Every file the system keeps: `<clip>.afp.json`, `Project.afproject` and its versions, `.afplocation`, `.afp/catalog`, `.afp-motion`, links, own folders, App Support; who reads and writes each |
| [reference/contracts.md](reference/contracts.md) | Between processes: the Library's batch (`AFP_BATCH`), the inbox, the GPU table's request, the probe, the stream; generated lists of script messages, `user-data` keys, `AFP_*` variables and `gyroflow-export` flags |
| [reference/settings-flow.md](reference/settings-flow.md) | Each setting: where it's stored, who reads it on each path (player, export, photo, moment, Library, stream), its twins and their guards |
| [reference/glossary.md](reference/glossary.md) | The words: moment, still, render, recipe, bundle, location, link, own file, GPU table… |

The generated lists in contracts.md are made by `python3 tools/gen_reference.py`; `tests/reference_test.py` fails when
they're out of date, when a link in any Markdown file is broken, or when a doc isn't listed here.

## Dev: how to work on it

| Doc | What |
|---|---|
| [dev/testing.md](dev/testing.md) | Every suite and how to run it, what CI runs, the live rigs (IPC, panel render, pipebench, catalog on the NAS, Library page) |
| [dev/release.md](dev/release.md) | A tag and `release.yml`, our mpv build, trial apps with `tools/dev_app.sh` and the Launch Services check, the Windows app, the NAS Helper image, the website |
| [dev/gotchas.md](dev/gotchas.md) | Traps by area: libplacebo shaders, Lua and mpv, the display, caches, SMB, AVFoundation… |

## Design: what was decided, and why

| Doc | Status |
|---|---|
| [design/360-cameras.md](design/360-cameras.md) | Built in #140 (v0.9.0), the sphere on the GPU since #144; some rigs and compass north not built |
| [design/360-reframe.md](design/360-reframe.md) | Built in #166, #171, #172, #177 (v0.11.0); its "Later" list not built |
| [design/albums-photos-share-concept.md](design/albums-photos-share-concept.md) | Partly built: albums, photo links, Timeline and Map, Share to a folder over the render cache (v0.14.0); smart albums, Apple Photos and more not built |
| [design/catalog-safeguards.md](design/catalog-safeguards.md) | Built in #250 (v0.14.0) and #284 |
| [design/export-gpu-path.md](design/export-gpu-path.md) | Built in #245, #247 (v0.14.0) |
| [design/export-settings.md](design/export-settings.md) | Built in #254 (v0.14.0) |
| [design/geolocation.md](design/geolocation.md) | Built in #188, #200, #239; a few parts not built |
| [design/gpu-render.md](design/gpu-render.md) | Built from #144 (v0.10.0) on; the player's only render path since #304 |
| [design/gpu-table-ahead.md](design/gpu-table-ahead.md) | Built in #272 (v0.14.0) and #279 |
| [design/library-management.md](design/library-management.md) | Partly built: locations, pointers, the project is its files (§8, #182), the catalog (§9); Apple Photos and more not built |
| [design/library-responsiveness.md](design/library-responsiveness.md) | Built in #236, #237, #248, #260 (v0.14.0) |
| [design/live-photos.md](design/live-photos.md) | Built in #267 (v0.14.0); HDR stills on Windows and Linux not built |
| [design/lossless-cut.md](design/lossless-cut.md) | Built in #123 (v0.8.0); DJI only |
| [design/luts.md](design/luts.md) | Partly built: the makers' log curves to HLG, NBCU's SDR, DJI's LUT |
| [design/mobile-app.md](design/mobile-app.md) | Proposed, not built (an iPhone prototype on an unmerged branch, #168) |
| [design/moments-and-workspaces.md](design/moments-and-workspaces.md) | Built in #124 (v0.8.0) and #126 (v0.9.0); later changed by "the project is its files" (#182) |
| [design/navigation-bar.md](design/navigation-bar.md) | Built in #166 (v0.11.0) and #269 |
| [design/nle-markers.md](design/nle-markers.md) | Proposed, not built |
| [design/notifications.md](design/notifications.md) | Built in #259 (v0.14.0); a few prompts still use the console |
| [design/optical-without-gyroflow.md](design/optical-without-gyroflow.md) | Built in #285, #288-#290, #296, #297 (via #300, untagged) |
| [design/responsiveness-journal.md](design/responsiveness-journal.md) | Built in #266 (v0.14.0) and #278 |
| [design/seek-and-scrub.md](design/seek-and-scrub.md) | Built in #286, #287 (via #300, untagged); afp-scrub on Windows not built |
| [design/tags-and-culling.md](design/tags-and-culling.md) | Partly built: own store, Finder tags, culling, comments (v0.14.0); XMP sidecars, Explorer, NAS photo apps not built |
| [design/usage-trace.md](design/usage-trace.md) | Built in #366 (untagged); the Library's markers have no picture |

Each file's own status line says more, with what isn't built.

## Archive: history

Kept as written; each starts with why it was archived. Their lasting facts are in reference/ and dev/.

| Doc | What it was |
|---|---|
| [archive/360-implementation-plan.md](archive/360-implementation-plan.md) | The 360 build plan and its per-step log (shipped in v0.9.0) |
| [archive/360-throughput-spike.md](archive/360-throughput-spike.md) | Two lens tracks through lavfi-complex and VapourSynth (superseded by the GPU sphere) |
| [archive/gpu-render-spike.md](archive/gpu-render-spike.md) | The GPU warp's first spike and measurements |
| [archive/upscaling-attempt.md](archive/upscaling-attempt.md) | MetalFX/FSR and photo upscalers, tried and set aside |
| [archive/albums-photos-share-comparison.md](archive/albums-photos-share-comparison.md) | The Library against other media managers, a what-if |
| [archive/night-integration.md](archive/night-integration.md) | The night of 2026-11-15/16: its branches in one (merged as #241) |
| [archive/night-handover-albums.md](archive/night-handover-albums.md) | Albums step 1 (#235) |
| [archive/night-handover-culling.md](archive/night-handover-culling.md) | Culling (#233) and Open at start (#234) |
| [archive/night-handover-media-types.md](archive/night-handover-media-types.md) | Albums step 2: photo links, two tabs (#238) |
| [archive/night-handover-responsive.md](archive/night-handover-responsive.md) | Screen-first writes and the pinpoint update (#236, #237) |
| [archive/night-handover-share.md](archive/night-handover-share.md) | Share and the render cache (#240) |
| [archive/night-handover-timeline-map.md](archive/night-handover-timeline-map.md) | Timeline and Map (#239) |
| [archive/library-ui-fixes.md](archive/library-ui-fixes.md) | Four Library page fixes after the first real try (#243) |
| [archive/share-fixes.md](archive/share-fixes.md) | Share and photo links on real clips (#242) |

## Adding a doc

- A new design goes into `design/` with `**Status:** proposed (not built).` under its title, and a line here.
  When it's built, change its status in the same PR, and move what it says about how things work into reference/.
- A handover or a spike's notes go into `archive/` once their facts are in reference/ or dev/.
- `tests/reference_test.py` fails until the doc is listed here.
