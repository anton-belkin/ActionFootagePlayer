// SPDX-License-Identifier: GPL-3.0-or-later
//
// A photo's own metadata as read from its XMP packet (docs/design/library-management.md §8, "Photos carry what they
// need"): its link to its clip (`afp:link`, the player's namespace, an element as vs-gyroflow's photo.rs writes it in a
// JPEG, or an attribute as ImageIO may in a HEIC) and its tags (`dc:subject`, which Photos and Lightroom read as
// keywords). The one copy of the reading: the catalog's look at a photo (inspect.rs), the Library's (index.rs) and
// afp-meta's (vs-gyroflow's xmp.rs, which also writes them).

use std::io::Read;
use std::path::Path;

/// How far into a photo its XMP packet is looked for: where photo.rs and ImageIO put it
pub const HEAD: u64 = 65536;

/// The XMP packet in a file's bytes: `<x:xmpmeta …>…</x:xmpmeta>`, its byte range
pub fn packet(bytes: &[u8]) -> Option<(usize, usize)> {
    let find = |hay: &[u8], needle: &[u8], from: usize| hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from);
    let a = find(bytes, b"<x:xmpmeta", 0)?;
    let end = b"</x:xmpmeta>";
    let b = find(bytes, end, a)? + end.len();
    Some((a, b))
}

/// A photo's XMP packet, within the file's first 64 KiB. None: no file, or no packet there
pub fn of_file(path: &Path) -> std::io::Result<Option<String>> {
    let mut head = Vec::with_capacity(HEAD as usize);
    std::fs::File::open(path)?.take(HEAD).read_to_end(&mut head)?;
    Ok(packet(&head).map(|(a, b)| String::from_utf8_lossy(&head[a..b]).into_owned()))
}

/// XML's text as it means: the five named entities and numeric ones (&#10;, &#x41F;: what a writer of XMP may use for
/// any character), each once, left to right; anything else left as it is
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.bytes().take(12).position(|b| b == b';') else { out.push('&'); rest = &rest[1..]; continue };
        let name = &rest[1..end];
        let ch = match name {
            "lt" => Some('<'), "gt" => Some('>'), "quot" => Some('"'), "apos" => Some('\''), "amp" => Some('&'),
            _ => name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")).map(|h| u32::from_str_radix(h, 16).ok())
                     .unwrap_or_else(|| name.strip_prefix('#').and_then(|d| d.parse::<u32>().ok())).and_then(char::from_u32),
        };
        match ch { Some(c) => { out.push(c); rest = &rest[end + 1..]; } None => { out.push('&'); rest = &rest[1..]; } }
    }
    out.push_str(rest);
    out
}

/// Text into XML (an element's text or an attribute's value)
pub fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

/// The link (`afp:link`), as its text: an element, as photo.rs writes it, or an attribute, as ImageIO may
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

/// The tags (`dc:subject`'s list), the empty ones left out
pub fn tags(xml: &str) -> Vec<String> {
    let Some(a) = xml.find("<dc:subject") else { return Vec::new() };
    let Some(len) = xml[a..].find("</dc:subject>") else { return Vec::new() };
    let mut out = Vec::new();
    let mut rest = &xml[a..a + len];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_by_name_and_by_number() {
        assert_eq!(unescape("&quot;a&amp;b&lt;c&gt;&#34;&#x41F;&#1088;&#10;&amp;amp;&bogus;&"), "\"a&b<c>\"Пр\n&amp;&bogus;&");
        assert_eq!(unescape(&escape("a & <b> \"c\"")), "a & <b> \"c\"");
    }

    /// A link and tags as afp-meta writes them, and as another writer of XMP may (entities by number, the link an
    /// attribute): the same for the catalog, the Library and afp-meta, which read them here. Before, the Library's and
    /// afp-meta's copies read &#x427; as it is
    #[test]
    fn link_and_tags_as_any_writer_puts_them() {
        let element = "<x:xmpmeta><rdf:Description><afp:link>{&quot;clip&quot;:&quot;&#x427;/A.MP4&quot;}</afp:link><dc:subject><rdf:Bag><rdf:li>sun</rdf:li><rdf:li> </rdf:li><rdf:li>Рыбы &amp; риф</rdf:li></rdf:Bag></dc:subject></rdf:Description></x:xmpmeta>";
        assert_eq!(link(element).as_deref(), Some(r#"{"clip":"Ч/A.MP4"}"#));
        assert_eq!(tags(element), vec!["sun".to_string(), "Рыбы & риф".to_string()]);
        let attribute = r#"<x:xmpmeta><rdf:Description afp:link="{&#34;clip&#34;:&#34;A.MP4&#34;}"/></x:xmpmeta>"#;
        assert_eq!(link(attribute).as_deref(), Some(r#"{"clip":"A.MP4"}"#));
        assert!(tags(attribute).is_empty());
        let bytes = [b"\xff\xd8junk".as_slice(), element.as_bytes(), b"tail"].concat();
        let (a, b) = packet(&bytes).unwrap();
        assert_eq!(&bytes[a..b], element.as_bytes());
    }
}
