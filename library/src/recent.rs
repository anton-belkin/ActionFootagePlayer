// This computer's list of projects opened
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- this computer's list of projects opened ----------

fn recent_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("recent.json"))
}

/// A project opened before: its folder, and the share it's on (as mount names it: "//user@host/share", and where in
/// it), for connecting it when it isn't mounted
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub(crate) struct RecentEntry {
    pub(crate) path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) share: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) rel: String,
    /// The NAS's other names, from the project's hosts when it was last reachable (its file is on the share: not read
    /// while it's offline)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) hosts: Vec<String>,
    /// "Open at start": the one project of the list the app opens when it's started with no file (start_entry). This
    /// computer's choice, kept here in the app's own data, never in the project
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) at_start: bool,
}

/// The project opened at start: the one of the list marked so (the first, should a list edited by hand mark several)
pub(crate) fn start_entry(list: &[RecentEntry]) -> Option<&RecentEntry> { list.iter().find(|e| e.at_start) }

/// "Open at start" put on the project at `path` (None: on none), taken off every other. false: it isn't in the list
pub(crate) fn set_start(list: &mut [RecentEntry], path: Option<&str>) -> bool {
    for e in list.iter_mut() { e.at_start = false; }
    match path {
        None => true,
        Some(p) => list.iter_mut().find(|e| e.path == p).map(|e| e.at_start = true).is_some(),
    }
}

/// The list with a project just opened put first: its own entry replaced (its "Open at start" kept), the list kept to
/// ten, the project opened at start never dropped off its end
pub(crate) fn remembered(mut list: Vec<RecentEntry>, mut new: RecentEntry) -> Vec<RecentEntry> {
    new.at_start = list.iter().any(|x| x.path == new.path && x.at_start);
    list.retain(|x| x.path != new.path);
    list.insert(0, new);
    if list.len() > 10 {
        let start = list.iter().skip(10).find(|e| e.at_start).cloned();
        list.truncate(10);
        if let Some(s) = start { list[9] = s; }
    }
    list
}

/// The list as kept: entries, or (before 0.13) the paths alone
pub(crate) fn parse_recent(text: &str) -> Vec<RecentEntry> {
    let Ok(Value::Array(a)) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    a.into_iter().filter_map(|v| match v {
        Value::String(path) => Some(RecentEntry { path, ..Default::default() }),
        v => serde_json::from_value::<RecentEntry>(v).ok(),
    }).collect()
}

pub(crate) fn recent_list(app: &tauri::AppHandle) -> Vec<RecentEntry> {
    recent_file(app).and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse_recent(&t)).unwrap_or_default()
}

pub(crate) fn write_recent(app: &tauri::AppHandle, list: &[RecentEntry]) {
    let Some(file) = recent_file(app) else { return };
    if let Some(d) = file.parent() { let _ = std::fs::create_dir_all(d); }
    let _ = std::fs::write(file, serde_json::to_vec(list).unwrap_or_default());
}

/// The share a folder is on, as mounted now (its source, and the folder's path in it)
pub(crate) fn share_of(path: &Path, mounts: &[(String, PathBuf)]) -> Option<(String, String)> {
    mounts.iter().filter(|(_, dir)| path.starts_with(dir)).max_by_key(|(_, dir)| dir.components().count())
        .map(|(src, dir)| (src.clone(), path.strip_prefix(dir).map(|r| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")).unwrap_or_default()))
}

/// Where a share is mounted now (the same host, by any of its names, and share), if it is
fn mounted_at(share: &str, mounts: &[(String, PathBuf)]) -> Option<PathBuf> {
    let (host, name, _) = locations::network(share)?;
    mounts.iter().find(|(src, _)| locations::network(src).is_some_and(|(h, n, _)| locations::host_key(&h) == locations::host_key(&host) && n.eq_ignore_ascii_case(&name)))
        .map(|(_, d)| d.clone())
}

/// Where an entry's project is now: its folder, or the same place on its share mounted elsewhere (/Volumes/ssd-1)
pub(crate) fn recent_here(e: &RecentEntry, mounts: &[(String, PathBuf)]) -> Option<PathBuf> {
    let p = PathBuf::from(&e.path);
    if is_project(&p) { return Some(p); }
    let mut d = mounted_at(e.share.as_deref()?, mounts)?;
    for part in e.rel.split('/').filter(|x| !x.is_empty()) { d.push(part); }
    is_project(&d).then_some(d)
}

/// The other names of the NAS a project is on, as its file lists them
pub(crate) fn other_names(root: &Path, share: Option<&str>) -> Vec<String> {
    let Some((host, _, _)) = share.and_then(locations::network) else { return Vec::new() };
    host_names(&host, read_doc(root).get("hosts")).into_iter().skip(1).collect()
}

/// An offline project's share by all its names: as mounted when it was last seen first, then its other names
pub(crate) fn recent_names(e: &RecentEntry) -> Vec<String> {
    let Some((host, _, _)) = e.share.as_deref().and_then(locations::network) else { return Vec::new() };
    let mut out = host_names(&host, None);
    for h in &e.hosts { if !out.iter().any(|o| o.eq_ignore_ascii_case(h)) { out.push(h.clone()); } }
    out
}

/// A project kept before its share was (recent.json of 0.12 had paths alone), not reachable now: its share guessed from
/// where its path is (/Volumes/<share>/…, as the Mac mounts one) and the NAS this computer has shares of (mounted now,
/// or the other projects' kept): the entry with that share, and the names to try. None: no guess (not under /Volumes,
/// no NAS known)
pub(crate) fn guess_share(e: &RecentEntry, mounts: &[(String, PathBuf)], kept: &[String]) -> Option<(RecentEntry, Vec<String>)> {
    if e.share.is_some() || !cfg!(target_os = "macos") { return None; }
    let rest = Path::new(&e.path).strip_prefix("/Volumes").ok()?;
    let mut parts = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
    let vol = parts.next()?;
    let rel = parts.collect::<Vec<_>>().join("/");
    // The NAS's names, each once as spelled, and the user its shares are mounted as
    let (mut names, mut user): (Vec<String>, Option<String>) = (Vec::new(), None);
    for src in mounts.iter().map(|(s, _)| s.as_str()).chain(kept.iter().map(String::as_str)) {
        let Some((host, _, _)) = locations::network(src) else { continue };
        let (u, h) = host.rsplit_once('@').map(|(u, h)| (Some(u.to_string()), h.to_string())).unwrap_or((None, host.clone()));
        let h = h.trim_end_matches('.').to_string();
        if user.is_none() { user = u; }
        if !names.iter().any(|n: &String| n.eq_ignore_ascii_case(&h)) { names.push(h); }
    }
    let first = names.first()?;
    let share = format!("//{}{first}/{vol}", user.map(|u| format!("{u}@")).unwrap_or_default());
    Some((RecentEntry { share: Some(share), rel, ..e.clone() }, names))
}

pub(crate) fn remember(app: &tauri::AppHandle, root: &Path) {
    let p = root.display().to_string();
    let (share, rel) = share_of(root, &locations::mounts()).map(|(s, r)| (Some(s), r)).unwrap_or_default();
    let hosts = other_names(root, share.as_deref());
    let list = remembered(recent_list(app), RecentEntry { path: p, share, rel, hosts, at_start: false });
    write_recent(app, &list);
    // Where it is here, by its id, for the pointers of its locations (the player finds it so)
    locations::note_here(root);
}
