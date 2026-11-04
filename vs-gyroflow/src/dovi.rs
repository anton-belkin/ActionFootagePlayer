// SPDX-License-Identifier: GPL-3.0-or-later
//
// Dolby Vision profile 8.4 RPUs for a "look": L8 trims for the 100-nit target and L3 offsets on top of an L1 scene
// description, so macOS's own display management (AVSampleBufferDisplayLayer, QuickTime) tone-maps an HLG frame the
// way the look was tuned in DV Lab (~/Downloads/DJI/stabilized/dv_lab, issue #96). The generator config is DV Lab's
// (DVLab.swift, generatorConfig), the RPUs are the dolby_vision crate's, which dovi_tool is built on: the bytes are
// what `dovi_tool generate -p 8.4 -j <config>` writes for the same config (tools/make_dv_rpu.sh checks it).
//
// Out: Annex-B, a 00 00 00 01 start code before each frame's RPU (the NAL from its header 7C 01 on, without it), one
// per frame; a file of one RPU stands for every frame. mpv's --macos-dovi-rpu-file and gyroflow-export --dv-rpu read
// it.

use std::io::Write;

use dolby_vision::rpu::extension_metadata::blocks::{
    ExtMetadataBlock, ExtMetadataBlockLevel1, ExtMetadataBlockLevel3, ExtMetadataBlockLevel6, ExtMetadataBlockLevel8,
};
use dolby_vision::rpu::generate::{ GenerateConfig, GenerateProfile, ShotFrameEdit, VideoShot };
use dolby_vision::rpu::vdr_dm_data::CmVersion;
use serde::Deserialize;

/// A look as DV Lab saves it (looks/*.json): CM 4.0 trims in 12-bit codes, neutral at 2048, the six-vector fields
/// at 128. What isn't in the file is neutral; what DV Lab adds beside it (its dovi_tool example) is ignored.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Look {
    #[serde(rename = "useL8")]
    pub use_l8: bool,
    pub slope: u16,
    pub offset: u16,
    pub power: u16,
    pub chroma_weight: u16,
    pub saturation: u16,
    pub ms_weight: u16,
    pub mid_contrast: u16,
    pub clip_trim: u16,
    pub sat_vec: [u8; 6],
    pub hue_vec: [u8; 6],
    pub l3_min: u16,
    pub l3_avg: u16,
    pub l3_max: u16,
}

impl Default for Look {
    fn default() -> Self {
        Self { use_l8: true, slope: 2048, offset: 2048, power: 2048, chroma_weight: 2048, saturation: 2048, ms_weight: 2048,
               mid_contrast: 2048, clip_trim: 2048, sat_vec: [128; 6], hue_vec: [128; 6], l3_min: 2048, l3_avg: 2048, l3_max: 2048 }
    }
}

/// A frame's L1 in 12-bit PQ codes, as DV Lab's frames.json has them. `scene_cut` (in a per-frame file) starts a new
/// shot at this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct L1 {
    pub min_pq: u16,
    pub avg_pq: u16,
    pub max_pq: u16,
    #[serde(default)]
    pub scene_cut: Option<bool>,
}

impl L1 {
    pub const fn new(min_pq: u16, avg_pq: u16, max_pq: u16) -> Self { Self { min_pq, avg_pq, max_pq, scene_cut: None } }

    /// `min,avg,max`
    pub fn parse(s: &str) -> Option<Self> {
        let v: Vec<u16> = s.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
        match v[..] { [min, avg, max] => Some(Self::new(min, avg, max)), _ => None }
    }

    fn block(&self) -> ExtMetadataBlock {
        ExtMetadataBlock::Level1(ExtMetadataBlockLevel1 { min_pq: self.min_pq, max_pq: self.max_pq, avg_pq: self.avg_pq })
    }
    fn same_values(&self, o: &Self) -> bool { (self.min_pq, self.avg_pq, self.max_pq) == (o.min_pq, o.avg_pq, o.max_pq) }
}

/// Where the shipped look's constant L1 comes from: the mean of DV Lab's six fit frames (frames.json: min 12, avg
/// 1604-1967, mean 1758, max 2603-2730, mean 2694), rounded.
pub const DEFAULT_L1: L1 = L1::new(12, 1750, 2700);

/// A per-frame L1 file: a JSON array of {"min_pq", "avg_pq", "max_pq"} (and "scene_cut": true where a shot starts).
pub fn read_l1_file(path: &std::path::Path) -> Result<Vec<L1>, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Vec<L1> = serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))?;
    if v.is_empty() { return Err(format!("{}: no frames", path.display())); }
    Ok(v)
}

fn look_blocks(look: &Look) -> Vec<ExtMetadataBlock> {
    let mut blocks = vec![ExtMetadataBlock::Level3(ExtMetadataBlockLevel3 {
        min_pq_offset: look.l3_min, max_pq_offset: look.l3_max, avg_pq_offset: look.l3_avg })];
    if look.use_l8 {
        let (s, h) = (look.sat_vec, look.hue_vec);
        blocks.push(ExtMetadataBlock::Level8(ExtMetadataBlockLevel8 {
            length: 25, target_display_index: 1,
            trim_slope: look.slope, trim_offset: look.offset, trim_power: look.power,
            trim_chroma_weight: look.chroma_weight, trim_saturation_gain: look.saturation, ms_weight: look.ms_weight,
            target_mid_contrast: look.mid_contrast, clip_trim: look.clip_trim,
            saturation_vector_field0: s[0], saturation_vector_field1: s[1], saturation_vector_field2: s[2],
            saturation_vector_field3: s[3], saturation_vector_field4: s[4], saturation_vector_field5: s[5],
            hue_vector_field0: h[0], hue_vector_field1: h[1], hue_vector_field2: h[2],
            hue_vector_field3: h[3], hue_vector_field4: h[4], hue_vector_field5: h[5],
        }));
    }
    blocks
}

/// DV Lab's generator config for `frames` frames. `l1` is one L1 for all of them, or one per frame. A per-frame list
/// with scene_cut set anywhere has shots from those frames (frame 0 always starts one) and, inside a shot, a frame
/// edit where L1 changes; without, each change of L1 starts a shot, so constant L1 is one shot and L1 that changes
/// every frame is a shot per frame.
pub fn generator_config(look: &Look, l1: &[L1], frames: usize) -> Result<GenerateConfig, String> {
    if frames == 0 { return Err("no frames".into()); }
    if l1.len() != 1 && l1.len() != frames {
        return Err(format!("{} L1 values for {frames} frames (one, or one per frame)", l1.len()));
    }
    let at = |i: usize| &l1[if l1.len() == 1 { 0 } else { i }];
    let marked = l1.iter().any(|f| f.scene_cut.is_some());
    let mut shots: Vec<VideoShot> = Vec::new();
    for i in 0..frames {
        let f = at(i);
        let cut = i == 0 || if marked { f.scene_cut == Some(true) } else { !f.same_values(at(i - 1)) };
        if cut {
            let mut blocks = vec![f.block()];
            blocks.extend(look_blocks(look));
            shots.push(VideoShot { start: i, duration: 1, metadata_blocks: blocks, ..Default::default() });
            continue;
        }
        // A frame edit is for that one frame: each frame whose L1 isn't the shot's has one
        let shot = shots.last_mut().unwrap();
        if !f.same_values(at(shot.start)) {
            shot.frame_edits.push(ShotFrameEdit { edit_offset: i - shot.start, metadata_blocks: vec![f.block()] });
        }
        shot.duration += 1;
    }
    let mut config = GenerateConfig {
        cm_version: CmVersion::V40,
        profile: GenerateProfile::Profile84,
        length: frames,
        source_min_pq: Some(62),
        source_max_pq: Some(3079),
        level6: Some(ExtMetadataBlockLevel6 {
            max_display_mastering_luminance: 1000, min_display_mastering_luminance: 50,
            max_content_light_level: 1000, max_frame_average_light_level: 400 }),
        shots,
        ..Default::default()
    };
    // What dovi_tool generate does with a JSON config before writing it
    config.l1_avg_pq_cm_version = Some(config.cm_version);
    config.fixup_l1();
    Ok(config)
}

/// The config as JSON, for `dovi_tool generate -j` (the check that both write the same bytes)
pub fn config_json(config: &GenerateConfig) -> String {
    serde_json::to_string_pretty(config).unwrap()
}

/// The RPUs of the config in the .rpu file format (Annex-B), as dovi_tool's GenerateConfig::write_rpus writes them
pub fn write_rpus(config: &GenerateConfig, out: &mut impl Write) -> Result<(), String> {
    let rpus = config.generate_rpu_list().map_err(|e| e.to_string())?;
    for rpu in &rpus {
        let nal = rpu.write_hevc_unspec62_nalu().map_err(|e| e.to_string())?;
        out.write_all(&[0, 0, 0, 1]).and_then(|_| out.write_all(&nal[2..])).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn rpu_bytes(config: &GenerateConfig) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    write_rpus(config, &mut v)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dolby_vision::rpu::dovi_rpu::DoviRpu;

    fn nbcu_fit() -> Look {
        serde_json::from_str(include_str!("../../mpv/scripts/dlogm/dv/nbcu_fit.json")).unwrap()
    }

    // The RPUs back from the bytes, with each frame's L1 and scene cut flag
    fn parse(bytes: &[u8]) -> Vec<(DoviRpu, u16, u16, u16, bool)> {
        let starts: Vec<usize> = (0..bytes.len().saturating_sub(3)).filter(|&i| bytes[i..i + 4] == [0, 0, 0, 1]).collect();
        assert_eq!(starts.first(), Some(&0));
        starts.iter().enumerate().map(|(n, &s)| {
            let end = starts.get(n + 1).copied().unwrap_or(bytes.len());
            let rpu = DoviRpu::parse_unspec62_nalu(&[&[0x7C, 0x01][..], &bytes[s + 4..end]].concat()).unwrap();
            let dm = rpu.vdr_dm_data.as_ref().unwrap();
            let Some(ExtMetadataBlock::Level1(l1)) = dm.get_block(1) else { panic!("no L1") };
            let (a, b, c) = (l1.min_pq, l1.avg_pq, l1.max_pq);
            let cut = dm.scene_refresh_flag == 1;
            (rpu, a, b, c, cut)
        }).collect()
    }

    #[test]
    fn look_from_dv_lab() {
        let l = nbcu_fit();
        assert_eq!((l.slope, l.power, l.saturation, l.mid_contrast, l.clip_trim), (2560, 2352, 2080, 544, 2928));
        assert_eq!((l.l3_min, l.l3_avg, l.l3_max), (2048, 2048, 2048));
        assert!(l.use_l8);
        assert_eq!(serde_json::from_str::<Look>("{}").unwrap(), Look::default());
    }

    #[test]
    fn constant_l1() {
        let c = generator_config(&nbcu_fit(), &[DEFAULT_L1], 3).unwrap();
        assert_eq!(c.shots.len(), 1);
        let frames = parse(&rpu_bytes(&c).unwrap());
        assert_eq!(frames.len(), 3);
        for (i, (rpu, min, avg, max, cut)) in frames.iter().enumerate() {
            assert_eq!((*min, *avg, *max, *cut), (12, 1750, 2700, i == 0));
            assert_eq!(rpu.dovi_profile, 8);
            let dm = rpu.vdr_dm_data.as_ref().unwrap();
            let Some(ExtMetadataBlock::Level8(l8)) = dm.get_block(8) else { panic!("no L8") };
            assert_eq!((l8.target_display_index, l8.trim_slope, l8.clip_trim), (1, 2560, 2928));
            let Some(ExtMetadataBlock::Level3(l3)) = dm.get_block(3) else { panic!("no L3") };
            assert_eq!(l3.avg_pq_offset, 2048);
        }
        // The shipped file is this, for one frame
        let one = rpu_bytes(&generator_config(&nbcu_fit(), &[DEFAULT_L1], 1).unwrap()).unwrap();
        assert_eq!(one, include_bytes!("../../mpv/scripts/dlogm/dv/nbcu_fit.rpu"));
    }

    #[test]
    fn per_frame_l1() {
        let f = |avg| L1::new(12, avg, 2700);
        // Without scene cuts: a shot wherever L1 changes
        let l1 = [f(1700), f(1700), f(1800), f(1900), f(1900)];
        let c = generator_config(&nbcu_fit(), &l1, 5).unwrap();
        assert_eq!(c.shots.iter().map(|s| (s.start, s.duration)).collect::<Vec<_>>(), [(0, 2), (2, 1), (3, 2)]);
        let got: Vec<(u16, bool)> = parse(&rpu_bytes(&c).unwrap()).iter().map(|f| (f.2, f.4)).collect();
        assert_eq!(got, [(1700, true), (1700, false), (1800, true), (1900, true), (1900, false)]);

        // With them: shots from the cuts, L1 changing inside a shot
        let mut l1 = l1;
        l1[3].scene_cut = Some(true);
        let c = generator_config(&nbcu_fit(), &l1, 5).unwrap();
        assert_eq!(c.shots.iter().map(|s| (s.start, s.duration)).collect::<Vec<_>>(), [(0, 3), (3, 2)]);
        let got: Vec<(u16, bool)> = parse(&rpu_bytes(&c).unwrap()).iter().map(|f| (f.2, f.4)).collect();
        assert_eq!(got, [(1700, true), (1700, false), (1800, false), (1900, true), (1900, false)]);

        // A change held for several frames inside a shot
        let mut l1 = [f(1700), f(1800), f(1800), f(1800), f(1700)];
        l1[0].scene_cut = Some(true);
        let got: Vec<u16> = parse(&rpu_bytes(&generator_config(&nbcu_fit(), &l1, 5).unwrap()).unwrap()).iter().map(|f| f.2).collect();
        assert_eq!(got, [1700, 1800, 1800, 1800, 1700]);

        assert!(generator_config(&nbcu_fit(), &l1, 4).is_err());
    }

    #[test]
    fn l1_clamped_like_dovi_tool() {
        // CM 4.0's lowest avg_pq
        let c = generator_config(&Look::default(), &[L1::new(0, 100, 2000)], 1).unwrap();
        assert_eq!(parse(&rpu_bytes(&c).unwrap())[0].2, 1229);
    }

    #[test]
    fn l1_arg() {
        assert_eq!(L1::parse("12, 1750,2700"), Some(DEFAULT_L1));
        assert_eq!(L1::parse("12,1750"), None);
        assert_eq!(L1::parse("a,b,c"), None);
    }
}
