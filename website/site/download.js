/* The download page: the links in the HTML go to the latest release on GitHub, which works without this script.
   With it, the latest release is read from GitHub's API (kept for an hour, so a visitor makes one request), and
   each link goes straight to its file, with the file's name, size and SHA-256, the version and the date filled in.
   The files are found by their names' pattern, which has the version in it. Also marks the visitor's system. */
(function () {
  "use strict";
  var API = "https://api.github.com/repos/anton-belkin/ActionFootagePlayer/releases/latest";
  var CACHE = "afp-latest-release";
  var MAX_AGE = 60 * 60 * 1000;

  // The few words this script writes, in the page's language (fr/, ru/ and zh/ use this same file)
  var LANGS = {
    "en": { title: "Version {v}", titleDate: "Version {v}, {d}", mb: "MB", kb: "KB" },
    "fr": { title: "Version {v}", titleDate: "Version {v}, {d}", mb: "Mo", kb: "ko" },
    "ru": { title: "Версия {v}", titleDate: "Версия {v}, {d}", mb: "МБ", kb: "КБ" },
    "zh-Hans": { title: "{v} 版", titleDate: "{v} 版，{d}", mb: "MB", kb: "KB" }
  };
  var LANG = LANGS[document.documentElement.lang] ? document.documentElement.lang : "en";
  var WORDS = LANGS[LANG];

  var ASSETS = {
    "mac-app": /^ActionFootagePlayer-.+-macos-arm64\.zip$/,
    "windows-app": /^ActionFootagePlayer-.+-windows-x86_64-setup\.exe$/,
    "windows-arm-app": /^ActionFootagePlayer-.+-windows-arm64-setup\.exe$/,
    "source": /^ActionFootagePlayer-.+-macos-arm64\.tar\.gz$/,
    "plugins-mac": /^mpv-plugins-.+-macos-arm64\.zip$/,
    "plugins-windows": /^mpv-plugins-.+-windows-x86_64\.zip$/,
    "plugins-windows-arm": /^mpv-plugins-.+-windows-arm64\.zip$/,
    "plugins-linux": /^mpv-plugins-.+-linux-x86_64\.zip$/,
    "sums": /^SHA256SUMS$/
  };

  function system() {
    var ua = navigator.userAgent || "";
    var p = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    // iPadOS asks for desktop pages as a Mac: the touch screen tells them apart
    if (/iPhone|iPad|iPod|Android/i.test(ua) || /^(iOS|Android)$/i.test(p) || (/Mac/.test(p) && navigator.maxTouchPoints > 1)) return "mobile";
    if (/Mac/i.test(p) || /Macintosh/.test(ua)) return "mac";
    if (/Win/i.test(p) || /Windows/.test(ua)) return "windows";
    if (/Linux|X11|CrOS/i.test(p) || /Linux|X11/.test(ua)) return "linux";
    return "";
  }

  // The visitor's system: its cards marked, and its app's download (Linux's: the way to the plugins) the one
  // orange button. Without knowing the system, none is
  function mark(os, on) {
    document.querySelectorAll('[data-os="' + os + '"]').forEach(function (el) {
      el.classList.toggle("is-yours", on);
      var pill = el.querySelector(".platform-head > .yours");
      if (pill) pill.hidden = !on;
      var btn = el.closest("#apps") && el.querySelector(".dl-btn");
      if (btn) btn.classList.toggle("primary", on);
    });
  }

  function markSystem() {
    var os = system();
    if (os === "mobile") { document.getElementById("mobile-note").hidden = false; return; }
    if (!os) return;
    mark(os, true);
    if (os === "windows") markArm();
  }

  // A Windows PC with an ARM processor: only Chromium browsers tell (the user agent says x86-64 everywhere). Its
  // cards become the visitor's
  function markArm() {
    var ua = navigator.userAgentData;
    if (!ua || !ua.getHighEntropyValues) return;
    ua.getHighEntropyValues(["architecture"]).then(function (v) {
      if (v.architecture !== "arm") return;
      mark("windows", false);
      mark("windows-arm", true);
    }).catch(function () { /* stays as it is */ });
  }

  function size(bytes) {
    var digits = bytes >= 1e6 && bytes < 1e8 ? 1 : 0;
    var n = bytes >= 1e6 ? bytes / 1e6 : Math.max(1, Math.round(bytes / 1e3));
    return n.toLocaleString(LANG, { minimumFractionDigits: digits, maximumFractionDigits: digits }) + "\u00A0" + (bytes >= 1e6 ? WORDS.mb : WORDS.kb);
  }

  function day(iso) {
    var d = new Date(iso);
    if (isNaN(d)) return "";
    // The last two words stay on one line: "2026 г." in Russian
    return d.toLocaleDateString(LANG === "en" ? "en-GB" : LANG, { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" })
      .replace(/ (\S+)$/, "\u00A0$1");
  }

  // The notes' first paragraph, as plain text: no Markdown is rendered, so nothing from them becomes markup
  function summary(body) {
    var first = (body || "").split(/\r?\n\s*\r?\n/)[0] || "";
    return first.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(/[*_`#>]/g, "").replace(/\s+/g, " ").trim();
  }

  function setText(selector, text, root) {
    (root || document).querySelectorAll(selector).forEach(function (el) { el.textContent = text; el.hidden = false; });
  }

  function fill(release) {
    var version = String(release.tag_name || "").replace(/^v/, "");
    if (!version) return;
    var date = day(release.published_at);
    setText('[data-release="version"]', version);
    if (date) {
      setText('[data-release="date"]', date);
      setText('[data-release="date-line"]', " · " + date);
    }
    setText('[data-release="title"]', (date ? WORDS.titleDate : WORDS.title).replace("{v}", version).replace("{d}", date));
    var text = summary(release.body);
    if (text) {
      setText('[data-release="summary"]', text);
      // The release notes are in English, also on the translated pages
      if (LANG !== "en") document.querySelectorAll('[data-release="summary"]').forEach(function (el) { el.lang = "en"; });
    }
    if (/^https:\/\/github\.com\//.test(release.html_url)) {
      document.querySelectorAll('[data-release="notes"]').forEach(function (a) { a.href = release.html_url; });
    }

    var assets = release.assets || [];
    document.querySelectorAll("[data-asset]").forEach(function (link) {
      var pattern = ASSETS[link.getAttribute("data-asset")];
      var asset = pattern && assets.filter(function (a) { return pattern.test(a.name); })[0];
      if (!asset || !/^https:\/\/github\.com\//.test(asset.browser_download_url)) return;
      link.href = asset.browser_download_url;
      // The file's own block: its card (with the install steps), or the note it's in
      var blocks = ".platform, .note";
      var scope = link.closest(blocks) || link;
      function put(what, text) {
        scope.querySelectorAll('[data-fill="' + what + '"]').forEach(function (el) {
          if (el.closest(blocks) === scope || scope === link) { el.textContent = text; el.hidden = false; }
        });
      }
      put("name", asset.name);
      put("size", size(asset.size));
      var sha = /^sha256:([0-9a-f]{64})$/.exec(asset.digest || "");
      if (sha) put("sha256", sha[1]);
    });
  }

  function cached() {
    try {
      var c = JSON.parse(localStorage.getItem(CACHE));
      if (c && Date.now() - c.at < MAX_AGE) return c.release;
    } catch (e) { /* no storage: fetched each time */ }
    return null;
  }

  function keep(release) {
    try { localStorage.setItem(CACHE, JSON.stringify({ at: Date.now(), release: release })); } catch (e) { /* fine */ }
  }

  // Only what the page uses, so the cache stays small
  function slim(r) {
    return {
      tag_name: r.tag_name, published_at: r.published_at, html_url: r.html_url, body: summary(r.body),
      assets: (r.assets || []).map(function (a) { return { name: a.name, size: a.size, digest: a.digest, browser_download_url: a.browser_download_url }; })
    };
  }

  markSystem();
  var release = cached();
  if (release) { fill(release); return; }
  if (!window.fetch) return;
  fetch(API, { headers: { Accept: "application/vnd.github+json" } })
    .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); })
    .then(function (r) { r = slim(r); keep(r); fill(r); })
    .catch(function () { /* the links to the latest release stay */ });
})();
