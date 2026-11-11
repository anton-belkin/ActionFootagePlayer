# Action Footage NAS Helper

What a NAS does for Action Footage Player and its Library, in one Docker container for a NAS with an x86-64
processor and an Intel GPU (built and run on a UGREEN DXP with an N100). It's for people at ease with Docker:
everything is set in the container's environment and volumes, described below, by `--help`, and on its page.

It does three things, each a setting of its own:

- **Collaboration** (`AFP_COLLABORATION`, on): several people and computers working in one library at once. The
  NAS's system tells it what changes on its own disks, and it writes that into each project's catalog
  (`<project>/.afp/catalog`, docs/library-management.md §9), so what one person does (a moment marked, clips sorted,
  a trip added, or a change made on the NAS itself) reaches everyone else's player and Library in about a second,
  none of them walking the share. It writes nothing but the catalog. A project newer than the helper (its
  `version` in Project.afproject) is refused, not touched: then update the helper.
- **The stream** (`AFP_STREAM`, on): every clip, moment and clip preview of the projects as an HLS stream, the way
  the player shows it: stabilized with Gyroflow's warp on the GPU, D-Log M in HDR (HLG), each file with its saved view
  or its project's settings. Rendered by the NAS's GPU as it's watched, kept in a cache. Its page,
  `http://<nas>:8099/`, lists the clips with their streams and pre-renders folders; `/settings` shows this
  helper's state and settings. Safari, iPhones, Apple TV and VLC play the streams as they are. Details:
  tools/stream/README.md.
- **Links for Jellyfin** (`AFP_JELLYFIN`, off): a folder a Jellyfin "Home Videos" library reads, with a `.strm`
  (the stream's address), a `.nfo` (title, date, length) and a cover per item, sorted into trips, the library
  refreshed after a change. Its scope (`AFP_JELLYFIN_SCOPE`) says what goes there: `moments`, `clips` (each clip
  preview: the whole clip) or `all`; what's outside it is removed. Jellyfin then plays the streams on its apps
  (TVs, phones) without transcoding. The `.strm` and `.nfo` files are Kodi's formats, which Emby and Kodi read too
  (with the refresh left to their own scans); Plex doesn't take them.

What it doesn't do: change the projects (the stream reads them read-only and refuses a folder it could write to),
exports, Dolby Vision streams, logins (the stream is for the home network: put it behind nothing reachable from
outside).

## Installing

From a release, `ActionFootageNASHelper-<version>-linux-x86_64.tar.gz` (the image) and
`ActionFootageNASHelper-<version>-compose.yaml` (an example). On the NAS, over SSH:

    docker load -i ActionFootageNASHelper-<version>-linux-x86_64.tar.gz

Then change the compose file's folders (the left side of each volume) to the NAS's, and start it, from the folder
with the file as `compose.yaml`, or as a project in the NAS's Docker app (UGOS: Docker → Project → Create, paste it):

    docker compose up -d
    docker logs -f afp-nas-helper

The log says what it found and runs (`helper: runs: collaboration[Trip], stream`) and any problem with the settings;
`docker run --rm actionfootage-nas-helper:<version> --help` describes every setting.

Each project is bound twice, by its folder's own name: `/projects/<name>` read-write for collaboration,
`/library/<name>` read-only (`:ro`) for the stream. The shares its locations are on (the `//NAS/photo/...` paths in
Project.afproject, as the Library added them on a Mac) go in `/shares/<share>`, read-only: the helper finds each by
its share's name, as a Mac finds it mounted.

To update: load the new release's image, change the tag in the compose file, `docker compose up -d`. What's in
`/data` and `/config` stays. To remove it: `docker compose down`, then the two folders; the catalog stays
in the projects (the Library keeps it as well).

## Settings

Each is an environment variable of the container (the compose file's `environment:`). Change one and recreate the
container (`docker compose up -d`).

### What runs

| Setting | Default | What it does |
|---|---|---|
| `AFP_COLLABORATION` | `on` | Collaboration: several people and computers working in one library at once. The NAS is told what changes on its own disks and writes it into each project's catalog (afp-catalog watch), so what one person does (a moment marked, clips sorted, a trip added, also on the NAS itself) reaches everyone else's player and Library in about a second, without anyone walking the share. It writes nothing but the catalog. A project newer than this helper is refused, not touched: update the helper. One of: `on`, `off`. |
| `AFP_STREAM` | `on` | The stream (afp-stream): every clip, moment and clip preview as an HLS stream, as the player plays it (stabilized on the GPU, D-Log M to HDR, the project's settings), rendered by the NAS's GPU as it is watched. Its page: http://<nas>:AFP_PORT/. Needs /dev/dri (an Intel GPU with VA-API). One of: `on`, `off`. |
| `AFP_JELLYFIN` | `off` | Links for Jellyfin: a folder (/jellyfin) a Jellyfin Home Videos library reads, a .strm (the stream's URL), .nfo and cover per item, sorted into trips; the library refreshed after a change. Needs the stream. One of: `on`, `off`. |

### What it looks at

| Setting | Default | What it does |
|---|---|---|
| `AFP_PROJECTS` | | The projects, ':'-separated paths inside the container. Empty: every folder in /projects that has a Project.afproject. |
| `AFP_SHARES` | | Where the shares the projects' locations name are here: '//HOST/share=/shares/share;...'. Empty: every folder in /shares, as a share of the NAS the projects' locations name (//DISKSTATIONDXP/photo for /shares/photo). |
| `AFP_COMPUTER_NAME` | `nas` | This NAS's name in the projects' journals, the first time collaboration runs (it's kept in /data/catalog after that, with an id of its own). |

### The stream

| Setting | Default | What it does |
|---|---|---|
| `AFP_PORT` | `8099` | The stream's port (its page, the streams, /settings). |
| `AFP_HEIGHT` | `1080` | The streams' height in pixels (never more than the clip's). |
| `AFP_BITRATE` | `30M` | The streams' bitrate (HEVC). It costs no speed; a TV's 100 Mbit/s port is the limit. |
| `AFP_SEGMENT` | `1` | Seconds per HLS segment. |
| `AFP_CACHE_GB` | `40` | What played stays in /data/stream/cache up to this many GB; the clips used longest ago go first. A clip's renders go when its project or these settings change. |
| `AFP_OPENING` | `8` | Seconds of each clip that 'pre-render openings' renders ahead, so a folder played in a row starts each clip at once. |
| `AFP_INTERP` | `bilinear` | The warp's interpolation. bilinear is the fastest (67-69 fps at 1080p on an N100); bspline, bicubic and lanczos are sharper and slower. One of: `bilinear`, `bspline`, `bicubic`, `lanczos`. |
| `AFP_CHROMATIC` | `0` | 1: the water's color fringes taken out where the player does it (slower: ~48 fps on an N100, not real time at 4K60). One of: `0`, `1`. |
| `AFP_TV_LOOK` | `hlg` | What TVs that take only MPEG-TS HLS get (Samsung Tizen before 5, LG webOS before 3.5; also any stream asked with ?fmt=ts): hlg, pq (HDR10) or sdr, optionally followed by x<peak> (highlights expanded up to that many nits), g<gain> (the whole picture brighter) and r<fps> (at most that many frames a second), e.g. pqg2.5r30. |
| `AFP_LOG_REQUESTS` | `1` | 1: every request in the log, with its User-Agent (who asked for what); 0: none. One of: `0`, `1`. |

### Links for Jellyfin

| Setting | Default | What it does |
|---|---|---|
| `AFP_JELLYFIN_SCOPE` | `all` | What goes to Jellyfin: moments (each moment of a project, in its trip), clips (each clip preview: the whole clip) or all. What's outside it is removed from /jellyfin. One of: `moments`, `clips`, `all`. |
| `AFP_PUBLIC_URL` | | The stream as Jellyfin's players reach it, written into each .strm: http://<the NAS's address>:AFP_PORT. Required with AFP_JELLYFIN=on. |
| `AFP_JELLYFIN_URL` | `http://127.0.0.1:8096` | Jellyfin's own address, to refresh the library after a change (with the container on the host's network, 127.0.0.1 and Jellyfin's port). |
| `AFP_JELLYFIN_KEY_FILE` | `/config/jellyfin.key` | A file with a Jellyfin API key (Dashboard → API Keys). Without it the library isn't refreshed; Jellyfin finds the changes at its next scan. |
| `AFP_JELLYFIN_PATH` | | The /jellyfin folder as Jellyfin's library has it (its own container's path, e.g. /data/_Action Footage), to refresh only that library. Empty: /jellyfin. |
| `AFP_JELLYFIN_NAMES` | `en` | The language of the folders and descriptions it writes (Moments/Clips, No trip...). One of: `en`, `ru`. |
| `AFP_JELLYFIN_EVERY` | `120` | Seconds between looks at the stream's catalog. |
| `AFP_JELLYFIN_USER` | | uid:gid the files in /jellyfin are written as (e.g. 1000:10). Empty: root. |

### Folders

| In the container | | What it is |
|---|---|---|
| `/projects/<name>` | read-write | Each project (the folder with Project.afproject), under its own folder's name: collaboration keeps its catalog in <project>/.afp/catalog. Nothing else is written there. |
| `/library/<name>` | read-only | The same project folders again, read-only, for the stream: the player it asks may want to fill a missing key into a moment's link, and here that write fails, as it should. The stream refuses a folder it could write to. |
| `/shares/<share>` | read-only | The network shares the projects' locations are on (//NAS/photo/... in Project.afproject), each under its share's name, as a Mac finds them mounted. |
| `/data` | read-write | The helper's own: this NAS's name in the projects' journals and its caches (/data/catalog), the stream's renders (/data/stream/cache, up to AFP_CACHE_GB) and what the player said (/data/stream/args). |
| `/config` | read-write | What you set: the player's defaults (/config/player/script-opts/gyroflow.conf and dlogm.conf, as in the app's mpv/script-opts), Jellyfin's API key (/config/jellyfin.key). |
| `/jellyfin` | read-write | Only with AFP_JELLYFIN=on: the folder a Jellyfin library reads (a Home Videos library). The helper writes only there and removes what it wrote. |

The player's own defaults for what streams (horizon lock, view, the edge fill: what the app's side panel saves to
`~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/gyroflow.conf` and `dlogm.conf` on a Mac) go in
`/config/player/script-opts/`, copied from the Mac's app; without them, the player's defaults. A change there renders
again what it changes.

## How it's built

`tools/nas-helper/Dockerfile`, from the repository's top: Fedora 43 with RPM Fusion's ffmpeg and Intel's media
driver, libplacebo 7.387 and mpv 0.42 from their sources, the Rust programs (`afp-catalog`, `gyroflow-gpu-table`
and the gyroflow plugin's), afp-render, and the player's Lua plugins (workspace, gyroflow, dlogm), which the stream
asks how each file plays. The release workflow builds it, tries it (`--check`) and saves it with `docker save`.

    docker build -f tools/nas-helper/Dockerfile --build-arg VERSION=dev -t actionfootage-nas-helper:dev .

`helper.py` runs the parts as processes and starts each again when it stops (after 5 s, longer while it keeps
failing; an hour when the project is newer than the helper); their lines go to the container's log with the
part's name, and their state to `/data/helper.json`. `nas_settings.py` is the list of settings this file's tables
come from (`helper.py --markdown`).
