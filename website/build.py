#!/usr/bin/env python3
"""The site's pages other than the home page: articles in the home page's look.

Reads each page of pages/ (the article's text: a whole page in the site's earlier markup), keeps its content (the
<main>), and writes it into site/ with the home page's header and footer, a side column to move around by (the site's
pages, and this page's sections), and article.css over the earlier styles. site/index.html, the home page, is written
by hand. Run after changing a page in pages/:  python3 website/build.py
"""
import html, pathlib, re

HERE = pathlib.Path(__file__).parent
SRC = HERE / "pages"
OUT = HERE / "site"

# The side column: (file, name), grouped
GROUPS = [
    ("Guides", [("log", "Log footage"), ("stabilization", "Stabilization"), ("photos", "Photos"),
                ("360", "360 video"), ("organizing", "Marks and moments"), ("features", "All features")]),
    ("Who it's for", [("divers", "Divers & snorkelers"), ("moments", "Active travelers")]),
    ("More", [("nas", "NAS Helper"), ("download", "Download"), ("about", "About the maker")]),
]
PAGES = [f for _, items in GROUPS for f, _ in items]

def between(s, a, b, inc=False):
    i = s.index(a); j = s.index(b, i)
    return s[i:j + (len(b) if inc else 0)]


def text(fragment):
    return html.unescape(re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", fragment))).strip()


landing = (OUT / "index.html").read_text()
FOOTER = between(landing, "<footer>", "</footer>", True)

HEADER = """<header class="top">
  <div class="wrap">
    <a class="brand" href="index.html"><img src="icon.svg" alt="">Action Footage Player</a>
    <nav class="nav" aria-label="Site">
      <a href="index.html#features">Features</a>
      <a href="index.html#specs">Cameras</a>
      <a href="features.html"{guides}>Guides</a>
      <a href="about.html"{about}>About</a>
    </nav>
    <a class="pill" href="download.html">Download</a>
  </div>
</header>"""


def side(page, toc):
    out = ['<aside class="side" aria-label="Pages and sections">']
    for title, items in GROUPS:
        out.append(f'  <p class="side-h">{html.escape(title)}</p>\n  <ul>')
        for f, name in items:
            if f == page:
                out.append(f'    <li class="here"><a href="{f}.html" aria-current="page">{html.escape(name)}</a>')
                if toc:
                    out.append('      <ul class="toc">')
                    out += [f'        <li><a href="#{i}">{html.escape(t)}</a></li>' for i, t in toc]
                    out.append('      </ul>')
                out.append('    </li>')
            else:
                out.append(f'    <li><a href="{f}.html">{html.escape(name)}</a></li>')
        out.append('  </ul>')
    out.append('</aside>')
    return "\n".join(out)


def build(page):
    src = (SRC / f"{page}.html").read_text()
    title = text(between(src, "<title>", "</title>")[7:])
    desc = re.search(r'<meta name="description" content="([^"]*)"', src)
    own_css = [c for c in re.findall(r'<link rel="stylesheet" href="([\w.-]+\.css)">', src) if c != "styles.css"]
    main = between(src, "<main", "</main>", True)
    main = main[main.index(">") + 1:-len("</main>")]
    tail = src[src.index("</main>"):]
    scripts = re.findall(r"<script\b[^>]*>.*?</script>", tail, flags=re.S)

    # The sections, for the side column
    toc = []
    for m in re.finditer(r'<section\b[^>]*\bid="([^"]+)"[^>]*>(.*?)</section>', main, flags=re.S):
        h2 = re.search(r"<h2\b[^>]*>(.*?)</h2>", m.group(2), flags=re.S)
        eb = re.search(r'class="eyebrow"[^>]*>(.*?)</p>', m.group(2), flags=re.S)
        if h2:
            toc.append((m.group(1), text(eb.group(1)) if eb and m.group(1) != "intro" else text(h2.group(1)).rstrip(".")))

    body = main

    def paths(s):
        s = re.sub(r'href="(?:index\.html)?#(?:cameras)"', 'href="index.html#specs"', s)
        s = re.sub(r'href="index\.html#how"', 'href="index.html#features"', s)
        return s

    head_links = "\n".join(re.findall(r'<link rel="(?:icon|apple-touch-icon)"[^>]*>', src))
    footer = FOOTER.replace('href="#top"', 'href="index.html"')

    css = "\n".join(f'<link rel="stylesheet" href="{c}">' for c in ["styles.css"] + own_css)
    out = f"""<!doctype html>
<html lang="en" data-theme="light">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
<meta name="description" content="{desc.group(1) if desc else ''}">
{head_links}
{css}
<link rel="stylesheet" href="article.css">
</head>
<body class="doc-page page-{page}">

{HEADER.format(guides=' aria-current="true"' if page not in ("about", "download") else '', about=' aria-current="page"' if page == "about" else '')}

<div class="doc">
{side(page, toc)}
<main class="art" id="top">
{paths(body)}
</main>
</div>

{footer}

{paths(chr(10).join(scripts))}
<script>
  // The side column marks the section on screen
  (function () {{
    var links = [].slice.call(document.querySelectorAll(".side .toc a")); if (!links.length || !window.IntersectionObserver) return;
    var byId = {{}}; links.forEach(function (a) {{ byId[a.getAttribute("href").slice(1)] = a; }});
    var io = new IntersectionObserver(function (es) {{
      es.forEach(function (e) {{ if (e.isIntersecting) {{ links.forEach(function (a) {{ a.classList.remove("now"); }}); byId[e.target.id].classList.add("now"); }} }});
    }}, {{ rootMargin: "-80px 0px -70% 0px" }});
    Object.keys(byId).forEach(function (id) {{ var el = document.getElementById(id); if (el) io.observe(el); }});
  }})();
</script>
</body>
</html>
"""
    (OUT / f"{page}.html").write_text(out)
    return len(toc)


if __name__ == "__main__":
    for p in PAGES:
        print(p, build(p), "sections")
