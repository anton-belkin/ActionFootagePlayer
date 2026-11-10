// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-meta: what the player keeps inside the library's own files, read and written from the command line: the
// library is its files (docs/library-management.md §8), and this is the way to see and fix them without the app.
//
// A moment, clip preview, export or render keeps its link to its clip as the MP4 key ch.actionfootageplayer.link (JSON:
// the clip's path in the project's tree where it was last seen, its size, creation time and fingerprint, the part's
// in and out, its tags…); a photo keeps the same as `afp:link` in its XMP, and its tags as `dc:subject`.

use serde_json::{ json, Value };
use std::path::Path;
use std::process::ExitCode;
use vs_gyroflow::{ mp4meta, xmp };

const USAGE: &str = "usage: afp-meta <file>...                    what of ours each file keeps, as JSON
       afp-meta <file> --link <json>         its link written (a video's ch.actionfootageplayer.link key, a photo's afp:link)
       afp-meta <file> --tags <json list>    its tags written (a video's in its link, a photo's dc:subject)
       afp-meta <file> --key <name> <value>  an MP4's metadata key written, its other keys kept

Videos: .mp4 .mov (moments, clip previews, exports, renders). Photos: .jpg .jpeg (written here) and .heic (written by
afp-photo, beside this, on a Mac).";

fn kind(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("mp4" | "mov" | "m4v") => "video",
        Some("jpg" | "jpeg") => "jpeg",
        Some("heic" | "heif") => "heic",
        _ => "",
    }
}

fn parse(text: Option<&str>) -> Value {
    text.map(|t| serde_json::from_str(t).unwrap_or_else(|_| json!(t))).unwrap_or(Value::Null)
}

fn read(path: &Path) -> Result<Value, String> {
    match kind(path) {
        "video" => {
            let keys = mp4meta::read_keys(path)?;
            let link = parse(keys.iter().find(|(k, _)| k == mp4meta::LINK_KEY).map(|(_, v)| v.as_str()));
            let tags = link.get("tags").cloned().filter(Value::is_array).unwrap_or(json!([]));
            let keys: serde_json::Map<String, Value> = keys.into_iter().map(|(k, v)| (k, json!(v))).collect();
            Ok(json!({ "file": path.display().to_string(), "type": "video", "link": link, "tags": tags, "keys": keys }))
        }
        "jpeg" | "heic" => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let (link, tags) = xmp::read(&bytes);
            Ok(json!({ "file": path.display().to_string(), "type": "photo", "link": parse(link.as_deref()), "tags": tags }))
        }
        _ => Err("not a video or photo of the library (.mp4, .mov, .jpg, .heic)".into()),
    }
}

/// A HEIC's XMP written by afp-photo (ImageIO: the image data copied as it is, the gain map and Live Photo kept)
fn heic_set(path: &Path, link: Option<&str>, tags: Option<&[String]>) -> Result<(), String> {
    if !cfg!(target_os = "macos") { return Err("a HEIC's metadata is written only on a Mac (afp-photo)".into()); }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let tool = me.with_file_name("afp-photo");
    if !tool.is_file() { return Err(format!("afp-photo isn't beside {}", me.display())); }
    let mut cmd = std::process::Command::new(&tool);
    cmd.arg("--set-meta").arg(path);
    if let Some(l) = link { cmd.arg("--link").arg(l); }
    if let Some(t) = tags { cmd.arg("--tags").arg(serde_json::to_string(t).unwrap()); }
    let o = cmd.output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
}

fn write(path: &Path, link: Option<&str>, tags: Option<&[String]>) -> Result<(), String> {
    match kind(path) {
        "video" => {
            let mut keys = mp4meta::read_keys(path)?;
            let mut l: Value = match link {
                Some(t) => serde_json::from_str(t).map_err(|e| format!("the link isn't JSON: {e}"))?,
                None => parse(keys.iter().find(|(k, _)| k == mp4meta::LINK_KEY).map(|(_, v)| v.as_str())),
            };
            if let Some(t) = tags {
                if !l.is_object() { return Err("it has no link to keep tags in".into()); }
                if t.is_empty() { l.as_object_mut().unwrap().remove("tags"); } else { l["tags"] = json!(t); }
            }
            keys.retain(|(k, _)| k != mp4meta::LINK_KEY);
            keys.push((mp4meta::LINK_KEY.to_string(), l.to_string()));
            let refs: Vec<(&str, &str)> = keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            mp4meta::write_keys(path, &refs)
        }
        "jpeg" => {
            if let Some(t) = link { serde_json::from_str::<Value>(t).map_err(|e| format!("the link isn't JSON: {e}"))?; }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let out = xmp::jpeg_set(&bytes, link, tags)?;
            let tmp = path.with_extension("afp-meta.tmp");
            std::fs::write(&tmp, out).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| { let _ = std::fs::remove_file(&tmp); e.to_string() })
        }
        "heic" => heic_set(path, link, tags),
        _ => Err("not a video or photo of the library (.mp4, .mov, .jpg, .heic)".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") { println!("{USAGE}"); return if args.is_empty() { ExitCode::from(1) } else { ExitCode::SUCCESS }; }
    let fail = |file: &str, e: String| { eprintln!("afp-meta: {file}: {e}"); ExitCode::from(1) };
    if args.len() > 1 && args[1].starts_with("--") {
        let file = &args[0];
        let path = Path::new(file);
        let (mut link, mut tags, mut key) = (None, None, None);
        let mut i = 1;
        while i < args.len() {
            match (args[i].as_str(), args.get(i + 1)) {
                ("--link", Some(v)) => { link = Some(v.clone()); i += 2; }
                ("--tags", Some(v)) => match serde_json::from_str::<Vec<String>>(v) { Ok(t) => { tags = Some(t); i += 2; } Err(e) => return fail(file, format!("--tags takes a JSON list of words: {e}")) },
                ("--key", Some(k)) => match args.get(i + 2) { Some(v) => { key = Some((k.clone(), v.clone())); i += 3; } None => { eprintln!("{USAGE}"); return ExitCode::from(1); } },
                (a, _) => { eprintln!("afp-meta: unknown argument {a}\n\n{USAGE}"); return ExitCode::from(1); }
            }
        }
        if let Some((k, v)) = key {
            if kind(path) != "video" { return fail(file, "--key is for videos".into()); }
            let mut keys = match mp4meta::read_keys(path) { Ok(k) => k, Err(e) => return fail(file, e) };
            keys.retain(|(x, _)| *x != k);
            keys.push((k, v));
            let refs: Vec<(&str, &str)> = keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            if let Err(e) = mp4meta::write_keys(path, &refs) { return fail(file, e); }
        }
        if link.is_some() || tags.is_some() {
            if let Err(e) = write(path, link.as_deref(), tags.as_deref()) { return fail(file, e); }
        }
        return match read(path) { Ok(v) => { println!("{}", serde_json::to_string_pretty(&v).unwrap()); ExitCode::SUCCESS } Err(e) => fail(file, e) };
    }
    let mut out = Vec::new();
    let mut failed = false;
    for file in &args {
        match read(Path::new(file)) {
            Ok(v) => out.push(v),
            Err(e) => { eprintln!("afp-meta: {file}: {e}"); failed = true; }
        }
    }
    let v = if out.len() == 1 && args.len() == 1 { out.remove(0) } else { json!(out) };
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
