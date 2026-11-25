// The render cache (docs/design/albums-photos-share-concept.md, "Renders": found by recipe, not by path). A render is the
// real pixels of one item (a moment, a photo link, a whole clip) in one profile. What decides its pixels is its recipe;
// the recipe's hash names its file: <project>/_Renders/<hash>.<profile>.<ext>. So a render is found whatever the item
// is called and wherever it lies: moved, renamed or copied items keep theirs, two copies of a moment share one, and a
// changed recipe is another file. The player makes them (its batch `share`, workspace/renders.lua): the Library works the
// recipe and the file out and hands both over, so only this file hashes.
//
// Renders made before (0.11: _Renders/<the moment's path>.<profile>.mov, a v1 recipe in the link) are still found, by
// that path, and count as current while their v1 recipe is the moment's.

use crate::index::{ self, same, trimmed };
use serde_json::{ json, Value };
use std::cell::RefCell;
use std::collections::{ HashMap, HashSet };
use std::path::{ Path, PathBuf };

pub const DIR: &str = "_Renders";
/// Raised when the renderer's pixels change for the same recipe: every render is then another one. 4: gyroflow-export
/// renders on the GPU path where it can, the player's own shaders (docs/design/export-gpu-path.md), not Gyroflow's engine
pub const VERSION: u64 = 4;

pub struct Profile {
    pub name: &'static str,
    pub photo: bool,
    /// What the player renders it with: hdr (HLG + Dolby Vision 8.4), nbcu (SDR), photo (a still with a gain map)
    pub look: &'static str,
    /// The files it can be (a photo is a HEIC where the system writes one, else a JPEG)
    pub exts: &'static [&'static str],
}

pub const PROFILES: [Profile; 3] = [
    // (.mp4: a moment with nothing to change, its part of the original cut as it is: share.rs `as_recorded`)
    Profile { name: "HDR", photo: false, look: "hdr", exts: &["mov", "mp4"] },
    Profile { name: "SDR", photo: false, look: "nbcu", exts: &["mov", "mp4"] },
    Profile { name: "Photo", photo: true, look: "photo", exts: &["heic", "jpg"] },
];

pub fn profile(name: &str) -> Option<&'static Profile> { PROFILES.iter().find(|p| p.name == name) }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Moment, Photo, Clip }

impl Kind {
    pub fn name(self) -> &'static str { match self { Kind::Moment => "moment", Kind::Photo => "photo", Kind::Clip => "clip" } }
    /// The profiles an item of this kind is rendered in
    pub fn profiles(self) -> Vec<&'static Profile> { PROFILES.iter().filter(|p| p.photo == (self == Kind::Photo)).collect() }
}

/// What's rendered: a moment or a photo link (`file`: the link's file, `rel`: its path in the project's folder,
/// `link`: what it says), or a whole clip (`rel`: its path in the project's tree)
pub struct Item {
    pub kind: Kind,
    pub file: Option<PathBuf>,
    pub rel: String,
    pub link: Value,
    pub name: String,
}

impl Item {
    /// The clip it's of, by its path in the project's tree
    pub fn tree(&self) -> Option<&str> { if self.kind == Kind::Clip { Some(&self.rel) } else { self.link["clip"].as_str() } }
}

// What a clip's own files say that decides a render of it
#[derive(Clone, Default)]
struct ClipFacts { doc: Value, gyroflow: Value, size: Value, file: Option<PathBuf> }

/// A look at the cache: the project, the player's defaults, and what's in _Renders (listed once for a scan of many
/// items; `fresh` asks the disk for each one instead, when something is about to be made or copied)
pub struct Ctx {
    pub root: PathBuf,
    /// The player's render defaults (its render-defaults.json): { defaults (the bundle), edges, edges_blur,
    /// temporal_denoise, preset }
    pub defaults: Value,
    /// The fingerprint of the Gyroflow preset the player stabilizes with where a clip has no project
    preset: Value,
    locs: Vec<crate::locations::Location>,
    names: Option<HashSet<String>>,
    clips: RefCell<HashMap<String, ClipFacts>>,
    ranges: RefCell<HashMap<String, Option<afp_catalog::range::Range>>>,
}

impl Ctx {
    pub fn new(root: &Path, fresh: bool) -> Ctx { Ctx::with(root, fresh, player_defaults()) }

    /// With the player's defaults given
    pub fn with(root: &Path, fresh: bool, defaults: Value) -> Ctx {
        let dir = root.join(DIR);
        let names = (!fresh).then(|| crate::seen::names_in(&dir).unwrap_or_else(|| {
            std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()
        }).into_iter().collect());
        let preset = defaults["preset"].as_str().map(Path::new).filter(|p| p.is_file()).and_then(crate::checks::fingerprint_kept).map(Value::from).unwrap_or(Value::Null);
        Ctx { root: root.to_path_buf(), defaults, preset, locs: crate::locations::locations(root, &crate::locations::mounts()),
              names, clips: RefCell::new(HashMap::new()), ranges: RefCell::new(HashMap::new()) }
    }

    fn has(&self, name: &str) -> bool {
        match &self.names { Some(n) => n.contains(name), None => self.root.join(DIR).join(name).is_file() }
    }

    fn clip(&self, tree: &str) -> ClipFacts {
        if let Some(c) = self.clips.borrow().get(tree) { return c.clone(); }
        // A view's look (not fresh): what the project's catalog last saw, where it can say
        let facts = self.names.as_ref().and_then(|_| self.clip_seen(tree)).unwrap_or_else(|| self.clip_disk(tree));
        self.clips.borrow_mut().insert(tree.to_string(), facts.clone());
        facts
    }

    /// The clip's facts from its files on the disk (tags::clip_own_in's choice of copy)
    fn clip_disk(&self, tree: &str) -> ClipFacts {
        let mut facts = ClipFacts::default();
        if let Ok((own, clip)) = crate::tags::clip_own_in(&self.locs, tree) {
            let doc = index::read_json(&own).unwrap_or(Value::Null);
            facts.doc = doc;
            // The clip's Gyroflow project, beside it (<stem>.gyroflow, or <name.ext>.gyroflow)
            let name = clip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            facts.gyroflow = [clip.with_extension("gyroflow"), clip.with_file_name(format!("{name}.gyroflow"))].iter()
                .find(|g| { afp_catalog::store::nas::stat(); g.is_file() }).and_then(|g| crate::checks::fingerprint_kept(g)).map(Value::from).unwrap_or(Value::Null);
            afp_catalog::store::nas::stat();
            facts.size = std::fs::metadata(&clip).map(|m| json!(m.len())).unwrap_or(Value::Null);
            facts.file = Some(clip);
        }
        facts
    }

    /// The same facts from what the project's catalog last saw of the clip's files (its own file as the catalog keeps
    /// it: everything but format, version, clip and synced, none of which a recipe reads), the same copy chosen. Each
    /// moment's clip asked of the share took 30 to 100 ms (2026-11-24 journal: 41 moments, 1.2 to 4.4 s); this asks
    /// nothing but a Gyroflow project's fingerprint the first time in a run. None: the catalog can't say (a location it
    /// hasn't looked at, not read yet): the disk then
    fn clip_seen(&self, tree: &str) -> Option<ClipFacts> {
        let seen = |p: &Path| -> Option<Option<afp_catalog::state::File>> { crate::seen::file(p).map(|f| f.filter(|f| !f.dir)) };
        let mut copies: Vec<(PathBuf, bool, afp_catalog::state::File)> = Vec::new();
        for l in &self.locs {
            let Some(f) = crate::locations::in_location(tree, l) else { continue };
            if let Some(x) = seen(&f)? { copies.push((f, l.copies || l.local, x)); }
        }
        copies.sort_by_key(|(_, late, _)| *late);
        let mut own: Option<(usize, afp_catalog::state::File)> = None;
        for (i, (f, _, _)) in copies.iter().enumerate() {
            if let Some(o) = seen(&f.with_extension("afp.json"))? { own = Some((i, o)); break; }
        }
        let i = match own.as_ref().map(|(i, _)| *i).or_else(|| copies.iter().position(|(_, late, _)| !late)).or((!copies.is_empty()).then_some(0)) {
            Some(i) => i,
            // In none of the project's locations
            None => return Some(ClipFacts::default()),
        };
        let (clip, _, file) = &copies[i];
        let mut facts = ClipFacts::default();
        if let Some((_, o)) = &own {
            // (its record without what's inside: not read whole, or written wrong — the disk says, as before)
            facts.doc = o.meta.as_ref().and_then(|m| m.get("own")).cloned()?;
        }
        let name = clip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for g in [clip.with_extension("gyroflow"), clip.with_file_name(format!("{name}.gyroflow"))] {
            if let Some(x) = seen(&g)? {
                facts.gyroflow = crate::checks::fingerprint_seen(&g, x.size, x.mtime).map(Value::from).unwrap_or(Value::Null);
                break;
            }
        }
        facts.size = json!(file.size);
        facts.file = Some(clip.clone());
        Some(facts)
    }

    /// A Gyroflow project's fingerprint, None where there's none: for a view's look from the catalog where it has the
    /// place (fresh: the disk, each time)
    fn gyroflow_print(&self, g: &Path) -> Option<String> {
        if self.names.is_some() {
            match crate::seen::file(g) {
                Some(Some(x)) if !x.dir => return crate::checks::fingerprint_seen(g, x.size, x.mtime),
                Some(_) => return None,
                None => {}
            }
        }
        afp_catalog::store::nas::stat();
        g.is_file().then(|| crate::checks::fingerprint(g)).flatten()
    }

    /// The clip's file an item is rendered from, when it's here
    pub fn original(&self, item: &Item) -> Option<PathBuf> { item.tree().and_then(|t| self.clip(t).file) }

}

/// What a render takes from this computer's player: what it wrote last (gyroflow/bundle.lua `publish`, every time it
/// starts, a batch too: <its data>/gyroflow/render-defaults.json, AFL_PLAYER_DEFAULTS to point elsewhere), else worked
/// out from its gyroflow.conf (AFL_PLAYER_OPTS) as the player would. A batch tells what it would have used, and a
/// share compares (main.rs `share_collect`)
fn player_defaults() -> Value {
    // (tests never read this computer's player, but those run with one: AFL_PLAYER)
    if cfg!(test) && std::env::var_os("AFL_PLAYER").is_none() { return json!({}); }
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = || -> Option<PathBuf> {
        let b = if cfg!(target_os = "macos") { env("HOME")?.join("Library/Application Support") }
                else if cfg!(windows) { env("LOCALAPPDATA")? }
                else { env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))? };
        Some(b.join("ActionFootagePlayer"))
    };
    let written = env("AFL_PLAYER_DEFAULTS").or_else(|| base().map(|b| b.join("gyroflow").join("render-defaults.json")));
    if let Some(v) = written.and_then(|f| index::read_json(&f)).filter(|v| v["defaults"].is_object()) { return v; }
    let conf = env("AFL_PLAYER_OPTS").or_else(|| base().map(|b| b.join("mpv").join("script-opts").join("gyroflow.conf")));
    let data_preset = base().map(|b| b.join("gyroflow").join("default.gyroflow")).filter(|p| p.is_file());
    let mut v = defaults_of(&conf.and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default());
    if let Some(p) = data_preset { v["preset"] = json!(p.display().to_string()); }
    v
}

/// The player's defaults from its gyroflow.conf, as gyroflow/bundle.lua's `defaults_bundle` makes them (its options'
/// own defaults where the file doesn't say). The preset's horizon isn't known here: "no". The file is read as mpv's
/// player/lua/options.lua reads it: a line starting with # is a comment, key=value split at the first =, nothing
/// trimmed (a key with a space is another key, unknown), a boolean only yes or no (tests/gyroflow_conf_cases.json,
/// checked on both sides)
fn defaults_of(conf: &str) -> Value {
    let mut o = serde_json::Map::new();
    for line in conf.lines() {
        if line.starts_with('#') { continue; }
        if let Some((k, v)) = line.split_once('=') { o.insert(k.to_string(), json!(v)); }
    }
    let get = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let yes = |k: &str, default: bool| match get(k).as_str() { "yes" => true, "no" => false, _ => default };
    let one = |k: &str, of: &[&str], default: &str| { let v = get(k); if of.contains(&v.as_str()) { v } else { default.to_string() } };
    // A setting of registry.json as gyroflow.conf says it (its option, the values it can take there, else its default)
    let reg = crate::lists::get();
    let conf = |key: &str| -> Value {
        let s = reg.setting(key).unwrap_or_else(|| panic!("registry.json: no setting {key}"));
        let (name, default) = (s.conf.as_deref().unwrap_or(key), s.conf_default.clone().unwrap_or(Value::Null));
        match s.kind.as_str() {
            "bool" => json!(yes(name, default.as_bool().unwrap_or(false))),
            _ => json!(one(name, &s.conf_values(), default.as_str().unwrap_or(""))),
        }
    };
    // The plugin's split_aspect: an aspect that isn't W:H is original; a portrait W:H (9:16, from before the orientation
    // was a setting) turned, portrait; else the orientation set (as-is by default), landscape for a W:H if that's no choice
    let split = {
        let a = get("aspect");
        let ratio = a.split_once(':').filter(|(w, h)| !w.is_empty() && !h.is_empty() && w.chars().chain(h.chars()).all(|c| c.is_ascii_digit()))
            .map(|(w, h)| (w.parse::<u64>().unwrap_or(0), h.parse::<u64>().unwrap_or(0), format!("{h}:{w}")));
        let set = if o.contains_key("orientation") { get("orientation") } else { "as-is".to_string() };
        match ratio {
            Some((w, h, turned)) if w < h => json!({ "aspect": turned, "orientation": "portrait" }),
            _ => {
                let aspect = if ratio.is_some() { a } else { "original".to_string() };
                let orientation = if ["as-is", "landscape", "portrait"].contains(&set.as_str()) { set } else if ratio.is_some() { "landscape".into() } else { "as-is".into() };
                json!({ "aspect": aspect, "orientation": orientation })
            }
        }
    };
    let blur: f64 = get("edges_blur").parse().unwrap_or(2.0);
    json!({
        "defaults": {
            "view": conf("view"),
            "level": conf("level"),
            // Its default, the preset's horizon, isn't known here: no
            "horizon_lock": one("horizon_lock", &reg.setting("horizon_lock").map(|s| s.conf_values()).unwrap_or_default(), "no"),
            "chromatic": conf("chromatic"),
            "aspect": split["aspect"], "orientation": split["orientation"],
            "projection": conf("projection"),
            "lens": conf("lens"),
            "denoise": conf("denoise"),
            "denoise_strong": conf("denoise_strong"),
            "preset_horizon": "no",
            // The lens under water for a source that doesn't say (#317); project is auto here, as the plugin reads it
            "underwater": conf("underwater"),
        },
        "edges": conf("edges"),
        "edges_blur": blur,
        "temporal_denoise": conf("temporal_denoise"),
    })
}

// ---------- What a render is made with: the player's transform.render_settings, the same way ----------
// (mpv/scripts/workspace/transform.lua; both are checked against tests/render_settings.json)

// The keys and their values: registry.json's settings (transform.lua reads the same)

/// The bundle's keys, in order (transform.KEYS)
fn bundle_keys() -> Vec<&'static str> { crate::lists::get().keys("bundle") }
/// The underwater colour's keys (transform.UW_KEYS): the source's input, in the settings only when it's on
fn uw_keys() -> Vec<&'static str> { crate::lists::get().keys("uw") }

/// A value the bundle's or the underwater colour's key takes (transform.valid)
fn valid(k: &str, v: &Value) -> bool {
    crate::lists::get().setting(k).is_some_and(|s| (s.group == "bundle" || s.group == "uw") && s.valid(v))
}

/// The bundle's keys of a view, None for none (transform.pick)
fn pick(view: &Value) -> Option<Value> {
    let o = view.as_object()?;
    let out: serde_json::Map<String, Value> = bundle_keys().into_iter().filter_map(|k| o.get(k).filter(|v| !v.is_null()).map(|v| (k.to_string(), v.clone()))).collect();
    (!out.is_empty()).then_some(Value::Object(out))
}

/// An aspect from before the orientation was a key: the ratio as it showed (transform's split_frame)
fn split_frame(b: &Value) -> Value {
    let Some(o) = b.as_object() else { return b.clone() };
    let Some(aspect) = o.get("aspect").filter(|a| !a.is_null()) else { return b.clone() };
    if o.get("orientation").is_some_and(|x| valid("orientation", x)) { return b.clone(); }
    let mut out = o.clone();
    let text = match aspect { Value::String(s) => s.clone(), x => x.to_string() };
    let ratio = text.split_once(':').filter(|(w, h)| !w.is_empty() && !h.is_empty() && w.chars().chain(h.chars()).all(|c| c.is_ascii_digit()));
    match ratio {
        Some((w, h)) if w.parse::<u64>().unwrap_or(0) < h.parse::<u64>().unwrap_or(0) => {
            out.insert("aspect".into(), json!(format!("{h}:{w}")));
            out.insert("orientation".into(), json!("portrait"));
        }
        Some(_) => { out.insert("orientation".into(), json!("landscape")); }
        None => { out.insert("orientation".into(), json!("as-is")); }
    }
    Value::Object(out)
}

/// The bundle with every key: a missing one NEUTRAL's, else the session's (transform.complete)
fn complete(bundle: &Value, session: &Value) -> serde_json::Map<String, Value> {
    let bundle = split_frame(bundle);
    let neutral = |k: &str| crate::lists::get().setting(k).and_then(|s| s.neutral.clone()).unwrap_or(Value::Null);
    let mut out = serde_json::Map::new();
    for k in bundle_keys() {
        let mut v = bundle.get(k).cloned().unwrap_or(Value::Null);
        if k == "horizon_lock" && v == json!("project") { v = session["preset_horizon"].clone(); }
        if !valid(k, &v) {
            v = if k == "lens" && bundle.as_object().is_some_and(|o| !o.is_empty()) {
                json!(if out.get("view") == Some(&json!("off")) { "as-recorded" } else { "corrected" })
            } else if !neutral(k).is_null() { neutral(k) } else { session[k].clone() };
            if !valid(k, &v) { continue; }
        }
        out.insert(k.to_string(), v);
    }
    out
}

/// An old .afp.json's input out of `view` into `source` (transform.migrate_source): the source as it reads then
fn migrated_source(doc: &Value) -> serde_json::Map<String, Value> {
    let mut s = doc["source"].as_object().cloned().unwrap_or_default();
    let v = &doc["view"];
    if v.is_object() {
        if !v["curve"].is_null() && s.get("curve").is_none_or(Value::is_null) { s.insert("curve".into(), v["curve"].clone()); }
        if !v["underwater"].is_null() && v["underwater"] != json!("project") && s.get("underwater").is_none_or(Value::is_null) {
            s.insert("underwater".into(), v["underwater"].clone());
        }
        let r = v["rotation"].as_f64().or_else(|| v["rotation"].as_str().and_then(|x| x.parse().ok()));
        let plain = v["aspect"].is_null() || v["aspect"] == json!("original");
        if !v["rotation"].is_null() && (r == Some(0.0) || plain) && s.get("rotation").is_none_or(Value::is_null) {
            if let Some(r) = r.filter(|r| *r != 0.0) { s.insert("rotation".into(), json!(r)); }
        }
    }
    s
}

/// What a render of a source is made with: the bundle (a moment's own `own`, else the source's saved view, else the
/// player's `defaults`) made whole with the defaults, the source's input over it. `doc`: the source's .afp.json
pub fn settings(doc: &Value, own: &Value, defaults: &Value) -> Value {
    let bundle = pick(own).or_else(|| pick(&doc["view"])).unwrap_or_else(|| defaults.clone());
    let mut out = complete(&bundle, defaults);
    if doc.is_object() { for (k, v) in migrated_source(doc) { out.insert(k, v); } }
    // The lens under water where the source doesn't say: the defaults' (gyroflow.conf's underwater, #317), as the
    // player plays it; auto (gyroflow-export's own) left out, so a recipe without it keeps its hash
    if out.get("underwater").is_none_or(Value::is_null) && (defaults["underwater"] == json!("yes") || defaults["underwater"] == json!("no")) {
        out.insert("underwater".into(), defaults["underwater"].clone());
    }
    // The underwater colour (the source's): where the source doesn't say, the defaults' (the player's uwcolor.conf, in
    // its render-defaults.json); only when it's on, so a recipe without it keeps the hash it had
    for k in uw_keys() {
        if out.get(k).is_none_or(Value::is_null) && valid(k, &defaults[k]) { out.insert(k.into(), defaults[k].clone()); }
    }
    if out.get("uw") != Some(&json!(true)) { for k in uw_keys() { out.remove(k); } }
    Value::Object(out)
}

/// Everything that decides the pixels of `item` rendered in `profile`. Its fields (docs/archive/share-fixes.md):
/// v, kind, clip { hash | path, size }, in / out / alone (a moment), t / crop / camera (a photo), settings (what it's
/// rendered with: `settings` from the clip's own file, a moment's own bundle and the player's defaults: the player
/// renders with exactly these, sent in its request), opts (the player's edge fill and temporal denoise), gyroflow (the
/// fingerprint of the Gyroflow project used: a moment's own, else the clip's), preset (the player's preset's, where
/// there is no project), profile { name, look }
pub fn recipe(ctx: &Ctx, item: &Item, profile: &Profile) -> Value { recipe_with(ctx, item, profile, &Value::Null) }

/// The export's settings an item is rendered with (Share's dialog: docs/design/export-settings.md), the values that aren't
/// the defaults: { size, encode, quality } for a video, { size, quality } for a photo. The defaults are what was
/// rendered before there were settings, so with them the recipe, its hash and its render stay what they were. Each
/// setting's values, in order, the first its default: registry.json's export settings
pub fn export_choices() -> Vec<(&'static str, Vec<&'static str>)> { crate::lists::get().export_choices() }

/// `settings` (Share's: { video: {...}, photo: {...} }, or one kind's) as a recipe has them for a photo or a video:
/// the valid values that aren't the defaults (each choice's first), nothing else
pub fn export_settings(settings: &Value, photo: bool) -> Value {
    let kind = if photo { "photo" } else { "video" };
    let s = if settings[kind].is_object() { &settings[kind] } else { settings };
    let mut out = serde_json::Map::new();
    let kind_keys = &crate::lists::get().export.kinds;
    let keys = if photo { &kind_keys.photo } else { &kind_keys.video };
    for (key, values) in export_choices() {
        if !keys.iter().any(|k| k == key) { continue; }
        if let Some(v) = s[key].as_str().filter(|v| values.contains(v) && *v != values[0]) { out.insert(key.into(), json!(v)); }
    }
    Value::Object(out)
}

/// The recipe with the export's settings (`export`: export_settings's; empty: the defaults, the recipe as it was)
pub fn recipe_with(ctx: &Ctx, item: &Item, profile: &Profile, export: &Value) -> Value {
    let l = &item.link;
    let tree = item.tree().unwrap_or("");
    let facts = ctx.clip(tree);
    // The original, by what it is: its fingerprint where the link has it, else its path in the tree; and its size
    let hash = l["hash"].as_str().filter(|h| !h.is_empty());
    let clip = json!({ "hash": hash, "path": if hash.is_none() { json!(tree) } else { Value::Null },
                       "size": if l["size"].is_number() { l["size"].clone() } else { facts.size.clone() } });
    let own = if item.kind == Kind::Moment { l["stab"].clone() } else { Value::Null };
    let d = &ctx.defaults;
    let mut r = json!({ "v": VERSION, "kind": item.kind.name(), "clip": clip,
                        "settings": settings(&facts.doc, &own, &d["defaults"]),
                        "opts": { "edges": d["edges"], "edges_blur": d["edges_blur"], "temporal_denoise": d["temporal_denoise"] },
                        "gyroflow": facts.gyroflow, "profile": { "name": profile.name, "look": profile.look } });
    match item.kind {
        Kind::Moment => {
            // Its own Gyroflow project, beside it (else the clip's, which gyroflow-export finds itself)
            if let Some(own) = own_project(item).and_then(|o| ctx.gyroflow_print(&o)) { r["gyroflow"] = json!(own); }
            for k in ["in", "out"] { r[k] = l[k].clone(); }
            // Stabilized on its own: a moment kept as made (snap) that says so (the player's render_moments)
            if l["snap"] == json!(true) && l["alone"] == json!(true) { r["alone"] = json!(true); }
        }
        Kind::Photo => { for k in ["t", "crop", "camera"] { r[k] = l[k].clone(); } }
        Kind::Clip => {}
    }
    if r["gyroflow"].is_null() { r["preset"] = ctx.preset.clone(); }
    // A render made with other settings is another render
    if export.as_object().is_some_and(|o| !o.is_empty()) { r["export"] = export.clone(); }
    trimmed(&r).unwrap_or(Value::Null)
}

/// index::render_recipe (a moment's render made the old way) from what the look already has: the clip's facts, the
/// moment's own Gyroflow project (from the catalog, for a view)
fn old_recipe(ctx: &Ctx, item: &Item, file: &Path, profile: &str) -> Value {
    if ctx.names.is_none() { return index::render_recipe(&ctx.root, file, &item.link, profile); }
    let input = item.tree().map(|t| ctx.clip(t).doc["source"].clone()).unwrap_or(Value::Null);
    let gyroflow = own_project(item).and_then(|o| ctx.gyroflow_print(&o)).map(Value::from).unwrap_or(Value::Null);
    index::render_recipe_of(&item.link, input, gyroflow, profile)
}

/// A moment's own Gyroflow project: <name>.gyroflow beside its link
fn own_project(item: &Item) -> Option<PathBuf> {
    item.file.as_ref().map(|f| {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        f.with_file_name(format!("{}.gyroflow", name.trim_end_matches(index::LINK_EXT)))
    })
}

/// What the player renders a recipe with (its request's `view`): the settings and the opts in one
pub fn render_view(recipe: &Value) -> Value {
    let mut v = recipe["settings"].as_object().cloned().unwrap_or_default();
    for (k, x) in recipe["opts"].as_object().into_iter().flatten() { v.insert(k.clone(), x.clone()); }
    Value::Object(v)
}

/// Whether what the player would have taken by itself (its batch's `used`) is what the recipe says: the keys that
/// differ, empty when none
pub fn differs(recipe: &Value, player: &Value) -> Vec<String> {
    let want = render_view(recipe);
    let mut keys: Vec<String> = want.as_object().into_iter().flatten().map(|(k, _)| k.clone())
        .chain(player.as_object().into_iter().flatten().map(|(k, _)| k.clone())).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter().filter(|k| {
        let (a, b) = (trimmed(&want[k.as_str()]).unwrap_or(Value::Null), trimmed(&player[k.as_str()]).unwrap_or(Value::Null));
        let (mut x, mut y) = (String::new(), String::new());
        canonical(&a, &mut x);
        canonical(&b, &mut y);
        x != y
    }).collect()
}

// A value written one way only: keys in order, whole numbers without a fraction, others to six places
fn canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str(&Value::String((*k).clone()).to_string());
                out.push(':');
                canonical(&o[*k], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() { if i > 0 { out.push(','); } canonical(x, out); }
            out.push(']');
        }
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 && f.abs() < 9e15 { out.push_str(&format!("{}", f as i64)); } else {
                let s = format!("{f:.6}");
                out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
            }
        }
        x => out.push_str(&x.to_string()),
    }
}

/// What the player is asked for, as its JSON has it (nothing empty)
pub fn trimmed_request(v: &Value) -> Value { trimmed(v).unwrap_or(Value::Null) }

/// A recipe's hash: what its render's file is called
pub fn hash(recipe: &Value) -> String {
    let mut text = String::new();
    canonical(&trimmed(recipe).unwrap_or(Value::Null), &mut text);
    crate::checks::sha256_hex(text.as_bytes())[..24].to_string()
}

/// Where the render of a recipe's hash in a profile goes
pub fn cache_path(root: &Path, hash: &str, profile: &Profile, ext: &str) -> PathBuf {
    root.join(DIR).join(format!("{hash}.{}.{ext}", profile.name))
}

/// An item's render in a profile, as the cache has it now
pub struct Found {
    pub recipe: Value,
    pub hash: String,
    /// The current render: in the cache by its hash, or one made the old way at the moment's path
    pub file: Option<PathBuf>,
    /// A render of before at the moment's path whose recipe isn't the moment's any more
    pub stale: Option<PathBuf>,
}

/// The profile an item is rendered in when `profile` is asked for: an SDR clip's HDR is its SDR render (gyroflow-export
/// keeps an SDR clip's colors as they are in both: one file, not two of the same pixels). A log clip, an HLG or PQ one,
/// one with the underwater colour on (made HLG by it), or one that can't be read keeps its HDR. Reads the clip's MP4
/// once per Ctx (afp_catalog::range)
pub fn resolve(ctx: &Ctx, item: &Item, profile: &'static Profile) -> &'static Profile {
    if profile.name != "HDR" || ctx.is_sdr(item) != Some(true) || ctx.underwater_colour(item) { return profile; }
    self::profile("SDR").unwrap()
}

impl Ctx {
    /// Whether the item is rendered with the underwater colour (its settings' uw: the source's input)
    pub fn underwater_colour(&self, item: &Item) -> bool {
        let Some(tree) = item.tree() else { return false };
        let own = if item.kind == Kind::Moment { item.link["stab"].clone() } else { Value::Null };
        settings(&self.clip(tree).doc, &own, &self.defaults["defaults"])["uw"] == json!(true)
    }

    /// Whether the clip an item is of is SDR (not log, not HLG or PQ): None where it can't be told
    pub fn is_sdr(&self, item: &Item) -> Option<bool> {
        let tree = item.tree()?;
        if let Some(r) = self.ranges.borrow().get(tree) { return r.map(|r| r == afp_catalog::range::Range::Sdr); }
        let facts = self.clip(tree);
        let curve = facts.doc["source"]["curve"].as_str().or(facts.doc["view"]["curve"].as_str());
        let r = facts.file.as_deref().and_then(|f| afp_catalog::range::of(f, curve));
        self.ranges.borrow_mut().insert(tree.to_string(), r);
        r.map(|r| r == afp_catalog::range::Range::Sdr)
    }
}

pub fn find(ctx: &Ctx, item: &Item, profile: &Profile) -> Found { find_with(ctx, item, profile, &Value::Null) }

/// An item's render with the export's settings (`export`: export_settings's)
pub fn find_with(ctx: &Ctx, item: &Item, profile: &Profile, export: &Value) -> Found {
    let found = find_as(ctx, item, profile, export);
    // An SDR clip's HDR is its SDR render (resolve): looked for only where the HDR's own isn't there and an SDR one is
    if profile.name == "HDR" && found.file.is_none() && !profile.photo {
        let sdr = self::profile("SDR").unwrap();
        let other = find_as(ctx, item, sdr, export);
        if other.file.is_some() && ctx.is_sdr(item) == Some(true) && !ctx.underwater_colour(item) { return other; }
    }
    found
}

fn find_as(ctx: &Ctx, item: &Item, profile: &Profile, export: &Value) -> Found {
    let recipe = recipe_with(ctx, item, profile, export);
    let hash = hash(&recipe);
    let mut found = Found { recipe, hash, file: None, stale: None };
    for ext in profile.exts {
        let name = format!("{}.{}.{ext}", found.hash, profile.name);
        if ctx.has(&name) { found.file = Some(ctx.root.join(DIR).join(name)); return found; }
    }
    // Made the old way: _Renders/<the moment's path>.<profile>.mov, current while its v1 recipe is the moment's
    let default = !export.as_object().is_some_and(|o| !o.is_empty());
    if let (Kind::Moment, false, Some(file), true) = (item.kind, profile.photo, &item.file, default) {
        let old = index::render_path(&ctx.root, &item.rel, profile.name);
        if crate::seen::is_file(&old) || (ctx.names.is_none() && old.is_file()) {
            let ok = index::link(&old).filter(|l| l["kind"] == "render").and_then(|l| trimmed(&l["recipe"]))
                .is_some_and(|r| same(&r, &old_recipe(ctx, item, file, profile.name)));
            if ok { found.file = Some(old); } else { found.stale = Some(old); }
        }
    }
    found
}

/// Each profile's render of an item: { HDR: "current" | "stale" }, absent: none (what the page's `it.renders` is)
pub fn states(ctx: &Ctx, item: &Item) -> Value {
    let mut out = serde_json::Map::new();
    for p in item.kind.profiles() {
        let f = find(ctx, item, p);
        if f.file.is_some() { out.insert(p.name.to_string(), json!("current")); } else if f.stale.is_some() { out.insert(p.name.to_string(), json!("stale")); }
    }
    Value::Object(out)
}

/// The link a render carries (the player writes it into the file): its recipe and hash, and the item's name and place
/// when it was made, for people reading the folder. A photo's goes inside the photo's own link (gyroflow-export keeps
/// kind photo, the clip and the time there), under `render`
pub fn link(ctx: &Ctx, item: &Item, profile: &Profile, found: &Found) -> Value {
    let l = &item.link;
    let about = json!({ "id": found.hash, "profile": profile.name, "recipe": found.recipe, "name": item.name, "item": item.rel, "of": item.kind.name() });
    let roots = json!([ctx.root.display().to_string()]);
    if profile.photo {
        return trimmed(&json!({ "clip": item.tree(), "size": l["size"], "created": l["created"], "roots": roots, "render": about })).unwrap_or(Value::Null);
    }
    let mut out = json!({ "v": 1, "kind": "render", "clip": item.tree(), "size": found.recipe["clip"]["size"], "created": l["created"], "hash": l["hash"],
                          "in": l["in"], "out": l["out"], "name": item.name, "look": profile.look, "roots": roots });
    for (k, v) in about.as_object().unwrap() { if k != "name" { out[k] = v.clone(); } }
    trimmed(&out).unwrap_or(Value::Null)
}

/// The name a render is saved under in a folder the user chose: the item's, the look when it isn't the usual one
pub fn readable(item: &Item, profile: &Profile, ext: &str) -> String {
    let safe: String = item.name.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    let safe = safe.trim().trim_matches('.').to_string();
    let base = if safe.is_empty() { item.kind.name().to_string() } else { safe };
    format!("{base}{}.{ext}", if profile.name == "SDR" { " (SDR)" } else { "" })
}

/// An item from what the page knows of it: `kind` (moment, photo, clip) and `path` (a moment's or photo link's path
/// in the project's folder; a clip's in the project's tree)
pub fn item(root: &Path, kind: &str, path: &str) -> Result<Item, String> {
    let stem = |name: &str, ends: &[&str]| -> String {
        let lower = name.to_lowercase();
        for e in ends { if lower.ends_with(e) { return name[..name.len() - e.len()].to_string(); } }
        name.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| name.to_string())
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    match kind {
        "clip" => Ok(Item { kind: Kind::Clip, file: None, rel: path.to_string(), link: Value::Null, name: stem(name, &[]) }),
        "moment" | "photo" => {
            // (a moment of before 0.11 is named by its path in _Moments)
            let file = if root.join(path).is_file() { root.join(path) } else { root.join("_Moments").join(path) };
            let link = index::link(&file).ok_or(format!("{path} has no link to its clip"))?;
            if link["clip"].as_str().is_none() { return Err(format!("{path} doesn't say which clip it's of")); }
            let kind = if kind == "moment" { Kind::Moment } else { Kind::Photo };
            if kind == Kind::Photo && !link["t"].is_number() { return Err(format!("{path} doesn't say which frame it is")); }
            Ok(Item { kind, file: Some(file), rel: path.to_string(), link, name: stem(name, &[index::LINK_EXT, crate::albums::PHOTO_EXT]) })
        }
        k => Err(format!("no renders of a {k}")),
    }
}

/// Renders of these moments and photo links (paths in the project's folder) removed: every profile's current one,
/// and the old ones at a moment's path; `stale_only`: only those whose recipe isn't the item's any more. A render two
/// items share goes for both: it's a cache
pub fn delete(root: &Path, items: &[String], stale_only: bool) -> (usize, Vec<String>) {
    let ctx = Ctx::new(root, true);
    let (mut n, mut problems, mut gone) = (0, Vec::new(), Vec::new());
    for rel in items {
        let kind = if rel.to_lowercase().ends_with(crate::albums::PHOTO_EXT) { "photo" } else { "moment" };
        let it = match item(root, kind, rel) { Ok(i) => i, Err(e) => { problems.push(e); continue } };
        for p in it.kind.profiles() {
            let f = find(&ctx, &it, p);
            for file in f.stale.into_iter().chain(f.file.filter(|_| !stale_only)) {
                match std::fs::remove_file(&file) { Ok(_) => { n += 1; gone.push(file); } Err(e) => problems.push(format!("{rel} {}: {e}", p.name)) }
            }
        }
    }
    crate::seen::note_now(&gone);
    (n, problems)
}

/// Whether a file in _Renders is one Share made: named by its recipe's hash (<24 hex>.<profile>.<ext>), or carrying a
/// render's link (made the old way, at a moment's path). Anything else there (a photo moved in by hand, the one-off
/// script's) is kept by `clear`
fn ours(file: &Path) -> bool {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut parts = name.splitn(3, '.');
    let (h, p, e) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if h.len() == 24 && h.chars().all(|c| c.is_ascii_hexdigit()) && PROFILES.iter().any(|x| x.name == p && x.exts.contains(&e.to_lowercase().as_str())) {
        return true;
    }
    index::link(file).is_some_and(|l| l["kind"] == "render" || l["render"].is_object())
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64)>, dirs: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() { dirs.push(e.path()); walk(&e.path(), out, dirs); }
        else if t.is_file() { out.push((e.path(), e.metadata().map(|m| m.len()).unwrap_or(0))); }
    }
}

/// The render cache's room: { files, bytes } of what Share made there (what `clear` would remove)
pub fn cache_size(root: &Path) -> Value {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    walk(&root.join(DIR), &mut files, &mut dirs);
    let mine: Vec<&(PathBuf, u64)> = files.iter().filter(|(f, _)| ours(f)).collect();
    json!({ "files": mine.len(), "bytes": mine.iter().map(|(_, b)| b).sum::<u64>(), "others": files.len() - mine.len() })
}

/// The render cache emptied (Project settings' Clear: never by itself): every render Share made, a photo's half-made
/// folder (.making), the folders left empty. Anything else there is kept. { files, bytes } freed, { kept }, problems
pub fn clear(root: &Path) -> (Value, Vec<String>) {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    walk(&root.join(DIR), &mut files, &mut dirs);
    let (mut n, mut bytes, mut kept, mut problems, mut gone) = (0u64, 0u64, 0u64, Vec::new(), Vec::new());
    for (f, b) in files {
        let making = f.ancestors().any(|a| a.extension().is_some_and(|e| e == "making"));
        if !(making || ours(&f)) { kept += 1; continue; }
        match std::fs::remove_file(&f) { Ok(_) => { n += 1; bytes += b; gone.push(f); } Err(e) => problems.push(format!("{}: {e}", f.display())) }
    }
    // The folders left empty, deepest first
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs { let _ = std::fs::remove_dir(&d); }
    crate::seen::note_now(&gone);
    (json!({ "files": n, "bytes": bytes, "kept": kept }), problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afl-renders-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn look(root: &Path, fresh: bool) -> Ctx { Ctx::with(root, fresh, json!({})) }

    // A project with one clip (and its own file), a moment and a photo link of it
    fn project(name: &str) -> (PathBuf, Item, Item, Item) {
        let root = temp(name);
        std::fs::write(root.join("Project.afproject"), r#"{"format":"actionfootageplayer-project","version":1}"#).unwrap();
        std::fs::create_dir_all(root.join("Day 1")).unwrap();
        std::fs::write(root.join("Day 1/A.MP4"), vec![7u8; 4096]).unwrap();
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"},"source":{"curve":"dlogm"}}"#).unwrap();
        let moment = Item { kind: Kind::Moment, file: Some(root.join("_Moments/Reef/Turtle.afplink.mp4")), rel: "_Moments/Reef/Turtle.afplink.mp4".into(), name: "Turtle".into(),
                            link: json!({ "v": 1, "kind": "moment", "clip": "Day 1/A.MP4", "size": 4096, "hash": "abc", "in": 1.5, "out": 4.0, "stab": { "view": "normal" } }) };
        let photo = Item { kind: Kind::Photo, file: Some(root.join("_Stills/A_2.50s.afplink.jpg")), rel: "_Stills/A_2.50s.afplink.jpg".into(), name: "A_2.50s".into(),
                           link: json!({ "v": 1, "kind": "photo", "clip": "Day 1/A.MP4", "size": 4096, "t": 2.5 }) };
        let clip = Item { kind: Kind::Clip, file: None, rel: "Day 1/A.MP4".into(), link: Value::Null, name: "A".into() };
        (root, moment, photo, clip)
    }

    // An SDR clip (not log, not HLG or PQ) has one render for both video profiles: its HDR is its SDR (resolve, find)
    #[test]
    fn an_sdr_clip_s_hdr_is_its_sdr_render() {
        let (root, moment, _, clip) = project("sdr-one-render");
        let (hdr, sdr) = (profile("HDR").unwrap(), profile("SDR").unwrap());
        // The test's clip can't be read as an MP4: it keeps its HDR
        assert_eq!(resolve(&look(&root, true), &moment, hdr).name, "HDR");
        // An MP4 with no colr and no log curve: SDR
        std::fs::write(root.join("Day 1/A.MP4"), [0u8, 0, 0, 8, b'm', b'o', b'o', b'v']).unwrap();
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"}}"#).unwrap();
        for item in [&moment, &clip] {
            let ctx = look(&root, true);
            assert_eq!(resolve(&ctx, item, hdr).name, "SDR");
            assert_eq!(resolve(&ctx, item, sdr).name, "SDR");
            let f = find(&ctx, item, sdr);
            assert!(find(&ctx, item, hdr).file.is_none());
            std::fs::create_dir_all(root.join(DIR)).unwrap();
            std::fs::write(cache_path(&root, &f.hash, sdr, "mov"), b"x").unwrap();
            // The SDR render is the HDR one too; the page shows both current
            assert_eq!(find(&look(&root, true), item, hdr).file, find(&look(&root, true), item, sdr).file);
            assert_eq!(states(&look(&root, false), item), json!({ "HDR": "current", "SDR": "current" }));
        }
        // The underwater colour on in the source: made HLG by it, its HDR is HDR, its moments' too (the source's input);
        // an old bundle's isn't read
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"},"source":{"uw":true}}"#).unwrap();
        assert_eq!(resolve(&look(&root, true), &clip, hdr).name, "HDR");
        assert_eq!(resolve(&look(&root, true), &moment, hdr).name, "HDR");
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium","uw":true}}"#).unwrap();
        assert_eq!(resolve(&look(&root, true), &clip, hdr).name, "SDR");
        // Its own log curve: HDR is HDR
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"},"source":{"curve":"dlogm"}}"#).unwrap();
        assert_eq!(resolve(&look(&root, true), &moment, hdr).name, "HDR");
        std::fs::remove_dir_all(&root).ok();
    }

    // The same cases the player's transform.render_settings is checked against (tests/render_settings_test.lua)
    #[test]
    fn settings_are_worked_out_as_the_player_does() {
        let doc: Value = serde_json::from_str(include_str!("../../tests/render_settings.json")).unwrap();
        let mut bad = Vec::new();
        for c in doc["cases"].as_array().unwrap() {
            let defaults = if c["defaults"].is_object() { &c["defaults"] } else { &doc["defaults"] };
            let got = settings(&c["doc"], &c["own"], defaults);
            let same = |a: &Value, b: &Value| { let (mut x, mut y) = (String::new(), String::new()); canonical(a, &mut x); canonical(b, &mut y); x == y };
            if !same(&got, &c["want"]) { bad.push(format!("{}:\n  got  {got}\n  want {}", c["name"], c["want"])); }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    // gyroflow.conf read as the gyroflow plugin reads it (tests/gyroflow_conf_test.lua, the same cases)
    #[test]
    fn defaults_are_read_as_the_player_does() {
        let doc: Value = serde_json::from_str(include_str!("../../tests/gyroflow_conf_cases.json")).unwrap();
        let mut bad = Vec::new();
        let mut checks = 0;
        for c in doc["cases"].as_array().unwrap() {
            let conf: Vec<&str> = c["conf"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            let got = defaults_of(&conf.join("\n"));
            for (k, want) in c["want"].as_object().unwrap() {
                checks += 1;
                let g = got[k].clone();
                let same = match (g.as_f64(), want.as_f64()) { (Some(a), Some(b)) => a == b, _ => g == *want };
                if !same { bad.push(format!("{}: {k}:\n  got  {g}\n  want {want}", c["name"])); }
            }
        }
        assert!(checks >= 30, "only {checks} checks read");
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn the_export_settings_are_part_of_a_render() {
        let (root, moment, photo, _clip) = project("export-settings");
        let ctx = look(&root, true);
        let (hdr, ph) = (profile("HDR").unwrap(), profile("Photo").unwrap());
        let base = hash(&recipe(&ctx, &moment, hdr));
        // The defaults, said or not, are the render as it was: renders made before the settings stay found
        assert_eq!(export_settings(&Value::Null, false), json!({}));
        let defaults = json!({ "video": { "size": "original", "encode": "fast", "quality": "displayed" }, "photo": { "size": "original", "quality": "displayed" } });
        assert_eq!(export_settings(&defaults, false), json!({}));
        assert_eq!(hash(&recipe_with(&ctx, &moment, hdr, &export_settings(&defaults, false))), base);
        // Each other value is another render
        let mut seen = HashSet::from([base.clone()]);
        for (k, v) in [("size", "1080"), ("size", "720"), ("encode", "quality"), ("quality", "quickest"), ("quality", "highest")] {
            let s = export_settings(&json!({ "video": { k: v } }), false);
            assert_eq!(s, json!({ k: v }));
            assert!(seen.insert(hash(&recipe_with(&ctx, &moment, hdr, &s))), "{k} {v} didn't change the hash");
        }
        // A value it doesn't have: left out (the default)
        assert_eq!(export_settings(&json!({ "video": { "size": "8K", "quality": "best" } }), false), json!({}));
        // A photo: its own settings, no encoder
        let s = export_settings(&json!({ "video": { "size": "720" }, "photo": { "size": "1080", "encode": "quality", "quality": "highest" } }), true);
        assert_eq!(s, json!({ "size": "1080", "quality": "highest" }));
        assert_ne!(hash(&recipe_with(&ctx, &photo, ph, &s)), hash(&recipe(&ctx, &photo, ph)));
        // A render with settings is found by them, not by the defaults'
        let found = find_with(&ctx, &moment, hdr, &json!({ "size": "1080" }));
        std::fs::create_dir_all(root.join(DIR)).unwrap();
        std::fs::write(cache_path(&root, &found.hash, hdr, "mov"), b"1080").unwrap();
        assert!(find_with(&look(&root, true), &moment, hdr, &json!({ "size": "1080" })).file.is_some());
        assert!(find(&look(&root, true), &moment, hdr).file.is_none());
    }

    /// What a recipe holds goes with VERSION, and a recipe keeps its hash: a render is found by it (its file is named
    /// by it), so a hash that changes by itself leaves every render made before stale. Changing what goes into a recipe
    /// is raising VERSION, and this test's lines with it; a change of hash alone (canonical, the hash, settings, the
    /// registry's settings) is a mistake. The player's recipe of before the cache by hash is index.rs's (RENDER_VERSION)
    #[test]
    fn the_recipe_goes_with_its_version_and_keeps_its_hash() {
        fn paths(v: &Value, at: &str, out: &mut Vec<String>) {
            for (k, x) in v.as_object().into_iter().flatten() {
                let p = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                if x.is_object() { paths(x, &p, out) } else { out.push(p) }
            }
        }
        let shape = |v: &Value| { let mut out = Vec::new(); paths(v, "", &mut out); out.sort(); out.join(" ") };
        let (root, moment, photo, clip) = project("recipe-version");
        let ctx = look(&root, true);
        let (hdr, ph) = (profile("HDR").unwrap(), profile("Photo").unwrap());
        let (m, p, c) = (recipe(&ctx, &moment, hdr), recipe(&ctx, &photo, ph), recipe(&ctx, &clip, hdr));
        let kept = format!("moment: {} / photo: {} / clip: {}", shape(&m), shape(&p), shape(&c));
        assert_eq!((VERSION, kept.as_str()), (4, concat!("moment: clip.hash clip.size in kind out profile.look profile.name settings.aspect settings.curve settings.denoise_strong settings.lens settings.orient",
                               "ation settings.projection settings.view v / photo: clip.path clip.size kind profile.look profile.name settings.aspect settings.curve settings.denoise_",
                               "strong settings.lens settings.level settings.orientation settings.projection settings.view t v / clip: clip.path clip.size kind profile.look profile.name settings.aspect settings.curve settings.denoise_strong settings.lens settings.level settings.orientation settings.projection settings.view v")), "what a recipe holds changed, or VERSION did: they change together");
        assert_eq!([hash(&m), hash(&p), hash(&c)], ["c3d1323d60720559a14f227f", "2f58eb104de1ceca73053fe0", "5dd5a11539e16cd587d8f210"], "the same recipes hash otherwise: every render made before is stale");
        let full = json!({ "v": 4, "kind": "moment", "clip": { "hash": "5f2c", "size": 1234567890 }, "in": 12.5, "out": 20.033333333,
                           "settings": { "view": "wide", "level": "medium", "horizon_lock": "yes", "fov": 1.25 },
                           "opts": { "edges": "fill", "temporal_denoise": "light" }, "gyroflow": "ab12", "profile": { "name": "HDR", "look": "hdr" },
                           "export": { "size": "1080" }, "empty": {}, "none": null });
        assert_eq!(hash(&full), "4d6e970673332a22ea71d399");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_same_recipe_hashes_the_same_and_every_setting_changes_it() {
        let (root, moment, photo, clip) = project("hash");
        let ctx = look(&root, true);
        let hdr = profile("HDR").unwrap();
        let base = hash(&recipe(&ctx, &moment, hdr));
        assert_eq!(base.len(), 24);
        // Again, by another look at the project; the item's name and place are no part of it
        let moved = Item { rel: "_Clips/Egypt/Best turtle.afplink.mp4".into(), name: "Best turtle".into(), file: moment.file.clone(), link: moment.link.clone(), kind: Kind::Moment };
        assert_eq!(hash(&recipe(&look(&root, false), &moved, hdr)), base);
        // Keys in another order, a whole number written as a float: the same
        assert_eq!(hash(&json!({ "b": 1.0, "a": [1, 2.5], "c": null })), hash(&json!({ "a": [1.0, 2.5], "b": 1 })));
        // Each thing that changes the pixels is another render
        let mut seen = HashSet::from([base.clone()]);
        let mut differs = |what: &str, h: String| assert!(seen.insert(h), "{what} didn't change the hash");
        let with = |k: &str, v: Value| { let mut l = moment.link.clone(); l[k] = v; Item { link: l, rel: moment.rel.clone(), name: moment.name.clone(), file: moment.file.clone(), kind: Kind::Moment } };
        differs("the profile", hash(&recipe(&ctx, &moment, profile("SDR").unwrap())));
        differs("the in point", hash(&recipe(&ctx, &with("in", json!(1.6)), hdr)));
        differs("the out point", hash(&recipe(&ctx, &with("out", json!(4.5)), hdr)));
        differs("the stabilization", hash(&recipe(&ctx, &with("stab", json!({ "view": "wide" })), hdr)));
        differs("the horizon", hash(&recipe(&ctx, &with("stab", json!({ "view": "normal", "horizon_lock": "yes" })), hdr)));
        differs("the crop's shape", hash(&recipe(&ctx, &with("stab", json!({ "view": "normal", "aspect": "9:16" })), hdr)));
        // Stabilized on its own: a moment kept as made (snap) that says so; without snap the player doesn't read it
        assert_eq!(hash(&recipe(&ctx, &with("alone", json!(true)), hdr)), base, "alone without snap changed the hash");
        let mut snapped = with("alone", json!(true));
        snapped.link["snap"] = json!(true);
        differs("stabilized alone", hash(&recipe(&ctx, &snapped, hdr)));
        differs("the original", hash(&recipe(&ctx, &with("hash", json!("abd")), hdr)));
        differs("the original's size", hash(&recipe(&ctx, &with("size", json!(4097)), hdr)));
        // The moment's own Gyroflow project
        std::fs::create_dir_all(root.join("_Moments/Reef")).unwrap();
        std::fs::write(root.join("_Moments/Reef/Turtle.gyroflow"), b"{\"smoothing\":1}").unwrap();
        differs("its Gyroflow project", hash(&recipe(&ctx, &moment, hdr)));
        std::fs::remove_file(root.join("_Moments/Reef/Turtle.gyroflow")).unwrap();
        // The clip's source settings (its own file), read again by a new look
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"},"source":{"curve":"dlogm","underwater":true}}"#).unwrap();
        differs("the source's input", hash(&recipe(&look(&root, true), &moment, hdr)));
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"},"source":{"curve":"dlogm"}}"#).unwrap();
        // The player's defaults
        let strong = hash(&recipe(&Ctx::with(&root, true, defaults_of("level=strong\nlive_photo=live\n")), &moment, hdr));
        // (an option that isn't about the pixels: nothing)
        assert_eq!(hash(&recipe(&Ctx::with(&root, true, defaults_of("# mine\nlevel=strong\nlive_photo=no\n")), &moment, hdr)), strong);
        differs("the player's defaults", strong);
        differs("the player's edge fill", hash(&recipe(&Ctx::with(&root, true, defaults_of("edges=off\n")), &moment, hdr)));
        differs("the player's temporal denoise", hash(&recipe(&Ctx::with(&root, true, defaults_of("temporal_denoise=light\n")), &moment, hdr)));
        // A moment with no bundle of its own renders with the clip's saved view
        let plain = with("stab", Value::Null);
        let before = hash(&recipe(&look(&root, true), &plain, hdr));
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"strong"},"source":{"curve":"dlogm"}}"#).unwrap();
        assert_ne!(hash(&recipe(&look(&root, true), &plain, hdr)), before, "the clip's view didn't change a moment without its own bundle");
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"wide","level":"medium"},"source":{"curve":"dlogm"}}"#).unwrap();

        // A photo: its frame, its crop, its camera, the clip's saved view
        let p = profile("Photo").unwrap();
        let mut seen = HashSet::from([hash(&recipe(&ctx, &photo, p))]);
        let mut differs = |what: &str, h: String| assert!(seen.insert(h), "{what} didn't change the photo's hash");
        let with = |k: &str, v: Value| { let mut l = photo.link.clone(); l[k] = v; Item { link: l, rel: photo.rel.clone(), name: photo.name.clone(), file: photo.file.clone(), kind: Kind::Photo } };
        differs("the frame", hash(&recipe(&ctx, &with("t", json!(2.54)), p)));
        differs("the crop", hash(&recipe(&ctx, &with("crop", json!({ "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 })), p)));
        differs("the camera", hash(&recipe(&ctx, &with("camera", json!({ "yaw": 10, "pitch": 0, "roll": 0, "zoom": 1.2 })), p)));
        std::fs::write(root.join("Day 1/A.afp.json"), r#"{"view":{"view":"normal","level":"medium"},"source":{"curve":"dlogm"}}"#).unwrap();
        let again = look(&root, true);
        differs("the clip's saved view", hash(&recipe(&again, &photo, p)));
        // A whole clip: another thing than a moment of it or a photo of it, in each profile
        let c = hash(&recipe(&again, &clip, hdr));
        assert_ne!(c, hash(&recipe(&again, &moment, hdr)));
        assert_ne!(c, hash(&recipe(&again, &clip, profile("SDR").unwrap())));
        // The clip's Gyroflow project
        std::fs::write(root.join("Day 1/A.gyroflow"), b"{}").unwrap();
        assert_ne!(c, hash(&recipe(&look(&root, true), &clip, hdr)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_render_is_found_by_its_hash_wherever_the_item_is() {
        let (root, mut moment, photo, _) = project("cache");
        let hdr = profile("HDR").unwrap();
        // A miss: nothing there
        let f = find(&look(&root, true), &moment, hdr);
        assert!(f.file.is_none() && f.stale.is_none());
        assert_eq!(states(&look(&root, false), &moment), json!({}));
        // Made: a hit, for this look at the cache and for a scan's
        std::fs::create_dir_all(root.join(DIR)).unwrap();
        let file = cache_path(&root, &f.hash, hdr, "mov");
        assert_eq!(file, root.join(format!("_Renders/{}.HDR.mov", f.hash)));
        std::fs::write(&file, b"pixels").unwrap();
        assert_eq!(find(&look(&root, true), &moment, hdr).file, Some(file.clone()));
        assert_eq!(states(&look(&root, false), &moment), json!({ "HDR": "current" }));
        // The moment renamed and moved into an album: its render is still its own
        moment.rel = "_Clips/Egypt/Best turtle.afplink.mp4".into();
        moment.name = "Best turtle".into();
        assert_eq!(find(&look(&root, true), &moment, hdr).file, Some(file.clone()));
        // Its recipe changed: a miss again (the old file is left for cleaning), and the other profile was never made
        moment.link["out"] = json!(5.0);
        let changed = find(&look(&root, true), &moment, hdr);
        assert!(changed.file.is_none() && changed.hash != f.hash);
        assert!(find(&look(&root, true), &moment, profile("SDR").unwrap()).file.is_none());
        // A photo's render is a HEIC or a JPEG
        let p = profile("Photo").unwrap();
        let pf = find(&look(&root, true), &photo, p);
        assert!(pf.file.is_none());
        std::fs::write(cache_path(&root, &pf.hash, p, "jpg"), b"jpeg").unwrap();
        assert_eq!(find(&look(&root, true), &photo, p).file, Some(root.join(format!("_Renders/{}.Photo.jpg", pf.hash))));
        assert_eq!(states(&look(&root, false), &photo), json!({ "Photo": "current" }));
        // The link a render carries says what it's of
        let l = link(&look(&root, true), &moment, hdr, &changed);
        assert_eq!((l["kind"].as_str(), l["id"].as_str(), l["item"].as_str(), l["name"].as_str()), (Some("render"), Some(changed.hash.as_str()), Some("_Clips/Egypt/Best turtle.afplink.mp4"), Some("Best turtle")));
        assert_eq!(hash(&l["recipe"]), changed.hash);
        let l = link(&look(&root, true), &photo, p, &pf);
        assert_eq!((l["render"]["id"].as_str(), l["kind"].as_str()), (Some(pf.hash.as_str()), None));
        // The views: a photo link has its render ("rendered" filters on it); the cache's files are never items
        std::fs::create_dir_all(root.join("_Stills")).unwrap();
        std::fs::write(root.join("_Stills/A_2.50s.afplink.jpg"), crate::albums::photo_file(&crate::albums::tests::jpeg(16, 8, [9, 9, 9]), &photo.link.to_string()).unwrap()).unwrap();
        std::fs::write(root.join(format!("_Renders/{}.Photo.jpg", pf.hash)), crate::albums::photo_file(&crate::albums::tests::jpeg(16, 8, [9, 9, 9]), &l.to_string()).unwrap()).unwrap();
        let stills = index::scan(&root, "stills", false);
        let listed: Vec<&str> = stills["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(listed, ["_Stills/A_2.50s.afplink.jpg"]);
        assert_eq!(stills["items"][0]["renders"], json!({ "Photo": "current" }));
        assert_eq!(readable(&moment, profile("SDR").unwrap(), "mov"), "Best turtle (SDR).mov");
        assert_eq!(readable(&photo, p, "heic"), "A_2.50s.heic");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clearing_the_cache_takes_only_what_share_made() {
        let root = temp("clear");
        let dir = root.join(DIR);
        std::fs::create_dir_all(dir.join("Reef")).unwrap();
        std::fs::create_dir_all(dir.join("0123456789abcdef01234567.Photo.making")).unwrap();
        std::fs::write(dir.join("0123456789abcdef01234567.HDR.mov"), vec![1u8; 100]).unwrap();
        std::fs::write(dir.join("0123456789abcdef01234568.Photo.heic"), vec![1u8; 10]).unwrap();
        std::fs::write(dir.join("0123456789abcdef01234567.Photo.making/x.heic"), vec![1u8; 5]).unwrap();
        std::fs::write(dir.join("Reef/Turtle.HDR.mov"), crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"render","clip":"A.MP4"}"#)).unwrap();
        std::fs::write(dir.join("Reef/Kept.heic"), b"a photo of someone's").unwrap();
        let size = cache_size(&root);
        assert_eq!((size["files"].as_u64(), size["others"].as_u64()), (Some(3), Some(2)), "{size}");
        let (freed, problems) = clear(&root);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(freed["files"], json!(4));
        assert_eq!(freed["kept"], json!(1));
        assert!(dir.join("Reef/Kept.heic").is_file());
        assert!(!dir.join("0123456789abcdef01234567.Photo.making").exists());
        assert_eq!(cache_size(&root)["files"], json!(0));
    }

    #[test]
    fn a_render_made_the_old_way_is_still_found() {
        let (root, moment, _, _) = project("old");
        let hdr = profile("HDR").unwrap();
        let old = index::render_path(&root, &moment.rel, "HDR");
        assert_eq!(old, root.join("_Renders/Reef/Turtle.HDR.mov"));
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::create_dir_all(root.join("_Moments/Reef")).unwrap();
        let file = moment.file.clone().unwrap();
        let write = |recipe: &Value| std::fs::write(&old, index::tests::tests_mp4_with_link(&json!({ "v": 1, "kind": "render", "recipe": recipe }).to_string())).unwrap();
        // Its v1 recipe is the moment's: current, and it's the file Share copies
        write(&index::render_recipe(&root, &file, &moment.link, "HDR"));
        let f = find(&look(&root, true), &moment, hdr);
        assert_eq!((f.file.as_deref(), f.stale.as_deref()), (Some(old.as_path()), None));
        assert_eq!(states(&look(&root, false), &moment), json!({ "HDR": "current" }));
        // The moment changed since: stale
        let mut changed = index::render_recipe(&root, &file, &moment.link, "HDR");
        // (and the file's size with it: links read are kept by the file's size and time)
        changed["out"] = json!(9.125);
        write(&changed);
        let f = find(&look(&root, true), &moment, hdr);
        assert_eq!((f.file.as_deref(), f.stale.as_deref()), (None, Some(old.as_path())));
        assert_eq!(states(&look(&root, false), &moment), json!({ "HDR": "stale" }));
        // One in the cache by its hash comes first
        let new = cache_path(&root, &f.hash, hdr, "mov");
        std::fs::write(&new, b"pixels").unwrap();
        assert_eq!(find(&look(&root, true), &moment, hdr).file, Some(new));
        let _ = std::fs::remove_dir_all(&root);
    }
}
