#!/bin/bash
# tools/make_dv_rpu.sh [<look.json> [<out.rpu>]]: the Dolby Vision RPU of a look, made by afp-rpu (vs-gyroflow,
# src/dovi.rs): one RPU, for every frame, with the constant L1 afp-rpu defaults to. Without arguments, the shipped
# look: mpv/scripts/dlogm/dv/nbcu_fit.json -> nbcu_fit.rpu, and for PQ video nbcu_fit.81.rpu (profile 8.1).
#
# With dovi_tool installed (brew install dovi_tool), it also checks that afp-rpu writes what dovi_tool does:
#   - one frame and 48 frames of the look, from a config made here the way DV Lab makes it (DVLab.swift,
#     generatorConfig), not from afp-rpu's;
#   - per-frame L1 with a scene cut and L1 changing inside a shot, from the config afp-rpu writes (--config);
#   - dovi_tool info reads the shipped RPU back as profile 8.4 with the look's L1, L3 and L8.
set -euo pipefail
cd "$(dirname "$0")/.."

look=${1:-mpv/scripts/dlogm/dv/nbcu_fit.json}
out=${2:-${look%.json}.rpu}

prefix=$(brew --prefix 2>/dev/null || echo /opt/homebrew)
command -v cargo >/dev/null || [ ! -x "$prefix/opt/rustup/bin/cargo" ] || PATH="$prefix/opt/rustup/bin:$PATH"
(cd vs-gyroflow && cargo build --release --bin afp-rpu --quiet)
afp=vs-gyroflow/target/release/afp-rpu

"$afp" --look "$look" -o "$out"
echo "$out: $(wc -c < "$out" | tr -d ' ') bytes"
out_pq=${out%.rpu}.81.rpu
"$afp" --look "$look" --profile 8.1 -o "$out_pq"
echo "$out_pq: $(wc -c < "$out_pq" | tr -d ' ') bytes (profile 8.1, for PQ)"

command -v dovi_tool >/dev/null || { echo "(no dovi_tool: not compared)"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# DV Lab's generator config, for <frames> frames of one shot
dv_lab_config() {
    python3 -I - "$look" "$1" <<'PY'
import json, sys
look, n = json.load(open(sys.argv[1])), int(sys.argv[2])
g = lambda k, d=2048: look.get(k, d)
blocks = [{"Level1": {"min_pq": 12, "avg_pq": 1750, "max_pq": 2700}},
          {"Level3": {"min_pq_offset": g("l3Min"), "avg_pq_offset": g("l3Avg"), "max_pq_offset": g("l3Max")}}]
if look.get("useL8", True):
    l8 = {"length": 25, "target_display_index": 1, "trim_slope": g("slope"), "trim_offset": g("offset"),
          "trim_power": g("power"), "trim_chroma_weight": g("chromaWeight"), "trim_saturation_gain": g("saturation"),
          "ms_weight": g("msWeight"), "target_mid_contrast": g("midContrast"), "clip_trim": g("clipTrim")}
    for i in range(6):
        l8[f"saturation_vector_field{i}"] = g("satVec", [128] * 6)[i]
        l8[f"hue_vector_field{i}"] = g("hueVec", [128] * 6)[i]
    blocks.append({"Level8": l8})
print(json.dumps({"cm_version": "V40", "length": n, "source_min_pq": 62, "source_max_pq": 3079,
                  "level6": {"max_display_mastering_luminance": 1000, "min_display_mastering_luminance": 50,
                             "max_content_light_level": 1000, "max_frame_average_light_level": 400},
                  "shots": [{"start": 0, "duration": n, "metadata_blocks": blocks}]}))
PY
}

same() {
    if cmp -s "$1" "$2"; then echo "   same as dovi_tool: $3"; else echo "DIFFERENT from dovi_tool: $3" >&2; exit 1; fi
}

for n in 1 48; do
    dv_lab_config "$n" > "$work/dvlab_$n.json"
    dovi_tool generate -p 8.4 -j "$work/dvlab_$n.json" -o "$work/dovi_$n.rpu" >/dev/null
    if [ "$n" = 1 ]; then mine="$out"; else mine="$work/afp_$n.rpu"; "$afp" --look "$look" --frames "$n" -o "$mine"; fi
    same "$mine" "$work/dovi_$n.rpu" "$n frame(s), DV Lab's config"
done

# Per frame: L1 changing every few frames, a scene cut at frame 6
python3 -I -c '
import json
f = [{"min_pq": 12, "avg_pq": 1600 + 40 * (i // 2), "max_pq": 2700 - 5 * i} for i in range(10)]
for e in f: e["scene_cut"] = False
f[0]["scene_cut"] = f[6]["scene_cut"] = True
print(json.dumps(f))' > "$work/l1.json"
"$afp" --look "$look" --l1 "$work/l1.json" --config "$work/afp_l1.json" -o "$work/afp_l1.rpu"
dovi_tool generate -p 8.4 -j "$work/afp_l1.json" -o "$work/dovi_l1.rpu" >/dev/null
same "$work/afp_l1.rpu" "$work/dovi_l1.rpu" "10 frames, per-frame L1 with a scene cut"
"$afp" --look "$look" --l1 "$work/l1.json" --frames 10 -o "$work/afp_l1b.rpu"
cmp -s "$work/afp_l1.rpu" "$work/afp_l1b.rpu"

dovi_tool info -i "$out" -f 0 > "$work/info.json" 2>/dev/null
python3 -I - "$work/info.json" "$look" <<'PY'
import json, sys
text = open(sys.argv[1]).read()
rpu, look = json.loads(text[text.index("{"):]), json.load(open(sys.argv[2]))
assert rpu["dovi_profile"] == 8, rpu["dovi_profile"]
blocks = {next(iter(b)): next(iter(b.values())) for b in rpu["vdr_dm_data"]["cmv29_metadata"]["ext_metadata_blocks"]
          + rpu["vdr_dm_data"]["cmv40_metadata"]["ext_metadata_blocks"]}
assert (blocks["Level1"]["min_pq"], blocks["Level1"]["avg_pq"], blocks["Level1"]["max_pq"]) == (12, 1750, 2700)
assert blocks["Level3"]["avg_pq_offset"] == look["l3Avg"]
assert blocks["Level8"]["trim_slope"] == look["slope"] and blocks["Level8"]["clip_trim"] == look["clipTrim"]
print("   dovi_tool info: profile 8, L1 12/1750/2700, the look's L3 and L8")
PY
