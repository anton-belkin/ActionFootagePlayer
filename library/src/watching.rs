// The project's folder and its locations watched, and the shares they're on mounted
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

/// What of the project's file says where its footage is: its locations (and the hosts they're on)
pub(crate) fn locations_key(root: &Path) -> String {
    let d = read_doc(root);
    json!([d.get("locations"), d.get("hosts")]).to_string()
}

/// Which of the project's locations are reachable here now, and where
pub(crate) fn reach_key(root: &Path) -> String {
    locations::locations(root, &locations::mounts()).iter()
        .map(|l| format!("{}={}\n", l.path, l.dir.as_ref().filter(|d| d.is_dir()).map(|d| d.display().to_string()).unwrap_or_default()))
        .collect()
}

/// Every few seconds, whether reach_key changed since `before`: `changed` set then, and the thread done. It ends too
/// when the watch it's for is dropped (the project closed, or watched again)
pub(crate) fn poll_reach(root: &Path, before: String, changed: &Arc<std::sync::atomic::AtomicBool>) {
    let (root, changed) = (root.to_path_buf(), Arc::downgrade(changed));
    std::thread::Builder::new().name("locations".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let Some(flag) = changed.upgrade() else { return };
        drop(flag);
        // Outside any lock: a share that hangs holds up only this thread
        if reach_key(&root) != before {
            if let Some(flag) = changed.upgrade() { flag.store(true, std::sync::atomic::Ordering::Relaxed); }
            return;
        }
    }).ok();
}

/// A NAS's names: the one given, then the others the project's hosts list for it ("DISKSTATIONDXP = web.nas,
/// 192.168.50.186" in Project settings), each once
pub(crate) fn host_names(host: &str, hosts: Option<&Value>) -> Vec<String> {
    let host = host.rsplit_once('@').map(|x| x.1).unwrap_or(host).trim_end_matches('.');
    let mut out = vec![host.to_string()];
    for (k, list) in hosts.and_then(Value::as_object).into_iter().flatten() {
        let mut all = vec![k.clone()];
        all.extend(list.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)));
        if !all.iter().any(|x| locations::host_key(x) == locations::host_key(host)) { continue; }
        // Each once as spelled (a Bonjour name and the plain one are two ways to reach it: both kept)
        for x in all { if !out.iter().any(|o| o.eq_ignore_ascii_case(&x)) { out.push(x); } }
    }
    out
}

/// One share mounted by one name, as Finder does: with the password the Keychain has for it (none: the Mac asks for
/// one). `who`: the host, or "user@host" as mount names it (the user kept: the Keychain's password is that user's)
fn mount_url(who: &str, share: &str) -> Result<(), String> {
    let enc = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
    let w = match who.rsplit_once('@') { Some((u, h)) => format!("{}@{}", enc(u), enc(h.trim_end_matches('.'))), None => enc(who.trim_end_matches('.')) };
    let url = format!("smb://{w}/{}", enc(share));
    let out = std::process::Command::new("/usr/bin/osascript").arg("-e").arg(format!("mount volume \"{url}\"")).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(format!("//{}/{share}: {}", who.rsplit_once('@').map(|x| x.1).unwrap_or(who), String::from_utf8_lossy(&out.stderr).trim())) }
}

/// A share mounted: `path` names it ("//[user@]host/share/…"), tried by each of `names` in turn (the NAS's names, the
/// one it's written with first) until one connects. Only the Mac; elsewhere the share is connected in the system and
/// then shows. Ok: the name that worked. Err: none did (a NAS not on this network: a dozen seconds a name)
pub(crate) fn mount_share(path: &str, names: &[String]) -> Result<String, String> {
    let (host, share, _) = locations::network(path).ok_or_else(|| format!("{path} isn't on a share"))?;
    if !cfg!(target_os = "macos") { return Err(format!("Connect //{host}/{share} in the system: it shows then")); }
    let user = host.rsplit_once('@').map(|x| x.0.to_string());
    let mut errors = Vec::new();
    for n in names {
        let who = user.as_ref().map(|u| format!("{u}@{n}")).unwrap_or_else(|| n.clone());
        match mount_url(&who, &share) { Ok(()) => return Ok(n.clone()), Err(e) => errors.push(e) }
    }
    Err(format!("{share} couldn't be connected: {}", errors.join("; ")))
}

/// The projects whose shares were mounted on opening them, once a run: one taken away afterwards (ejected) stays away
static MOUNTED_ON_OPEN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// On opening a project (the first time this run), its locations' shares not mounted: mounted, in the background (the
/// look every 5 s then sees them)
fn mount_on_open(root: &Path) {
    // (not from tests: the shares are this Mac's)
    if !cfg!(target_os = "macos") || cfg!(test) { return; }
    { let mut m = MOUNTED_ON_OPEN.lock().unwrap(); if m.iter().any(|r| r == root) { return; } m.push(root.to_path_buf()); }
    let root = root.to_path_buf();
    std::thread::Builder::new().name("mount".into()).spawn(move || {
        let mut shares: Vec<(String, String)> = Vec::new();
        for l in locations::locations(&root, &locations::mounts()) {
            if l.local || l.dir.is_some() { continue; }
            let Some((host, share, _)) = locations::network(&l.path) else { continue };
            let key = (locations::host_key(&host), share.to_lowercase());
            if shares.contains(&key) { continue; }
            shares.push(key);
            let _ = mount_share(&l.path, &host_names(&host, read_doc(&root).get("hosts")));
        }
    }).ok();
}

/// `moved`: its own files just moved here (Move the project…), its catalog with them
pub(crate) fn watch(root: &Path, moved: bool) -> Watch {
    use notify::Watcher;
    // Before the catalog's thread reads them: a share mounted meanwhile is seen as a change
    let reach = reach_key(root);
    let reach_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    poll_reach(root, reach, &reach_changed);
    mount_on_open(root);
    let changes = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let last = Arc::new(Mutex::new(Instant::now()));
    let (c, l) = (changes.clone(), last.clone());
    let runner = seen::start(root, changes.clone(), last.clone(), moved);
    let (events, me) = (runner.events.clone(), root.to_path_buf());
    // A file's attributes changed (a Finder tag set by hand, FSEvents' ItemXattrMod; this Mac's disks, and this Mac's
    // own changes on a share): that original or moment synced with the library's tag stores, a second after it settles
    // (docs/design/tags-and-culling.md, Sync). Our own writes, noticed again, find nothing to do
    let (attrs, attrs_rx) = std::sync::mpsc::channel::<PathBuf>();
    let sync_root = root.to_path_buf();
    std::thread::spawn(move || {
        while let Ok(first) = attrs_rx.recv() {
            let mut paths = vec![first];
            while let Ok(p) = attrs_rx.recv_timeout(Duration::from_millis(1000)) { paths.push(p); }
            let stores = tags::Stores::of(&sync_root);
            if stores.list.is_empty() { continue; }
            let mut items: Vec<Value> = Vec::new();
            for p in paths { if let Some(it) = tags::item_of_file(&sync_root, &p) { if !items.contains(&it) { items.push(it); } } }
            if items.is_empty() { continue; }
            let tool = link_tool();
            let (came, problems) = tags::sync(&sync_root, &items, &|f, l| write_link(tool.as_ref(), f, l), &stores);
            if came > 0 || !problems.is_empty() { eprintln!("tags: {came} item(s) changed in Finder taken in; {problems:?}"); }
        }
    });
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(e) = res else { return };
        if matches!(e.kind, notify::EventKind::Access(_)) { return; }
        if matches!(e.kind, notify::EventKind::Modify(notify::event::ModifyKind::Metadata(_)) | notify::EventKind::Modify(notify::event::ModifyKind::Any)) {
            for p in &e.paths { let _ = attrs.send(p.clone()); }
        }
        // Temporary and hidden files (a write through a temporary file shows as its final name too), the NAS's recycle bin
        let ours = e.paths.iter().any(|p| !p.components().any(|c| { let n = c.as_os_str().to_string_lossy(); n.starts_with('.') || n.starts_with('#') })
                                          && !p.to_string_lossy().ends_with(".tmp"));
        if !ours { return; }
        // The catalog looks at those folders, and counts the change once it has it; until it's read, counted here
        let _ = events.send(e.paths.clone());
        if seen::is_on(&me) { return; }
        c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *l.lock().unwrap() = Instant::now();
    }).ok().and_then(|mut w| {
        let mut dirs = vec![root.to_path_buf()];
        for l in locations::locations(root, &locations::mounts()) {
            if let Some(d) = l.dir { if d.is_dir() && !dirs.iter().any(|x| d.starts_with(x)) { dirs.push(d); } }
        }
        let mut any = false;
        for d in dirs { any |= w.watch(&d, notify::RecursiveMode::Recursive).is_ok(); }
        any.then_some(w)
    });
    let stamp = std::fs::metadata(root.join(PROJECT_FILE)).and_then(|m| m.modified()).ok();
    Watch { root: root.to_path_buf(), stamp, locs: locations_key(root), _watcher: watcher, changes, last, runner, reach_changed }
}
