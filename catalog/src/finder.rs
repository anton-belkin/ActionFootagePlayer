// SPDX-License-Identifier: GPL-3.0-or-later
//
// macOS Finder tags, written beside the library's own (docs/library-management.md §7): the extended attribute
// com.apple.metadata:_kMDItemUserTags, a binary property list, an array of strings ("Name", or "Name\n6" with Finder's
// colour). The library's tags (a clip's own file, a link, a photo's XMP) are the truth; Finder's follow them: the ones
// taken off go, the library's are put on, and a tag set by hand in Finder that the library doesn't have stays. The
// player's twin is mpv/scripts/workspace/tags.lua (through /usr/bin/xattr). Elsewhere than on a Mac: nothing.

use std::path::Path;

pub const XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

fn be(v: u64, n: usize) -> Vec<u8> { (0..n).rev().map(|k| (v >> (8 * k)) as u8).collect() }
fn bytes_for(v: u64) -> usize { if v < 0x100 { 1 } else if v < 0x10000 { 2 } else if v < 0x1_0000_0000 { 4 } else { 8 } }
fn marker(kind: u8, count: usize) -> Vec<u8> {
    if count < 15 { return vec![kind << 4 | count as u8]; }
    let n = bytes_for(count as u64);
    let mut out = vec![kind << 4 | 15, 0x10 | n.trailing_zeros() as u8];
    out.extend(be(count as u64, n));
    out
}

/// Finder's tags as the attribute's bytes
pub fn plist(list: &[String]) -> Vec<u8> {
    let n = list.len() + 1;
    let r = if n < 256 { 1 } else { 2 };
    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(n);
    let mut top = marker(0xA, list.len());
    for i in 1..n { top.extend(be(i as u64, r)); }
    objects.push(top);
    for s in list {
        if s.is_ascii() {
            let mut o = marker(0x5, s.len());
            o.extend(s.as_bytes());
            objects.push(o);
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            let mut o = marker(0x6, units.len());
            for u in units { o.extend(u.to_be_bytes()); }
            objects.push(o);
        }
    }
    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::with_capacity(n);
    for o in &objects { offsets.push(out.len() as u64); out.extend(o); }
    let table = out.len() as u64;
    let osize = bytes_for(table);
    for off in offsets { out.extend(be(off, osize)); }
    out.extend([0u8; 6]);
    out.push(osize as u8);
    out.push(r as u8);
    out.extend(be(n as u64, 8));
    out.extend(be(0, 8));
    out.extend(be(table, 8));
    out
}

fn read_be(d: &[u8], at: usize, n: usize) -> Option<u64> {
    d.get(at..at + n).map(|b| b.iter().fold(0u64, |v, x| v << 8 | *x as u64))
}

/// The attribute's bytes as Finder's strings; None when it isn't a property list of strings
pub fn parse(d: &[u8]) -> Option<Vec<String>> {
    if d.len() < 40 || &d[..8] != b"bplist00" { return None; }
    let t = d.len() - 32;
    let (osize, r) = (d[t + 6] as usize, d[t + 7] as usize);
    let (count, top, table) = (read_be(d, t + 8, 8)?, read_be(d, t + 16, 8)?, read_be(d, t + 24, 8)? as usize);
    let offset = |i: u64| read_be(d, table + i as usize * osize, osize).map(|o| o as usize);
    let head = |at: usize| -> Option<(u8, usize, usize)> {
        let m = *d.get(at)?;
        let (kind, c) = (m >> 4, (m & 15) as usize);
        if c < 15 { return Some((kind, c, at + 1)); }
        let n = 1usize << (*d.get(at + 1)? & 15);
        Some((kind, read_be(d, at + 2, n)? as usize, at + 2 + n))
    };
    if top >= count { return None; }
    let (kind, c, start) = head(offset(top)?)?;
    if kind != 0xA { return None; }
    let mut out = Vec::with_capacity(c);
    for k in 0..c {
        let o = read_be(d, start + k * r, r)?;
        if o >= count { return None; }
        let (sk, sc, ss) = head(offset(o)?)?;
        match sk {
            0x5 => out.push(String::from_utf8_lossy(d.get(ss..ss + sc)?).into_owned()),
            0x6 => {
                let units: Vec<u16> = (0..sc).map(|i| read_be(d, ss + i * 2, 2).map(|u| u as u16)).collect::<Option<_>>()?;
                out.push(String::from_utf16_lossy(&units));
            }
            _ => return None,
        }
    }
    Some(out)
}

fn name(f: &str) -> &str { f.rsplit_once('\n').filter(|(_, c)| c.chars().all(|c| c.is_ascii_digit())).map(|(n, _)| n).unwrap_or(f) }

/// Finder's tags made to follow the library's: `removed` taken off, `list` all there (a colour kept for one there
/// already), the others left. None when nothing changes
pub fn follow(finder: &[String], list: &[String], removed: &[String]) -> Option<Vec<String>> {
    let has = |l: &[String], n: &str| l.iter().any(|x| x.eq_ignore_ascii_case(n));
    let mut changed = false;
    let mut out: Vec<String> = Vec::new();
    for f in finder {
        if has(removed, name(f)) && !has(list, name(f)) { changed = true; } else { out.push(f.clone()); }
    }
    for t in list {
        if !out.iter().any(|f| name(f).eq_ignore_ascii_case(t)) { out.push(t.clone()); changed = true; }
    }
    changed.then_some(out)
}

#[cfg(target_os = "macos")]
mod sys {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    fn c(path: &Path) -> std::io::Result<CString> { CString::new(path.as_os_str().as_bytes()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e)) }

    pub fn get(path: &Path, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        let (p, n) = (c(path)?, CString::new(name).unwrap());
        unsafe {
            let size = libc::getxattr(p.as_ptr(), n.as_ptr(), std::ptr::null_mut(), 0, 0, 0);
            if size < 0 {
                let e = std::io::Error::last_os_error();
                return if e.raw_os_error() == Some(libc::ENOATTR) { Ok(None) } else { Err(e) };
            }
            let mut buf = vec![0u8; size as usize];
            let got = libc::getxattr(p.as_ptr(), n.as_ptr(), buf.as_mut_ptr() as *mut _, buf.len(), 0, 0);
            if got < 0 { return Err(std::io::Error::last_os_error()); }
            buf.truncate(got as usize);
            Ok(Some(buf))
        }
    }

    pub fn set(path: &Path, name: &str, value: Option<&[u8]>) -> std::io::Result<()> {
        let (p, n) = (c(path)?, CString::new(name).unwrap());
        let r = unsafe {
            match value {
                Some(v) => libc::setxattr(p.as_ptr(), n.as_ptr(), v.as_ptr() as *const _, v.len(), 0, 0),
                None => libc::removexattr(p.as_ptr(), n.as_ptr(), 0),
            }
        };
        if r < 0 {
            let e = std::io::Error::last_os_error();
            if value.is_none() && e.raw_os_error() == Some(libc::ENOATTR) { return Ok(()); }
            return Err(e);
        }
        Ok(())
    }
}

/// A file's Finder tags (none: an empty list; not a Mac: None)
pub fn read(path: &Path) -> Option<Vec<String>> {
    #[cfg(target_os = "macos")]
    { return sys::get(path, XATTR).ok().map(|v| v.and_then(|d| parse(&d)).unwrap_or_default()); }
    #[allow(unreachable_code)]
    { let _ = path; None }
}

/// A file's Finder tags made to follow the library's (`list` on, `removed` off): true when written. Not a Mac: nothing
pub fn set(path: &Path, list: &[String], removed: &[String]) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let now = sys::get(path, XATTR).map_err(|e| format!("{}: {e}", path.display()))?.and_then(|d| parse(&d)).unwrap_or_default();
        let Some(new) = follow(&now, list, removed) else { return Ok(false) };
        let bytes = (!new.is_empty()).then(|| plist(&new));
        sys::set(path, XATTR, bytes.as_deref()).map_err(|e| format!("{}: Finder tags: {e}", path.display()))?;
        return Ok(true);
    }
    #[allow(unreachable_code)]
    { let _ = (path, list, removed); Ok(false) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_round_trip() {
        let mut list: Vec<String> = vec!["Red\n6".into(), "Привет".into(), "🐢 turtle".into()];
        for i in 0..20 { list.push(format!("tag {i}")); }
        assert_eq!(parse(&plist(&list)), Some(list));
        assert_eq!(parse(&plist(&[])), Some(vec![]));
        assert_eq!(parse(b"nope"), None);
    }

    #[test]
    fn follows_the_library() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(follow(&s(&["Turtle\n6", "Mine"]), &s(&["turtle", "reef"]), &s(&["old"])), Some(s(&["Turtle\n6", "Mine", "reef"])));
        assert_eq!(follow(&s(&["Turtle\n6"]), &s(&["Turtle"]), &[]), None);
        assert_eq!(follow(&s(&["Turtle\n6", "old", "Mine"]), &s(&["Turtle"]), &s(&["OLD"])), Some(s(&["Turtle\n6", "Mine"])));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn written_where_finder_reads() {
        let f = std::env::temp_dir().join(format!("afp-finder-{}.mp4", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(read(&f), Some(vec![]));
        assert!(set(&f, &["Turtle".into(), "Ёж".into()], &[]).unwrap());
        assert_eq!(read(&f), Some(vec!["Turtle".to_string(), "Ёж".into()]));
        // The system's own tool reads it as an array of these strings
        let o = std::process::Command::new("/usr/bin/xattr").arg("-px").arg(XATTR).arg(&f).output().unwrap();
        let hex: String = String::from_utf8_lossy(&o.stdout).split_whitespace().collect();
        let bytes: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect();
        let tmp = f.with_extension("plist");
        std::fs::write(&tmp, &bytes).unwrap();
        let json = std::process::Command::new("/usr/bin/plutil").args(["-convert", "json", "-o", "-"]).arg(&tmp).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&json.stdout).trim(), r#"["Turtle","Ёж"]"#);
        // Taken off: the attribute goes
        assert!(set(&f, &[], &["turtle".into(), "Ёж".into()]).unwrap());
        assert_eq!(read(&f), Some(vec![]));
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(&tmp);
    }
}
