# ActionFootagePlayer: rules for every session

Read this before changing anything. It holds the rules and the things that must agree with each other. How each part
works is in README.md ("How it works", "Layout") and docs/. Where a doc says "design" or "not built yet", check the
code, which is the truth. Many sessions work on this repository at once, and this file is what they all share. When
you learn a rule or a trap that will matter again, add it here (or to the doc it belongs to) in your PR, not only to
your own memory.

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
  - If another open PR is in the same big file (`workspace/main.lua`, `gyroflow/main.lua`, `library/src/main.rs`,
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
2. Export (`gyroflow-export`), on both its GPU path and its pipes path (`lib.rs`'s engine). Check what `export.lua`
   passes, from both the live state (`as_it_plays`) and the saved bundle.
3. Photo, Live Photo and Motion Photo (`still.rs`, `photo`).
4. Moment, and the Save frame view.
5. Library: the render recipe and its hash (`library/src/renders.rs`, `BUNDLE_KEYS`/`valid`/`defaults_of`), Source
   settings, and batch renders (`AFP_BATCH`).
6. NAS stream (`tools/stream/afp_stream.py`) when it changes what the player sends.
7. Windows and Linux packaging: `install.sh`, `tools/make_app.sh`, `tools/make_windows_app.sh`. The test
   `tests/packaging_test.py` checks the Lua modules.

### The same logic is in more than one language

Change these copies together, in the same PR. The test after each fails when they drift apart; a guard that reads the
sources has an allow-list for what it can't resolve, with why (add to it rather than around it).

- **Workspace and pointer lookup:** `workspace.lua`, `vs-gyroflow/src/workspace.rs` and `catalog/src/locations.rs`.
  The shared cases are in `tests/workspace_cases.txt`, run by all three (the catalog: locations, `relative`,
  `original`).
- **Render settings:**
  - `mpv/scripts/workspace/transform.lua` and `library/src/renders.rs`, shared cases in `tests/render_settings.json`;
    the keys and their valid values: `tests/render_keys_test.py`;
  - the gyroflow.conf defaults: `gyroflow/main.lua` `defaults_bundle` and `renders.rs` `defaults_of`, shared cases in
    `tests/gyroflow_conf_cases.json` (`tests/gyroflow_conf_test.lua` and cargo). The conf is read as mpv's
    `options.lua` reads it: nothing trimmed, booleans only `yes`/`no`;
  - the render recipe: workspace `RENDER_VERSION` and `renders.rs` `VERSION`.
- **Curves:** `dlogm/main.lua` `CURVES` and `gyroflow-export.rs` `CURVES`. A curve the player offers must either be
  accepted by the export or be mapped before `--curve`. Also the packagers' `make_hdr_lut.py` curves (each export
  curve needs its `luts/<id>_hlg.cube`), the Library's `app.js` `SRC_CURVES`, `tags.rs`'s curve list and `albums.js`
  `ALBUM_CURVES`. `tests/curves_twin_test.lua`.
- **Export choices:** `gyroflow/exportsettings.lua`, `library/ui/share.js` and `renders.rs` `EXPORT_CHOICES`, plus what
  `gyroflow-export` takes (`--resolution`, `--encode`) and `still.rs` `MAX_RADIUS`. `tests/export_choices_test.py`.
- **The project's own folders** (`_Moments`, `_Stills`, `_Renders`, `Generated`, `.afp`, …) are left out of the
  footage by `library/src/index.rs` `OWN_FOLDERS`, `catalog/src/inspect.rs` `OWN_FOLDERS`, `library/src/checks.rs`
  `OWN`, `workspace/moments.lua` `OWN_FOLDERS` and `tools/stream/afp_stream.py`. A new folder goes into each.
  `tests/project_folders_test.py`.
- **Script messages and key bindings:** every `script-message-to <plugin> <msg>` (Lua, the Library's IPC, the rigs)
  has its `register_script_message` in that plugin; a binding named like a message takes it over unless the message is
  registered after it (#225). `tests/script_messages_test.py`.
- **user-data:** every `user-data/<key>` read is written somewhere (`tests/user_data_test.py`).
- **The probe's answer:** what `gyroflow-probe --library` answers and `library/src/index.rs` `GEOTAG` (and the probe's
  own `"geotag"`) change together: `index.rs`'s `probe_version` tests.
- **Tags:** `tags.lua`, `library/src/tags.rs` and `catalog/src/finder.rs`.
- **The clip's own file (`<clip>.afp.json`)** is found in `sidecar.lua`, `workspace.rs`, `tags.rs` (`clip_own_in`) and
  `reframe.rs`.
- **The 360 view:** `view360.glsl`, `view360.wgsl` and `view.lua`.

## Stored data: the rules Anton set

- **The project is its files.** `Project.afproject` keeps only `locations`, `hosts` and `id`. Everything else lives in
  the files themselves (sidecars, XMP, links) or in a per-computer cache. Never add new state to `Project.afproject`.
  (`docs/library-management.md` §8.)
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
  - (#299; keep workspace.lua, workspace.rs and the catalog's `pointer_project` in step.)
- **Migrations are one-off scripts** in `tools/one_off/`, run once on Anton's library when he says so. There is no
  migration feature or dual-format reader in the product.
- **The NAS runs only what Anton starts.** No background pre-renders or scans on the NAS. Measure its disk I/O before
  and after any change there.

## Tests

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
  the panel render rig. Run them yourself for changes they cover, and say which you ran.
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
- **Build a trial app with `tools/dev_app.sh`,** not `install.sh --apps`. Then:
  - check `Contents/Helpers/mpv --version`;
  - unregister every other copy from Launch Services;
  - check that an MP4 opens the copy in `~/Applications`.
- **Merging to main and tagging a release are Anton's decisions.**

## Traps

- **The Lua 200-locals limit.** `workspace/main.lua` sits at Lua's 200 top-level locals, and `gyroflow/main.lua` near
  it (~170 since the VapourSynth path left, 2026-11). Put
  new code in a module of its own (as `transform.lua`, `sidecar.lua` and `ahead.lua` are), not in another
  `(function() end)()` block. Add the module to the packagers; `packaging_test.py` checks it.
- **libplacebo user shaders:**
  - `//!` inside comments is read as a directive;
  - `keepaspect=no` drops margins;
  - the frame count counts every pass;
  - r16f textures read 0.
- **mpv script messages and key bindings share one namespace.** A binding named like a message takes it over (#225).
- **Windows checkouts may have CRLF line ends.** Shaders and parsers must accept them (fa28a24).
