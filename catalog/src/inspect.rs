// SPDX-License-Identifier: GPL-3.0-or-later
//
// What the catalog keeps of what's inside the project's files (docs/library-management.md §9), read the same way by
// every program that looks: the Library's looks, afp-catalog's notes for the player.
//
// - a moment, clip preview, export or render (a video in the project's own area, or a .afplink.mp4 anywhere): its link,
//   the MP4 key ch.actionfootageplayer.link — { "link": … }
// - a photo there: its link and tags, from its XMP — { "link": …, "tags": […] }
// - a clip's sidecar (.afp.json, .marks.json), anywhere: its marks, view, source, tags, rating and location — { "own": … }
//
// A file that couldn't be read is an error (the look leaves its folder to be listed again), not a file without a link.

use crate::catalog::Inspect;
use serde_json::{ json, Value };
use std::fs::File;
use std::io::{ self, Read, Seek, SeekFrom };
use std::path::Path;

pub const LINK_KEY: &str = "ch.actionfootageplayer.link";
pub const LINK_EXT: &str = ".afplink.mp4";
/// A project's own folders (at its top, or at a location's top where they aren't footage)
pub const OWN_FOLDERS: [&str; 13] = ["_moments", "_clips", "_exports", "_stills", "_renders", "_cache", "moments", "clips", "exports", "stabilized",
                                     "_actionfootageplayer", "generated", "#recycle"];

fn be32(s: &[u8], i: usize) -> Option<u32> { s.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) }

fn file_boxes(f: &mut File, first: u64, last: u64) -> io::Result<Vec<([u8; 4], u64, u64)>> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let mut head = [0u8; 16];
        f.seek(SeekFrom::Start(pos))?;
        let n = f.read(&mut head)?;
        if n < 8 { break; }
        let mut size = be32(&head, 0).unwrap() as u64;
        let mut header = 8;
        if size == 1 && n >= 16 { size = u64::from_be_bytes(head[8..16].try_into().unwrap()); header = 16; } else if size == 0 { size = last - pos; }
        if size < header || pos + size > last { break; }
        out.push((head[4..8].try_into().unwrap(), pos + header, pos + size));
        pos += size;
    }
    Ok(out)
}

fn mem_boxes(s: &[u8], first: usize, last: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let Some(size) = be32(s, pos).map(|v| v as usize) else { break };
        if size < 8 || pos + size > last { break; }
        out.push((s[pos + 4..pos + 8].try_into().unwrap(), pos + 8, pos + size));
        pos += size;
    }
    out
}

/// An MP4's mdta keys (moov/meta, then moov/udta/meta)
pub fn mp4_keys(path: &Path) -> io::Result<Vec<(String, String)>> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let Some((_, a, b)) = file_boxes(&mut f, 0, size)?.into_iter().find(|(t, _, _)| t == b"moov") else { return Ok(Vec::new()) };
    if b - a > 64 * 1024 * 1024 { return Ok(Vec::new()); }
    let mut s = vec![0u8; (b - a) as usize];
    f.seek(SeekFrom::Start(a))?;
    f.read_exact(&mut s)?;
    let mut metas = Vec::new();
    for (t, a, b) in mem_boxes(&s, 0, s.len()) {
        if &t == b"meta" { metas.insert(0, (a, b)); }
        if &t == b"udta" { for (ut, ua, ub) in mem_boxes(&s, a, b) { if &ut == b"meta" { metas.push((ua, ub)); } } }
    }
    let mut keys: Vec<(String, String)> = Vec::new();
    for (a, b) in metas {
        let first = if s.get(a + 4..a + 8) == Some(b"hdlr") { a } else { a + 4 };
        let parts = mem_boxes(&s, first, b);
        let handler = parts.iter().find(|(t, _, _)| t == b"hdlr").and_then(|(_, ha, _)| s.get(ha + 8..ha + 12));
        if handler != Some(b"mdta") { continue; }
        let (Some((_, ka, kb)), Some((_, ia, ib))) = (parts.iter().find(|(t, _, _)| t == b"keys"), parts.iter().find(|(t, _, _)| t == b"ilst")) else { continue };
        let mut names = Vec::new();
        let n = be32(&s, ka + 4).unwrap_or(0);
        let mut pos = ka + 8;
        for _ in 0..n {
            let Some(size) = be32(&s, pos).map(|v| v as usize) else { break };
            if size < 8 || pos + size > *kb { break; }
            names.push(String::from_utf8_lossy(&s[pos + 8..pos + size]).into_owned());
            pos += size;
        }
        for (t, a, b) in mem_boxes(&s, *ia, *ib) {
            let idx = u32::from_be_bytes(t) as usize;
            let Some(name) = idx.checked_sub(1).and_then(|i| names.get(i)) else { continue };
            for (dt, da, db) in mem_boxes(&s, a, b) {
                if &dt == b"data" && db - da >= 8 && be32(&s, da) == Some(1) && !keys.iter().any(|(k, _)| k == name) {
                    keys.push((name.clone(), String::from_utf8_lossy(&s[da + 8..db]).into_owned()));
                }
            }
        }
    }
    Ok(keys)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#34;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// A photo's link and tags, from the XMP in its first 64 KiB
pub fn photo_meta(path: &Path) -> io::Result<(Option<Value>, Vec<String>)> {
    let mut head = Vec::with_capacity(65536);
    File::open(path)?.take(65536).read_to_end(&mut head)?;
    let s = String::from_utf8_lossy(&head);
    let Some(a) = s.find("<x:xmpmeta") else { return Ok((None, Vec::new())) };
    let Some(len) = s[a..].find("</x:xmpmeta>") else { return Ok((None, Vec::new())) };
    let s = &s[a..a + len];
    let text = if let Some(i) = s.find("<afp:link>") {
        let from = i + "<afp:link>".len();
        s[from..].find("</afp:link>").map(|e| s[from..from + e].to_string())
    } else if let Some(i) = s.find("afp:link=\"") {
        let from = i + "afp:link=\"".len();
        s[from..].find('"').map(|e| s[from..from + e].to_string())
    } else { None };
    let link = text.and_then(|t| serde_json::from_str(&unescape(&t)).ok());
    let mut tags = Vec::new();
    if let Some(i) = s.find("<dc:subject") {
        if let Some(len) = s[i..].find("</dc:subject>") {
            let mut rest = &s[i..i + len];
            while let Some(j) = rest.find("<rdf:li") {
                let Some(gt) = rest[j..].find('>') else { break };
                let from = j + gt + 1;
                let Some(to) = rest[from..].find("</rdf:li>") else { break };
                let t = unescape(rest[from..from + to].trim());
                if !t.is_empty() { tags.push(t); }
                rest = &rest[from + to..];
            }
        }
    }
    Ok((link, tags))
}

pub fn is_video(lower: &str) -> bool { lower.ends_with(".mp4") || lower.ends_with(".mov") || lower.ends_with(".m4v") }
pub fn is_photo(lower: &str) -> bool { [".heic", ".heif", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e)) }
pub fn is_sidecar(lower: &str) -> bool { lower.ends_with(".afp.json") || lower.ends_with(".marks.json") }

/// What a clip's own file holds that the catalog always says, each null when the file has none. A change of this list
/// (or of what `read` gives for a link or a photo) is a change of what the catalog keeps: project::VERSION is raised
/// with it (the test at the end of this file holds the two together)
pub const OWN_KEPT: [&str; 7] = ["marks", "view", "source", "tags", "rating", "pick", "location"];
/// What's in a clip's own file about the file itself, not kept
const OWN_LEFT: [&str; 4] = ["format", "version", "clip", "synced"];

/// A clip's own file as the catalog keeps it: OWN_KEPT, and whatever else it holds as it is. A program older than a
/// field carries it through, where it used to drop it and so wipe it from every view (2026-11-11 the locations,
/// 2026-11-17 Accept / Reject and the stars)
pub fn own_kept(own: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for k in OWN_KEPT { out.insert(k.to_string(), own.get(k).cloned().unwrap_or(Value::Null)); }
    for (k, v) in own.as_object().into_iter().flatten() {
        if !OWN_LEFT.contains(&k.as_str()) && !out.contains_key(k) { out.insert(k.clone(), v.clone()); }
    }
    Value::Object(out)
}

/// What's inside a file that counts, by its kind (None: nothing of ours in it)
pub fn read(path: &Path) -> io::Result<Option<Value>> {
    crate::store::nas::read();
    let lower = path.to_string_lossy().to_lowercase();
    if is_sidecar(&lower) {
        let text = std::fs::read(path)?;
        let Ok(own) = serde_json::from_slice::<Value>(&text) else { return Ok(None) };
        return Ok(Some(json!({ "own": own_kept(&own) })));
    }
    if is_video(&lower) {
        let keys = mp4_keys(path)?;
        return Ok(keys.iter().find(|(k, _)| k == LINK_KEY).and_then(|(_, v)| serde_json::from_str::<Value>(v).ok()).map(|l| json!({ "link": l })));
    }
    if is_photo(&lower) {
        let (link, tags) = photo_meta(path)?;
        if link.is_none() && tags.is_empty() { return Ok(None); }
        return Ok(Some(json!({ "link": link, "tags": tags })));
    }
    Ok(None)
}

/// The rules of what's kept and read, for a project: which roots are locations, where its footage is in its own folder
pub struct Ours {
    /// Folders of footage inside the project's folder (a project next to its footage), paths in it
    pub footage_in_project: Vec<String>,
    /// Only the project's own files looked at: the footage's folders inside it left as on record (originals move
    /// only when the library is mended after; docs §9)
    pub own_only: bool,
}

impl Ours {
    /// In the project's folder, the project's own: its own folders at the top, anywhere not among its footage
    pub fn own_area(&self, rel: &str) -> bool {
        let top = rel.split('/').next().unwrap_or("").to_lowercase();
        if top.starts_with('_') || OWN_FOLDERS.contains(&top.as_str()) { return true; }
        !self.footage_in_project.iter().any(|d| d.is_empty() || rel == d || rel.starts_with(&format!("{d}/")))
    }
}

pub fn hidden(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('#') || name.starts_with("~~") || name.starts_with('@') || name.eq_ignore_ascii_case("Thumbs.db")
}

impl Inspect for Ours {
    fn skip(&self, root: &str, rel: &str, name: &str, dir: bool) -> bool {
        if hidden(name) { return true; }
        // A project's own folders at the top of a location aren't footage (this project's, or one that was there)
        root != "p" && dir && !rel.contains('/') && OWN_FOLDERS.contains(&name.to_lowercase().as_str())
    }
    fn wants(&self, root: &str, rel: &str) -> bool {
        let lower = rel.to_lowercase();
        if is_sidecar(&lower) { return true; }
        root == "p" && (is_video(&lower) || is_photo(&lower)) && (lower.ends_with(LINK_EXT) || self.own_area(rel))
    }
    fn inspect(&self, path: &Path, _root: &str, _rel: &str) -> io::Result<Option<Value>> { read(path) }
    fn leave(&self, root: &str, rel: &str, _dir: bool) -> bool {
        // A project that is its footage's folder (no folder of footage of its own) is looked at whole
        self.own_only && (root != "p" || self.footage_in_project.iter().any(|d| !d.is_empty() && (rel == d || rel.starts_with(&format!("{d}/")))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the catalog keeps of each kind of file, with the project's version it is kept under. Changed one without
    /// the other, an older program (the NAS Helper, another computer's Library) goes on writing its own way into a
    /// project it should refuse. Raise project::VERSION (and the player's M.VERSION, projectfile.lua) and say what the
    /// new number means there, then change this line
    #[test]
    fn what_is_kept_goes_with_the_projects_version() {
        let dir = std::env::temp_dir().join(format!("afp-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let keys = |v: &Value| v.as_object().map(|o| { let mut k: Vec<&str> = o.keys().map(String::as_str).collect(); k.sort(); k.join(" ") }).unwrap_or_default();
        let own = dir.join("A.afp.json");
        std::fs::write(&own, br#"{"format":"actionfootageplayer-clip","version":1,"clip":{"file":"A.MP4","size":1},"synced":{"finder":[]}}"#).unwrap();
        let sidecar = read(&own).unwrap().unwrap();
        let photo = dir.join("A.jpg");
        std::fs::write(&photo, "\u{ff}\u{d8}<x:xmpmeta><afp:link>{&quot;v&quot;:1}</afp:link><dc:subject><rdf:Bag><rdf:li>sun</rdf:li></rdf:Bag></dc:subject></x:xmpmeta>").unwrap();
        let photo = read(&photo).unwrap().unwrap();
        let kept = format!("sidecar: {} / own: {} / photo: {}", keys(&sidecar), keys(&sidecar["own"]), keys(&photo));
        assert_eq!((crate::project::VERSION, kept.as_str()),
                   (4, "sidecar: own / own: location marks pick rating source tags view / photo: link tags"),
                   "what the catalog keeps changed, or the version did: they change together (see this test's comment)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A field this program doesn't know is carried through as it is; what's about the file itself is left
    #[test]
    fn an_unknown_field_is_kept() {
        let own = json!({ "format": "actionfootageplayer-clip", "version": 1, "clip": { "file": "A.MP4" }, "synced": {}, "pick": "reject", "later": { "x": 1 } });
        assert_eq!(own_kept(&own), json!({ "marks": null, "view": null, "source": null, "tags": null, "rating": null, "pick": "reject", "location": null, "later": { "x": 1 } }));
    }

    /// The player's copy of the version is the same number
    #[test]
    fn the_players_version_is_the_same() {
        let lua = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../mpv/scripts/workspace/projectfile.lua")).unwrap();
        assert!(lua.lines().any(|l| l.trim() == format!("M.VERSION = {}", crate::project::VERSION)), "projectfile.lua's M.VERSION isn't {}", crate::project::VERSION);
    }
}
