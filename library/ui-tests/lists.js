// The lists the app puts in the page before its scripts run (window.AFP_LISTS: mpv/scripts/workspace/registry.json,
// src/lists.rs page_script), for the rigs that run the page without the app. Not a test itself
const fs = require("fs");
const path = require("path");

const REGISTRY = path.join(__dirname, "..", "..", "mpv", "scripts", "workspace", "registry.json");
const text = () => fs.readFileSync(REGISTRY, "utf8");

module.exports = {
  REGISTRY,
  // Into a jsdom window, before the page's scripts
  set: (w) => { w.AFP_LISTS = JSON.parse(text()); },
  // As a page's first script's text (headless Chrome's pages)
  script: () => "window.AFP_LISTS = " + text() + ";",
};
