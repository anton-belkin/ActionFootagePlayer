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
  // The header changes height with the window's width and once the web fonts arrive
  if (window.ResizeObserver) new ResizeObserver(measure).observe(top);
})();
