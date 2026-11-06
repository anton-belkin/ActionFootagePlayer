"use strict";
// The window's page: what the app (src/main.rs) says, asked twice a second, and its commands on the buttons
const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

let current = null;        // the project's path
let info = null;           // its project() answer
let macos = false;
let lastFinished = 0;      // finished jobs seen: a new one reads the project again (its counts)

function showError(text) {
  const el = $("error");
  el.textContent = text || "";
  el.hidden = !text;
  if (text) setTimeout(() => { if (el.textContent === text) el.hidden = true; }, 8000);
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    showError(String(e));
    throw e;
  }
}

function plural(n, one, many) { return `${n} ${n === 1 ? one : many}`; }

async function readProject() {
  info = await invoke("project");
  if (!info) return;
  $("name").textContent = info.name;
  $("path").textContent = info.path;
  $("legacy").hidden = !info.legacy;
  $("clips_flat").checked = info.clips_flat;
  $("moments_alone").checked = info.moments_alone;
  $("counts").innerHTML = [
    ["_Clips", plural(info.clip_previews, "clip preview", "clip previews")],
    ["_Moments", plural(info.moments, "moment", "moments")],
    ["_Stills", plural(info.stills, "photo", "photos")],
    ["_Exports", plural(info.exports, "export", "exports")],
  ].map(([dir, label]) => `<button type="button" data-reveal="${dir}" title="Show ${dir}">${esc(label)}</button>`).join("");
  const failed = info.clips_failed || [];
  $("failed").hidden = failed.length === 0;
  $("failed-sum").textContent = `${plural(failed.length, "clip preview", "clip previews")} couldn't be made last time`;
  $("failed-list").innerHTML = failed.map(([clip, why]) => `<li><code>${esc(clip)}</code>: ${esc(why)}</li>`).join("");
}

function jobLine(j) {
  const parts = [];
  const fin = ["done", "stopped", "failed"].includes(j.state);
  if (j.state === "waiting") parts.push("waiting");
  else if (j.state === "starting") parts.push("starting the player");
  else if (j.state === "looking") parts.push("looking through the project");
  else if (fin) {
    if (j.made != null) parts.push(`${j.made} made`);
  } else if (j.total > 0) parts.push(`${j.done} of ${j.total}`);
  if (j.failed > 0) parts.push(`<span class="err">${j.failed} failed</span>`);
  if (j.current && !fin) parts.push(esc(j.current));
  if (j.message) parts.push(j.state === "failed" ? `<span class="err">${esc(j.message)}</span>` : esc(j.message));
  return parts.join(" · ");
}

function renderJobs(jobs) {
  $("qempty").hidden = jobs.length > 0;
  $("clear").hidden = !jobs.some((j) => ["done", "stopped", "failed"].includes(j.state));
  $("jobs").innerHTML = jobs.map((j) => {
    const fin = ["done", "stopped", "failed"].includes(j.state);
    let pct = 0;
    if (j.total > 0) pct = Math.min(100, ((j.done + (j.fraction || 0)) / j.total) * 100);
    if (j.state === "done") pct = 100;
    const where = j.project !== current ? ` <span class="dim">· ${esc(j.project_name)}</span>` : "";
    const btn = fin ? "" : `<button type="button" data-cancel="${j.id}">${j.state === "waiting" ? "Remove" : "Stop"}</button>`;
    const fails = (j.failures || []).length
      ? `<details><summary>${plural(j.failures.length, "failure", "failures")}</summary><ul>${j.failures.map(([what, why]) => `<li><code>${esc(what)}</code>: ${esc(why)}</li>`).join("")}</ul></details>`
      : "";
    const bar = !fin && j.state !== "waiting" ? `<div class="bar"><i style="width:${pct.toFixed(1)}%"></i></div>` : "";
    return `<li class="job${fin ? " finished" : ""}"><div class="r1"><b>${esc(j.title)}${where}</b><span class="pill ${j.state}">${j.state}</span>${btn}</div>
      <div class="r2">${jobLine(j)}</div>${bar}${fails}</li>`;
  }).join("");
}

async function poll() {
  let s;
  try { s = await invoke("snapshot"); } catch (e) { return; }
  macos = s.macos;
  if (s.project !== current) {
    current = s.project;
    $("empty").hidden = !!current;
    $("project").hidden = !current;
    $("show").hidden = !current;
    if (current) await readProject();
    else { $("name").textContent = "Action Footage Library"; $("path").textContent = ""; }
  }
  renderJobs(s.jobs);
  const finished = s.jobs.filter((j) => ["done", "stopped", "failed"].includes(j.state)).length;
  if (finished !== lastFinished) {
    lastFinished = finished;
    if (current) readProject();
  }
  $("ask").hidden = !s.close_asked;
}

document.addEventListener("click", async (ev) => {
  const t = ev.target.closest("button");
  if (!t) return;
  if (t.id === "open") { await call("choose_project").catch(() => {}); return poll(); }
  if (t.id === "show") return call("reveal", {}).catch(() => {});
  if (t.dataset.reveal) return call("reveal", { what: t.dataset.reveal }).catch(() => {});
  if (t.id === "clear") { await call("clear_finished"); return poll(); }
  if (t.id === "keep") { await call("answer_close", { quit: false }); return poll(); }
  if (t.id === "quit") return call("answer_close", { quit: true });
  if (t.dataset.cancel) { await call("cancel_job", { id: Number(t.dataset.cancel) }); return poll(); }
  if (t.dataset.job) {
    const kind = t.dataset.job;
    const options = kind === "photos" ? { photo: $("photo").value } : kind === "export" ? { sdr: $("look").value === "sdr" } : {};
    await call("add_job", { kind, options }).catch(() => {});
    return poll();
  }
});

for (const key of ["clips_flat", "moments_alone"]) {
  $(key).addEventListener("change", (ev) => call("set_setting", { key, value: ev.target.checked }).catch(() => readProject()));
}

// A Project.afproject (or a folder of the trip) dropped on the window
try {
  window.__TAURI__.webview.getCurrentWebview().onDragDropEvent(async (ev) => {
    if (ev.payload.type === "drop" && ev.payload.paths.length) {
      await call("open_project", { path: ev.payload.paths[0] }).catch(() => {});
      poll();
    }
  });
} catch (e) { /* no drops then */ }

(async () => {
  try {
    await invoke("engine");
  } catch (e) {
    $("engine").textContent = String(e);
    $("engine").hidden = false;
  }
  await poll();
  if (!macos) {
    for (const o of document.querySelectorAll("option.mac")) o.remove();
    $("show").textContent = "Show in Explorer";
  }
  setInterval(poll, 500);
})();
