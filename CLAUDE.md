# ActionFootagePlayer: rules for every session

Read this before changing anything. It holds the rules and the things that must agree with each other. Many sessions
work on this repository at once, and this file is what they all share.

- **How it is:** `docs/reference/`: the parts and processes (`architecture.md`), every file kept
  (`files-and-formats.md`), the contracts between processes (`contracts.md`), each setting on each path
  (`settings-flow.md`), the words (`glossary.md`). **How to work on it:** `docs/dev/` (`testing.md`, `release.md`,
  `gotchas.md`). Index with each doc's status: `docs/README.md`. The code is the truth: where a doc disagrees, fix
  the doc.
- **Keep them true in your PR.** A change to a format, a contract or a setting's path changes its reference doc in the
  same PR. Script messages, `user-data` keys, `AFP_*` variables and `gyroflow-export` flags are listed by
  `python3 tools/gen_reference.py`; `tests/reference_test.py` fails until you run it, and on any broken link.
- **A rule or a trap that will matter again** goes here or into `docs/dev/gotchas.md` in your PR, not only into your
  own memory. Designs go into `docs/design/` with a status line (see `docs/README.md`).

## The parts

- **Player:** mpv with the Lua plugins in `mpv/scripts/` (workspace, gyroflow, dlogm, side-panel, view360, live-photo,
  footage-menu, …).
  - There is one render path, the **GPU path**: the `gyroflow-gpu-table` server plus `shaders/gyroflow/warp.glsl` in
    libplacebo (mpv 0.42 or our own, vo=gpu-next). The VapourSynth path was taken out (2026-11). What the warp can't do
    (`gpu_table.rs` `unsupported`) and an mpv that can't run it play as recorded, with a card saying why.
  - Our mpv is built by `tools/build_mpv.sh`, with the patches in `tools/mpv-patches/` and
    `tools/libplacebo-patches/`.
- **vs-gyroflow/** (Rust): the engine (`lib.rs`'s `Engine`, which the export's pipes path renders with), plus the
  binaries in `src/bin/` (`gyroflow-export`,
  `gyroflow-gpu-table`, `gyroflow-probe`, `afp-*` …). `gyroflow-export` makes exports, photos, Live Photos and moments.
- **catalog/** (`afp-catalog`): the project, its journal and snapshots, and the safeguards.
- **library/**: the Library app (Tauri, `src/` plus `ui/`). It links `afp-catalog` only.
  - **Never link vs-gyroflow or mpv into it.** It runs them as processes (`AFP_BATCH`, `gyroflow-probe`, `afp-meta`).
- **tools/nas-helper/**, **tools/stream/**: the NAS Helper container (catalog guardian, `afp-stream`, Jellyfin links).

## Before you start

- **Work in your own worktree, never in the shared checkout.** The shared checkout is `…__local`, which other
  sessions use. Start like this:

  ```
  git worktree add -b claude/<name> .claude/worktrees/<name> origin/main
  ```

- **Check what open PRs touch the files you'll change** with `gh pr list` and `gh pr view N --json files`.
  - If another open PR is in the same big file (`library/src/main.rs`,
    `library/ui/app.js`, `vs-gyroflow/src/lib.rs`, `library/src/index.rs`), say so to Anton before starting.
  - Better still, build on that PR or wait for it.
- **Keep stacks to two or three PRs deep**, merged bottom first.
  - Aim for one feature per PR, about 1,500 lines at most.
  - A PR that replaces another closes it at once, saying "replaced by #N".

## A feature works on every path, or says where it doesn't

Most regressions so far were a feature that worked in the live player but not on another path (#128, #183, #193,
#271, #291). For any change to what the picture or a setting does, go through every path below. In the PR, mark each
one *done*, *n/a because…*, or *not done → issue #N*:

1. Player (`gpu_table.rs`, `warp.glsl`, `user_shader.rs`).
2. Export (`gyroflow-export`), on both its GPU path and its pipes path (`lib.rs`'s engine). It takes the player's
   settings JSON as they are (`--settings`, keys in registry.json `settings_json`, read by `apply_settings`): the clip
   playing's (`gpu_settings`) and a saved view's (`gyroflow/settings.lua` `of_view`). Never pass it a flag a key
   stands for (`tests/settings_json_test.py`).
3. Photo, Live Photo and Motion Photo (`still.rs`, `photo`).
4. Moment, and the Save frame view.
5. Library: the render recipe and its hash (`library/src/renders.rs`, `BUNDLE_KEYS`/`valid`/`defaults_of`), Source
   settings, and batch renders (`AFP_BATCH`).
6. NAS stream (`tools/stream/afp_stream.py`) when it changes what the player sends.
7. Windows and Linux packaging: `install.sh`, `tools/make_app.sh`, `tools/make_windows_app.sh`. The test
   `tests/packaging_test.py` checks the Lua modules.

### The same logic is in more than one language

Plain lists live in one file, `mpv/scripts/workspace/registry.json`: the curves, an export's settings, the transform's
keys (values, defaults, gyroflow.conf names, recipe/exported, gyroflow-export flags, gpu-table fields) and the project's
own folders. Change a list there, never in a reader. The player reads it with `require 'registry'` (registry.lua, a
JSON reader of its own), vs-gyroflow and the Library with `include_str!` into `afp_catalog::registry`'s types
(`lists.rs` in each), the Library's page as `window.AFP_LISTS` (put there by `main.rs` before its scripts;
`library/ui-tests/lists.js` for the rigs). The catalog, built from its folder alone, keeps consts that its
`registry.rs` tests check. `tests/registry_test.py` checks each reader reads it and flags a line that quotes several
names of one list (a copy creeping back), with an allow-list saying why. Plugins zipped alone carry a copy of
registry.lua/json (`tests/packaging_test.py`).

Rust that the Library and vs-gyroflow both need lives once, in `afp-catalog` (which both link; the Library links
nothing else): the workspace and pointer lookup, a clip's own file and a file's fingerprint (`workspace.rs`, over
`locations.rs`), a photo's XMP link and tags (`xmp.rs`), Finder's tags (`finder.rs`). Never copy it into either; it
must build in `catalog/Dockerfile` (its folder alone, `--locked`: no new crates without a reason).

What's left is logic in more than one language. Change these copies together, in the same PR. The test after each
fails when they drift apart; a guard that reads the sources has an allow-list for what it can't resolve, with why (add
to it rather than around it).

- **Workspace and pointer lookup:** `workspace.lua` and the catalog's `workspace.rs`/`locations.rs` (vs-gyroflow's
  `workspace.rs` re-exports them). The shared cases are in `tests/workspace_cases.txt`, run by both.
- **Render settings:**
  - `mpv/scripts/workspace/transform.lua` and `library/src/renders.rs`, shared cases in `tests/render_settings.json`;
    the keys and their valid values are the registry's; what it says of the plugin's options, gyroflow-export's flags
    and the warp's request: `tests/render_keys_test.py`;
  - the gyroflow.conf defaults: `gyroflow/bundle.lua` `defaults_bundle` and `renders.rs` `defaults_of`, shared cases in
    `tests/gyroflow_conf_cases.json` (`tests/gyroflow_conf_test.lua` and cargo). The conf is read as mpv's
    `options.lua` reads it: nothing trimmed, booleans only `yes`/`no`;
  - the render recipe: `renders.rs` `VERSION` goes with what a recipe holds, and the hashes stay
    (`the_recipe_goes_with_its_version_and_keeps_its_hash`); the moment renders at a moment's path, workspace
    `renders.lua`'s `RENDER_VERSION`/`render_recipe` and `index.rs` `render_recipe_of` (`render_recipe_twin`). Both
    take the clip's lens as the probe says it (`lens_info` `from`/`print`, `gyroflow-probe --lens-id`) and its
    `.afp-motion`'s fingerprint (#318).
- **Curves** are the registry's. Still apart: `export.lua`'s mapping before `--curve` (a curve the player plays must
  reach the export as one of the registry's), the packagers' `make_hdr_lut.py` curves (each curve's `lut` in `luts/` or
  made), the cameras' `DJI_CURVES`/`GOPRO_CURVES` and `probe.rs`, `catalog/src/range.rs` `log_curve`, gyroflow-export's
  help. `tests/curves_twin_test.lua`, the catalog's `registry.rs` tests.
- **Export choices** are the registry's. Still apart: what `gyroflow-export` takes (`--resolution`, `--encode`), the
  plugin's `export_*`/`photo_*` option defaults (`tests/export_choices_test.py`) and `still.rs` `MAX_RADIUS`
  (vs-gyroflow's `lists.rs` test).
- **The project's own folders** (`_Moments`, `_Stills`, `_Renders`, `Generated`, `.afp`, …) are the registry's
  `own_folders` (index.rs, checks.rs, moments.lua read it; `catalog/src/inspect.rs` `OWN_FOLDERS` is a checked const;
  `tools/stream/afp_stream.py` leaves out every `_` folder). A folder made has to be left out by each:
  `tests/project_folders_test.py`.
- **Script messages and key bindings:** every `script-message-to <plugin> <msg>` (Lua, the Library's IPC, the rigs)
  has its `register_script_message` in that plugin; a binding named like a message takes it over unless the message is
  registered after it (#225). `tests/script_messages_test.py`.
- **user-data:** every `user-data/<key>` read is written somewhere (`tests/user_data_test.py`).
- **The probe's answer:** what `gyroflow-probe --library` answers and `library/src/index.rs` `GEOTAG` (and the probe's
  own `"geotag"`) change together: `index.rs`'s `probe_version` tests.
- **Tags:** `tags.lua` and `library/src/tags.rs` (over the catalog's `finder.rs` and `xmp.rs`).
- **The clip's own file (`<clip>.afp.json`)** is found by `workspace.lua`'s `own_file` (`sidecar.lua` writes it) and
  the catalog's `workspace.rs` (`own_at`, `own_file`), which the Library (`tags.rs` `clip_own`) and vs-gyroflow
  (`workspace::own_file_of`: reframe, lens, water, geotag) use: `tests/workspace_cases.txt`.
- **The 360 view:** `view360.glsl`, `view360.wgsl` and `view.lua`.
- **The underwater colour's rule:** `dlogm/uw_estimate.lua` and `vs-gyroflow/src/uwcolor.rs`, shared grids in
  `tests/uwcolor_cases.json` (`tests/uwcolor_test.lua` writes them, cargo reads them).

## Stored data: the rules Anton set

- **The project is its files.** `Project.afproject` keeps only `locations`, `hosts` and `id`. Everything else lives in
  the files themselves (sidecars, XMP, links) or in a per-computer cache. Never add new state to `Project.afproject`.
  (`docs/design/library-management.md` §8. What it still holds against the rule, the Library's `settings`
  and `generated_runs`: `docs/reference/files-and-formats.md`.)
- **Raise the version whenever what's kept changes.**
  - What the catalog or project keeps: `afp_catalog::project::VERSION` and `projectfile.lua` `M.VERSION`, together.
    The test `what_is_kept_goes_with_the_projects_version` guards this.
  - What the probe answers: `library/src/index.rs` `GEOTAG`. If you forget it, the old answers stay cached (#227).
    The test `geotag_goes_with_what_the_probe_answers` guards this.
  - The render recipe: as above.
  - Older programs must refuse newer data, never wipe it (#220, #249).
- **A `.afplocation` pointer counts only for its project's id.**
  - When the path fails, fall back to this computer's record by id.
  - Never search shares by name.
  - Write hosts into every pointer.
  - (#299; keep workspace.lua and the catalog's `pointer_project` in step.)
- **Migrations are one-off scripts** in `tools/one_off/`, run once on Anton's library when he says so. There is no
  migration feature or dual-format reader in the product.
- **The NAS runs only what Anton starts.** No background pre-renders or scans on the NAS. Measure its disk I/O before
  and after any change there.

## Tests

- **CI for Anton's branches and main runs on his Mac.** The `macos` and `tests` jobs run on the self-hosted runner
  `anton-mac` (`~/actions-runner`, a service under his user, label `afp`). Releases (tags) and fork PRs run on
  GitHub's macOS. The runner keeps its build folders between runs, so prefer CI over a build of your own in a worktree
  (each worktree's `target/` costs 4-20 GB). It only runs while the Mac is awake and Anton is logged in. Required on
  a PR: `macos` and `tests`; the PR needn't be up to date with `main`. A draft PR runs only the quick tests: mark it
  ready for review when Anton should try its build. The Windows, Linux and NAS builds run only on pushes to `main` and on releases: after a
  merge, check `main`'s run (`gh run list --branch main`) and fix a break there in its own PR.
- **Everything CI runs, locally:**

  ```
  sh tools/run_tests.sh                      # every tests/*_test.{lua,py,sh}, found by name
  (cd vs-gyroflow && cargo test --release)
  (cd catalog && cargo test)
  (cd library && cargo test)
  (cd library/ui-tests && npm ci && npm test) # the Library page in jsdom
  ```

  Cargo needs `$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin` on PATH.
- **A new test needs no list.** Name it `tests/<what>_test.lua|py|sh`, or put it in a crate's `#[cfg(test)]` or
  `tests/`, or in `library/ui-tests/*_test.js`.
- **A bug fixed gets a test that fails without the fix**, whenever the bug can be reached without a screen.
- **Live rigs are not in CI:** `tests/*_ipc.py`, `tests/*_e2e.py`, `tests/photo_link_frame.py`, `tools/pipebench`,
  the panel render rig. Run them yourself for changes they cover, and say which you ran. How to run each:
  `docs/dev/testing.md`.
- **Ask Anton first before any test that takes the screen** (opens windows in a loop, takes focus, keeps the display
  awake). Ask again after any break. mpv's gpu-next hangs while the Mac is locked.
- **"Not verified" is a debt, not a footnote.** Every "not tried / not verified" line in a PR gets an issue labelled
  `needs-live-check`. Don't merge with "test later" unless Anton says so for that PR.

## PRs and builds

- **Fill in the PR template** (`.github/pull_request_template.md`).
- **PR titles say what the user gets**, in plain words, the way the existing PRs do.
- **UI changes carry before/after images in the PR itself.** UI means the side panel, the Library window, menus or
  the site. Use the panel render rig, headless Chrome, or a capture made with Anton's OK.
- **Before asking to merge:** rebase on the current main, run the tests above again, and check that CI is green.
- **A fix made on a trial or integration branch goes back into its PR, or into its own PR.** It must never live only
  on a local trial branch.
- **An integration PR's tree must equal the build Anton tried** (`git diff <trial> HEAD` empty).
- **Builds come from CI, not from this Mac or the NAS** (Anton, 2026-11-26: memory is the bottleneck on both,
  the internet is fast). Never `docker build` locally, on the Mac or the NAS: the NAS Helper image is CI's `nas-helper`
  artifact (main's run, or `gh workflow run release.yml --ref <branch>` to have GitHub build a branch). A trial app is
  CI's `release-macos` artifact of the commit: downloaded (say the file and size), its two apps taken out, the download
  deleted (`docs/dev/release.md`). `tools/dev_app.sh`, never `install.sh --apps`, only when CI hasn't built that tree.
- **After installing a trial app:**
  - check `Contents/Helpers/mpv --version`;
  - unregister every other copy from Launch Services;
  - check that an MP4 opens the copy in `~/Applications`.
- **Merging to main and tagging a release are Anton's decisions.**

## Traps

The ones that bite most often; the rest, by area, are in `docs/dev/gotchas.md`.

- **The Lua 200-locals limit.** A chunk holds at most 200 locals. `workspace/main.lua` and `gyroflow/main.lua` are
  only their plugins' wiring: the parts are modules that share `W` (and the gyroflow plugin's `S`,
  `docs/dev/gotchas.md`). Put new code in the part it belongs to, or a module of its own, not in a
  `(function() end)()` block. Add a new module to the packagers; `packaging_test.py` checks it.
- **libplacebo user shaders:**
  - `//!` inside comments is read as a directive;
  - `keepaspect=no` drops margins;
  - the frame count counts every pass;
  - r16f textures read 0.
- **mpv script messages and key bindings share one namespace.** A binding named like a message takes it over (#225).
- **Windows checkouts may have CRLF line ends.** Shaders and parsers must accept them (fa28a24).
