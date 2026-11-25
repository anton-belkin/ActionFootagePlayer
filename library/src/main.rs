// SPDX-License-Identifier: GPL-3.0-or-later
//
// Action Footage Library: a project's footage, moments, photos and exports, browsed as folders, searched and filtered
// (index.rs reads them from the project's files), and the commands on what's selected, made in the background: clip
// previews, moments of the marks, photos of the marks, exports of moments. One at a time, in a queue that shows how far
// each is and can stop it. It opens a project, the folder with Project.afproject in it (a double click on that file
// opens it here); what's opened plays in ActionFootagePlayer's own window.
//
// It plays and renders nothing itself: each batch is ActionFootagePlayer run with no window (the workspace plugin's
// batch mode, AFP_BATCH), which makes them as the player does, with its plugins and settings, and writes how far it
// is to a status file this reads. What a batch makes goes into Generated/<date> <time> <what>/, laid out as chosen for
// that run (docs/design/library-management.md §8); nothing of it is kept in Project.afproject.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod albums;
mod changed;
mod checks;
mod geo;
mod index;
// The responsiveness journal (docs/design/responsiveness-journal.md)
mod journal;
mod lists;
mod locations;
mod renders;
mod seen;
mod share;
mod tags;
mod takein;
mod thumbs;

// The page's commands by area, and what they run on: each module's items are the crate's, as when they were
// all here (tauri's generate_handler! below lists them by name)
mod batch;
mod browsing;
mod care;
mod items;
mod lens;
mod moving;
mod playing;
mod project;
mod recent;
mod settings;
mod tagging;
mod watching;

use batch::*;
use browsing::*;
use care::*;
use items::*;
use lens::*;
use moving::*;
use playing::*;
use project::*;
use recent::*;
use settings::*;
use tagging::*;
use watching::*;

use serde::{ Deserialize, Serialize };
use serde_json::{ json, Value };
use std::collections::VecDeque;
use std::path::{ Path, PathBuf };
use std::process::{ Child, Command, Stdio };
use std::sync::{ Arc, Mutex };
use std::time::{ Duration, Instant };
use tauri::{ Emitter, Manager, RunEvent, State, WindowEvent };

const PROJECT_FILE: &str = "Project.afproject";
const LEGACY_DIR: &str = "_ActionFootagePlayer";
const FORMAT: &str = "actionfootage-project";
// No status from the player this long after it starts: it didn't start the batch
const START_TIMEOUT: Duration = Duration::from_secs(40);
// A batch asked to stop that's still running this long after: stopped for good
const STOP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum JobState { Waiting, Starting, Looking, Running, Stopping, Done, Stopped, Failed }

impl JobState {
    fn finished(self) -> bool { matches!(self, JobState::Done | JobState::Stopped | JobState::Failed) }
    fn from_status(s: &str) -> Option<JobState> {
        Some(match s {
            "starting" => JobState::Starting,
            "looking" => JobState::Looking,
            "running" => JobState::Running,
            "stopping" => JobState::Stopping,
            "done" => JobState::Done,
            "stopped" => JobState::Stopped,
            "failed" => JobState::Failed,
            _ => return None,
        })
    }
}

#[derive(Serialize, Clone)]
struct Job {
    id: u64,
    kind: String,
    title: String,
    project: String,
    project_name: String,
    state: JobState,
    done: u64,
    total: u64,
    failed: u64,
    made: Option<u64>,
    fraction: Option<f64>,
    current: Option<String>,
    message: Option<String>,
    failures: Vec<(String, String)>,
    /// What the batch left out, and why (Stabilize optically: clips with their own motion data, a lens not known...)
    left_out: Vec<(String, String)>,
    /// The clips Stabilize optically has made (its status' `made_paths`): left out of it when it runs again after
    /// stepping aside (a move of the project)
    #[serde(skip)]
    made_paths: Vec<String>,
    #[serde(skip)]
    request: Value,
    /// Share: where the selection goes, and each item's state (share.rs)
    share: Option<share::Share>,
    /// Find lens / Estimate lens: what was found for each clip (gyroflow-optical's status "results")
    #[serde(skip_serializing_if = "Option::is_none")]
    results: Option<Value>,
}

// The batch running: the player, the files it talks through, and macOS's caffeinate keeping the Mac awake meanwhile
struct Running {
    id: u64,
    child: Child,
    dir: PathBuf,
    started: Instant,
    stop_asked: Option<Instant>,
    awake: Option<Child>,
    // Stopped to let another job run first, to be queued again
    yielding: bool,
}

#[derive(Default)]
struct Inner {
    project: Option<PathBuf>,
    jobs: Vec<Job>,
    running: Option<Running>,
    next_id: u64,
    // The window's close asked while batches are queued: the page asks whether to stop them
    close_asked: bool,
    // …or while the page's changes are still being written (WRITES_PENDING): the window stays until they are, the
    // page saying so
    close_waits: bool,
    // Projects opened before the page was ready (Finder's double click as the app starts)
    pending_open: VecDeque<PathBuf>,
    // The project's folder and its locations watched: what the player (or Finder, or anyone) changes there is seen
    watch: Option<Watch>,
    // A project being moved: its tasks wait (start_job passes them over), then run from its new place
    held: Option<String>,
    // The project's own files being moved (Move the project…): the page shows how far it is
    moving: Option<Moving>,
    // Moved (its catalog with it): its next watch looks at its folder again before the views read the catalog
    moved_to: Option<PathBuf>,
    // Clips Stabilize optically measured (their .afp-motion beside them): their probe's answer worked out again (it
    // says how they're stabilized now), by the worker that has the probes
    measured: Vec<String>,
}

// How far a move of the project's files is: bytes copied of all (a move on one disk is a rename: nothing to count),
// the file being copied
#[derive(Serialize, Clone, Default)]
struct Moving {
    to: String,
    done: u64,
    total: u64,
    current: String,
}

// The file system's own notices (FSEvents, ReadDirectoryChangesW, inotify) for the project's folder and its locations:
// each change counted, and when it was, so that the page reads the project again once the changes settle. The
// project's file's time is kept too: its locations changed, they're watched again
struct Watch {
    root: PathBuf,
    stamp: Option<std::time::SystemTime>,
    /// Its locations as the project's file had them: watched again (and the catalog read again) only when they change,
    /// not on every write of the file (a setting, the player's lists)
    locs: String,
    _watcher: Option<notify::RecommendedWatcher>,
    changes: Arc<std::sync::atomic::AtomicU64>,
    last: Arc<Mutex<Instant>>,
    /// The project's catalog kept up to date (seen.rs): what's changed here is sent to it, and it counts the changes
    /// once it has them
    runner: seen::Runner,
    /// Set when a location became reachable or stopped being (a share mounted after the Library opened, or taken
    /// away): watched again, and the catalog started again with it, as on opening the project then
    reach_changed: Arc<std::sync::atomic::AtomicBool>,
}

type Shared = Arc<Mutex<Inner>>;

/// How many of the page's changes are still being written (ui/writes.js: a change shows at once and is written after):
/// the window isn't closed under them
static WRITES_PENDING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[tauri::command]
fn writes_pending(n: u64) { WRITES_PENDING.store(n, std::sync::atomic::Ordering::Relaxed); }

/// What a close of the window does: asked about (batches queued), waited with until the page's changes are written,
/// or done. Asked a second time while waiting, it closes (a page that never answers mustn't keep the window)
#[derive(Debug, PartialEq)]
enum Close { Ask, Wait, Now }
fn close_decision(batches: bool, writes: u64, waiting: bool) -> Close {
    if batches { Close::Ask } else if writes > 0 && !waiting { Close::Wait } else { Close::Now }
}

// The window's close, answered: stop the batches and quit, or keep them going (the window stays)
#[tauri::command]
fn answer_close(app: tauri::AppHandle, shared: State<Shared>, quit: bool) {
    let mut inner = shared.lock().unwrap();
    inner.close_asked = false;
    inner.close_waits = false;
    if quit {
        stop_all(&mut inner);
        drop(inner);
        app.exit(0);
    }
}

fn busy(shared: &Shared) -> bool {
    shared.lock().unwrap().jobs.iter().any(|j| !j.state.finished())
}

fn main() {
    // The responsiveness journal: first, so the start is measured from here (its header says when the process began)
    journal::init();
    let shared: Shared = Arc::new(Mutex::new(Inner::default()));
    // A project given on the command line (Explorer's double click)
    for arg in std::env::args_os().skip(1) {
        let p = PathBuf::from(&arg);
        if !arg.to_string_lossy().starts_with('-') {
            if let Some(root) = project_of(&p).filter(|r| usable(r).map_err(|e| eprintln!("{e}")).is_ok()) { shared.lock().unwrap().project = Some(root); }
        }
    }
    let probes: index::SharedProbes = Arc::new(Mutex::new(index::Probes::default()));
    probes.lock().unwrap().tool = index::probe_tool(player().ok().as_deref());
    let probe_worker = probes.clone();
    // What's known of the clips is kept between runs: the app's cache folder, found once Tauri is up (setup below)
    let keep_probes: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
    let keep_for_worker = keep_probes.clone();
    std::thread::spawn(move || {
        let keep = loop {
            if let Some(k) = keep_for_worker.lock().unwrap().clone() { break k; }
            std::thread::sleep(Duration::from_millis(100));
        };
        index::probe_worker(probe_worker, Some(keep))
    });
    let probes_for_setup = probes.clone();
    let worker = shared.clone();
    let probes_for_tick = probes.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(300));
        let measured = { let mut inner = worker.lock().unwrap(); tick(&mut inner); std::mem::take(&mut inner.measured) };
        if !measured.is_empty() { probes_for_tick.lock().unwrap().forget(&measured); }
        share_collect(&worker);
    });

    // A player here wrote something (afp-catalog note tells): the catalog's journals read at once (live.rs)
    let mut live = afp_catalog::live::listen(|_| seen::read_now()).map_err(|e| eprintln!("not listening for the player: {e}")).ok();
    let for_instance = shared.clone();
    let for_window = shared.clone();
    let for_run = shared.clone();
    let app = tauri::Builder::default()
        // The lists shared with the player (registry.json: curves, export choices): window.AFP_LISTS, before the page's
        // scripts run
        .plugin(tauri::plugin::Builder::<tauri::Wry>::new("afp-lists").js_init_script(lists::page_script()).build())
        // A second one started (a project double-clicked in Explorer while this runs): its project opens here
        .plugin(tauri_plugin_single_instance::init(move |app, argv, _cwd| {
            if let Some(p) = argv.iter().skip(1).find(|a| !a.starts_with('-')) {
                opened(&for_instance, app, PathBuf::from(p));
            }
        }))
        .manage(shared)
        .manage(probes)
        .setup(move |app| {
            if let Ok(c) = app.path().app_cache_dir() {
                let f = c.join("probes.json");
                probes_for_setup.lock().unwrap().load(&f);
                *keep_probes.lock().unwrap() = Some(f);
            }
            // Place names looked up on this computer (geo.rs), each once
            let places = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("ActionFootageLibrary")).join("places.json");
            let places: geo::SharedPlaces = Arc::new(Mutex::new(geo::Places::load(places)));
            if let Some(out) = std::env::var_os("AFL_GEO_DUMP") {
                geo::dump_for_test(app.state::<Shared>().inner().clone(), probes_for_setup.clone(), places.clone(), PathBuf::from(out));
            }
            app.manage(places);
            // What the catalog knows changed: the page told at once (it re-reads what it shows)
            let h = app.handle().clone();
            let _ = seen::ON_CHANGE.set(Box::new(move |n| { let _ = h.emit("catalog-changed", n); }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![snapshot, project, connect_location, forget_recent, set_start_project, open_at_start, close_project, connect_project, engine, open_project, choose_project, add_job, cancel_job, set_focus, writes_pending, changes_since, view_items,
                                                 clear_finished, set_setting, reveal, answer_close, recent, scan, probes_since,
                                                 thumb, open_items, open_list, reveal_path, cached_view, quick_look, rename_item, open_in_gyroflow, new_folder, project_settings, save_locations, choose_location,
                                                 choose_local, save_local, clear_cache, move_project, run_check, find_moved, apply_moved,
                                                 choose_other, take_in_plan, take_in_run, set_tags, set_rating, set_pick, set_comment, sync_tags, save_tag_stores, rename_tag, recent_tags, lens_types, set_lens_type, set_source, delete_renders, clear_renders, delete_items,
                                                 export_library, mend, missing_links, catalog_look, catalog_care, geo::photos_access, geo::suggest_locations, geo::set_locations,
                                                 geo::parse_location, geo::places_of, geo::open_map, geo::mini_map, lens_info, save_project_lens, assign_lens, keep_found_lens, lens_choices, try_lens, promote_lens,
                                                 rename_album, albums::album_add, albums::album_move, albums::album_copy, albums::photo_add, share,
                                                 journal::journal_write, journal::journal_hello])
        .on_window_event(move |window, event| {
            // Closed while batches are queued: the page asks first
            if let WindowEvent::CloseRequested { api, .. } = event {
                let waiting = for_window.lock().unwrap().close_waits;
                match close_decision(busy(&for_window), WRITES_PENDING.load(std::sync::atomic::Ordering::Relaxed), waiting) {
                    Close::Ask => {
                        api.prevent_close();
                        for_window.lock().unwrap().close_asked = true;
                        let _ = window.set_focus();
                    }
                    // The page's changes still being written: it closes the window once they are (answer_close)
                    Close::Wait => {
                        api.prevent_close();
                        for_window.lock().unwrap().close_waits = true;
                        let _ = window.emit("close-waits", ());
                    }
                    Close::Now => {}
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("couldn't start Action Footage Library");
    app.run(move |app, event| {
        match event {
            // Finder's double click on a Project.afproject (and files dropped on the app's icon)
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                for url in urls {
                    if let Ok(path) = url.to_file_path() { opened(&for_run, app, path); }
                }
            }
            RunEvent::Exit => { stop_all(&mut for_run.lock().unwrap()); live.take(); }
            _ => {}
        }
    });
}

// Where the time goes on each changing command, measured on a project of thousands of clips (perf.rs)
#[cfg(test)]
mod perf;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/share_e2e.rs"]
mod share_e2e;

#[cfg(test)]
#[path = "tests/export_tests.rs"]
mod export_tests;

#[cfg(test)]
#[path = "tests/trash_tests.rs"]
mod trash_tests;

#[cfg(test)]
#[path = "tests/audit.rs"]
mod audit;

#[cfg(test)]
#[path = "tests/e2e.rs"]
mod e2e;

#[cfg(test)]
#[path = "tests/live_e2e.rs"]
mod live_e2e;

#[cfg(test)]
#[path = "tests/project_version.rs"]
mod project_version;
