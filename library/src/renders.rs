// The render cache (docs/albums-photos-share-concept.md, "Renders": found by recipe, not by path). A render is the
// real pixels of one item (a moment, a photo link, a whole clip) in one profile. What decides its pixels is its recipe;
// the recipe's hash names its file: <project>/_Renders/<hash>.<profile>.<ext>. So a render is found whatever the item
// is called and wherever it lies: moved, renamed or copied items keep theirs, two copies of a moment share one, and a
// changed recipe is another file. The player makes them (its batch `share`, workspace/main.lua): the Library works the
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
/// Raised when the renderer's pixels change for the same recipe: every render is then another one
pub const VERSION: u64 = 3;

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
              names, clips: RefCell::new(HashMap::new()) }
    }

    fn has(&self, name: &str) -> bool {
        match &self.names { Some(n) => n.contains(name), None => self.root.join(DIR).join(name).is_file() }
    }

    fn clip(&self, tree: &str) -> ClipFacts {
        if let Some(c) = self.clips.borrow().get(tree) { return c.clone(); }
        let mut facts = ClipFacts::default();
        if let Ok((own, clip)) = crate::tags::clip_own_in(&self.locs, tree) {
            let doc = index::read_json(&own).unwrap_or(Value::Null);
            facts.doc = doc;
            // The clip's Gyroflow project, beside it (<stem>.gyroflow, or <name.ext>.gyroflow)
            let name = clip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            facts.gyroflow = [clip.with_extension("gyroflow"), clip.with_file_name(format!("{name}.gyroflow"))].iter()
                .find(|g| g.is_file()).and_then(|g| crate::checks::fingerprint_kept(g)).map(Value::from).unwrap_or(Value::Null);
            facts.size = std::fs::metadata(&clip).map(|m| json!(m.len())).unwrap_or(Value::Null);
            facts.file = Some(clip);
        }
        self.clips.borrow_mut().insert(tree.to_string(), facts.clone());
        facts
    }

    /// The clip's file an item is rendered from, when it's here
    pub fn original(&self, item: &Item) -> Option<PathBuf> { item.tree().and_then(|t| self.clip(t).file) }

}

/// What a render takes from this computer's player: what it wrote last (gyroflow/main.lua `publish`, every time it
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

/// The player's defaults from its gyroflow.conf, as gyroflow/main.lua's `defaults_bundle` makes them (its options'
/// own defaults where the file doesn't say). The preset's horizon isn't known here: "no"
fn defaults_of(conf: &str) -> Value {
    let mut o = serde_json::Map::new();
    for line in conf.lines() {
        let line = line.trim();
        if line.starts_with('#') { continue; }
        if let Some((k, v)) = line.split_once('=') { o.insert(k.trim().to_string(), json!(v.trim())); }
    }
    let get = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let yes = |k: &str, default: bool| match get(k).as_str() { "yes" | "true" => true, "no" | "false" => false, _ => default };
    let one = |k: &str, of: &[&str], default: &str| { let v = get(k); if of.contains(&v.as_str()) { v } else { default.to_string() } };
    let split = split_frame(&json!({ "aspect": if get("aspect").is_empty() { "original".to_string() } else { get("aspect") },
                                     "orientation": if get("orientation").is_empty() { Value::Null } else { json!(get("orientation")) } }));
    let blur: f64 = get("edges_blur").parse().unwrap_or(2.0);
    json!({
        "defaults": {
            "view": if get("view") == "wide" { "wide" } else { "normal" },
            "level": one("level", &["weak", "medium", "strong"], "medium"),
            "horizon_lock": one("horizon_lock", &["yes", "no"], "no"),
            "chromatic": yes("chromatic", true),
            "aspect": split["aspect"], "orientation": split["orientation"],
            "projection": one("projection", &["linear", "stereographic", "equidistant", "equisolid", "orthographic"], "linear"),
            "lens": if get("lens") == "as-recorded" { "as-recorded" } else { "corrected" },
            "denoise": yes("still", true),
            "denoise_strong": yes("denoise_strong", false),
            "preset_horizon": "no",
        },
        "edges": if get("edges").is_empty() { "sphere".to_string() } else { get("edges") },
        "edges_blur": blur,
        "temporal_denoise": one("temporal_denoise", &["full", "light", "off"], "full"),
    })
}

// ---------- What a render is made with: the player's transform.render_settings, the same way ----------
// (mpv/scripts/workspace/transform.lua; both are checked against tests/render_settings.json)

const BUNDLE_KEYS: [&str; 10] = ["view", "level", "horizon_lock", "chromatic", "aspect", "orientation", "projection", "lens", "denoise", "denoise_strong"];

fn valid(k: &str, v: &Value) -> bool {
    let s = v.as_str().unwrap_or("");
    match k {
        "view" => matches!(s, "normal" | "wide" | "off"),
        "level" => matches!(s, "weak" | "medium" | "strong"),
        "horizon_lock" => matches!(s, "yes" | "no"),
        "chromatic" | "denoise" | "denoise_strong" => v.is_boolean(),
        // Any ratio landscape-wise (W >= H), "original"
        "aspect" => match s.split_once(':') {
            Some((w, h)) if !w.is_empty() && !h.is_empty() && w.chars().chain(h.chars()).all(|c| c.is_ascii_digit()) => {
                let (w, h): (f64, f64) = (w.parse().unwrap_or(0.0), h.parse().unwrap_or(0.0));
                w >= h && h > 0.0
            }
            _ => s == "original",
        },
        "orientation" => matches!(s, "as-is" | "landscape" | "portrait"),
        "projection" => matches!(s, "linear" | "stereographic" | "equidistant" | "equisolid" | "orthographic" | "project"),
        "lens" => matches!(s, "corrected" | "as-recorded"),
        _ => false,
    }
}

/// The bundle's keys of a view, None for none (transform.pick)
fn pick(view: &Value) -> Option<Value> {
    let o = view.as_object()?;
    let out: serde_json::Map<String, Value> = BUNDLE_KEYS.iter().filter_map(|k| o.get(*k).filter(|v| !v.is_null()).map(|v| (k.to_string(), v.clone()))).collect();
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
    let neutral = json!({ "aspect": "original", "orientation": "as-is", "projection": "linear", "denoise_strong": false });
    let mut out = serde_json::Map::new();
    for k in BUNDLE_KEYS {
        let mut v = bundle.get(k).cloned().unwrap_or(Value::Null);
        if k == "horizon_lock" && v == json!("project") { v = session["preset_horizon"].clone(); }
        if !valid(k, &v) {
            v = if k == "lens" && bundle.as_object().is_some_and(|o| !o.is_empty()) {
                json!(if out.get("view") == Some(&json!("off")) { "as-recorded" } else { "corrected" })
            } else if !neutral[k].is_null() { neutral[k].clone() } else { session[k].clone() };
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
    Value::Object(out)
}

/// Everything that decides the pixels of `item` rendered in `profile`. Its fields (docs/share-fixes.md):
/// v, kind, clip { hash | path, size }, in / out / alone (a moment), t / crop / camera (a photo), settings (what it's
/// rendered with: `settings` from the clip's own file, a moment's own bundle and the player's defaults: the player
/// renders with exactly these, sent in its request), opts (the player's edge fill and temporal denoise), gyroflow (the
/// fingerprint of the Gyroflow project used: a moment's own, else the clip's), preset (the player's preset's, where
/// there is no project), profile { name, look }
pub fn recipe(ctx: &Ctx, item: &Item, profile: &Profile) -> Value {
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
            if let Some(own) = own_project(item).filter(|o| o.is_file()).and_then(|o| crate::checks::fingerprint(&o)) { r["gyroflow"] = json!(own); }
            for k in ["in", "out"] { r[k] = l[k].clone(); }
            // Stabilized on its own: a moment kept as made (snap) that says so (the player's render_moments)
            if l["snap"] == json!(true) && l["alone"] == json!(true) { r["alone"] = json!(true); }
        }
        Kind::Photo => { for k in ["t", "crop", "camera"] { r[k] = l[k].clone(); } }
        Kind::Clip => {}
    }
    if r["gyroflow"].is_null() { r["preset"] = ctx.preset.clone(); }
    trimmed(&r).unwrap_or(Value::Null)
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

pub fn find(ctx: &Ctx, item: &Item, profile: &Profile) -> Found {
    let recipe = recipe(ctx, item, profile);
    let hash = hash(&recipe);
    let mut found = Found { recipe, hash, file: None, stale: None };
    for ext in profile.exts {
        let name = format!("{}.{}.{ext}", found.hash, profile.name);
        if ctx.has(&name) { found.file = Some(ctx.root.join(DIR).join(name)); return found; }
    }
    // Made the old way: _Renders/<the moment's path>.<profile>.mov, current while its v1 recipe is the moment's
    if let (Kind::Moment, false, Some(file)) = (item.kind, profile.photo, &item.file) {
        let old = index::render_path(&ctx.root, &item.rel, profile.name);
        if crate::seen::is_file(&old) || (ctx.names.is_none() && old.is_file()) {
            let ok = index::link(&old).filter(|l| l["kind"] == "render").and_then(|l| trimmed(&l["recipe"]))
                .is_some_and(|r| same(&r, &index::render_recipe(&ctx.root, file, &item.link, profile.name)));
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

    // The same cases the player's transform.render_settings is checked against (tests/render_settings_test.lua)
    #[test]
    fn settings_are_worked_out_as_the_player_does() {
        let doc: Value = serde_json::from_str(include_str!("../../tests/render_settings.json")).unwrap();
        let mut bad = Vec::new();
        for c in doc["cases"].as_array().unwrap() {
            let got = settings(&c["doc"], &c["own"], &doc["defaults"]);
            let same = |a: &Value, b: &Value| { let (mut x, mut y) = (String::new(), String::new()); canonical(a, &mut x); canonical(b, &mut y); x == y };
            if !same(&got, &c["want"]) { bad.push(format!("{}:\n  got  {got}\n  want {}", c["name"], c["want"])); }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
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
        assert_eq!(hash(&recipe(&Ctx::with(&root, true, defaults_of("# mine\nlevel = strong\nlive_photo=no\n")), &moment, hdr)), strong);
        differs("the player's defaults", strong);
        differs("the player's edge fill", hash(&recipe(&Ctx::with(&root, true, defaults_of("edges=black\n")), &moment, hdr)));
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
