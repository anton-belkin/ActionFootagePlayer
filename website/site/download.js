/* The download page: the links in the HTML go to the latest release on GitHub, which works without this script.
   With it, the latest release is read from GitHub's API (kept for an hour, so a visitor makes one request), and
   each link goes straight to its file, with the file's name, size and SHA-256, the version and the date filled in.
   The files are found by their names' pattern, which has the version in it. Also marks the visitor's system. */
(function () {
  "use strict";
  var API = "https://api.github.com/repos/anton-belkin/ActionFootagePlayer/releases/latest";
  var CACHE = "afp-latest-release";
  var MAX_AGE = 60 * 60 * 1000;

  var ASSETS = {
    "mac-app": /^ActionFootagePlayer-.+-macos-arm64\.zip$/,
    "windows-app": /^ActionFootagePlayer-.+-windows-x86_64-setup\.exe$/,
    "source": /^ActionFootagePlayer-.+-macos-arm64\.tar\.gz$/,
    "dlogm": /^mpv-dlogm-.+\.zip$/,
    "telemetry": /^mpv-dji-telemetry-.+\.zip$/,
    "highlights": /^mpv-highlights-.+\.zip$/,
    "menu": /^mpv-footage-menu-.+\.zip$/,
    "gyroflow-mac": /^mpv-gyroflow-.+-macos-arm64\.zip$/,
    "gyroflow-linux": /^mpv-gyroflow-.+-linux-x86_64\.zip$/,
    "gyroflow-windows": /^mpv-gyroflow-.+-windows-x86_64\.zip$/,
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

  function markSystem() {
    var os = system();
    if (os === "mobile") { document.getElementById("mobile-note").hidden = false; return; }
    if (!os) return;
    document.querySelectorAll('[data-os="' + os + '"]').forEach(function (el) {
      el.classList.add("is-yours");
      var pill = el.querySelector(".yours");
      if (pill) pill.hidden = false;
    });
  }

  function size(bytes) {
    if (bytes >= 1e6) return (bytes / 1e6).toFixed(bytes >= 1e8 ? 0 : 1) + " MB";
    return Math.max(1, Math.round(bytes / 1e3)) + " KB";
  }

  function day(iso) {
    var d = new Date(iso);
    if (isNaN(d)) return "";
    var lang = document.documentElement.lang || "en";
    return d.toLocaleDateString(lang === "en" ? "en-GB" : lang, { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
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
    setText('[data-release="title"]', "Version " + version + (date ? ", " + date : ""));
    var text = summary(release.body);
    if (text) setText('[data-release="summary"]', text);
    if (/^https:\/\/github\.com\//.test(release.html_url)) {
      document.querySelectorAll('[data-release="notes"]').forEach(function (a) { a.href = release.html_url; });
    }

    var assets = release.assets || [];
    document.querySelectorAll("[data-asset]").forEach(function (link) {
      var pattern = ASSETS[link.getAttribute("data-asset")];
      var asset = pattern && assets.filter(function (a) { return pattern.test(a.name); })[0];
      if (!asset || !/^https:\/\/github\.com\//.test(asset.browser_download_url)) return;
      link.href = asset.browser_download_url;
      var scope = link.closest(".platform, tr, .help-card") || link;
      setText('[data-fill="name"]', asset.name, scope);
      setText('[data-fill="size"]', size(asset.size), scope);
      var sha = /^sha256:([0-9a-f]{64})$/.exec(asset.digest || "");
      if (sha) setText('[data-fill="sha256"]', sha[1], scope);
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
