# Website

The product website for Action Footage Player: a static site (plain HTML, CSS and one script, no build step),
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
  styles.css     shared styles, light and dark
  scene.js       the drawn before/after illustration in the hero (no real footage yet)
```

## Preview

```sh
python3 -m http.server -d website/site 8000
# then open http://localhost:8000
```

Opening `site/index.html` straight from disk works too. The fonts come from Google Fonts; everything else is
local.

## Deployment

Cloudflare Pages, project `actionfootageplayer`: <https://actionfootageplayer.pages.dev>, and `actionfootageplayer.ch`
(with `www.`) once the domain's nameservers point to Cloudflare. `.github/workflows/website.yml` publishes `site/` on
every push that changes it, to the `website` branch or `main`; it can also be run by hand from the Actions tab.

The release workflow runs on every pull request, so a site-only pull request also triggers a full build; a
`paths-ignore: ['website/**']` on its `pull_request` trigger would avoid that.
