#!/bin/bash
# tools/update_lens_ratings.sh: refreshes vs-gyroflow/resources/lens_ratings.txt, the votes Gyroflow's users gave its lens
# profiles (the stars in Gyroflow's lens profile list), from the same place Gyroflow reads them when it starts:
# https://api.gyroflow.xyz/rate?get_ratings=1, a JSON object {checksum: [good, bad, stars]}. The player picks the best
# rated profile when Gyroflow's database has no exact one for a clip (vs-gyroflow/src/lens_pick.rs) and reads it offline
# from this file, built into the plugin. Run it before a release; the file is sorted so its diff shows what changed.
set -euo pipefail
cd "$(dirname "$0")/.."
out=vs-gyroflow/resources/lens_ratings.txt
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsS --max-time 60 "https://api.gyroflow.xyz/rate?get_ratings=1" -o "$tmp"
mkdir -p "$(dirname "$out")"
python3 -I - "$tmp" "$out" <<'EOF'
import json, sys
votes = json.load(open(sys.argv[1]))
lines = []
for crc, v in votes.items():
    if isinstance(v, list) and len(v) == 3 and len(crc) == 8 and all(c in "0123456789abcdef" for c in crc):
        good, bad = int(v[0]), int(v[1])
        if good or bad:
            lines.append(f"{crc} {good} {bad}")
lines.sort()
with open(sys.argv[2], "w") as f:
    f.write("# Votes for Gyroflow's lens profiles: checksum, good, bad (tools/update_lens_ratings.sh)\n")
    f.write("\n".join(lines) + "\n")
print(f"{len(lines)} profiles with votes -> {sys.argv[2]}")
EOF
