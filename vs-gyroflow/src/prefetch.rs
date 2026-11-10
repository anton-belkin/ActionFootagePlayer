// SPDX-License-Identifier: GPL-3.0-or-later
//
// A video file for telemetry-parser with what it will read fetched ahead, in parallel: the metadata track's samples.
//
// telemetry-parser reads them one at a time, a seek and a read of ~550 B per frame, spread over the whole file. From a
// local disk that's ~0.1 s for a minute of 4K60; from a NAS over SMB each read waits for the server and the read-ahead
// macOS does behind it, 6-10 ms, so 14-34 s. Here they're read in parallel straight from the sample table, and the
// parser's own reads come from memory. Its big reads (parse_mp4 reads the first and last 15 MB, more than once) are
// kept too, so they cross the network once.
//
// Or only the samples of the clip's first seconds (`open_until`), or of a stretch of it (`open_range`: a moment's):
// the ones not read yet read as zeros, which telemetry-parser can't decode and skips, so Gyroflow gets the motion data
// of those seconds alone. The rest is read as the clip plays (motion.rs), or after (gpu_table.rs's server), and the
// parser goes over it all again from memory.
//
// On a share (SMB, NFS) the samples aren't read one by one but the file in stretches of STRETCH, the samples kept:
// each sample read alone is a seek on the NAS's disks, and a clip's ~550 B every ~190 KB (4K60) cold from them went
// at ~320 reads a second whatever the threads (2.5 GB, 13 000 samples: 40 s, macOS's read-ahead on or off), the
// same part of the file read through in 8 MB reads 2.8 times faster (a 24 s stretch: 4.8 s against 1.7 s, measured
// 2026-11-10 on the UGREEN NAS). From a local disk one by one stays (~0.1 s a minute of 4K60).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ Read, Seek, SeekFrom, Error, ErrorKind, Result };
use std::path::Path;
use std::sync::atomic::{ AtomicUsize, Ordering };
use gyroflow_core::telemetry_parser;

/// Reads at once: over SMB to a NAS each takes ~0.5 ms of its time with 64 (~0.9 with 16), more don't make it faster
const THREADS: usize = 64;
/// Samples this close together are read as one
const GAP: u64 = 16 * 1024;
/// A read the parser makes this big is kept
const KEEP: usize = 1024 * 1024;
/// On a share: samples this close together read as one stretch of the file, in reads of STRETCH, by STRETCH_THREADS
const SHARE_GAP: u64 = 2 * 1024 * 1024;
const STRETCH: u64 = 8 * 1024 * 1024;
const STRETCH_THREADS: usize = 8;

pub struct Prefetched {
    file: File,
    len: u64,
    pos: u64,
    /// The samples read ahead, by offset
    chunks: BTreeMap<u64, Vec<u8>>,
    /// The parser's big reads, by offset: apart, as one can start before a sample and end after it
    kept: BTreeMap<u64, Vec<u8>>,
    /// The metadata track's samples, by offset, and whether each is read
    samples: Vec<Sample>,
    read: Vec<bool>,
    /// The samples not read yet read as zeros (`open_until`), not from the file
    blank_unread: bool,
    /// Reads the parser made from the file, and their bytes: what wasn't read ahead
    pub from_file: (usize, u64),
    /// On a share: the samples read in stretches of the file (see the top)
    share: bool,
}

/// A metadata sample: its bytes in the file, and its time in the clip, s
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample { pub start: u64, pub end: u64, pub time: f64 }

impl Prefetched {
    /// The file and its size, with the metadata track's samples read
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, usize)> {
        Self::open_until(path, None)
    }

    /// The file and its size, with the metadata track's samples up to `until` s into the clip read, all without it.
    /// With it, the others read as zeros until they're read (`fetch`, `copy_from`)
    pub fn open_until(path: impl AsRef<Path>, until: Option<f64>) -> Result<(Self, usize)> {
        Self::open_range(path, until.map(|u| (f64::NEG_INFINITY, u)))
    }

    /// The same with the samples from `range.0` to `range.1` s into the clip read, all without it
    pub fn open_range(path: impl AsRef<Path>, range: Option<(f64, f64)>) -> Result<(Self, usize)> {
        let file = File::open(path)?;
        let size = file.metadata()?.len() as usize;
        let len = size as u64;
        let share = on_share(&file);
        let mut this = Self { file, len, pos: 0, chunks: BTreeMap::new(), kept: BTreeMap::new(), samples: Vec::new(), read: Vec::new(), blank_unread: false, from_file: (0, 0), share };
        // The sample table, as telemetry-parser reads it (its reads of the file's ends are kept for it)
        if let Ok(ctx) = telemetry_parser::util::parse_mp4(&mut this, len as usize) {
            this.samples = metadata_samples(&ctx);
            this.read = vec![false; this.samples.len()];
            let wanted = this.within(range);
            this.fetch(&wanted);
            this.blank_unread = range.is_some();
        }
        this.pos = 0;
        Ok((this, len as usize))
    }

    /// The samples (indices) from `range.0` to `range.1` s into the clip, all without it
    pub fn within(&self, range: Option<(f64, f64)>) -> Vec<usize> {
        (0..self.samples.len()).filter(|&i| range.is_none_or(|(a, b)| self.samples[i].time >= a && self.samples[i].time <= b)).collect()
    }

    /// The metadata samples, by offset
    pub fn samples(&self) -> &[Sample] { &self.samples }

    pub fn is_read(&self, i: usize) -> bool { self.read[i] }

    /// Every sample is read
    pub fn complete(&self) -> bool { self.read.iter().all(|r| *r) }

    pub fn file(&self) -> &File { &self.file }

    pub fn len(&self) -> usize { self.len as usize }

    /// For the parser to read from the start again
    pub fn rewind(&mut self) { self.pos = 0; }

    /// Reads the samples `wanted` (indices into `samples`) in parallel, the ones close together as one; on a share
    /// in stretches of the file
    pub fn fetch(&mut self, wanted: &[usize]) {
        let ranges = merged(wanted.iter().map(|&i| (self.samples[i].start, self.samples[i].end)).collect());
        if self.share { self.fetch_stretches(ranges, STRETCH_THREADS) } else { self.fetch_ranges(ranges) }
    }

    /// The same, at most `threads` reads at once: from a share as a clip plays, leaving its video's reads room
    pub fn fetch_gently(&mut self, wanted: &[usize], threads: usize) {
        let ranges = merged(wanted.iter().map(|&i| (self.samples[i].start, self.samples[i].end)).collect());
        if self.share { self.fetch_stretches(ranges, threads.max(1)) } else { self.fetch_ranges(ranges) }
    }

    /// The ranges read in stretches of the file (STRETCH at a time, `threads` at once), each range taken from its
    /// stretch: a share's disks read a file through faster than they seek to each sample
    fn fetch_stretches(&mut self, ranges: Vec<(u64, u64)>, threads: usize) {
        let pieces = stretches(&ranges, SHARE_GAP, STRETCH);
        let next = AtomicUsize::new(0);
        let (file, pieces, ranges) = (&self.file, &pieces, &ranges);
        let chunks: Vec<(u64, Vec<u8>)> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads.min(pieces.len())).map(|_| s.spawn(|| {
                let mut read = Vec::new();
                while let Some(&(from, to, a, b)) = pieces.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let mut buf = vec![0u8; (to - from) as usize];
                    if read_exact_at(file, &mut buf, from).is_err() { continue; }
                    for &(start, end) in &ranges[a..b] {
                        read.push((start, buf[(start - from) as usize..(end - from) as usize].to_vec()));
                    }
                }
                read
            })).collect();
            workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
        });
        self.chunks.extend(chunks);
        self.mark_read();
    }

    /// The samples `wanted` copied from `mapped`, the file as mapped into memory (motion.rs: its pages that are in the
    /// file cache already, which the video's reads put there)
    pub fn copy_from(&mut self, wanted: &[usize], mapped: &[u8]) {
        for &i in wanted {
            let s = self.samples[i];
            if let Some(bytes) = mapped.get(s.start as usize..s.end as usize) {
                self.chunks.insert(s.start, bytes.to_vec());
            }
        }
        self.mark_read();
    }

    /// Reads the ranges in parallel. One that fails is left to the parser, which reads it again and reports the error
    fn fetch_ranges(&mut self, ranges: Vec<(u64, u64)>) {
        let next = AtomicUsize::new(0);
        let (file, ranges) = (&self.file, &ranges);
        let chunks: Vec<(u64, Vec<u8>)> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..THREADS.min(ranges.len())).map(|_| s.spawn(|| {
                let mut read = Vec::new();
                while let Some(&(start, end)) = ranges.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let mut buf = vec![0u8; (end - start) as usize];
                    if read_exact_at(file, &mut buf, start).is_ok() { read.push((start, buf)); }
                }
                read
            })).collect();
            workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
        });
        self.chunks.extend(chunks);
        self.mark_read();
    }

    /// The samples the chunks have all of
    fn mark_read(&mut self) {
        for i in 0..self.samples.len() {
            if self.read[i] { continue; }
            let s = self.samples[i];
            self.read[i] = self.chunks.range(..=s.start).next_back().is_some_and(|(&at, data)| at + data.len() as u64 >= s.end);
        }
    }

    /// The unread sample `pos` is in, as zeros: its end
    fn blank_at(&self, pos: u64) -> Option<u64> {
        if !self.blank_unread { return None; }
        let i = self.samples.partition_point(|s| s.start <= pos).checked_sub(1)?;
        let s = self.samples[i];
        (pos < s.end && !self.read[i]).then_some(s.end)
    }
}

impl Prefetched {
    /// What's been read from `pos` on, in the samples or the big reads
    fn cached(&self, pos: u64) -> Option<&[u8]> {
        [&self.chunks, &self.kept].into_iter().find_map(|map| {
            let (&start, data) = map.range(..=pos).next_back()?;
            data.get((pos - start) as usize..).filter(|rest| !rest.is_empty())
        })
    }
}

impl Read for Prefetched {
    /// Fills `buf` up to the end of the file, as a file does: telemetry-parser reads the file's ends with one read()
    /// and takes what it gets for all there is (read_beginning_and_end), so a read stopped at a sample's end leaves it
    /// without the camera's tags
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let max = self.len.saturating_sub(self.pos).min(buf.len() as u64) as usize;
        let mut n = 0;
        while n < max {
            let pos = self.pos + n as u64;
            if let Some(data) = self.cached(pos) {
                let k = (max - n).min(data.len());
                buf[n..n + k].copy_from_slice(&data[..k]);
                n += k;
                continue;
            }
            if let Some(end) = self.blank_at(pos) {
                let k = ((end - pos) as usize).min(max - n);
                buf[n..n + k].fill(0);
                n += k;
                continue;
            }
            // Not read ahead: a box header, or something read once
            let k = read_full_at(&self.file, &mut buf[n..max], pos)?;
            self.from_file = (self.from_file.0 + 1, self.from_file.1 + k as u64);
            if k >= KEEP { self.kept.insert(pos, buf[n..n + k].to_vec()); }
            n += k;
            break;
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Prefetched {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
        }.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Invalid seek to a negative position"))?;
        self.pos = new_pos;
        Ok(new_pos)
    }
}

/// The samples telemetry-parser reads for the metadata (get_track_samples): the metadata track's, or the unknown
/// tracks' when there's none; by offset
fn metadata_samples(ctx: &mp4parse::MediaContext) -> Vec<Sample> {
    use mp4parse::TrackType;
    let has = |t| ctx.tracks.iter().any(|x| x.track_type == t);
    let typ = if !has(TrackType::Metadata) && has(TrackType::Unknown) { TrackType::Unknown } else { TrackType::Metadata };
    let mut samples: Vec<Sample> = ctx.tracks.iter().filter(|x| x.track_type == typ && x.timescale.is_some())
        .filter_map(|x| {
            let scale = x.timescale?.0.max(1) as f64;
            Some(mp4parse::unstable::create_sample_table(x, 0.into())?.into_iter()
                .map(move |s| Sample { start: s.start_offset.0 as u64, end: s.end_offset.0 as u64, time: s.start_composition.0 as f64 / scale }))
        })
        .flatten()
        .filter(|s| s.end > s.start + 4)
        .collect();
    samples.sort_by_key(|s| s.start);
    samples
}

/// The stretches of the file that `ranges` (in order, apart) are read in: the ranges less than `gap` apart in one, each
/// at most `max` long unless a range alone is longer; (from, to, the first range in it, one past the last)
fn stretches(ranges: &[(u64, u64)], gap: u64, max: u64) -> Vec<(u64, u64, usize, usize)> {
    let mut out: Vec<(u64, u64, usize, usize)> = Vec::new();
    for (i, &(start, end)) in ranges.iter().enumerate() {
        match out.last_mut() {
            Some(last) if start <= last.1 + gap && end - last.0 <= max => { last.1 = end; last.3 = i + 1; }
            _ => out.push((start, end, i, i + 1)),
        }
    }
    out
}

/// Whether the file is on a share (SMB, NFS, AFP, WebDAV), where reading the file through beats seeking to each sample
#[cfg(target_os = "macos")]
fn on_share(file: &File) -> bool {
    use std::os::fd::AsRawFd;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(file.as_raw_fd(), &mut st) } != 0 { return false; }
    let name: Vec<u8> = st.f_fstypename.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    matches!(name.as_slice(), b"smbfs" | b"nfs" | b"afpfs" | b"webdav")
}
#[cfg(target_os = "linux")]
fn on_share(file: &File) -> bool {
    use std::os::fd::AsRawFd;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(file.as_raw_fd(), &mut st) } != 0 { return false; }
    // SMB, CIFS, SMB2, NFS
    matches!(st.f_type as i64, 0x517B | 0xFF534D42 | 0xFE534D42 | 0x6969)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn on_share(_file: &File) -> bool { false }

/// The ranges in order, with the ones less than GAP apart joined
fn merged(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match out.last_mut() {
            Some(last) if start <= last.1 + GAP => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// As much of `buf` as the file has from `offset`
fn read_full_at(file: &File, buf: &mut [u8], offset: u64) -> Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        #[cfg(unix)]
        let r = std::os::unix::fs::FileExt::read_at(file, &mut buf[n..], offset + n as u64);
        #[cfg(windows)]
        let r = std::os::windows::fs::FileExt::seek_read(file, &mut buf[n..], offset + n as u64);
        match r {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == ErrorKind::Interrupted => {},
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> Result<()> {
    if read_full_at(file, buf, offset)? == buf.len() { Ok(()) } else { Err(ErrorKind::UnexpectedEof.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretches_hold_whole_ranges() {
        let r = vec![(0, 500), (190_000, 190_500), (380_000, 380_500), (5_000_000, 5_000_500), (5_100_000, 5_100_500)];
        // Close together in one, a jump past the gap starts another
        assert_eq!(stretches(&r, 1_000_000, 8_000_000), vec![(0, 380_500, 0, 3), (5_000_000, 5_100_500, 3, 5)]);
        // Not longer than max: split between ranges
        assert_eq!(stretches(&r, 10_000_000, 200_000), vec![(0, 190_500, 0, 2), (380_000, 380_500, 2, 3), (5_000_000, 5_100_500, 3, 5)]);
    }

    #[test]
    fn reads_stretches_as_samples() {
        let path = std::env::temp_dir().join(format!("vs-gyroflow-prefetch-stretch-{}.bin", std::process::id()));
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8 + 1).collect();
        std::fs::write(&path, &data).unwrap();
        let (mut f, _) = Prefetched::open(&path).unwrap();
        f.samples = (0..10).map(|i| Sample { start: 100_000 + i * 250_000, end: 100_600 + i * 250_000, time: i as f64 }).collect();
        f.read = vec![false; 10];
        f.blank_unread = true;
        f.share = true;
        // A stretch of the clip: 3 to 6 s
        let w = f.within(Some((3.0, 6.0)));
        assert_eq!(w, vec![3, 4, 5, 6]);
        f.fetch_gently(&w, 2);
        assert!((0..10).all(|i| f.is_read(i) == (3..=6).contains(&i)));
        let mut buf = vec![0u8; 600];
        f.seek(SeekFrom::Start(100_000 + 4 * 250_000)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[1_100_000..1_100_600]);
        // Only the samples kept, not the stretches read
        assert!(f.chunks.values().map(|c| c.len()).sum::<usize>() <= 4 * 600);
        f.seek(SeekFrom::Start(100_000 + 8 * 250_000)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert!(buf.iter().all(|b| *b == 0));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn merges_close_ranges() {
        assert_eq!(merged(vec![(100_000, 100_548), (0, 548), (600, 1000)]), vec![(0, 1000), (100_000, 100_548)]);
    }

    #[test]
    fn reads_what_a_file_reads() {
        let path = std::env::temp_dir().join(format!("vs-gyroflow-prefetch-{}.bin", std::process::id()));
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &data).unwrap();
        let (mut f, len) = Prefetched::open(&path).unwrap();
        assert_eq!(len, data.len());
        // A big read is kept, and read back from memory
        let mut big = vec![0u8; 2_000_000];
        f.seek(SeekFrom::Start(500_000)).unwrap();
        f.read_exact(&mut big).unwrap();
        assert_eq!(big, data[500_000..2_500_000]);
        assert_eq!(f.kept.len(), 1);
        // Across the kept chunk's end, and from the end
        let mut buf = vec![0u8; 1000];
        f.seek(SeekFrom::Start(2_499_500)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[2_499_500..2_500_500]);
        f.seek(SeekFrom::End(-10)).unwrap();
        let mut tail = Vec::new();
        f.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, data[data.len() - 10..]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn samples_not_read_yet_read_as_zeros() {
        // open_until: telemetry-parser gets the seconds read so far, and zeros it skips for the rest
        let path = std::env::temp_dir().join(format!("vs-gyroflow-prefetch-blank-{}.bin", std::process::id()));
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8 + 1).collect();
        std::fs::write(&path, &data).unwrap();
        let (mut f, _) = Prefetched::open(&path).unwrap();
        f.samples = vec![Sample { start: 100_000, end: 100_600, time: 0.0 }, Sample { start: 200_000, end: 200_600, time: 8.0 }];
        f.read = vec![false; 2];
        f.blank_unread = true;
        f.fetch(&[0]);
        assert!(f.is_read(0) && !f.is_read(1) && !f.complete());
        let mut buf = vec![0u8; 600];
        f.seek(SeekFrom::Start(100_000)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[100_000..100_600]);
        f.seek(SeekFrom::Start(200_000)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert!(buf.iter().all(|b| *b == 0));
        // Read later, as from the file cache: then it's the file's
        f.copy_from(&[1], &data);
        assert!(f.complete());
        f.seek(SeekFrom::Start(200_000)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[200_000..200_600]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_past_a_sample_in_one_read() {
        // Clip 0104: a sample read ahead started just before the file's last 5 MB, and the camera's detection got
        // only the rest of the sample
        let path = std::env::temp_dir().join(format!("vs-gyroflow-prefetch-sample-{}.bin", std::process::id()));
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &data).unwrap();
        let (mut f, _) = Prefetched::open(&path).unwrap();
        f.chunks.insert(999_000, data[999_000..1_004_000].to_vec());
        let mut buf = vec![0u8; 1_500_000];
        f.seek(SeekFrom::Start(1_000_000)).unwrap();
        assert_eq!(f.read(&mut buf).unwrap(), buf.len());
        assert_eq!(buf, data[1_000_000..2_500_000]);
        // A big read that covers a sample isn't hidden by it
        f.seek(SeekFrom::Start(1_002_000)).unwrap();
        assert_eq!(f.read(&mut buf[..100_000]).unwrap(), 100_000);
        assert_eq!(buf[..100_000], data[1_002_000..1_102_000]);
        std::fs::remove_file(&path).unwrap();
    }
}
