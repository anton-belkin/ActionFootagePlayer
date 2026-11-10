// SPDX-License-Identifier: GPL-3.0-or-later
//
// The journals: each computer's own folder in .afp/catalog/journal/, written only by it, of segments named by the
// number of their first line (12 digits). A line: 8 hex digits of its text's SHA-256, a space, its text (JSON), a
// newline. A segment's first line names its computer and itself; the others are batches of records, numbered, with
// the version they're written at.
//
// A line cut off (the connection lost while it was written, the program stopped) has no newline or the wrong sum: it's
// left out. After a write that failed, the segment isn't written to again: the next line starts a new segment, named
// by that line's number. A line whose number was read already is left out (a line written twice: it had arrived).

use crate::state::{ entry_from, entry_json, hex, File };
use serde_json::{ json, Value };
use sha2::{ Digest, Sha256 };

pub struct Line {
    pub seq: u64,
    pub l: u64,
    pub entries: Vec<(String, String, Option<File>)>,
}

pub fn segment_name(first: u64) -> String { format!("{first:012}.log") }

pub fn segment_number(name: &str) -> Option<u64> {
    let n = name.strip_suffix(".log")?;
    (n.len() == 12 && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.parse().ok()).flatten()
}

fn frame(text: &str) -> Vec<u8> {
    let sum = hex(&Sha256::digest(text.as_bytes())[..4]);
    format!("{sum} {text}\n").into_bytes()
}

pub fn header(by: &str, first: u64) -> Vec<u8> { frame(&json!({ "computer": by, "segment": first, "format": 1 }).to_string()) }

pub fn line(seq: u64, l: u64, entries: &[(String, String, Option<File>)]) -> Vec<u8> {
    let e: Vec<Value> = entries.iter().map(|(r, p, f)| entry_json(r, p, f.as_ref())).collect();
    frame(&json!({ "q": seq, "l": l, "e": e }).to_string())
}

pub struct Parsed {
    pub lines: Vec<Line>,
    /// The bytes up to the end of the last whole line
    pub consumed: usize,
    /// Whole lines left out (a wrong sum: written over by a cut-off one)
    pub bad: usize,
    /// The segment's computer, from its first line
    pub computer: Option<String>,
    /// Whether it ends with a line cut off
    pub torn: bool,
}

pub fn parse(bytes: &[u8]) -> Parsed {
    let mut out = Parsed { lines: Vec::new(), consumed: 0, bad: 0, computer: None, torn: false };
    let mut at = 0;
    while at < bytes.len() {
        let Some(nl) = bytes[at..].iter().position(|&b| b == b'\n') else { out.torn = true; break };
        let raw = &bytes[at..at + nl];
        at += nl + 1;
        out.consumed = at;
        let ok = (|| {
            let s = std::str::from_utf8(raw).ok()?;
            let (sum, text) = s.split_once(' ')?;
            if hex(&Sha256::digest(text.as_bytes())[..4]) != sum { return None; }
            serde_json::from_str::<Value>(text).ok()
        })();
        let Some(v) = ok else { out.bad += 1; continue };
        if let Some(c) = v.get("computer").and_then(Value::as_str) { out.computer = Some(c.to_string()); continue; }
        let (Some(seq), Some(l), Some(e)) = (v.get("q").and_then(Value::as_u64), v.get("l").and_then(Value::as_u64), v.get("e").and_then(Value::as_array)) else { out.bad += 1; continue };
        out.lines.push(Line { seq, l, entries: e.iter().filter_map(entry_from).collect() });
    }
    out
}
