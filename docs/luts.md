# Log profiles and their LUTs

What the camera makers publish for turning their log footage into Rec.709 (and HDR, where they offer it), so
the `dlogm` plugin can do for other cameras what it does for the Osmo Action 6: recognize the profile from the
file's metadata, and use the maker's LUT, downloaded at install and checked, never redistributed.

Checked on 2026-10-23, by downloading every file below that is a plain download (no login, no form to accept)
and hashing it. Sizes are the 3D LUT's grid (33 = 33×33×33). Dates are those the maker's page gives.

## In short

- **Every official LUT here can be fetched at install the way DJI's is**: a plain HTTPS download, no login, no
  terms to accept. GoPro's come from a Salesforce file link and Insta360's are zips, so the install needs a
  little more than one `curl`, but nothing a user has to click.
- **None of them comes with a license.** Not on the download pages, not in the files; the sites are "all
  rights reserved". So, as with DJI's: don't redistribute, download from the maker and check the SHA-256.
- **DJI: one LUT per profile generation, not per model**, despite a page per model. One file (its header says
  "Mavic 3 Pro, D-Log M, 2023-03-24") is served for 15 products, drones and Osmo cameras alike; a
  second covers the 2025–26 drones; the Osmo Action 6's 11.17 is a third, of its own. New: the Osmo Pocket 4
  and 4P shoot **D-Log** (not D-Log M) and the 4P also **D-Log2**, each with LUTs of their own.
- **GoPro: one LUT per camera, and for the HERO12, per firmware**: GP-Log changed with firmware 1.20 and 2.00.
  The MISSION 1 series has a new **GP-Log2** with five LUTs. The camera writes the profile in the GPMF `PTCL`
  tag (`LOG`, `GPLOG2`).
- **Insta360: a LUT per model for 8-bit I-Log, and a new 10-bit I-Log (X6, Luna) whose curve and gamut are
  published**, with LUTs to Rec.709, **HLG and PQ**. A published curve means we could build our own HDR
  conversion for it, as `luts/dlogm_hlg.cube` is ours for D-Log M, and ship it under the GPL.
- **Gaps**: Insta360's metadata for I-Log isn't known; DJI's D-Log M and GoPro's GP-Log gamut are
  undocumented; GoPro's LUT depends on firmware, so the plugin would need the firmware version from the file;
  some models with a log mode have no LUT listed (below).

## Camera by camera

Rows are grouped by log curve, then by LUT file: cameras in one row share the file. **SDR** is a LUT to
Rec.709 from the maker. **HDR** is what an HDR path could be built from: the maker's LUT to HLG/PQ, our own
conversion (as `luts/dlogm_hlg.cube`), or a published curve and gamut to build one from.

| Curve | Cameras | SDR (Rec.709) | HDR | Missing |
|---|---|---|---|---|
| DJI D-Log M | Osmo Action 6 | DJI's 11.17 (file C) | ours, `dlogm_hlg.cube`, verified on real clips | — |
| | Mavic 3, 3 Classic, 3 Pro; Air 3, Air 3S; Mini 4 Pro; Avata 2; Flip; O4 Air Unit; Osmo Action 4, Action 5 Pro; Pocket 3; Osmo 360, 360 II; Osmo Nano | DJI's, one file for all (file A, 2023) | no DJI LUT. Ours, if their D-Log M is the Action 6's curve; third-party fits to scene-linear/DWG: Zeb Gardner's (free, from a Mini 4 Pro), Thatcher Freeman's per model (Mini 4 Pro, Action 5, Pocket 3, Mavic 3 Pro, Air 2S, Mavic 2 Pro; no license), Gamut's BaseLUTs to DWG ($35 per camera) | whether the curve is the same; clips |
| | Mavic 4 Pro, Mini 5 Pro, Avata 360, Lito X1 | DJI's, one file (file B, 2025) | as above (Thatcher Freeman: Mini 5 Pro; Gamut: Mini 5 Pro, Mavic 4 Pro) | as above |
| | Mavic 2, Air 2S | DJI's (file D, 2018) | as above (Thatcher Freeman: both) | as above |
| DJI D-Log | Mavic 3 family | DJI's, plain and vivid (2021, 2022) | no LUT; **DJI's own ACES input transform** (curve and D-Gamut primaries, 2023), so an HLG conversion of ours | the conversion; metadata telling D-Log from D-Log M |
| | Mavic 4 Pro; Osmo Pocket 4, 4P | DJI's V1, one file for all three; Pocket 4 and 4P each have their own V2 (2026) and vivid | as above, if it's the same D-Log | as above |
| | Zenmuse X9 (Inspire 3, Ronin 4D) | DJI's | **DJI's LUT to Rec.2020 HLG** | — (not a camera this player targets) |
| DJI D-Log2 | Osmo Pocket 4P | DJI's, 33 and 65, plain and vivid | no LUT; a D-Log2/D-Gamut2 ACES transform said to be DJI's (copies only, below), an MIT one (AJ3DVFX), Gamut's BaseLUTs to DWG ($35) | DJI's own publication; metadata |
| GoPro GP-Log | HERO12 Black, firmware 1.10/1.12 | GoPro's, 16³, one per white balance | no GoPro LUT. **GP-Tune LUT Pack ($19): LUTs to PQ and HLG** (HERO9–12, +2/+3 EV); free to DWG/I: Zeb Gardner's; curve known (Protune log, base 400), gamut only fitted | the gamut from GoPro; firmware version from the file |
| | HERO12 Black, firmware 1.20 | GoPro's | as above | as above |
| | HERO12 Black firmware 2.00+, HERO13 Black | GoPro's, one file | as above (GP-Tune lists models up to HERO12) | as above |
| | MAX2 | GoPro's, technical and creative | as above, none MAX2-specific | as above |
| | HERO11 Black (Labs firmware, `WIDE=1`, log base 400) | only the Labs maintainer's, unofficial | as above (GP-Tune has Labs WIDE) | an official LUT |
| GoPro GP-Log2 | MISSION 1, MISSION 1 PRO | GoPro's, five (technical, creative, 33 and 65) | no LUT; **curve (log base 600) and Rec.2020 primaries from GoPro Labs' own LUT generator, Apache-2.0**, so an HLG conversion of ours; Gamut's BaseLUTs to DWG ($35) | the conversion |
| Insta360 10-bit I-Log | X6; Luna Ultra, Luna Pro | Insta360's, one set for all | **Insta360's LUTs to HLG and PQ**, and the curve and BT.2020 gamut in a white paper; an ACES package | metadata |
| Insta360 8-bit I-Log | Ace Pro 2, GO Ultra | Insta360's, one file | none found, free or paid | everything for HDR; metadata |
| | X5 | Insta360's | as above | as above |
| | X3, ONE X2 | Insta360's, one file | as above | as above |
| | ONE X | Insta360's (64³) | as above | as above |
| | ONE R; ONE RS; Sphere | Insta360's, one per lens (Sphere's = ONE RS's 360) | as above | as above |
| ? | Insta360 Ace Pro, Ace, X4, X4 Air, GO 3, GO 3S | none listed | none | whether they have I-Log at all |

Of all of these, only the Action 6 has been tried in the player; everything else is from the makers' files and
pages. How the camera marks its log profile in the file is known for DJI's Action 6 (and guessed from the `_D`
file name for other DJI cameras) and for GoPro (`PTCL`, not yet seen on a GP-Log clip); for Insta360 it isn't.

## HDR beyond the makers' LUTs

An HDR path doesn't need a LUT that ends in HLG. It needs the log footage turned into scene light in a known
gamut: then the HLG (or PQ) step, and the gamut matrix to Rec.2020, are standard math the plugin can do
itself, as it does for D-Log M. So anything that ends in scene-linear, ACES AP0 or DaVinci Wide
Gamut/Intermediate (Blackmagic publishes DWG/I) will do, and that's what most of the following are. Ready
HLG/PQ LUTs are rare: besides Insta360's 10-bit and DJI's X9, only the GP-Tune pack has them.

Official, and usable as published formulas:

- **DJI D-Log / D-Gamut**: DJI's "ACES Workflow Guide" (2023-12, linked from the Mavic 3 and Mavic 4 Pro
  download pages), `https://terra-1-g.djicdn.com/851d20f7b9f64838a34cd02351370894/ACES/ACES%20Workflow%20Guide%20.zip`
  (`76473cdcd09c61f1f16ca99216a97b9dc7a60765b66c5a239906869978f60fb8`): an ACES input transform as CTL, CLF and
  DCTL. D-Log to linear: `(x - 0.0929) / 6.025` for `x ≤ 0.14`, else `(10^(3.89616·x - 2.27752) - 0.0108) / 0.9892`;
  D-Gamut primaries R (0.71, 0.31), G (0.21, 0.88), B (0.09, −0.08), white D65. Nothing for D-Log M in it.
- **GoPro GP-Log2**: GoPro Labs' [GP-Log2 LUT Generator](https://gopro.github.io/labs/gplog2/) (in
  [gopro/labs](https://github.com/gopro/labs), Apache-2.0, updated 2026-09-22): `log(L·599 + 1) / log(600)`,
  Rec.2020 primaries, with GoPro's EV offset (+1.8 by default) and highlight roll-off. Its outputs are SDR
  (Rec.709 or Rec.2020 with the BT.709 curve), but the code can be reused, license and all.
- **Insta360 10-bit I-Log**: the white paper (above), and an ACES package, "Insta360 I-Log & ACES Workflow"
  (v1.0, 2026-06-09), `https://wassets.insta360.com/common/bc6e6ba50b8b4980b2f8ff480ad5714c/Insta360_I-Log&ACES_Workflow.zip`
  on `https://www.insta360.com/download/aces` (not fetched).

Third-party, free:

- **Zeb Gardner**: D-Log M → DWG/I (`DLOGM_DWG_I_SHAPER_ZRG_V1.zip`,
  `d4a392c59efcca0b394d41e72dc7e0c8b05489218110680ec012d849ddada34a`) and GP-Log → DWG/I
  (`GP_LOG-to-DWG_I-ZRG-V1-j3dz.cube`, `aff985958704a8c03e5684ae6a165f354b8a3ec083a7bfefe1293a5848225b4a`), 1D
  shaper + 65³, fitted from test charts; also a D-Log2 → Rec.709 LUT (`/s/D_log2-to-Rec709.cube`). No license
  stated: download at install, or ask him.
- **Thatcher Freeman**, [thatcherfreeman/dwg-transforms](https://github.com/thatcherfreeman/dwg-transforms):
  DCTLs to DWG with a fitted curve and estimated primaries **per camera**: D-Log M for Action 5, Air 2S,
  Mavic 2 Pro, Mavic 3 Pro, Mini 4 Pro, Mini 5 Pro, Pocket 3; D-Log for Mavic 3 Pro Cine and Phantom 4; D-Log2;
  GP-Log, GP-Log2, Protune; an Insta360 X4 Flat work in progress. Its `specs/` has a
  `DLog2_DGamut2_to_ACES_AP0_cat02.dctl` (D-Gamut2 to XYZ matrix and the curve), which Gamut says is the DCTL
  DJI made public for D-Log2. **No license**, so as reference only, or with the author's permission. The D-Log M
  fits differ by model, which says the "one LUT for 15 products" isn't the whole story either.
- **AJ3DVFX/dji-dlog2-to-aces-idt** ([GitHub](https://github.com/AJ3DVFX/dji-dlog2-to-aces-idt), MIT,
  2026-08): D-Log2/D-Gamut2 → ACES AP0 DCTL.
- **xtremestuff/protune-transforms** (MIT; above): GP-Log and GP-Log2 to ACES/DWG/Rec.2020, with a HERO5-era
  matrix.

Paid (none may be redistributed; a user who buys one would put the file where the player can read it):

- **GP-Tune LUT Pack**, xtremestuff, $19, <https://xtremestuff.net/store/gp-tune-lut-pack/>: 110+ 33³ `.cube`
  LUTs for Protune Flat, Natural and GP-Log (Native, Rec.709 and Labs WIDE gamuts; HERO9–HERO12 by model, older
  ones generic), including **HDR LUTs to 1000-nit PQ and to HLG, at +2 and +3 EV**. Knows GP-Log before and
  after HERO12 firmware 1.20 (mid-grey 49 % vs 76 % IRE). No GP-Log2, MAX2 or HERO13 named. The license isn't on
  the page. Its siblings GP-Tune Transform ($24, Resolve DCTL/ACES) and GP-Tune Plugin ($29) are Resolve-only.
- **Gamut BaseLUTs**, gamut.io, $35 per camera (or a $129/year membership), e.g.
  <https://gamut.io/product/baseluts-for-dji-osmo-action-6/>: per-camera LUTs to Rec.709, to **DWG**
  (scene-referred) and to an ARRI 709 look, made from charts. Cameras: DJI Osmo Action 5 Pro, Action 6, Pocket 3,
  Pocket 4/4P (D-Log and D-Log2), Mini 3 Pro, Mini 4 Pro, Mini 5 Pro, Avata 2, Mavic 3 Classic, Mavic 3 Pro,
  Mavic 4 Pro, Ronin 4D; GoPro MISSION 1; Insta360 Luna Ultra. License (gamut.io/license): one buyer, up to two
  computers, no redistribution.
- **"D-Log M ACES IDT & DCTL"**, a Patreon post (`https://www.patreon.com/posts/d-log-m-aces-idt-122210348`):
  not checked (Patreon wasn't reachable from here).
- Not useful here: Leeming LUT Pro (Rec.709 only, personal license for 3 devices), FilmConvert and Dehancer
  (plugins, HDR left to the host app), the many look packs.

Nothing, free or paid, turned up for Insta360's 8-bit I-Log (Ace Pro 2, GO Ultra, X5 and older) beyond its
Rec.709 LUTs. Fitting it ourselves from test clips is the remaining route (Zeb Gardner published his Python
tool for that).

## DJI

All on DJI's Download Center, one page per LUT (`https://www.dji.com/global/downloads/softwares/<slug>`; the
product pages' Downloads tabs link the same files). Every page embeds the whole "Transcoders" catalog, which is
where this list comes from. Files are on `terra-1-g.djicdn.com/851d20f7b9f64838a34cd02351370894/` unless
noted; Mac and Windows entries point to the same file (where the URLs differ, the bytes don't). All are
`.cube` files made with Resolve's LUT Editor, Rec.709 out, values in 0–1.

### D-Log M → Rec.709

| File | Size | SHA-256 | Served as (date on DJI's page) |
|---|---|---|---|
| **A** "Mavic 3 Pro, D-Log M, 2023-03-24" in its header, 1 042 315 bytes | 33 | `b18162854ab47702068410c33afa98a8cb6eef159fc5a04ce0e65fad0fd8947e` | Mavic 3, 3 Classic, 3 Pro (2023-04-25); Osmo Action 4 "vivid" (2023-08-02); Air 3 (2023-08-03); Air 3S (2023-08-03); Mini 4 Pro (2023-09-25); Osmo Pocket 3 (2023-10-25); Avata 2 (2024-04-11); Osmo Action 5 Pro "vivid" (2024-09-19); O4 Air Unit series (2025-01-09); Flip (2025-01-14); Osmo 360 and Osmo 360 II (2025-07-31); Osmo Nano "vivid" (2025-09-23) |
| **B** no date in its header, 1 401 638 bytes | 33 | `f00d2f8621d8fe3edcfc77a4d4969a8361aaf0a5804196df7d37dcaa2e925fed` | Mavic 4 Pro (2025-05-13); Mini 5 Pro (2025-09-17); Avata 360 (2026-03-26); Lito X1 (2026-04-23) |
| **C** "LUT-11.17", no header, 753 738 bytes | 33 | `1972eb9b05699ba19d4aaa67aaa5fb00b46935c60ba9219183289f79e044cbb2` | Osmo Action 6 (2025-11-13); the one `macos/setup.sh` uses |
| **D** "DLog-M to Rec709", 1 401 631 bytes | 33 | `ac3e114ba6467d27f97295bf91532428bddc05fb61922635d9271d57ecbc9d48` | Mavic 2 / Air 2S (2018-09-19) |

"vivid" in a title doesn't mean a different file: the Action 4, Action 5 Pro and Nano "vivid" LUTs are file A,
like the drones'. A, B and C differ, but not by much (mid-grey ramp, D-Log M code 0.5 → 0.49, 0.48, 0.51).

URLs of A (any of them gives the same file; the paths are DJI's):

- `M3/DJI%20Mavic%203%20D-Log%20M%20to%20Rec.709%20V1.cube` (Mavic 3 family)
- `DJI%20Air%203%20Lut/DJI%20Air%203%20D-Log%20M%20to%20Rec.709%20V1_.cube` (Air 3)
- `234_lut/DJI%20Air%203S%20%20D-Log%20M%20to%20Rec.709%20V1_.cube` (Air 3S)
- `140%20lut/DJI%20Mini%204%20Pro%20D-Log%20M%20to%20Rec.709%20V1_.cube` (Mini 4 Pro, Mac; Windows `V1._.cube`)
- `141%20LUT/DJI%20Flip%20D-Log%20M%20to%20Rec.709%20V1_.cube` (Flip, Mac; Windows `V1._.cube`)
- `avata2%20d-log/DJI%20Avata%202%20D-Log%20M%20to%20Rec.709%20V1._.cube` (Avata 2)
- `ZA530%20LUT/DJI%20O4%20Air%20Unit%20Series%20D-Log%20M%20to%20Rec.709%20V1_.cube` (O4 Air Unit)
- `203/DJI%20OSMO%20Action%204%20D-Log%20M%20to%20Rec.709%20V1.cube` (Action 4)
- `AC204%E8%BD%AF%E4%BB%B6/DJI%20OSMO%20Action%205%20Pro%20D-Log%20M%20to%20Rec.709%20V1.cube` (Action 5 Pro)
- `OQ101%20LUT/DJI%20Osmo%20360%20D-Log%20M%20to%20Rec.709%20V1.cube` (Osmo 360, 360 II)
- `OW001%20LUT/DJI%20OSMO%20Osmo%20Nano%20D-Log%20M%20to%20Rec.709%20V1.cube` (Osmo Nano)
- Pocket 3's is on another host, `https://www-dl.djicdn.com/5e45168b46b342d5b88f72c458ba6e79/OP3%20LUT%E6%96%87%E4%BB%B6%E7%89%B9%E6%AE%8A%E5%A4%84%E7%90%86/DJI%20OSMO%20Pocket%203%20D-Log%20M%20to%20Rec.709%20V1.cube`,
  which answers 403 to curl's default User-Agent (any `Mozilla/5.0` works). Better to take A from one of the
  others.

URLs of B: `WA341%20LUT/DJI%20Mavic%204%20Pro%20D-Log%20M%20to%20Rec.709%20V1.cube`,
`WA150%20LUT/DJI%20Mini%205%20Pro%20D-Log%20M%20to%20Rec.709%20LUT.cube`,
`WA530%20Lut/DJI%20Avata%20360%20D-Log%20M%20to%20Rec.709%20V1.cube`,
`WA151%20Lut/DJI%20Lito%20X1%20D-Log%20M%20to%20Rec.709%20LUT.cube`.
C: `AC206%20LUT/DJI%20OSMO%20Action%206%20D-LogM%20to%20Rec.709%20LUT-11.17.cube`.
D: `https://docs.djicdn.com/Products+info/DLog-M+to+Rec709.cube`.

So the plugin needs three D-Log M LUTs (A, B, C) and a table from model to LUT, and the model is in DJI's
metadata already. For a model DJI hasn't published a LUT for yet, A is the safe guess.

### D-Log and D-Log2 → Rec.709

| Profile, models | File (DJI's title, date) | Size | SHA-256 |
|---|---|---|---|
| D-Log, Mavic 3 family | D-Log to Rec.709 V1 (2021-11-09; header "DJI Color Space to Rec709, Revision 20210924") | 33 | `5e21b78dc23af8287b14301ec2c3803e6707385681ad348f4754aae2fd4562ee` |
| | D-Log to Rec.709 vivid V1 (2022-05-31) | 33 | `5cd863a4776a6c2c6adea662d5ff1f06a032a6853506f1b1cd5b5bd910270a90` |
| D-Log, Mavic 4 Pro, Osmo Pocket 4, 4P | D-Log to Rec.709 V1: Mavic 4 Pro (2025-05-13), Pocket 4 V1.0 (2026-04-16), Pocket 4P V1.0 size33 (2026-06-15), one file | 33 | `ca8bc7d3bf382512ed9501137c8b88d1829ae9b22a84965d7d2e1339f615de72` |
| | Mavic 4 Pro D-Log to Rec.709 vivid V1 (2025-05-13) | 33 | `4f6a8b09849b09af3faf8af0fd7646b5c68658bd9f27170480fd4074cb7fbfbe` |
| | Pocket 4 D-Log to Rec.709 V2.0 (2026-09-24) | 33 | `d029a958519a41f3fb98ce9ad11cc1f3fe9c7c8c48a91955136363bcc19847b9` |
| | Pocket 4P D-Log to Rec.709 V2.0 size33 (2026-08-17) | 33 | `04757dd8dd669d60a2d58ea1834f185616ca9952922c168fe69dba29bb2be1b1` |
| | Pocket 4 vivid V1.0 (2026-04-16) | 33 | `bb3c9b282c21e2974e67d73c9b62ef6c3bc9092d63b713c44ecc4e0de3eccfad` |
| | Pocket 4P vivid V1.0 size33 (2026-06-15) | 33 | `31344b50751590e267835a2843b2a5ca9f6120392707ba8242269c681922575a` |
| | Pocket 4 vivid V2.0 (2026-09-24) and Pocket 4P vivid V2.0 size33 (2026-08-17), one file | 33 | `95afff501afd0343c3c1a5f1cf7bc34c8d5ed5765474444b58e9db8a8b7e58fb` |
| D-Log2, Osmo Pocket 4P | D-Log2 to Rec.709 V1.0 size33 (2026-06-15) | 33 | `ca19fb2da9ddb82261a6baa86a511ce22bb46a1d6ca5bf50ca4c4dbbd71eb1a1` |
| | D-Log2 to Rec.709 V1.0 size65 | 65 | `fa6537da3235c281da6e361806d3e9ee93f5532ad95471878c5d6c05157caa9c` |
| | D-Log2 to Rec.709 vivid V1.0 size33 | 33 | `7d66f59d8020439795482fdfe2dba697d5ee272c3d63b802e3858015bdda830c` |
| | D-Log2 to Rec.709 vivid V1.0 size65 | 65 | `32b9b49d52fde38f95cbc1d0a4f486fcbf176eb82d28481b25d4f9a0b006734e` |
| D-Log, Zenmuse X9 (Inspire 3, Ronin 4D) | X9 D-Log to Rec.709 (2021-12-28) | 33 | `583a08a77c920156f84770a69d4124f586196a9f0f76b2d02f23a1a05a863d06` |
| | X9 D-Log to **Rec.2020 HLG** (2021-12-28), DJI's only LUT to HDR | 33 | `c5d105941334d87e5474fbedc42dc25c2aa73559c37d7ba470a7aa279544eb4f` |

Paths: Mavic 3 `260%20downloads/DJI%20Mavic%203%20D-Log%20to%20Rec.709%20V1.cube` (DJI's page has the spaces
unencoded) and `mavic%203/DJI%20Mavic%203%20D-Log%20to%20Rec.709%20vivid%20V1.cube`; Mavic 4 Pro
`WA341%20LUT/DJI%20Mavic%204%20Pro%20D-Log%20to%20Rec.709%20V1.cube` (and `...vivid%20V1.cube`); Pocket 4
`HG214%20Lut/DJI%20OSMO%20Pocket%204%20D-Log%20to%20Rec.709[%20vivid]%20V1.0.cube` (and `V2.0`); Pocket 4P
`HG224%20LUT/DJI%20OSMO%20Pocket%204P%20D-Log[2]%20to%20Rec.709[%20vivid]%20V<n>%20size<33|65>.cube`. The X9
LUTs are behind `https://mydjiflight.dji.com/file/links/DJI_ZENMUSE_X9_DLOG_TO_REC709_3D_LUT_V1` and
`..._DLOG_TO_REC2020HLG_3D_LUT_V1`, redirects to the files. Older Phantom 3/4 and Zenmuse X5/X7 D-Log LUTs
(zips on `dl.djicdn.com`, 2016–2018) weren't fetched.

### What D-Log M is

DJI publishes no curve or gamut for D-Log M (Zeb Gardner, below, worked it out from test charts). D-Log and
D-Gamut have a white paper, `https://dl.djicdn.com/downloads/zenmuse+x7/20171010/D-Log_D-Gamut_Whitepaper.pdf`,
and the Mavic 3 and Mavic 4 Pro pages have an ACES workflow guide
(`.../ACES/ACES%20Workflow%20Guide%20.zip`, D-Log only; see HDR beyond the makers' LUTs). For D-Log2, DJI is
said to have made a DCTL public, but not on its download pages; copies are on GitHub.

### Metadata

For D-Log M the plugin already reads the Action 6's color profile from its metadata, and other DJI cameras by
the `_D` in the file name only (see README, Other cameras). Whether D-Log, D-Log2 and D-Log M are told apart
in the metadata of the Pocket 4/4P and Mavic 4 Pro wasn't looked into.

## GoPro

GoPro's page for all of them: "GoPro Log Profiles LUT Downloads",
`https://community.gopro.com/s/article/GoPro-Log-Profiles-LUT-Downloads?language=en_US` (updated 2026-06-22),
and per camera the "10-Bit Log Encoding" articles (HERO12: `.../s/article/10-Bit-Log-Encoding`, HERO13:
`.../s/article/HERO13-Black-10-Bit-Log-Encoding`). Both need JavaScript.

Each LUT there is a Salesforce share link (`https://gopro.my.salesforce.com/sfc/p/o0000000HJuF/a/<id>/<token>`)
that opens a preview page. The file itself downloads with plain curl from

    https://gopro.my.salesforce.com/sfc/dist/version/download/?oid=00Do0000000HJuF&ids=<version id>&d=%2Fa%2F<id>%2F<token>&asPdf=false

where the version id is the file version the share link shows (the preview page asks for it; it's listed below).
That's how Salesforce serves shared files, not something GoPro documents; the HERO12 links have worked since
2023. If GoPro uploads a new version, the version id changes.

| Camera, profile | File (GoPro's name, date) | Size | SHA-256 | Version id, share link `<id>/<token>` |
|---|---|---|---|---|
| HERO12 Black, GP-Log, firmware 1.10 and 1.12 | "GP-LOG Release LUTs" zip (2023-09-12): `GPLOG_Auto_WB.cube`, `GPLOG_Native_WB_5600K.cube` | 16 | zip `6e6e8beaa2bd2120693036c3318b59ad23802e3c1c16c9136ce7660a7706bf39`; Auto WB `4aeed37873d2f574f7b0aeeb3f09d68e115724ea13c654a466ed701bf9ccf005`; Native 5600K `b24eac529266678f1ac42442f28bdc14ccfb402455379422552e068a3cd83f9b` | `0688a00000UghlaAAB`, `8a000000AhkY/kCui4DkvleBnZwPqiCK413q2W.n5oRLACF_L_XmnO3o` |
| HERO12 Black, GP-Log, firmware 1.20 | `GPLOG_Creative_EV0` (2023-10-31) | 33 | `4cfcbd2009008b37882fe545c96f95971b00f8602e9e72c3ef1244302001f066` | `0688a00000XNEvbAAH`, `8a000000raNr/ej.q1IAzIDffUUwBgkzOC.RTxTYKd5n.ETcssvfwqjE` |
| HERO12 Black firmware 2.00 and later, HERO13 Black, GP-Log | `GPLOG_Creative_EV0` (2023-12-19) | 33 | `08e3e0e9b459e4949b4c8231e1b0cd013b3757c60865ce7f15b1a87f14a19778` | `068Uv000000PYmaIAG`, `Uv00000009cn/NkUMDdoR..Fjn6RxrBvOBoWlupfMmhTXOaa_Kkd9YR4` |
| MAX2, GP-Log | "Technical": GP-Log to Rec.709, no look (2025-11-12) | 33 | `595f39c5a0cefc122b3fef0c86b07ba90aa966170458e066ad3fb86f928ad7c5` | `068Uo00000Uu6eAIAR`, `Uo000002QHSH/N9hFoWmhb2AlJsFlExRELISr3VqHiKKiZK8eyvA.d3E` |
| | "Creative": roughly the camera's Natural profile (2025-11-12) | 33 | `a7b8f47f79cffd8e563e86c3adb1267045714fa36d992fce0de02669082bdbbe` | `068Uo00000Utkx5IAB`, `Uo000002QH5i/z1K4bhA6I2J5GvJImkXgZaLqiUpPaE5L.1ZBMN4Jl3c` |
| MISSION 1, MISSION 1 PRO, GP-Log2 | `GPLOG2_to_Rec709_65_CinematicProfile` "Creative" (2026-06-22) | 65 | `936dd7aa134da28b597ffeae0a816a0a1ed4dafae238e4f1fc2961d0a05e0091` | `068Uo00000gaFDKIA2`, `Uo000005CTzR/Bsmw_XA4pZ7C9PxNQeNQ8w4crLoxC_xPCSOs5NNcrJw` |
| | `GPLOG2_to_Rec709_65_EV+1.8_Rolloff_v2` "Technical" (2026-06-16) | 65 | `88d28e69d1de91f000f150be876391436c3fb71bb5219eac5113d7f1108cfba7` | `068Uo00000gISNcIAO`, `Uo0000057L6T/6zjCHQqjLoH9RI.YeYuUcuRge.pVKuWy_rZcSbXDizM` |
| | `GPLOG2_to_Rec709_33_EV+1.8_Rolloff_v2` (2026-06-16) | 33 | `4af551fb1e7d83e4d7a44c2c287645c8a3d183228e913cb69201d8fb36e0644c` | `068Uo00000gIjg2IAC`, `Uo0000057L85/uXR.qOrG3pj8Wsew7OzpqVHWHQjkulQP3UG4wY_dkhU` |
| | `GPLOG2_to_Rec709_33_EV+1.8_CreativeStart` (2026-05-27) | 33 | `61b0c799f0bb4bd640ac40d573543421f0c2522c79fca9e6f5f19d9c7984bfeb` | `068Uo00000fGUqaIAG`, `Uo000004qZAf/2LWAkbEqjlrV1zfiyJ9NGarro64bpUaYYxSZD.ZGeAg` |
| | `GPLOG2_to_Rec709_65_EV+1.8_CreativeStart` (2026-05-27) | 65 | `a84e893289fdf625650a84a447d72a47f03c10b95f00e1e3567c62dcc54fa445` | `068Uo00000fGQdHIAW`, `Uo000004qZCH/x370x_Ao03z3HzDqF5I.VpgNpk9NMbMv0xAQkFTjwcQ` |

Notes:

- **Which cameras have GP-Log.** HERO12 Black from launch (firmware 1.10, September 2023) and HERO13 Black,
  as "10-Bit Log" (10-bit, high bitrate, ISO limited to 400). MAX2 (2025), 8K only as one user reports;
  firmware of June 2026 added it to Time Lapse. MISSION 1 and MISSION 1 PRO (2026) have **GP-Log2**. HERO11
  Black has it only through GoPro Labs firmware (`WIDE=1` and log base 400), with no LUT from GoPro's support
  site. Other current models (HERO (2024) and the like) aren't on GoPro's LUT page.
- **The HERO12's GP-Log changed with firmware.** 1.10/1.12 had two LUTs (16³, one per white-balance setting);
  early firmware recorded 2 EV under, which its LUT pushed back up (per Zeb Gardner, below); 1.20 got a new
  LUT, "EV0"; 2.00 (December 2023, "Optimizes GP-Log tuning
  with wide-gamut color space" in GoPro's release notes) another, which HERO13 uses too. So for a HERO12 the
  plugin would need the firmware version from the file, and should prefer the 2.00 LUT when it can't tell.
- **The curve.** GP-Log is Protune's log curve with log base 400 (Protune Flat is 113), the one GoPro Labs
  exposes as `LOGB`; GP-Log2 uses 600 and Rec.2020 primaries, per the community transforms below. GP-Log's
  "wide" gamut isn't documented (GoPro's Labs maintainer: the tone mapping is internal and scene-dependent).
- **Metadata.** The GPMF `PTCL` (Protune color) tag takes `GOPRO`/`VIBRANT`, `FLAT`, `NATURAL`, `HDR`, `LOG`,
  `GPLOG2` and `CINEMATIC` ([gpmf-parser's README](https://github.com/gopro/gpmf-parser#readme)), so GP-Log
  is `LOG` and GP-Log2 `GPLOG2`. Not checked on a real GP-Log clip yet.
- **Not official, but from GoPro's Labs maintainer:** conversion LUTs for Labs' `WIDE=1` modes, posted in
  [gopro/labs discussion 627](https://github.com/gopro/labs/discussions/627) (2023-11-03), on Dropbox:
  `https://www.dropbox.com/scl/fi/jl4kuwcwekbb5exdsta6r/WIDE_Convertion_LUTs.zip?rlkey=zswcgy0aejn2f1sn80q2m2xuw&dl=1`.
  All 33³. `WIDE_LOG400_to_Rec709.cube` `d725899ccaf2d04298516a2a2df37e2a6d235610fffb503d4f8e4b3dd6eeb42d`,
  `WIDE_LOG400_to_709_LOG400.cube` (color only, keeps the curve)
  `b14a2227e356801272a1a6f825b24e93b5f758aac0e503dfc663742bc264a56f`, plus three for Flat and Natural. No
  license either.

## Insta360

One page for all: `https://www.insta360.com/download/i-log` ("Post-Production → I-Log"; there's also an ACES
page). Its Download buttons open plain zips on `https://wassets.insta360.com/common/<id>/<name>.zip`; the
zips carry macOS `__MACOSX` folders to skip when unpacking. The Insta360 app and Studio apply the same LUTs
for X5, Ace Pro 2 and GO Ultra.

**10-bit I-Log** (X6, Luna Ultra and Luna Pro). The two zips hold the same six files, "Version 2.0, Create
Date 2026.05.20" in their headers. Output: Rec.709 with BT.1886, HLG and PQ (ST 2084), both Rec.2020, 1000 nits
peak and diffuse white at 203 nits:

| File (in both zips) | Size | SHA-256 |
|---|---|---|
| `*_I-Log_to_Rec709_BT1886_s33_v2.cube` | 33 | `e3e5c3ab4ca7c166f3ebb740b45ed8cbce649b0c18dca9d5661754c53b9ee17d` |
| `*_I-Log_to_Rec709_BT1886_s65_v2.cube` | 65 | `06db0a02784fed3a5db4ef7fb3ee617c63c3b45b699653108927fa3917578d80` |
| `*_I-Log_to_Rec2020_HLG_1K_DW203_s33_v2.cube` | 33 | `312332772b49774138967dcb7648921a0fa6bce78ef259d2788942a47dbe13fc` |
| `*_I-Log_to_Rec2020_HLG_1K_DW203_s65_v2.cube` | 65 | `c1a70fe2977df8c9d52f913188a6d8edfc4b0befdef869e957589deefbb97fc5` |
| `*_I-Log_to_Rec2020_St2084_1K_DW203_s33_v2.cube` | 33 | `4243713458a72e351c8dff1ff626ba0d64b817d69d7fa542a1d43363c2860616` |
| `*_I-Log_to_Rec2020_St2084_1K_DW203_s65_v2.cube` | 65 | `b3bb1e772817bc4b66b737220923824a6f8f51d38aa1bb0f04aa869a0e20dfdb` |

Zips: X6 (page date 2026-08-12) `8d7c0e7275dd49baa6a2295197f46744/X6_I-Log_LUTs.zip`
`c59e65179835af2c6244ca1eb7dd42351950887517bd13578af05d0665d443fb`; Luna series (2026-06-10)
`b211cceb8b724b0e9d1858fe356f6a56/Luna_Series_I-Log_LUTs.zip`
`70c275155bda7bcc8b2ae1e2f391f7619a485553982398663d58758cf2589a35`.

The **"Insta360 10-bit I-Log White Paper"** (v1.0, June 2026,
`31f0330509384b7b8ba0a07f4ff4eb13/Insta360_10-bit_I-Log_White_Paper.pdf`) defines the curve and the gamut: BT.2020
primaries and D65 white, and from scene-linear reflection `x` to I-Log `y`

    y = α·x + β                  for x ≤ 0.01104854
    y = δ + η·log10(x + θ)       for x > 0.01104854
    α = 5.77837328, β = 0.09055934, δ = 0.623992, η = 0.280055, θ = 0.01

(18 % grey → 0.422, 10-bit code 432; 2200 % → 1.0). With that, an I-Log → HLG conversion of our own needs no
LUT from Insta360, the way `dlogm_hlg.cube` is made for D-Log M.

**8-bit I-Log**, one zip per model (page dates all 2025-12-10; the files inside are from 2025-04-22, GO
Ultra's 2025-12-03), Rec.709 out only:

| Model | File in the zip | Size | SHA-256 of the .cube | Zip `<id>/<name>`, SHA-256 |
|---|---|---|---|---|
| Ace Pro 2 | `AcePro2_I-Log_To_Rec.709_V1.0.cube` | 33 | `7ffabf0359afdcc64132ad166513b052ba2f796fad451aa6faa8232f9a0929a7` | `bafb3ae5ff7c43eba22e788ddea14b73/Insta360 Ace Pro 2 LUT.zip`, `5782a1ba117c44898b0f5792fc59986918f3c746f2de454638d379290e427498` |
| GO Ultra | `GO Ultra_I-Log_To_Rec.709_V1.0.cube`, the same file as Ace Pro 2's | 33 | same | `c4c53625ca1f450bb6546b381297ee25/Insta360 GO Ultra LUT.zip`, `2df443933ce3e8f4786a4e63c7f08754b1b3317b3ca25778ea58d9d6c95b41bb` |
| X5 | `X5_I-Log_To_Rec.709_V1.0.cube` | 65 | `edba4055614bb2c3e8fa66435998aa18fc9ed8da4dbf2de23ce57038670f2e56` | `056218ae92504c73965e6f40eb394d79/Insta360 X5 LUT.zip`, `0c05ecb78842f5b21fa96d7e705ef2346f4ac5ec68a62bfac92a9e58576d527c` |
| X3 | `X3-LUT-V1.0.0.cube` | 65 | `1e540e964333eb984481b95fffb3178dd08abc4c210644ca810d21a535c62b3e` | `00dba868915d4fd9ba44aacafc538186/Insta360 X3 LUT.zip`, `7b459111cf781a76bf2343bd95d82a15928e16c4571689cbf0539eb6bd86cd92` |
| ONE X2 | `ONE-X2-LUT-Final-V1.0.0.cube`, the same file as X3's | 65 | same | `5f707eb89f484b729dacd4a0f7a1ef49/Insta360 ONE X2 LUT.zip`, `71e4376d8c065bdb9de7192f9d3f92627bc9f0876c1ceb42864fe5d12e43c3d5` |
| ONE X | `ONE-X-LUT-Final-V1.0.0.cube` | 64 | `bab317aa511787c484b9e721955c43747ef4a6f6ad8ff8f77cd8fcaf76d9f098` | `c6f85b7627874b6a96bc324f5c553944/Insta360 ONE X LUT.zip`, `3042499cf566f8911b7bd1b640f1afc0d01f50828c5178bce760f0d83e2f448f` |
| ONE R | one per lens: `1-INCH.CUBE`, `4K-WIDE-ANGLE.CUBE`, `DUAL-LENS-360.CUBE` | 65 | `5676f41f…`, `94d429e8…`, `d8b3c80d…` | `24a67d2b865f42d086cc848632203579/Insta360 ONE R LUT.zip`, `714fb1e81b7509e1208ea524d2af3ca3fd8e97f650406416a6961afd45e6fa4e` |
| ONE RS | one per lens (1-inch, 4K wide, 4K Boost, 360), each 33 and 65 | 33, 65 | 8 files | `257dddf895ff46088916ebdb62d23c9e/Insta360 ONE RS LUT.zip`, `8b1b240c6b8662e45cbd995a274f9c5a84c893a982c1f7e460c25201a2aa716e` |
| Sphere | ONE RS's `DUAL-LENS-360_33.CUBE` and `_65.CUBE` | 33, 65 | `80629082…`, `09329898…` | `ffcdb5cc018545e3a71fafcbe80cab14/Insta360 Sphere LUT.zip`, `12840b6d11319d3ca844a079914286464935c590a3dd3f6c48d404692970ba63` |

Notes:

- **No LUT listed** for Ace Pro (the first), Ace, X4, X4 Air, GO 3, GO 3S, though the online manual names
  Insta360 Studio's LUT support for "Luna Ultra, ONE X, X2, X3, X5, Ace Pro 2, GO Ultra, ONE R, ONE RS".
  Whether those others have an I-Log mode wasn't checked.
- 8-bit I-Log's curve isn't published; the white paper is for the 10-bit one ("new transfer function for 10
  bit container", 2025-07-07).
- **Metadata**: how an Insta360 MP4 says it's I-Log wasn't found. telemetry-parser reads Insta360's own
  trailer; a color-mode field there is the first place to look.

## Third-party LUTs

None of these is the maker's, and none states a license, so they'd be downloaded at install like the
official ones, if at all, except the MIT-licensed transforms.

- **Zeb Gardner** ([zebgardner.com](https://www.zebgardner.com/photo-and-video-editing)): technical
  conversions, fitted from test charts and compared for accuracy with the makers' LUTs (he finds GoPro's
  adds "a slight 'look'"). Free downloads, no terms given; files on Squarespace's CDN
  (`static1.squarespace.com/...`, reached through `https://www.zebgardner.com/s/<name>`).
  - D-Log M → Rec.709, 65³, V1 (2026-02-13), `DLOGM_REC709_65_CUBE_ZRG_V1.zip` (zip
    `97c1e79abff96b8702fdd0c3e04ca8b1e5fb7e258223393e649260e91b5aa3f0`, cube
    `14dc0e5d7a07886c5e8fbb3fb56d8bbfda9942e0a5e97a9046346d12c53bdde9`); and D-Log M → DaVinci Wide
    Gamut/Intermediate with a 1D shaper, `DLOGM_DWG_I_SHAPER_ZRG_V1.zip`.
  - GP-Log → Rec.709, 65³, V1 (2026), `GP_LOG-to-REC709-ZRG-V1.cube`
    `aeb940eed62deffd61fc235e44f9d5c14cc9047569d5f5eb61e86b18a1f04a60`; and GP-Log → DWG/I with a shaper.
  - Pages: "D-Log-M Color Management LUT comparison", "GoPro Log Color Management Accuracy Testing", and a
    Python program that fits a color space transform for undocumented camera spaces.
- **xtremestuff/protune-transforms** ([GitHub](https://github.com/xtremestuff/protune-transforms), **MIT**):
  DCTLs for DaVinci Resolve for Protune, GP-Log (log base 400) and GP-Log2 (600, Rec.2020), to ACES, DWG,
  Rec.2020 and Rec.709. Shaders, not LUTs, but they write down the curves and matrices, which is what an HDR
  path for GoPro would be built from; "Native" gamut from a HERO5-era matrix, so approximate on newer sensors.
  Its author sells per-camera versions (GP-Tune).
- Paid packs (Leeming LUT Pro for GoPro, DJI, Insta360; many on Gumroad/Sellfy) and free creative looks
  (Cine Color's free GP-Log LUT, an S-Log3-derived set for D-Log M on xdcam-user.com): not for this.

## Sources

- DJI Download Center, e.g. <https://www.dji.com/global/downloads/softwares/flip-dlog-to-rec709> (the catalog
  of all LUTs is in every such page); product pages `https://www.dji.com/<product>/downloads`.
- GoPro: <https://community.gopro.com/s/article/GoPro-Log-Profiles-LUT-Downloads?language=en_US>,
  <https://community.gopro.com/s/article/10-Bit-Log-Encoding?language=en_US>,
  <https://community.gopro.com/s/article/HERO13-Black-10-Bit-Log-Encoding?language=en_US>,
  <https://github.com/gopro/labs/discussions/627>, <https://github.com/gopro/gpmf-parser#readme>; MAX2 GP-Log
  in Time Lapse: GoPro's press release of June 2026
  (<https://s21.q4cdn.com/291350743/files/doc_news/GoPro-Elevates-Award-Winning-MAX2-360-Camera-Image-Quality-with-Powerful-New-Firmware-Update-2026.pdf>).
- Insta360: <https://www.insta360.com/download/i-log>, the white paper above, and Studio's manual
  <https://onlinemanual.insta360.com/studio/zh-cn/operation_guide/plugin_features/lut>.
- Zeb Gardner: <https://www.zebgardner.com/photo-and-video-editing/d-log-m-color-management-lut-comparison>,
  <https://www.zebgardner.com/photo-and-video-editing/gopro-log-color-management-accuracy-testing>.
