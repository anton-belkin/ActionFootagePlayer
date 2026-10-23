// SPDX-License-Identifier: GPL-3.0-or-later

// A Gyroflow project stored inside the MP4/MOV file it belongs to, so the clip doesn't need a `.gyroflow` next to it.
//
// Stock Gyroflow doesn't know about it: `gyroflow-embed` moves a project saved by Gyroflow into the video (and back
// out, to open it in Gyroflow), and the plugin and `gyroflow-probe` read it. Same format as the `embedded_project`
// module of the `embedded-project` branch of anton-belkin/gyroflow, which adds this to the app itself.
//
// The project is a top-level `uuid` box appended at the end of the file: `size | "uuid" | EMBEDDED_PROJECT_UUID | project JSON`.
// Players and editors skip boxes they don't know, and appending doesn't move anything the file's tables point to, so
// saving takes no copy of the video. Saving again replaces the box when it's still the last one; an older one that
// isn't (something was appended after it) is turned into a `free` box in place. Remuxing the file drops the box.
//
// telemetry-parser detects formats from the first and last megabytes of the file, which the box would push the
// camera's metadata out of, so the core reads video files with the box cut off (`Content`).

use std::io::{ Read, Seek, SeekFrom, Write, ErrorKind, Error, Result };
use std::path::Path;
use std::fs::OpenOptions;

pub const EMBEDDED_PROJECT_UUID: [u8; 16] = [0x4B, 0x18, 0xA4, 0xCC, 0xE6, 0x37, 0x43, 0x36, 0xB7, 0xEA, 0x4D, 0xAE, 0xC8, 0xF1, 0x9B, 0xD0];

const HEADER_SIZE: u64 = 8 + 16;

#[derive(Debug, Clone, Copy)]
struct BoxInfo {
    offset: u64,
    header_size: u64,
    size: u64,
    typ: [u8; 4],
    /// Size field is 0: the box extends to the end of the file
    to_eof: bool,
    is_project: bool,
}

/// Top-level boxes of an ISO BMFF (MP4/MOV) file, or None when the data isn't one: the first box must be one a
/// file can start with, and the boxes must cover the file exactly.
fn top_level_boxes<T: Read + Seek>(stream: &mut T, file_size: u64) -> Result<Option<Vec<BoxInfo>>> {
    let mut boxes = Vec::new();
    let mut offset = 0u64;
    while offset < file_size {
        if file_size - offset < 8 { return Ok(None); }
        let mut header = [0u8; 16 + 16];
        stream.seek(SeekFrom::Start(offset))?;
        let available = (file_size - offset).min(header.len() as u64) as usize;
        stream.read_exact(&mut header[..available])?;
        let typ = [header[4], header[5], header[6], header[7]];
        if !typ.iter().all(|c| c.is_ascii_graphic() || *c == b' ') { return Ok(None); }
        if boxes.is_empty() && !matches!(&typ, b"ftyp" | b"wide" | b"free" | b"skip" | b"mdat" | b"moov" | b"pnot") { return Ok(None); }

        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let (size, header_size, to_eof) = match size32 {
            0 => (file_size - offset, 8, true),
            1 => {
                if available < 16 { return Ok(None); }
                (u64::from_be_bytes(header[8..16].try_into().unwrap()), 16, false)
            },
            s => (s, 8, false)
        };
        if size < header_size || size > file_size - offset { return Ok(None); }
        let is_project = &typ == b"uuid" && header_size == 8 && size >= HEADER_SIZE && available >= HEADER_SIZE as usize && header[8..24] == EMBEDDED_PROJECT_UUID;
        boxes.push(BoxInfo { offset, header_size, size, typ, to_eof, is_project });
        offset += size;
    }
    Ok(if boxes.is_empty() { None } else { Some(boxes) })
}

/// Size of the file without the embedded project at its end, which is what telemetry-parser should see.
/// `file_size` when there's none, or the data isn't an MP4/MOV file. Leaves the stream at the start.
pub fn content_size<T: Read + Seek>(stream: &mut T, file_size: usize) -> usize {
    let size = match top_level_boxes(stream, file_size as u64) {
        Ok(Some(boxes)) => boxes.last().filter(|b| b.is_project).map(|b| b.offset as usize).unwrap_or(file_size),
        _ => file_size
    };
    let _ = stream.seek(SeekFrom::Start(0));
    size
}

/// The file as the camera wrote it: the stream ends before the embedded project, also for `SeekFrom::End`,
/// which parsers use to find metadata at the end of the file.
pub struct Content<'a, T: Read + Seek> {
    inner: &'a mut T,
    len: u64,
    pos: u64,
}
impl<'a, T: Read + Seek> Content<'a, T> {
    /// The content and its size
    pub fn new(inner: &'a mut T, file_size: usize) -> (Self, usize) {
        let len = content_size(inner, file_size);
        (Self { inner, len: len as u64, pos: 0 }, len)
    }
}
impl<T: Read + Seek> Read for Content<'_, T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let max = self.len.saturating_sub(self.pos).min(buf.len() as u64) as usize;
        if max == 0 { return Ok(0); }
        let n = self.inner.read(&mut buf[..max])?;
        self.pos += n as u64;
        Ok(n)
    }
}
impl<T: Read + Seek> Seek for Content<'_, T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
        }.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Invalid seek to a negative position"))?;
        self.pos = self.inner.seek(SeekFrom::Start(new_pos))?;
        Ok(self.pos)
    }
}

fn read_from<T: Read + Seek>(stream: &mut T, file_size: u64) -> Result<Option<Vec<u8>>> {
    let Some(boxes) = top_level_boxes(stream, file_size)? else { return Ok(None); };
    let Some(b) = boxes.iter().rev().find(|b| b.is_project) else { return Ok(None); };
    let mut data = vec![0u8; (b.size - HEADER_SIZE) as usize];
    stream.seek(SeekFrom::Start(b.offset + HEADER_SIZE))?;
    stream.read_exact(&mut data)?;
    Ok(Some(data))
}

/// The project JSON embedded in the video at `path`, as it was saved
pub fn read_raw(path: &Path) -> Result<Option<Vec<u8>>> {
    if !is_supported(path) { return Ok(None); }
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    read_from(&mut f, size)
}

/// The project embedded in the video at `path`, pointing at the video where it is now: the file may have been
/// moved or renamed since the project was saved, which is the point of keeping the project inside it.
pub fn read(path: &Path) -> Result<Option<serde_json::Value>> {
    let Some(data) = read_raw(path)? else { return Ok(None); };
    let mut obj: serde_json::Value = serde_json::from_slice(&data)?;
    let Some(map) = obj.as_object_mut() else { return Err(Error::new(ErrorKind::InvalidData, "Embedded project is not a JSON object")); };
    let url = path_to_url(path)?;
    let saved_url = map.get("videofile").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    map.insert("videofile".into(), serde_json::Value::String(url.clone()));
    map.remove("videofile_bookmark");
    if let Some(gyro) = map.get_mut("gyro_source").and_then(|x| x.as_object_mut()) {
        let gyro_url = gyro.get("filepath").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        // Motion data from the video itself moves with it; a separate gyro log stays where it was
        if gyro_url.is_empty() || gyro_url == saved_url || file_name(&gyro_url) == file_name(&saved_url) {
            gyro.insert("filepath".into(), serde_json::Value::String(url));
            gyro.remove("filepath_bookmark");
        }
    }
    Ok(Some(obj))
}

fn path_to_url(path: &Path) -> Result<String> {
    let path = crate::canonicalize(path)?;
    url::Url::from_file_path(&path).map(|u| u.to_string()).map_err(|_| Error::new(ErrorKind::InvalidInput, format!("Can't make a URL of {}", path.display())))
}
fn file_name(url_or_path: &str) -> &str {
    url_or_path.rsplit(['/', '\\']).next().unwrap_or_default()
}

pub fn is_supported(path: &Path) -> bool {
    let ext = path.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "mp4" | "mov" | "m4v" | "insv" | "360" | "lrv" | "lrf")
}

/// Stores `data` (the project JSON) in the video at `path`, replacing a project stored there before.
/// The video data and tables aren't touched, and the file keeps its modification time.
pub fn write(path: &Path, data: &[u8]) -> Result<()> {
    if !is_supported(path) { return Err(Error::new(ErrorKind::Unsupported, "A project can be stored only in MP4 and MOV files")); }
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let size = f.metadata()?.len();
    let modified = f.metadata().and_then(|m| m.modified()).ok();
    write_to(&mut f, size, data)?;
    f.sync_all()?;
    if let Some(modified) = modified {
        let _ = f.set_modified(modified);
    }
    Ok(())
}

/// Removes the project stored in the video at `path`. Returns whether there was one.
pub fn remove(path: &Path) -> Result<bool> {
    if !is_supported(path) { return Ok(false); }
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let size = f.metadata()?.len();
    let modified = f.metadata().and_then(|m| m.modified()).ok();
    let removed = remove_from(&mut f, size)?.is_some();
    f.sync_all()?;
    if let Some(modified) = modified {
        let _ = f.set_modified(modified);
    }
    Ok(removed)
}

/// Removes the projects from the file: a trailing one is cut off, others become `free` boxes.
/// Returns the new file size, or None when there was nothing to remove.
fn remove_from(f: &mut std::fs::File, file_size: u64) -> Result<Option<u64>> {
    let Some(boxes) = top_level_boxes(f, file_size)? else {
        return Err(Error::new(ErrorKind::InvalidData, "Not an MP4/MOV file, or its structure is damaged"));
    };
    let mut new_size = file_size;
    let mut removed = false;
    // Cut off the trailing ones, then neutralize the rest
    for b in boxes.iter().rev() {
        if !b.is_project { break; }
        new_size = b.offset;
    }
    for b in boxes.iter().filter(|b| b.is_project && b.offset < new_size) {
        f.seek(SeekFrom::Start(b.offset + 4))?;
        f.write_all(b"free")?;
        removed = true;
    }
    if new_size < file_size {
        f.set_len(new_size)?;
        removed = true;
    }
    Ok(removed.then_some(new_size))
}

fn write_to(f: &mut std::fs::File, file_size: u64, data: &[u8]) -> Result<()> {
    let box_size = HEADER_SIZE + data.len() as u64;
    if box_size > u32::MAX as u64 { return Err(Error::new(ErrorKind::InvalidInput, "Project is too large")); }

    let file_size = remove_from(f, file_size)?.unwrap_or(file_size);

    // A last box that runs to the end of the file would swallow the project: give it its actual size
    if let Some(last) = top_level_boxes(f, file_size)?.and_then(|b| b.last().copied()) {
        if last.to_eof {
            if last.size > u32::MAX as u64 {
                return Err(Error::new(ErrorKind::Unsupported, format!("The last box of the file ({}) has no size, and it's too large to set one", String::from_utf8_lossy(&last.typ))));
            }
            debug_assert_eq!(last.header_size, 8);
            f.seek(SeekFrom::Start(last.offset))?;
            f.write_all(&(last.size as u32).to_be_bytes())?;
        }
    }

    let mut buf = Vec::with_capacity(box_size as usize);
    buf.extend_from_slice(&(box_size as u32).to_be_bytes());
    buf.extend_from_slice(b"uuid");
    buf.extend_from_slice(&EMBEDDED_PROJECT_UUID);
    buf.extend_from_slice(data);

    f.seek(SeekFrom::Start(file_size))?;
    if let Err(e) = f.write_all(&buf) {
        // Leave the file as it was, without a partial box
        let _ = f.set_len(file_size);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mp4(boxes: &[(&[u8; 4], usize)]) -> Vec<u8> {
        let mut v = Vec::new();
        for (typ, size) in boxes {
            v.extend_from_slice(&(*size as u32).to_be_bytes());
            v.extend_from_slice(*typ);
            v.extend(std::iter::repeat(0xAB).take(size - 8));
        }
        v
    }
    fn temp_file(data: &[u8]) -> (std::path::PathBuf, std::fs::File) {
        let path = std::env::temp_dir().join(format!("gf_embedded_{}_{:?}.mp4", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path, data).unwrap();
        let f = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
        (path, f)
    }

    #[test]
    fn write_replace_remove() {
        let video = mp4(&[(b"ftyp", 24), (b"mdat", 1000), (b"moov", 300)]);
        let (path, mut f) = temp_file(&video);
        let len = || std::fs::metadata(&path).unwrap().len();

        assert_eq!(content_size(&mut f, video.len()), video.len());
        write_to(&mut f, len(), b"{\"a\":1}").unwrap();
        assert_eq!(len(), video.len() as u64 + HEADER_SIZE + 7);
        assert_eq!(read_from(&mut f, len()).unwrap().unwrap(), b"{\"a\":1}");
        assert_eq!(content_size(&mut f, len() as usize), video.len());

        // Replaced in place of the old one
        write_to(&mut f, len(), b"{\"b\":22}").unwrap();
        assert_eq!(len(), video.len() as u64 + HEADER_SIZE + 8);
        assert_eq!(read_from(&mut f, len()).unwrap().unwrap(), b"{\"b\":22}");

        // Something appended after it: the old one becomes free space
        let mut tail = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        tail.write_all(&mp4(&[(b"free", 16)])).unwrap();
        write_to(&mut f, len(), b"{\"c\":3}").unwrap();
        assert_eq!(read_from(&mut f, len()).unwrap().unwrap(), b"{\"c\":3}");
        let boxes = top_level_boxes(&mut f, len()).unwrap().unwrap();
        assert_eq!(boxes.iter().filter(|b| b.is_project).count(), 1);
        assert_eq!(&boxes[3].typ, b"free");

        assert!(remove_from(&mut f, len()).unwrap().is_some());
        assert!(read_from(&mut f, len()).unwrap().is_none());
        assert_eq!(std::fs::read(&path).unwrap()[..video.len()], video[..]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn content_hides_project() {
        let video = mp4(&[(b"ftyp", 24), (b"mdat", 100), (b"moov", 40)]);
        let (path, mut f) = temp_file(&video);
        write_to(&mut f, video.len() as u64, b"{\"a\":1}").unwrap();
        let file_size = std::fs::metadata(&path).unwrap().len() as usize;
        let (mut c, len) = Content::new(&mut f, file_size);
        assert_eq!(len, video.len());
        let mut tail = [0u8; 40];
        c.seek(SeekFrom::End(-40)).unwrap();
        c.read_exact(&mut tail).unwrap();
        assert_eq!(&tail[4..8], b"moov");
        let mut all = Vec::new();
        c.seek(SeekFrom::Start(0)).unwrap();
        c.read_to_end(&mut all).unwrap();
        assert_eq!(all, video);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn mdat_to_eof() {
        let mut video = mp4(&[(b"ftyp", 24), (b"moov", 300), (b"mdat", 1000)]);
        video[324..328].copy_from_slice(&[0, 0, 0, 0]);
        let (path, mut f) = temp_file(&video);
        let len = || std::fs::metadata(&path).unwrap().len();
        write_to(&mut f, len(), b"{}").unwrap();
        let boxes = top_level_boxes(&mut f, len()).unwrap().unwrap();
        assert_eq!(boxes[2].size, 1000);
        assert!(!boxes[2].to_eof);
        assert!(boxes[3].is_project);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn not_mp4() {
        let csv = b"Clock:Tick,IMU_ATTI(0):gyroX\n1,2\n";
        let (path, mut f) = temp_file(csv);
        assert_eq!(content_size(&mut f, csv.len()), csv.len());
        assert!(read_from(&mut f, csv.len() as u64).unwrap().is_none());
        assert!(write_to(&mut f, csv.len() as u64, b"{}").is_err());
        let _ = std::fs::remove_file(path);
    }
}
