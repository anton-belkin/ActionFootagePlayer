#!/usr/bin/env python3
"""tools/make_plugin_bundles.py <dist>: all the plugins in one zip per system, from the release's zips in <dist>

  mpv-plugins-<version>-macos-arm64.zip      dlogm, dji-telemetry, highlights, footage-menu, side-panel, workspace,
  mpv-plugins-<version>-linux-x86_64.zip     gyroflow and view360 (both in gyroflow's zip), each a folder at the zip's
  mpv-plugins-<version>-windows-x86_64.zip   top, as in their own zips: unzipped into mpv's scripts/ folder
  mpv-plugins-<version>-windows-arm64.zip    (~/.config/mpv/scripts/, %APPDATA%\\mpv\\scripts\\)

One for each mpv-gyroflow-<version>-<os>-<arch>.zip there (tools/make_release.sh's and tools/make_plugin_zip.sh's),
with the plain plugins' zips of the same version. The entries are copied as they are, with their executable bits.
.github/workflows/release.yml runs it once the builds are done; the plugins' own zips stay in the release too.
"""
import re
import sys
import zipfile
from pathlib import Path

PLAIN = ("mpv-dlogm-{v}.zip", "mpv-dji-telemetry-{v}.zip", "mpv-highlights-{v}.zip", "mpv-footage-menu-{v}.zip",
         "mpv-side-panel-{v}.zip", "mpv-workspace-{v}.zip")


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: tools/make_plugin_bundles.py <dist>")
    dist = Path(sys.argv[1])
    made = []
    for gyroflow in sorted(dist.glob("mpv-gyroflow-*.zip")):
        m = re.fullmatch(r"mpv-gyroflow-(.+)-(macos|linux|windows)-([a-z0-9_]+)\.zip", gyroflow.name)
        if not m:
            sys.exit(f"{gyroflow.name}: not mpv-gyroflow-<version>-<os>-<arch>.zip")
        version, os_, arch = m.groups()
        parts = [dist / p.format(v=version) for p in PLAIN] + [gyroflow]
        missing = [p.name for p in parts if not p.exists()]
        if missing:
            sys.exit(f"{gyroflow.name}: missing {', '.join(missing)}")
        out = dist / f"mpv-plugins-{version}-{os_}-{arch}.zip"
        seen = set()
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as bundle:
            for part in parts:
                with zipfile.ZipFile(part) as z:
                    for info in z.infolist():
                        if info.filename in seen:
                            sys.exit(f"{out.name}: {info.filename} is in two zips")
                        seen.add(info.filename)
                        bundle.writestr(info, z.read(info), compress_type=info.compress_type or zipfile.ZIP_DEFLATED)
        made.append(out)
    if not made:
        sys.exit(f"no mpv-gyroflow-*.zip in {dist}")
    for out in made:
        print(f"{out.stat().st_size:>11}  {out}")


if __name__ == "__main__":
    main()
