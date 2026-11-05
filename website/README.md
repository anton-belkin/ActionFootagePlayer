# Website

The product website for Action Footage Player: a static site (plain HTML, CSS and three scripts, no build step),
kept here so it can move to its own repository later without changes (`git subtree split --prefix=website`).

Status: **draft, deployed** (see Deployment). Written between 2026-10-25 and 2026-10-29 and moved here on 2026-11-02.

> **About the facts on the pages.** What the site says about the player itself (features, keys, cameras,
> formats, system requirements) was taken from this repository's README around 2026-10-25. Work on the player
> has gone on since: 13 commits landed between 2026-10-27 and 2026-10-30 alone (filled edges on the sphere,
> the lens in or out of water, color fringes under water, the clean still frame, the app's version in the
> menu, …). Those facts were **not** brought up to date when the site moved here. Check them against the
> README before publishing.

## Layout

```
site/            what is served, as is
  index.html     the hub; routes to the landings
  divers.html    the landing for divers and snorkelers
  moments.html   the landing for people who shoot a lot (often in log): moments, marks, sharing, the editor
  features.html  every feature on one page, the reference the other pages link to (footer: All features)
  hub.css, divers.css, moments.css, features.css
                 each page's own styles, next to the shared styles.css
  media/         real footage from an Osmo Action 6: stab-underwater.mp4 (+ .jpg poster; as recorded | as it
                 plays), log-grey-color.jpg (grey log | in color), player-marks.jpg (the camera's marks on the
                 seek bar). No people's faces in them
  download.html  the downloads: the app for macOS and Windows, the plugins alone, the release's notes
  download.js    fills download.html in from latest.json: each link straight to its file, its size, the version
  styles.css     shared styles, light and dark
  top.js         measures the sticky header, so links to #anchors land below it with the heading in view; opens a
                 FAQ <details> a link points to; and sets <html data-os="mac|win"> for the visitor's own words
                 (Finder or File Explorer, …) and download order: see "The visitor's system" below
  scene.js       the drawn before/after illustration in the hero (no real footage yet)
  icon.svg       the icon in the header and the tab: macos/icon/AppIcon.svg cropped to the icon itself
  favicon.ico, apple-touch-icon.png
                 rendered from icon.svg (the touch icon with square corners: iOS rounds them)
  fr/ ru/ zh/    the same pages, machine-translated (see Translations)
```

## Preview

```sh
python3 -m http.server -d website/site 8000
# then open http://localhost:8000
```

Opening `site/index.html` straight from disk works too. The fonts come from Google Fonts; everything else is
local.

`download.html` links to GitHub's latest release, and works like that without JavaScript. `download.js` reads
that release from `/latest.json`, which the deploy writes from
<https://api.github.com/repos/anton-belkin/ActionFootagePlayer/releases/latest> (only into the deployed copy), and
points each link at its file, found by its name's pattern (`ActionFootagePlayer-*-macos-arm64.dmg`, …). The release
workflow deploys the site again once it has published a release, so the page has it within a minute or two. Where
there's no `latest.json`, as in a local preview, it asks GitHub's API itself (kept in `localStorage` for 5 minutes;
the API allows 60 requests an hour per address, and when it refuses, the release kept from an earlier visit stays).
A new release needs no change here, unless an asset's name changes: then its pattern in `download.js` too.

## Translations

French, Russian and Simplified Chinese live in `site/fr/`, `site/ru/` and `site/zh/`, under the same file names as the
English pages at the root, and use the shared `styles.css`, `scene.js`, `download.js` and icons through `../`. They are
served at `/fr/`, `/ru/`, `/zh/` (and `/fr/divers`, `/fr/download` …). English is the original; the translations are **machine translations**
(made with Claude, 2026-11-03) and each translated page says so in its own language, in a slim banner under the header
and again in the footer, with a link to the English page.

Every page, English included, carries:

- `<html lang>`: `en`, `fr`, `ru`, `zh-Hans`;
- `<link rel="alternate" hreflang>` for all four languages and `x-default` (English), with absolute
  `https://actionfootageplayer.ch/…` URLs;
- the language switcher in the header: a `<details>` disclosure, so it works without script, linking to the same
  page in each language.
- the header's links are the same on every page and in every language: Under water (divers), Hours of clips
  (moments), All features, Download, with the page you're on marked by `aria-current`; links to a page's own
  sections stay in the page. Below 760px they take a second row of the header; French uses the short labels
  (Heures de clips, Fonctions) so all four fit a 375px phone.

Fonts: Archivo, the display face, covers Latin only. Russian pages load Roboto Flex (Cyrillic, with a width axis like
Archivo's) for headings; IBM Plex Sans and Mono cover Cyrillic. Chinese uses the system's CJK face after the Latin
ones (PingFang SC, Hiragino Sans GB, Noto Sans SC, Microsoft YaHei); nothing CJK is downloaded. Both are set in
`styles.css` with `:lang()` rules.

Kept in English everywhere: product, camera and format names (D-Log M, GP-Log, I-Log, HDR, EIS, Gyroflow, …), keyboard
shortcuts, file names, and the player's on-screen telemetry in the illustration.

`download.js` writes a few words of its own (the release's title, file sizes, dates): it takes them from its `LANGS`
table by the page's `<html lang>`, and dates come from the browser in that language. The first paragraph of the
release notes it puts in stays in English, marked `lang="en"`.

**After editing the English text**, bring the translations along in the same pull request, or the languages drift
apart:

1. See what changed since the translations were last synced:
   `git log -1 --format=%h -- website/site/fr/` gives the last sync, then
   `git diff <that commit> -- website/site/index.html website/site/divers.html website/site/download.html`.
2. Apply the same change to `fr/`, `ru/` and `zh/` with the text translated, keeping the markup identical to the
   English page (asset paths with `../`, the switcher, the notices). Asking Claude to "re-sync the translations
   with this diff" works well.
3. A new page: copy the hreflang links and the switcher from an existing page and change the file name in them; then
   put a translated copy in each language folder, with `../` asset paths, its own `lang`, the current language
   marked in the switcher (`aria-current="page"`) and the two notices, as on the other translated pages.

## Deployment

Cloudflare Pages, project `actionfootageplayer`: <https://actionfootageplayer.pages.dev>, and `actionfootageplayer.ch`
(with `www.`) once the domain's nameservers point to Cloudflare. `.github/workflows/website.yml` publishes `site/` on
every push that changes it, to the `website` branch or `main`; it can also be run by hand from the Actions tab.

Before publishing, the workflow adds each file's content hash to the pages' links to `styles.css`, `scene.js` and
`download.js` (`styles.css?v=7a7a2bf0`), in the deployed copy only. A changed stylesheet or script gets a new URL, so a
browser can't pair a new page with an old copy from its cache: Safari did, showing the language switcher unstyled
after it landed. Nothing to do by hand; the HTML in the repository keeps the plain names.

The release workflow runs on every pull request, so a site-only pull request also triggers a full build; a
`paths-ignore: ['website/**']` on its `pull_request` trigger would avoid that.

## The visitor's system

Where a page names something of the desktop (Finder or File Explorer, Quick Look or the preview pane, Finder tags
or star ratings), it has both, side by side: `<span class="os-mac">Finder</span><span class="os-win">File
Explorer</span>` (or a pair of `<p>`s). `top.js` sets `<html data-os>`: `mac` on a Mac, iPhone or iPad, `win`
anywhere else (Windows; Android and Linux visitors are taken to use Windows). `styles.css` hides the other one;
without script, the Mac's shows. A pair of download buttons in a container marked `data-os-swap`, with `.dl-mac` and
`.dl-win`, gets the visitor's first and primary. Facts that are Apple's only (Dolby Vision on iPhones, iPads and
Macs, the Mac app's own features) are the same for everyone. Translations carry both variants too.
