// SPDX-License-Identifier: GPL-3.0-or-later
//
// A photo's own metadata, the player's part of it (docs/library-management.md §8, "Photos carry what they need"): its
// link to its clip (`afp:link`, the player's namespace, as photo.rs writes it in a JPEG and afp-photo in a HEIC) and
// its tags (`dc:subject`, which Photos and Lightroom read as keywords), in the file's XMP packet. Read from any photo;
// written into a JPEG here (its XMP segment replaced, the image data as it is: a Motion Photo's video after it keeps
// its length, which its XMP says from the end), into a HEIC by afp-photo on a Mac (ImageIO copies the image as it is).

use crate::photo::AFP_NS;

const XMP_NS: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";

/// The XMP packet in a file's bytes: `<x:xmpmeta …>…</x:xmpmeta>`, its byte range
pub fn packet(bytes: &[u8]) -> Option<(usize, usize)> {
    let find = |hay: &[u8], needle: &[u8], from: usize| hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from);
    let a = find(bytes, b"<x:xmpmeta", 0)?;
    let end = b"</x:xmpmeta>";
    let b = find(bytes, end, a)? + end.len();
    Some((a, b))
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#34;", "\"").replace("&amp;", "&")
}
fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

/// The link (`afp:link`): an element, as photo.rs writes it, or an attribute, as ImageIO may
pub fn link(xml: &str) -> Option<String> {
    if let Some(a) = xml.find("<afp:link>") {
        let from = a + "<afp:link>".len();
        let to = from + xml[from..].find("</afp:link>")?;
        return Some(unescape(&xml[from..to]));
    }
    let a = xml.find("afp:link=\"")? + "afp:link=\"".len();
    let to = a + xml[a..].find('"')?;
    Some(unescape(&xml[a..to]))
}

/// The tags (`dc:subject`'s list)
pub fn tags(xml: &str) -> Vec<String> {
    let Some(a) = xml.find("<dc:subject") else { return Vec::new() };
    let Some(len) = xml[a..].find("</dc:subject>") else { return Vec::new() };
    let part = &xml[a..a + len];
    let mut out = Vec::new();
    let mut rest = part;
    while let Some(i) = rest.find("<rdf:li") {
        let Some(gt) = rest[i..].find('>') else { break };
        let from = i + gt + 1;
        let Some(to) = rest[from..].find("</rdf:li>") else { break };
        let t = unescape(rest[from..from + to].trim());
        if !t.is_empty() { out.push(t); }
        rest = &rest[from + to..];
    }
    out
}

/// What's between `open` (an element's start, `<afp:link`) and its end taken out, wherever it is
fn without_element(xml: &str, name: &str) -> String {
    let mut s = xml.to_string();
    let open = format!("<{name}");
    let close = format!("</{name}>");
    while let Some(a) = s.find(&open) {
        // `<afp:linkX` is another element
        if !s[a + open.len()..].starts_with(['>', ' ', '/', '\t', '\n', '\r']) { break; }
        let self_closing = s[a..].find('>').is_some_and(|g| s[a..a + g].ends_with('/'));
        let end = if self_closing { a + s[a..].find('>').unwrap() + 1 } else { match s[a..].find(&close) { Some(c) => a + c + close.len(), None => break } };
        s.replace_range(a..end, "");
    }
    s
}

/// An attribute `name="…"` taken out of the tags it's in
fn without_attribute(xml: &str, name: &str) -> String {
    let mut s = xml.to_string();
    let pat = format!(" {name}=\"");
    while let Some(a) = s.find(&pat) {
        let from = a + pat.len();
        let Some(q) = s[from..].find('"') else { break };
        s.replace_range(a..from + q + 1, "");
    }
    s
}

/// The packet with the link and the tags given set (None: left as they are; an empty list: no tags), everything else
/// in it kept. A packet with no rdf:Description gets one
pub fn with(xml: &str, link: Option<&str>, tags: Option<&[String]>) -> String {
    let mut s = xml.to_string();
    let mut add = String::new();
    if let Some(l) = link {
        s = without_attribute(&without_element(&s, "afp:link"), "afp:link");
        add.push_str(&format!("<afp:link>{}</afp:link>", escape(l)));
    }
    if let Some(t) = tags {
        s = without_element(&s, "dc:subject");
        if !t.is_empty() {
            add.push_str("<dc:subject><rdf:Bag>");
            for x in t { add.push_str(&format!("<rdf:li>{}</rdf:li>", escape(x))); }
            add.push_str("</rdf:Bag></dc:subject>");
        }
    }
    if add.is_empty() { return s; }
    // Into the first rdf:Description, with the namespaces it needs declared on it
    if let Some(d) = s.find("<rdf:Description") {
        let gt = d + s[d..].find('>').unwrap_or(0);
        let self_closing = s[d..gt].ends_with('/');
        let mut decl = String::new();
        if link.is_some() && !s[d..gt].contains("xmlns:afp=") && !s.contains(&format!("xmlns:afp=\"{AFP_NS}\"")) { decl.push_str(&format!(" xmlns:afp=\"{AFP_NS}\"")); }
        if tags.is_some_and(|t| !t.is_empty()) && !s[d..gt].contains("xmlns:dc=") && !s.contains(&format!("xmlns:dc=\"{DC_NS}\"")) { decl.push_str(&format!(" xmlns:dc=\"{DC_NS}\"")); }
        if self_closing {
            s.replace_range(gt - 1..gt + 1, &format!("{decl}>{add}</rdf:Description>"));
        } else {
            s.replace_range(gt..gt + 1, &format!("{decl}>{add}"));
        }
        return s;
    }
    let body = format!("<rdf:Description rdf:about=\"\" xmlns:afp=\"{AFP_NS}\" xmlns:dc=\"{DC_NS}\">{add}</rdf:Description>");
    if let Some(r) = s.find("</rdf:RDF>") { s.insert_str(r, &body); return s; }
    fresh(&body)
}

fn fresh(description: &str) -> String {
    format!(concat!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
                    "{}</rdf:RDF></x:xmpmeta>"), description)
}

/// A JPEG with its XMP segment (APP1 with Adobe's namespace) set to `xml`: replaced where there's one, else added
/// after the JFIF APP0. The rest of the file is as it was
fn jpeg_with_xmp(jpeg: &[u8], xml: &str) -> Result<Vec<u8>, String> {
    if jpeg.get(0..2) != Some(&[0xFF, 0xD8]) { return Err("not a JPEG".into()); }
    let packet = format!("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>{xml}<?xpacket end=\"w\"?>");
    let mut payload = XMP_NS.to_vec();
    payload.extend_from_slice(packet.as_bytes());
    if payload.len() > 65533 { return Err("the XMP is too large for a JPEG segment".into()); }
    let segment = |p: &[u8]| { let mut v = vec![0xFF, 0xE1]; v.extend_from_slice(&((p.len() + 2) as u16).to_be_bytes()); v.extend_from_slice(p); v };
    // The segments before the image data: an XMP APP1 replaced in place
    let mut at = 2;
    let mut after_app0 = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF {
        let marker = jpeg[at + 1];
        if marker == 0xDA || marker == 0xD9 { break; }
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        let end = at + 2 + len;
        if end > jpeg.len() { return Err("a JPEG segment runs past the end".into()); }
        if marker == 0xE0 && at == 2 { after_app0 = end; }
        if marker == 0xE1 && jpeg[at + 4..end].starts_with(XMP_NS) {
            let mut out = jpeg[..at].to_vec();
            out.extend(segment(&payload));
            out.extend_from_slice(&jpeg[end..]);
            return Ok(out);
        }
        at = end;
    }
    let mut out = jpeg[..after_app0].to_vec();
    out.extend(segment(&payload));
    out.extend_from_slice(&jpeg[after_app0..]);
    Ok(out)
}

/// What a photo keeps of ours: its link (as JSON text) and its tags
pub fn read(bytes: &[u8]) -> (Option<String>, Vec<String>) {
    match packet(bytes) {
        Some((a, b)) => { let xml = String::from_utf8_lossy(&bytes[a..b]); (link(&xml), tags(&xml)) }
        None => (None, Vec::new()),
    }
}

/// A JPEG's bytes with the link and tags given set (None: as they are)
pub fn jpeg_set(jpeg: &[u8], link: Option<&str>, tags: Option<&[String]>) -> Result<Vec<u8>, String> {
    // The JPEG's own XMP segment, not a packet further on (the gain map's, the video's)
    let mut xml = None;
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF && jpeg[at + 1] != 0xDA {
        let end = at + 2 + u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        if end > jpeg.len() { break; }
        if jpeg[at + 1] == 0xE1 && jpeg[at + 4..end].starts_with(XMP_NS) {
            let p = &jpeg[at + 4 + XMP_NS.len()..end];
            xml = packet(p).map(|(a, b)| String::from_utf8_lossy(&p[a..b]).into_owned());
            break;
        }
        at = end;
    }
    let xml = xml.unwrap_or_else(|| fresh(""));
    jpeg_with_xmp(jpeg, &with(&xml, link, tags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_and_tags() {
        let link_json = r#"{"v":1,"kind":"photo","clip":"Day 1/A & B.MP4","t":12.5}"#;
        // A JPEG of the smallest kind: SOI, APP0, SOS, the data, EOI; then something after it (a Motion Photo's video)
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        jpeg.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02, 0x33, 0x44, 0xFF, 0xD9]);
        jpeg.extend_from_slice(b"VIDEO-AFTER");
        let tags_in = vec!["turtle".to_string(), "best & brightest".to_string()];
        let one = jpeg_set(&jpeg, Some(link_json), Some(&tags_in)).unwrap();
        assert_eq!(read(&one), (Some(link_json.to_string()), tags_in.clone()));
        assert!(one.ends_with(b"\xFF\xD9VIDEO-AFTER"));
        // Again: replaced, not added; the link kept when only the tags change
        let two = jpeg_set(&one, None, Some(&["reef".to_string()])).unwrap();
        assert_eq!(read(&two), (Some(link_json.to_string()), vec!["reef".to_string()]));
        assert_eq!(two.windows(XMP_NS.len()).filter(|w| *w == XMP_NS).count(), 1);
        let three = jpeg_set(&two, Some(r#"{"clip":"New/A.MP4"}"#), Some(&[])).unwrap();
        assert_eq!(read(&three), (Some(r#"{"clip":"New/A.MP4"}"#.to_string()), Vec::<String>::new()));
        // A packet as ImageIO writes it: the link as an attribute, other properties kept
        let apple = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:afp="https://actionfootageplayer.ch/ns/1.0/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" afp:link="{&quot;clip&quot;:&quot;A.MP4&quot;}" xmp:Rating="4"/></rdf:RDF></x:xmpmeta>"#;
        assert_eq!(link(apple).as_deref(), Some(r#"{"clip":"A.MP4"}"#));
        let set = with(apple, Some(r#"{"clip":"B.MP4"}"#), Some(&["x".to_string()]));
        assert_eq!(link(&set).as_deref(), Some(r#"{"clip":"B.MP4"}"#));
        assert_eq!(tags(&set), vec!["x".to_string()]);
        assert!(set.contains("xmp:Rating=\"4\"") && set.contains("xmlns:dc=") && !set.contains("afp:link=\""));
    }
}
