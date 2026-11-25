# Releases and builds
Checked against 295101c (2026-11-25).

How a release is made, how to build a trial app for Anton, and how the other parts (Windows, Linux, the NAS Helper,
the website) are built and shipped. Tests are in [testing.md](testing.md).

## Who decides

- **Merging to `main` and tagging a release are Anton's decisions.** "Finished" or "CI green" is not a go.
- Before a release, build the combined candidate locally and install it for Anton to try. Merge and tag only after
  his OK. The PR's tree must equal the build he tried (`git diff <trial> HEAD` empty).
- Deploying the website and changing anything on the NAS also need his explicit go.
- `main` is protected, admins included: a PR, up to date with `main`, with the required checks green.
- A token without GitHub's `workflow` scope can't push or merge changes under `.github/workflows/`.

## A release, from the tag

```
git tag -a v0.15.0 -m "<release notes>"
git push origin v0.15.0
```

- The tag must be **annotated**. Its message becomes the release notes (`gh release create --notes-from-tag`). The
  `publish` job refuses a lightweight tag.
- The version is the tag without `v`. Untagged CI runs use `dev-<sha7>`.
- [.github/workflows/release.yml](../../.github/workflows/release.yml) builds everything, then `publish` uploads it
  as a GitHub release with one `SHA256SUMS` for all files, then `website` redeploys the site so its download page
  shows the new release.

What each job builds:

| Job | Runner | Makes |
|---|---|---|
| `macos` | macos-15 (Apple Silicon) | `tools/build_mpv.sh` (cached), then `tools/make_release.sh` with `BUNDLE_RUNTIME`: the `.dmg` and `.zip` (player + Library), the source `.tar.gz` with prebuilt `bin/`, and one zip per mpv plugin. |
| `plugin` | ubuntu-22.04, windows-2022, windows-11-arm | `tools/make_plugin_zip.sh`: `mpv-gyroflow-<v>-<os>-<arch>.zip`. On Windows also `tools/make_windows_app.sh`: `ActionFootagePlayer-<v>-windows-<arch>-setup.exe`, then a smoke test. |
| `nas-helper` | ubuntu-latest | `tools/nas-helper/Dockerfile`: `ActionFootageNASHelper-<v>-linux-x86_64.tar.gz` (`docker save`) and `ActionFootageNASHelper-<v>-compose.yaml`. |
| `plugins` | ubuntu-latest | `tools/make_plugin_bundles.py`: `mpv-plugins-<v>-<os>-<arch>.zip`, all plugins per system. |
| `tests` | macos-15 | The catalog, Library and Library page tests (not a build). |
| `publish` | ubuntu-latest | Tags only: the GitHub release. |
| `website` | (the website branch's workflow) | Tags only: the site redeployed with `latest.json`. |

Pull requests run the same builds as checks. Their files are kept as the run's artifacts (`release-macos`,
`release-windows-x86_64`, `release-windows-arm64`, `release-linux-x86_64`, `release-plugins`, `nas-helper`). Fetch one
with `gh run download <run id> -n <artifact>`.

## The macOS release: tools/make_release.sh

```
MPV_BIN=build/mpv-afp/out/mpv BUNDLE_RUNTIME=$(brew --prefix) tools/make_release.sh <version>
```

- Builds `vs-gyroflow` and `library` in release, from HEAD. It refuses uncommitted changes in `vs-gyroflow`.
- The app is `tools/make_app.sh`; the Library is `tools/make_library_app.sh`.
- With `BUNDLE_RUNTIME`, `tools/bundle_runtime.py` copies mpv, ffmpeg, what they link and Python into the app, so it
  needs no Homebrew. The bottles set the oldest macOS the app runs on.
- Binaries are stripped and signed ad hoc. Nothing is notarized.
- Output in `dist/`, with its own `SHA256SUMS`.

## Our mpv: tools/build_mpv.sh

```
tools/build_mpv.sh                 # brew deps, then the build into build/mpv-afp/out/mpv
tools/build_mpv.sh --no-deps       # skip brew install
tools/build_mpv.sh --print-deps    # versions of what it links, for CI's cache key
```

- [tools/mpv.lock](../../tools/mpv.lock) pins mpv (`MPV_COMMIT`, upstream master) and libplacebo (`LIBPLACEBO_TAG`,
  `LIBPLACEBO_COMMIT`). libplacebo is built as a meson subproject and linked in statically.
- `tools/mpv-patches/*.patch` are applied with `git am` (they are `git format-patch` output of the branch they were
  written on). `tools/libplacebo-patches/*.patch` are applied with `git apply` to a clean libplacebo tree each build.
- It refuses to build when the mpv clone in `build/mpv-afp/src/mpv` has uncommitted changes. Don't edit there.
- `build/mpv-afp/out/mpv.lock` records the lock, `MPV_DESCRIBE` and the patch lists. The app's
  `Resources/ThirdParty/SOURCES.txt` is made from it.
- CI rebuilds mpv when the lock, the script, a patch or a linked Homebrew library changes.

To change a patch: clone mpv at `MPV_COMMIT` somewhere else, `git am` the patches, change and commit there, then
`git format-patch <MPV_COMMIT> -o tools/mpv-patches/` (keep the numbering). Rebuild and run the rigs the patch
touches. To move mpv: change `MPV_COMMIT`, rebase the patch branch, write the patches again.

What the patches add, by name: presentation through `AVSampleBufferDisplayLayer` with Dolby Vision RPUs (0001-0007,
0016), trackpad gestures to scripts (0008, 0012), the Metal layer's HDR state (0009), latency logging (0010), redraw on
a shader parameter change (0011), no vsync wait in a window (0013), `--video-join` for two lens tracks (0014),
unloading user shaders no longer listed (0015). libplacebo's 0001 adds the linearize helpers only to hooks that use
them (docs/design/gpu-render.md, "Pipeline builds").

## A trial build for Anton: tools/dev_app.sh

```
tools/build_mpv.sh            # once per worktree, or MPV_BIN=<another worktree>/build/mpv-afp/out/mpv
tools/dev_app.sh              # build and install into ~/Applications
tools/dev_app.sh --no-install # only build/ActionFootagePlayer.app and build/ActionFootageLibrary.app
```

- Use this, not `install.sh --apps`. It builds the app the way the release does.
- It installs the player **and** the Library into `~/Applications`. The replaced player is kept as
  `build/ActionFootagePlayer-previous.app`. It runs the setup (DJI's LUT, Quick Actions, the MP4 default).
- The version is `<last tag>-dev.<commits>+<sha>`, `.dirty` with uncommitted changes. Uncommitted changes are built.
- Its mpv is `build/mpv-afp/out/mpv` when built, else `MPV_BIN`, else Homebrew's. **Homebrew's mpv (0.41.0) can't
  run the GPU path: clips then play as recorded.** It only prints a "Note:".
- Its ffmpeg for exports is built with libplacebo into `~/Library/Caches/afp-ffmpeg-build` (about 10 minutes, once
  per patch set). `NO_GPU_FFMPEG=1` takes Homebrew's (the export then takes the pipes).
- `APPS_DIR` installs elsewhere; then it leaves the MP4 default alone.
- Anton's standing rule: when side sessions' PRs are ready, merge them into a local trial branch, build and install
  into `~/Applications` without asking, unless parallel work conflicts. Don't kill a running app to install. When
  he asks for the trial "beside" the installed app, put it elsewhere and unregister it.

### After every install

1. The mpv is ours:

   ```
   ~/Applications/ActionFootagePlayer.app/Contents/Helpers/mpv --no-config --version | head -1
   ```

   A plain `mpv v0.41.0` is Homebrew's (no GPU path). Ours names a commit (`v0.41.0-<n>-g<hash>`), matching
   `MPV_DESCRIBE` in `build/mpv-afp/out/mpv.lock`.
2. Only the `~/Applications` copy is registered with Launch Services (next section).
3. An MP4 opens the `~/Applications` copy:

   ```
   f=$(mktemp -d)/x.MP4; : > "$f"
   osascript -l JavaScript -e "ObjC.import('AppKit'); \$.NSWorkspace.sharedWorkspace.URLForApplicationToOpenURL(\$.NSURL.fileURLWithPath('$f')).path.js"
   ```

   It must print `~/Applications/ActionFootagePlayer.app` (expanded). If not, run
   `~/Applications/ActionFootagePlayer.app/Contents/Resources/setup.sh default`.
4. Say in your report that you checked these.

### Launch Services

Every copy of the app registers the same bundle id, `io.github.anton-belkin.action-footage-player`: worktrees'
`build/` apps, `-previous.app`, the rigs' clones in `$TMPDIR`, copies in the Trash. A double click may then open any
of them. List them:

```
LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
$LSREG -dump | awk -v id=io.github.anton-belkin.action-footage-player '
  /^-{20,}/ { if (hit && p) print p; p = ""; hit = 0; next }
  /^path:/ { sub(/^path: +/, ""); sub(/ \(0x[0-9a-f]+\)$/, ""); p = $0 }
  /^identifier:/ { if ($2 == id) hit = 1 }
  END { if (hit && p) print p }' | sort -u
```

Unregister each one except the installed app, then register that one again:

```
$LSREG -u "<path>"                       # for each other copy
$LSREG -f ~/Applications/ActionFootagePlayer.app
```

Do the same for the Library (`io.github.anton-belkin.action-footage-library`). Opening a test copy later registers it
again: repeat after live tests.

## Windows

- `tools/make_windows_app.sh <version> [<rust target>]` runs on Windows in Git Bash. It needs Inno Setup (`ISCC`) and
  7-Zip. CI installs both with Chocolatey.
- It builds `vs-gyroflow`, `windows/launcher` and `library`, then downloads what the app carries into
  `build/windows-deps/`, each checked against a pinned SHA-256: mpv (shinchiro's build), Python (NuGet), the Vulkan
  loader, ffmpeg (BtbN's GPL build). The installer is `windows/installer.iss`, per user, no admin.
- To update a dependency, download it, update URL and SHA-256 in the script, for **both** architectures.
- You can't build it on the Mac. Take the `release-windows-*` artifact of a PR run.
- Anton has used the x86-64 app (alpha, little testing). Don't call Windows untried.

### arm64

- Built on GitHub's `windows-11-arm` runner, target `aarch64-pc-windows-msvc`. Everything in mpv's process is arm64.
- shinchiro publishes the arm64 mpv only on GitHub, for a few weeks. The pinned copy lives in this repository's
  `windows-deps` pre-release. To move mpv, upload the new `.7z` there and update URL and SHA-256.
- BtbN's `winarm64` ffmpeg has no NVENC, Quick Sync or AMF. Exports use libx265 (or the GPU path's `hevc_vulkan`).
- As far as known, only the CI smoke test has run it, on a runner without a GPU: the stabilization itself is untried
  there.

## Linux

- There is no Linux app package. The release has `mpv-gyroflow-<v>-linux-x86_64.zip` and the plugin bundle, unzipped
  into `~/.config/mpv/scripts/`. From a checkout, `./install.sh` builds and installs all the plugins; `--apps` adds a
  menu entry and the MP4 default, with the system's mpv.
- The stabilization needs the distribution's mpv **0.42 or later** with `vo=gpu-next`. Older mpv plays as recorded,
  with a card. Check a distribution's mpv version before promising it on the site.
- The binaries are built on ubuntu-22.04, so they need glibc 2.35 or later.
- Some distributions' stock ffmpeg lacks HEVC and H.264 (Fedora: RPM Fusion; openSUSE: Packman).

## The NAS Helper (tools/nas-helper)

One Docker image for an x86-64 NAS: collaboration (the catalog watcher), the stream (`tools/stream/afp_stream.py`)
and the Jellyfin links. User docs: [tools/nas-helper/README.md](../../tools/nas-helper/README.md) and
[tools/stream/README.md](../../tools/stream/README.md).

- **Build:** CI's `nas-helper` job, or by hand from the repository's top (slow under emulation on a Mac):

  ```
  docker build -f tools/nas-helper/Dockerfile --build-arg VERSION=<v> -t actionfootage-nas-helper:<v> .
  ```

  Fedora with RPM Fusion's ffmpeg, libplacebo and mpv built from source, the Rust binaries, the player's Lua.
- **Install or update on the NAS:** copy the release's `.tar.gz` there, `docker load -i <file>`, change the image tag
  in the NAS's compose file, `docker compose up -d`, `docker logs -f afp-nas-helper`. `/data` and `/config` stay.
  Keep a copy of the compose file before you change it.
- **Settings** are the container's environment (`AFP_*`, listed by `--help` and on `http://<nas>:8099/settings`).
  Change behaviour there, not with new containers. The list lives in `tools/nas-helper/nas_settings.py`.
- **A fix before a release:** a small Dockerfile `FROM` the released image that copies the changed Python files in.
  A changed Rust binary (`afp-catalog`) must be built static for Linux (musl) to drop in.
- **Rules:** it needs privileges Anton grants for a limited time; ask first. Measure the NAS's disk I/O before and
  after a change. Nothing runs on the NAS that Anton didn't start. A helper older than a project refuses it (the
  project version gate): update the helper when `afp_catalog::project::VERSION` goes up.
- After a restart, the stream re-asks the catalog for a few minutes (`pending`). Jellyfin may show items briefly
  until it settles.

## The website

The site lives on the `website` branch, not on `main`, under `website/`:

- `website/site/` is served as is. `site/index.html` (the landing) is written by hand.
- The articles are built: edit `website/pages/<page>.html`, then run `python3 website/build.py`. Never edit
  `site/*.html` other than `index.html` by hand. `site-old/` is a backup, not a source.
- `.github/workflows/website.yml` (on that branch) deploys `website/site/` to Cloudflare Pages on a push to `website`
  that changes it, by hand, or from the release workflow after `publish`. It writes `latest.json` (the latest
  release) for the download page and adds content hashes to the shared CSS and JS links in the deployed copy.
- It needs two repository secrets: the Cloudflare account id and a token with Pages Write only.
- English only since 2026-11-18. Copy rules: say only what has shipped; the Library is not advertised yet.
- A push to `website` goes live. Anton's go first. Site changes are UI: their PR carries before/after pictures.
