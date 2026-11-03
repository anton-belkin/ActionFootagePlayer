// SPDX-License-Identifier: GPL-3.0-or-later
//
// A clip without a project plays from its first seconds of motion data (`START_S` past the look-ahead), and the rest
// comes in as it plays, the stabilization smoothed again with it (levels.rs, smoothing.rs: what's `lookahead` s
// behind the newest motion data is as it is with all of it).
//
// The rest isn't read from the file next to the video: on a NAS any read in between the video's breaks the read-ahead
// macOS does for it (measured over SMB: with 2 to 64 threads reading the samples, the video's 1 MB reads went from
// 0-1 ms to 34-45 ms, and it fell 0.75-11.7 s behind its bitrate). The metadata samples sit in the file next to the
// video of the same moment, and mpv reads the file ahead of what it shows (its cache, demuxer-readahead-secs): so
// they're copied from the file cache, the pages already there (mincore on the file mapped into memory), every
// `TAKE_EVERY` as a burst, which leaves the video's reads as they were (1 ms, measured). Only what the look-ahead needs
// and isn't there yet (after a seek, from a local disk that mpv reads less far ahead) is read from the file, and while
// it's paused the rest is, at full speed, with nothing else reading.

use std::sync::Arc;
use std::sync::atomic::{ AtomicBool, AtomicI64, Ordering };
use std::time::{ Duration, Instant };
use gyroflow_core::StabilizationManager;
use gyroflow_core::gyro_source::{ FileLoadOptions, GyroSource };
use gyroflow_core::stabilization::ComputeParams;
use parking_lot::Mutex;

use crate::levels::{ self, Level };
use crate::water::Underwater;
use crate::prefetch::Prefetched;
use crate::smoothing::Predictive;

/// Motion data read before playback starts, past the look-ahead
pub const START_S: f64 = 4.0;
/// How often the samples in the file cache are taken while it plays
const TAKE_EVERY: Duration = Duration::from_secs(2);
/// How far past the look-ahead they're taken
const AHEAD_S: f64 = 8.0;
/// Paused: no frame asked for in this long
const PAUSED: Duration = Duration::from_millis(1500);
/// Read at once while paused, s of the clip
const PAUSED_BATCH_S: f64 = 10.0;
/// Read from the file before where it plays, when what's there isn't read yet (a seek): the smoothing's start
const WARM_UP_S: f64 = 3.0;
/// Motion data this far apart has nothing between (smoothing.rs's HOLE_S)
const HOLE_US: i64 = 500_000;
/// Where frames played uncorrected for want of motion data (after a seek), how long the correction takes to come in
/// once it's there, instead of in one frame
const FADE_IN_US: i64 = 500_000;

/// What a level's stabilization is made of: the frame's geometry for its margin (levels.rs)
#[derive(Clone, Copy)]
pub struct Geometry { pub still_fov: f64, pub fx: f64, pub output_aspect: f64, pub width: f64 }

impl Geometry {
    pub fn of(stab: &StabilizationManager, still_fov: f64) -> Geometry {
        let p = stab.params.read();
        let fx = stab.lens.read().get_camera_matrix(p.size)[(0, 0)];
        Geometry { still_fov, fx, output_aspect: p.output_size.0 as f64 / p.output_size.1.max(1) as f64, width: p.size.0 as f64 }
    }

    pub fn smoothing(&self, level: Level) -> Predictive {
        Predictive { smoothness: level.smoothness(), margin: levels::margin(level, self.still_fov, self.fx, self.output_aspect, self.width) }
    }
}

/// `gyro`'s path smoothed for `level`, horizon lock and all, as Gyroflow's recompute_smoothness does it. `fade_in`: from
/// where the correction comes in (FADE_IN_US), µs
pub fn smooth_into(stab: &StabilizationManager, gyro: &mut GyroSource, geometry: &Geometry, level: Level, fade_in: Option<i64>) {
    let mut params = ComputeParams::from_manager(stab);
    params.calculate_camera_fovs();
    let horizon_lock = stab.smoothing.read().horizon_lock.clone();
    let (mut quats, max_angles) = gyro.recompute_smoothness(&geometry.smoothing(level), horizon_lock, &params);
    let end_us = (stab.params.read().get_scaled_duration_ms() * 1000.0) as i64;
    if let Some(from) = fade_in {
        for (t, q) in quats.range_mut(from..from + FADE_IN_US) {
            *q = gyroflow_core::gyro_source::Quat64::identity().slerp(q, (*t - from) as f64 / FADE_IN_US as f64);
        }
    }
    unstabilized_where_unknown(&mut quats, end_us);
    gyro.smoothed_quaternions = quats;
    gyro.max_angles = max_angles;
}

/// Where there's no motion data yet (past what's read, or before what's read after a seek), the frame as it was shot:
/// no correction, which Gyroflow would otherwise take from the nearest moment it knows, seconds away. `corrections`:
/// Gyroflow's smoothed quaternions, the correction from the smoothed path to the camera's, by µs
fn unstabilized_where_unknown(corrections: &mut gyroflow_core::gyro_source::TimeQuat, end_us: i64) {
    use gyroflow_core::gyro_source::Quat64;
    let keys: Vec<i64> = corrections.keys().copied().collect();
    let (Some(&first), Some(&last)) = (keys.first(), keys.last()) else { return };
    let none = Quat64::identity();
    if first > HOLE_US { corrections.insert(0, none); corrections.insert(first - 1000, none); }
    for w in keys.windows(2) {
        if w[1] - w[0] > HOLE_US { corrections.insert(w[0] + 1000, none); corrections.insert(w[1] - 1000, none); }
    }
    if end_us - last > HOLE_US { corrections.insert(last + 1000, none); corrections.insert(end_us, none); }
}

/// The stabilization smoothed again for `level`: worked out on a copy, then swapped in, so the frames rendered
/// meanwhile have the one before
pub fn restabilize(stab: &StabilizationManager, geometry: &Geometry, level: Level) {
    let t = Instant::now();
    let mut gyro = stab.gyro.read().clone();
    smooth_into(stab, &mut gyro, geometry, level, None);
    *stab.gyro.write() = gyro;
    log::debug!("Smoothed for {} in {:.0} ms", level.name(), t.elapsed().as_secs_f64() * 1000.0);
}

/// A clip's file and the motion data read from it, kept across the engines that play it (another one for each view
/// of it that needs one: another horizon lock, lens, display size), so they don't read it again
struct ClipFile {
    key: String,
    file: Mutex<Prefetched>,
    len: usize,
    /// The latest frame any engine of the clip was asked for, µs into the clip, and when
    playhead: AtomicI64,
    asked: Mutex<Instant>,
}

static FILES: Mutex<Vec<Arc<ClipFile>>> = Mutex::new(Vec::new());
const MAX_FILES: usize = 2;

/// The clip's motion data as it comes in, shared by an engine (what it plays) and the thread that reads it for it
pub struct Motion {
    clip: Arc<ClipFile>,
    url: String,
    pub lookahead: f64,
    /// When this engine was last asked for a frame: an engine of the clip that isn't playing it leaves the work to the
    /// one that is
    asked: Mutex<Instant>,
    stop: AtomicBool,
    /// The lens under water as the player has it, and the clip: Auto follows the camera's In water / Out of water as
    /// far as it's read (water.rs)
    underwater: Mutex<Option<(Underwater, String)>>,
}

impl Motion {
    /// The clip's file with the motion data of its first `lookahead` + START_S seconds read, for load_video_file; or as
    /// much as an engine before this one read of it. A `lookahead` of 0: all of it
    pub fn open(path: &str, url: &str, lookahead: f64) -> std::io::Result<(Arc<Motion>, usize)> {
        let meta = std::fs::metadata(path)?;
        let key = format!("{path}|{}|{:?}", meta.len(), meta.modified().ok());
        let mut files = FILES.lock();
        let clip = match files.iter().position(|f| f.key == key) {
            Some(i) => files.remove(i),
            None => {
                let until = (lookahead > 0.0).then_some(lookahead + START_S);
                let (file, len) = Prefetched::open_until(path, until)?;
                let read = (0..file.samples().len()).filter(|&i| file.is_read(i)).count();
                match until {
                    Some(until) => log::info!("{path}: motion data of the first {until:.0} s read ({read} of {} samples), the rest as it plays", file.samples().len()),
                    None => log::info!("{path}: motion data read ({read} samples)"),
                }
                Arc::new(ClipFile { key, file: Mutex::new(file), len, playhead: AtomicI64::new(0), asked: Mutex::new(Instant::now()) })
            }
        };
        files.push(clip.clone());
        while files.len() > MAX_FILES { files.remove(0); }
        drop(files);
        if lookahead <= 0.0 {
            // All of it: what an engine that plays it as it comes in hasn't read yet
            let mut file = clip.file.lock();
            let unread: Vec<usize> = (0..file.samples().len()).filter(|&i| !file.is_read(i)).collect();
            if !unread.is_empty() { file.fetch(&unread); }
        }
        let len = clip.len;
        Ok((Arc::new(Motion { clip, url: url.to_string(), lookahead, asked: Mutex::new(Instant::now()), stop: AtomicBool::new(false), underwater: Mutex::new(None) }), len))
    }

    pub fn set_underwater(&self, underwater: Underwater, video: &str) {
        *self.underwater.lock() = Some((underwater, video.to_string()));
    }

    /// The lens under water has been set from it before (water.rs)
    pub fn water_known(&self) -> bool { self.underwater.lock().is_some() }

    /// For load_video_file: the file as read so far, from the start
    pub fn with_file<R>(&self, f: impl FnOnce(&mut Prefetched) -> R) -> R {
        let mut file = self.clip.file.lock();
        file.rewind();
        f(&mut file)
    }

    /// A frame asked for (Engine::process): where playback is
    pub fn asked(&self, timestamp_us: i64) {
        self.clip.playhead.store(timestamp_us, Ordering::Relaxed);
        let now = Instant::now();
        *self.clip.asked.lock() = now;
        *self.asked.lock() = now;
    }

    pub fn stop(&self) { self.stop.store(true, Ordering::Relaxed); }

    /// The thread that reads the rest, adds it to `stab`'s motion data and smooths it again; and smooths again when the
    /// level changes (`level_now`, the control file's)
    pub fn run(self: Arc<Motion>, stab: Arc<StabilizationManager>, geometry: Geometry, mut level: Level, level_now: impl Fn() -> Level) {
        let mapped = Mapped::new(&self.clip.file.lock());
        let (mut taken, mut parsed) = (Instant::now(), Instant::now());
        let mut parse_cost = Duration::ZERO;
        // What's read and not in this engine's motion data yet: an engine of the clip made after this one may have read some
        let mut new_data = false;
        let mut known = { let f = self.clip.file.lock(); (0..f.samples().len()).filter(|&i| !f.is_read(i)).count() };
        // The samples in this engine's motion data (what was read when it was parsed last), and their times
        let (times, mut in_engine): (Vec<f64>, Vec<bool>) = {
            let f = self.clip.file.lock();
            (f.samples().iter().map(|s| s.time).collect(), (0..f.samples().len()).map(|i| f.is_read(i)).collect())
        };
        // How far past `head` this engine's motion data goes without a gap
        let mut fade_in = None;
        let known_until = |in_engine: &[bool], head: f64| (0..times.len())
            .filter(|&i| !in_engine[i] && times[i] >= head).map(|i| times[i]).fold(f64::INFINITY, f64::min);
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if self.stop.load(Ordering::Relaxed) { break; }
            let now_level = level_now();
            if now_level != level {
                level = now_level;
                if !new_data { restabilize(&stab, &geometry, level); }
            }
            let head = self.clip.playhead.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            // Another engine of the clip plays it: it reads for both, and this one catches up if it's played again
            let idle = self.asked.lock().elapsed() >= PAUSED;
            let paused = self.clip.asked.lock().elapsed() >= PAUSED;
            if idle && !paused { continue; }
            let unread_now = { let f = self.clip.file.lock(); (0..f.samples().len()).filter(|&i| !f.is_read(i)).count() };
            if unread_now < known { new_data = true; }
            known = unread_now;
            let complete = unread_now == 0;
            if complete && !new_data { continue; }

            // Less ahead than the look-ahead needs (soon after it starts, before mpv read far, or after a seek): now,
            // not at the next turn
            let short = !paused && known_until(&in_engine, head) < head + self.lookahead + 1.0;
            if !complete && (paused || short || taken.elapsed() >= TAKE_EVERY) {
                taken = Instant::now();
                let mut file = self.clip.file.lock();
                let unread: Vec<usize> = (0..file.samples().len()).filter(|&i| !file.is_read(i)).collect();
                let times: Vec<f64> = file.samples().iter().map(|s| s.time).collect();
                let time = |i: usize| times[i];
                let before = unread.len();
                if paused {
                    // From where it's paused on, then what's before it
                    let mut next: Vec<usize> = unread.iter().copied().filter(|&i| time(i) >= head && time(i) < head + PAUSED_BATCH_S).collect();
                    if next.is_empty() { next = unread.iter().copied().filter(|&i| time(i) >= head).take(600).collect(); }
                    if next.is_empty() { next = unread.iter().copied().take(600).collect(); }
                    file.fetch(&next);
                } else {
                    let wanted: Vec<usize> = unread.iter().copied().filter(|&i| time(i) >= head - WARM_UP_S && time(i) < head + self.lookahead + AHEAD_S).collect();
                    let (cached, missing): (Vec<usize>, Vec<usize>) = match mapped.as_ref() {
                        Some(m) => wanted.iter().partition(|&&i| { let s = file.samples()[i]; m.resident(s.start, s.end) }),
                        None => (Vec::new(), wanted.clone()),
                    };
                    if let Some(m) = mapped.as_ref() { file.copy_from(&cached, m.bytes()); }
                    // What the look-ahead needs now and the cache doesn't have yet
                    let urgent: Vec<usize> = missing.into_iter().filter(|&i| time(i) >= head - WARM_UP_S && time(i) < head + self.lookahead + 2.0).collect();
                    if !urgent.is_empty() {
                        log::debug!("Motion data: {} samples read from the file for {head:.1}-{:.1} s, not in the file cache", urgent.len(), head + self.lookahead + 2.0);
                        file.fetch(&urgent);
                    }
                }
                known = (0..file.samples().len()).filter(|&i| !file.is_read(i)).count();
                if known < before { new_data = true; }
            }
            // Parsed again once a while: it goes over all that's read, which takes longer as it grows
            let complete = known == 0;
            if new_data && (complete || short || parsed.elapsed() >= TAKE_EVERY.max(parse_cost * 4)) {
                let t = Instant::now();
                let before = self.clip.file.lock().from_file;
                // Frames played uncorrected until now: the correction comes in from here
                if known_until(&in_engine, head) <= head + 0.1 { fade_in = Some((head * 1e6) as i64); }
                in_engine = { let f = self.clip.file.lock(); (0..f.samples().len()).map(|i| f.is_read(i)).collect() };
                match self.update(&stab, &geometry, level, fade_in) {
                    Ok(()) => {
                        let after = self.clip.file.lock().from_file;
                        log::debug!("Motion data {} parsed again in {:.0} ms, {} reads ({} kB) from the file", if complete { "all read," } else { "so far" },
                            t.elapsed().as_secs_f64() * 1000.0, after.0 - before.0, (after.1 - before.1) / 1000);
                    }
                    Err(e) => log::error!("Motion data: {e}"),
                }
                parse_cost = t.elapsed();
                parsed = Instant::now();
                new_data = false;
                if complete { log::info!("{}: all motion data read", filesystem_path(&self.url)); }
            }
        }
    }

    /// The motion data read so far parsed again, transformed and smoothed as the clip's, and swapped in
    fn update(&self, stab: &StabilizationManager, geometry: &Geometry, level: Level, fade_in: Option<i64>) -> Result<(), String> {
        let (fps, size, duration_ms) = { let p = stab.params.read(); (p.fps, p.size, p.get_scaled_duration_ms()) };
        let md = {
            let mut file = self.clip.file.lock();
            file.rewind();
            GyroSource::parse_telemetry_file(&mut *file, self.clip.len, &self.url, &FileLoadOptions::default(), size, fps, |_| (), Arc::new(AtomicBool::new(false)))
                .map_err(|e| format!("{e:?}"))?
        };
        // Taken out first: water.rs asks for it again
        let underwater = self.underwater.lock().clone();
        if let Some((underwater @ Underwater::Auto, video)) = underwater {
            crate::water::apply(stab, underwater, &video, Some(self));
        }
        let mut gyro = stab.gyro.read().clone();
        // load_from_telemetry starts the transforms and offsets over: they're the preset's
        let (transforms, offsets) = (gyro.imu_transforms.clone(), gyro.get_offsets().clone());
        gyro.duration_ms = duration_ms;
        gyro.load_from_telemetry(md);
        gyro.imu_transforms = transforms;
        gyro.set_offsets(offsets);
        gyro.apply_transforms();
        smooth_into(stab, &mut gyro, geometry, level, fade_in);
        *stab.gyro.write() = gyro;
        Ok(())
    }
}

fn filesystem_path(url: &str) -> String { gyroflow_core::filesystem::url_to_path(url) }

/// The file mapped into memory, read only for the pages already in the file cache (`resident`). Not on Windows: there
/// what the look-ahead needs is read from the file
#[cfg(unix)]
struct Mapped { ptr: *mut libc::c_void, len: usize, page: usize }
#[cfg(unix)]
unsafe impl Send for Mapped {}

#[cfg(not(unix))]
struct Mapped;
#[cfg(not(unix))]
impl Mapped {
    fn new(_: &Prefetched) -> Option<Mapped> { None }
    fn bytes(&self) -> &[u8] { &[] }
    fn resident(&self, _: u64, _: u64) -> bool { false }
}

#[cfg(unix)]
impl Mapped {
    fn new(file: &Prefetched) -> Option<Mapped> {
        use std::os::fd::AsRawFd;
        let len = file.file().metadata().ok()?.len() as usize;
        if len == 0 { return None; }
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, file.file().as_raw_fd(), 0) };
        if ptr == libc::MAP_FAILED { log::warn!("Motion data: the file can't be mapped, read from it instead"); return None; }
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as usize;
        Some(Mapped { ptr, len, page })
    }

    fn bytes(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.len) } }

    /// Every page of `start..end` is in memory
    fn resident(&self, start: u64, end: u64) -> bool {
        let (start, end) = (start as usize, (end as usize).min(self.len));
        if start >= end { return false; }
        let first = start / self.page * self.page;
        let pages = (end - first).div_ceil(self.page);
        let mut vec = vec![0 as libc::c_char; pages];
        let r = unsafe { libc::mincore((self.ptr as *mut u8).add(first) as *mut libc::c_void, end - first, vec.as_mut_ptr()) };
        r == 0 && vec.iter().all(|v| (*v as i32) & 1 != 0)
    }
}

#[cfg(unix)]
impl Drop for Mapped {
    fn drop(&mut self) { unsafe { libc::munmap(self.ptr, self.len); } }
}
