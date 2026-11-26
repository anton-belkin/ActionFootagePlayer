// The feedback form's sender (site/feedback.html posts here): a Worker on the route actionfootageplayer.ch/api/feedback
// that emails each answer to Anton through Email Routing's send_email binding. Pages Functions can't have that binding,
// so this is a Worker of its own; the site stays static. Deployed by .github/workflows/website.yml (wrangler.toml beside
// this file); the destination is the secret FEEDBACK_TO, a verified Email Routing destination address.
//
// Takes JSON (the page's script) or a plain form post (no script: answered with a page). Spam: the page's honeypot
// field `website`, a per-address rate limit (the LIMIT binding), posts from the site's own pages only, and sizes capped.
import { EmailMessage } from "cloudflare:email";

const FROM = "feedback@actionfootageplayer.ch";
// The fields and the most of each that's kept
const ANSWERS = { shoot: 2000, compare: 10, stuck: 4000, email: 200 };
const TECH = { v: 120, os: 120, camera: 120, curve: 120, motion: 120 };
const COMPARE = { better: "Better", same: "The same", worse: "Worse" };
const TECH_NAMES = { v: "Player", os: "System", camera: "Camera", curve: "Log curve", motion: "Motion data" };

export default {
  async fetch(request, env) {
    const json = (request.headers.get("Content-Type") || "").includes("application/json");
    const answer = (status, error) => json || status !== 200
      ? Response.json(error ? { error } : { ok: true }, { status, headers: { "Cache-Control": "no-store" } })
      : Response.redirect(new URL("/feedback?sent=1", request.url).toString(), 303);

    if (request.method !== "POST") return answer(405, "Send the form from actionfootageplayer.ch/feedback.");
    const origin = request.headers.get("Origin");
    // Only the site's own page: a browser's post says where it comes from, the same host as this route
    if (origin && hostOf(origin) !== new URL(request.url).host) return answer(403, "Not from the site.");
    if (Number(request.headers.get("Content-Length") || 0) > 32768) return answer(413, "That's too long.");

    let body;
    try {
      body = json ? await request.json() : Object.fromEntries(await request.formData());
    } catch {
      return answer(400, "The form didn't come through whole. Try again.");
    }
    if (!body || typeof body !== "object") return answer(400, "The form didn't come through whole. Try again.");
    const field = (k, max) => (typeof body[k] === "string" ? body[k] : "").trim().slice(0, max);

    // A bot filled in the field people don't see: it's told it went, and nothing is sent
    if (field("website", 200)) return answer(200);

    const ip = request.headers.get("CF-Connecting-IP") || "unknown";
    if (env.LIMIT) {
      const { success } = await env.LIMIT.limit({ key: ip });
      if (!success) return answer(429, "That's a lot of feedback at once. Try again in a minute.");
    }

    const a = Object.fromEntries(Object.entries(ANSWERS).map(([k, max]) => [k, field(k, max)]));
    const t = Object.fromEntries(Object.entries(TECH).map(([k, max]) => [k, field(k, max).replace(/[\r\n]+/g, " ")]));
    if (!a.shoot && !a.stuck && !COMPARE[a.compare]) return answer(400, "Answer at least one question.");
    const email = /^[^\s@<>"]+@[^\s@<>"]+\.[^\s@<>"]+$/.test(a.email) ? a.email : "";

    const lines = [
      "1. Which camera, and how do you shoot?", a.shoot || "-", "",
      "2. Compared to how you watched your clips before: " + (COMPARE[a.compare] || "-"), "",
      "3. Where did you get stuck, or anything else?", a.stuck || "-", "",
      "Email: " + (email || (a.email ? a.email + " (doesn't look like an address)" : "-")), "",
      "From the player:",
      ...Object.keys(TECH).map((k) => "  " + TECH_NAMES[k] + ": " + (t[k] || "-")),
    ];
    const subject = "Feedback: " + [COMPARE[a.compare], t.camera || null, t.v ? "v" + t.v.replace(/^v/, "") : "from the site"]
      .filter(Boolean).join(" · ");

    if (!env.EMAIL || !env.FEEDBACK_TO) return answer(503, "Feedback isn't set up yet. Write to hello@actionfootageplayer.ch.");
    try {
      await env.EMAIL.send(new EmailMessage(FROM, env.FEEDBACK_TO, mime({ to: env.FEEDBACK_TO, replyTo: email, subject, text: lines.join("\n") })));
    } catch (e) {
      console.log("send failed", String(e));
      return answer(502, "It didn't go through. Try again, or write to hello@actionfootageplayer.ch.");
    }
    return answer(200);
  },
};

function hostOf(url) {
  try { return new URL(url).host; } catch { return ""; }
}

// A plain-text message, its text in base64 (any language, any line length), its subject as RFC 2047 words
export function mime({ to, replyTo, subject, text }) {
  const b64 = (s) => {
    let bin = "";
    for (const b of new TextEncoder().encode(s)) bin += String.fromCharCode(b);
    return btoa(bin);
  };
  const id = crypto.randomUUID() + "@actionfootageplayer.ch";
  const head = [
    `From: "Action Footage Player feedback" <${FROM}>`,
    `To: <${to}>`,
    ...(replyTo ? [`Reply-To: <${replyTo}>`] : []),
    `Subject: =?UTF-8?B?${b64(subject.replace(/[\r\n]+/g, " "))}?=`,
    `Message-ID: <${id}>`,
    `Date: ${new Date().toUTCString()}`,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=utf-8",
    "Content-Transfer-Encoding: base64",
  ];
  return head.join("\r\n") + "\r\n\r\n" + b64(text).replace(/.{1,76}/g, "$&\r\n");
}
