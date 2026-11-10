// SPDX-License-Identifier: GPL-3.0-or-later
//
// Snapshots: what the journals said up to some line of each, written once and never changed.
//
// .afp/catalog/snap/<generation, 8 digits>-<hash>.json: the snapshot's list (a manifest): its generation, which line of
// each computer's journal it has read up to (`covers`), each computer's highest version in it, and its parts.
// .afp/catalog/parts/<hash>.gz: a part, the records whose key falls in one bucket, gzipped JSON lines. Both named by
// their content's SHA-256: two computers writing the same name wrote the same bytes, and a file read is checked
// against its name (a cut-off or damaged one isn't used).
//
// Records go into buckets by what a reader looks for: a moment, clip preview, export or photo by its original (its
// path in the link, else its fingerprint), anything else by its folder. The player, opening a clip, reads
// the one or two parts its clip's moments are in, not the whole catalog.

use crate::state::{ entry_from, entry_json, hex, parent, split, File, Rec, State, Version };
use flate2::{ read::GzDecoder, write::GzEncoder, Compression };
use serde_json::{ json, Value };
use sha2::{ Digest, Sha256 };
use std::collections::BTreeMap;
use std::io::{ Read, Write };

pub const FORMAT: u64 = 1;

#[derive(Clone, Debug)]
pub struct Manifest {
    pub gen: u64,
    pub by: String,
    pub covers: BTreeMap<String, u64>,
    pub lamports: BTreeMap<String, u64>,
    pub l: u64,
    pub bits: u32,
    pub parts: Vec<String>,
    pub count: u64,
    /// Where each computer's lines it has read end: (segment, byte), for a reader to read on from there
    pub at: BTreeMap<String, (u64, u64)>,
}

pub fn sha(b: &[u8]) -> String { hex(&Sha256::digest(b)[..16]) }

impl Manifest {
    pub fn to_bytes(&self) -> Vec<u8> {
        json!({ "format": FORMAT, "gen": self.gen, "by": self.by, "covers": self.covers, "lamports": self.lamports, "l": self.l,
                "bits": self.bits, "parts": self.parts, "count": self.count,
                "at": self.at.iter().map(|(k, (s, o))| (k.clone(), json!([s, o]))).collect::<serde_json::Map<String, Value>>() }).to_string().into_bytes()
    }
    pub fn name(&self, bytes: &[u8]) -> String { format!("{:08}-{}.json", self.gen, sha(bytes)) }

    /// Read and checked against its name; Err: not one to use (a newer format says so)
    pub fn from_bytes(name: &str, b: &[u8]) -> Result<Manifest, String> {
        let (gen, hash) = name_parts(name).ok_or("not a snapshot's name")?;
        if sha(b) != hash { return Err("damaged (its content isn't what its name says)".into()); }
        let v: Value = serde_json::from_slice(b).map_err(|e| e.to_string())?;
        if v["format"].as_u64() != Some(FORMAT) { return Err(format!("format {} (this one reads {FORMAT})", v["format"])); }
        let map = |k: &str| -> BTreeMap<String, u64> {
            v[k].as_object().map(|o| o.iter().filter_map(|(k, x)| x.as_u64().map(|n| (k.clone(), n))).collect()).unwrap_or_default()
        };
        let m = Manifest {
            gen: v["gen"].as_u64().ok_or("no generation")?,
            by: v["by"].as_str().unwrap_or("").to_string(),
            covers: map("covers"),
            lamports: map("lamports"),
            l: v["l"].as_u64().unwrap_or(0),
            bits: v["bits"].as_u64().unwrap_or(0) as u32,
            parts: v["parts"].as_array().ok_or("no parts")?.iter().filter_map(|p| p.as_str().map(str::to_string)).collect(),
            count: v["count"].as_u64().unwrap_or(0),
            at: v["at"].as_object().map(|o| o.iter().filter_map(|(k, x)| Some((k.clone(), (x.get(0)?.as_u64()?, x.get(1)?.as_u64()?)))).collect()).unwrap_or_default(),
        };
        if m.gen != gen || m.parts.len() != 1usize << m.bits { return Err("inconsistent".into()); }
        Ok(m)
    }
}

/// A snapshot's name: (generation, hash)
pub fn name_parts(name: &str) -> Option<(u64, String)> {
    let stem = name.strip_suffix(".json")?;
    let (g, h) = stem.split_once('-')?;
    (g.len() == 8 && h.len() == 32).then(|| g.parse().ok().map(|g| (g, h.to_string()))).flatten()
}

/// What a record is looked for by: its original (link), or its folder
pub fn bucket_key(root: &str, rel: &str, f: Option<&File>) -> String {
    if let Some(link) = f.and_then(|f| f.meta.as_ref()).and_then(|m| m.get("link")) {
        if let Some(c) = link.get("clip").and_then(Value::as_str) { return format!("c:{c}"); }
        if let Some(h) = link.get("hash").and_then(Value::as_str).filter(|h| !h.is_empty()) { return format!("h:{h}"); }
    }
    format!("d:{root}/{}", parent(rel))
}

pub fn bucket(k: &str, bits: u32) -> u32 {
    if bits == 0 { return 0; }
    let h = Sha256::digest(k.as_bytes());
    u32::from_be_bytes([h[0], h[1], h[2], h[3]]) >> (32 - bits)
}

/// Parts for this many records: about 2000 in each, at most 256
pub fn bits_for(count: usize) -> u32 {
    let mut bits = 0;
    while bits < 8 && (count >> bits) > 2000 { bits += 1; }
    bits
}

pub fn encode_part(recs: &[(&String, &Rec)]) -> Vec<u8> {
    let mut text = String::new();
    for (k, r) in recs {
        let (root, rel) = split(k);
        let mut v = entry_json(root, rel, r.f.as_ref());
        v["l"] = json!(r.v.l);
        v["b"] = json!(&*r.v.by);
        text.push_str(&v.to_string());
        text.push('\n');
    }
    let mut e = GzEncoder::new(Vec::new(), Compression::new(6));
    e.write_all(text.as_bytes()).unwrap();
    e.finish().unwrap()
}

pub fn part_name(bytes: &[u8]) -> String { format!("{}.gz", sha(bytes)) }

/// A part's records into the state; Err: damaged (not what its name says, or not readable)
pub fn decode_part(name: &str, bytes: &[u8], state: &mut State) -> Result<usize, String> {
    if part_name(bytes) != name { return Err(format!("part {name} damaged")); }
    let mut text = String::new();
    GzDecoder::new(bytes).read_to_string(&mut text).map_err(|e| format!("part {name}: {e}"))?;
    let mut n = 0;
    for line in text.lines() {
        let v: Value = serde_json::from_str(line).map_err(|e| format!("part {name}: {e}"))?;
        let (root, rel, f) = entry_from(&v).ok_or_else(|| format!("part {name}: a record without its path"))?;
        let by = state.id(v["b"].as_str().unwrap_or(""));
        let r = Rec { v: Version { l: v["l"].as_u64().unwrap_or(0), by }, f };
        state.apply(crate::state::key(&root, &rel), r);
        n += 1;
    }
    Ok(n)
}
