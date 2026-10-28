// SPDX-License-Identifier: GPL-3.0-or-later
//
// A video file for telemetry-parser with what it will read fetched ahead, in parallel: the metadata track's samples.
//
// telemetry-parser reads them one at a time, a seek and a read of ~550 B per frame, spread over the whole file. From a
// local disk that's ~0.1 s for a minute of 4K60; from a NAS over SMB each read waits for the server and the read-ahead
// macOS does behind it, 6-10 ms, so 14-34 s. Here they're read in parallel straight from the sample table, and the
// parser's own reads come from memory. Its big reads (parse_mp4 reads the first and last 15 MB, more than once) are
// kept too, so they cross the network once.

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

pub struct Prefetched {
    file: File,
    len: u64,
    pos: u64,
    /// What's been read, by offset
    chunks: BTreeMap<u64, Vec<u8>>,
}

impl Prefetched {
    /// The file and its size, with the metadata track's samples read
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, usize)> {
        let file = File::open(path)?;
        let size = file.metadata()?.len() as usize;
        let len = size as u64;
        let mut this = Self { file, len, pos: 0, chunks: BTreeMap::new() };
        // The sample table, as telemetry-parser reads it (its reads of the file's ends are kept for it)
        if let Ok(ctx) = telemetry_parser::util::parse_mp4(&mut this, len as usize) {
            this.fetch(merged(metadata_samples(&ctx)));
        }
        this.pos = 0;
        Ok((this, len as usize))
    }

    /// Reads the ranges in parallel. One that fails is left to the parser, which reads it again and reports the error
    fn fetch(&mut self, ranges: Vec<(u64, u64)>) {
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
    }
}

impl Read for Prefetched {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let max = self.len.saturating_sub(self.pos).min(buf.len() as u64) as usize;
        if max == 0 { return Ok(0); }
        let pos = self.pos;
        if let Some((&start, data)) = self.chunks.range(..=pos).next_back() {
            let at = (pos - start) as usize;
            if at < data.len() {
                let n = max.min(data.len() - at);
                buf[..n].copy_from_slice(&data[at..at + n]);
                self.pos += n as u64;
                return Ok(n);
            }
        }
        // Not read ahead: a box header, or something read once
        let n = read_full_at(&self.file, &mut buf[..max], pos)?;
        if n >= KEEP { self.chunks.insert(pos, buf[..n].to_vec()); }
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

/// The byte ranges of the samples telemetry-parser reads for the metadata (get_track_samples): the metadata track's,
/// or the unknown tracks' when there's none
fn metadata_samples(ctx: &mp4parse::MediaContext) -> Vec<(u64, u64)> {
    use mp4parse::TrackType;
    let has = |t| ctx.tracks.iter().any(|x| x.track_type == t);
    let typ = if !has(TrackType::Metadata) && has(TrackType::Unknown) { TrackType::Unknown } else { TrackType::Metadata };
    ctx.tracks.iter().filter(|x| x.track_type == typ && x.timescale.is_some())
        .filter_map(|x| mp4parse::unstable::create_sample_table(x, 0.into()))
        .flatten()
        .map(|s| (s.start_offset.0 as u64, s.end_offset.0 as u64))
        .filter(|&(start, end)| end > start + 4)
        .collect()
}

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
        assert_eq!(f.chunks.len(), 1);
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
}
