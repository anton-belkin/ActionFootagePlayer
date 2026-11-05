/* The visitor's system, for the words the pages use: Finder or File Explorer, Quick Look or the preview pane, and
   which download comes first. <html data-os> is "mac" on a Mac, iPhone or iPad, "win" anywhere else (Windows, and
   Android, whose users are taken to use Windows; Linux too). Text for each is in .os-mac and .os-win (styles.css
   shows the one that matches; without this script, the Mac's). A pair of download buttons marked data-os-swap,
   with .dl-mac and .dl-win inside, puts the visitor's first, as the primary one. */
(function () {
  "use strict";
  var p = ((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "") + " " +
    navigator.userAgent;
  var os = /Mac|iPhone|iPad|iPod/i.test(p) && !/Android/i.test(p) ? "mac" : "win";
  document.documentElement.setAttribute("data-os", os);
  if (os !== "win") return;
  var pairs = document.querySelectorAll("[data-os-swap]");
  for (var i = 0; i < pairs.length; i++) {
    var mac = pairs[i].querySelector(".dl-mac"), win = pairs[i].querySelector(".dl-win");
    if (!mac || !win || mac.parentNode !== win.parentNode) continue;
    if (mac.classList.contains("primary")) { mac.classList.remove("primary"); win.classList.add("primary"); }
    mac.parentNode.insertBefore(win, mac);
  }
})();

/* Links to #anchors land below the sticky header: its height goes into --top-h, which styles.css takes for
   scroll-padding-top. The header is 57 to 113px tall depending on the page, its language and the screen's width,
   so it is measured; without this script styles.css falls back to a height that fits most of them. */
(function () {
  "use strict";
  var top = document.querySelector(".top");
  if (!top) return;
  var root = document.documentElement;

  function measure() {
    var h = top.offsetHeight + "px";
    if (root.style.getPropertyValue("--top-h") === h) return false;
    root.style.setProperty("--top-h", h);
    return true;
  }

  // A page opened at an #anchor has scrolled there with the fallback height: go there again with the real one
  if (measure() && location.hash) {
    var target = document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (target) target.scrollIntoView({ behavior: "instant" });
  }
  // A link to a question in a FAQ opens it
  function openTarget() {
    var t = location.hash && document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (t && t.tagName === "DETAILS") t.open = true;
  }
  openTarget();
  window.addEventListener("hashchange", openTarget);
  // The header changes height with the window's width and once the web fonts arrive
  if (window.ResizeObserver) new ResizeObserver(measure).observe(top);
})();

/* The header's menus (this page's sections, the language) close when a link in them is chosen, on a click
   outside them and on Escape, as menus do; without this script they still open and close on their summary. */
(function () {
  "use strict";
  var menus = document.querySelectorAll(".top details");  // Features ▾, On this page, the language
  if (!menus.length) return;
  function closeAll(except) {
    for (var i = 0; i < menus.length; i++) if (menus[i] !== except) menus[i].open = false;
  }
  for (var i = 0; i < menus.length; i++) {
    menus[i].addEventListener("click", function (e) {
      if (e.target.closest("a")) this.open = false;
    });
    menus[i].addEventListener("toggle", function () { if (this.open) closeAll(this); });
  }
  document.addEventListener("click", function (e) { if (!e.target.closest(".top details")) closeAll(null); });
  document.addEventListener("keydown", function (e) { if (e.key === "Escape") closeAll(null); });
})();
