# docs-site

The Kavka website. Two files: `index.html` and `styles.css`.

## Running it

Open `index.html` in a browser. That is the whole workflow — there is no build
step, no dependency, no dev server, no `npm install`. Every path in the page is
relative, so it renders identically from `file://`, from a project site under a
path prefix (`https://sahilhirani.github.io/kavka/`) and from a domain root
(`https://kavka.io/`).

That constraint is the point rather than an omission. A marketing page for a
desktop app is edited a few times a year, usually in a hurry, often by someone
who has not touched it since the last release. A page with a toolchain is a page
whose toolchain has rotted by then.

## Design

It is built on the app's own tokens — see `docs/DESIGN.md`. The rules that
apply here as much as they do in the product:

- **No cards.** Grouping is whitespace plus a top hairline. The feature grid,
  the download columns and the screenshot frames all follow that; none of them
  has a border, a radius or a shadow.
- **The ledger rule is the signature.** Each section carries a mono address in a
  fixed left gutter with a 1px rule beside it, exactly as the app's tables carry
  offsets and broker ids. It collapses below 900px and the rule stays — the same
  behaviour the app has when a table runs out of width.
- **The accent means live.** `--accent` appears twice on the page: the wire
  across the top, and one word in the headline. It is never a button fill.
- **Nothing moves.** Two colour transitions on hover, and a
  `prefers-reduced-motion` block that removes even those.

The token values are **copied** into `styles.css` rather than imported. The site
has to render from a static host with no relationship to the app's build, so it
cannot reach into `apps/desktop/src/styles.css`. If the audited values are ever
re-derived there, this file is the second place to change.

## Screenshots — all still TODO

Search either file for `TODO` — there are three:

| Marker | What belongs there |
|---|---|
| `TODO — screenshot` (features) | The message browser on a busy topic: live tail running, inspector open on a JSON payload, dev cluster. |
| `TODO — screenshot` (AI & MCP) | A Claude Code transcript browsing a topic through the MCP server. Better story than the config dialog. |
| `TODO(human)` (`<head>`) | `og-card.png`, 1200×630, plus an absolute `og:image` URL — Open Graph rejects relative ones, so the tag is deliberately absent rather than broken. |

Each screenshot placeholder currently renders a mono skeleton of the view that
belongs there, in `--text-absent`, which is the same device `docs/DESIGN.md` §7
prescribes for an empty state. **Do not replace them with `<img>` tags before
the files exist** — a broken image icon is a worse placeholder than an honest
one. The replacement markup is written out in a comment above each figure.

Capture at 1440×900 on a 2× display, dark theme, default density, on a **dev**
cluster. A screenshot of a production cluster puts a real bootstrap address on
the internet, and the coral guardrail makes the page look like something is
wrong.

## Publishing to GitHub Pages

Not wired up, deliberately — the repository is private until launch, and
switching Pages on is one of the things that makes it public.

When it is time, the smallest version that works:

1. **Settings → Pages → Source: Deploy from a branch**, branch `main`, folder
   `/docs`. GitHub Pages only serves the repository root or `/docs`, and
   `/docs` is taken by the project's specification. So either
   - rename this directory to `docs/` and move the specification elsewhere, or
   - add a Pages workflow that uploads `docs-site/` as the artifact
     (`actions/upload-pages-artifact` with `path: docs-site`), which keeps both
     directories where they are and is what this repository should do.
2. `.nojekyll` is already here so Pages serves the files as they are instead of
   running them through Jekyll.
3. For the custom domain, add a `CNAME` file containing `kavka.io` **after** the
   domain is registered and its DNS points at GitHub, and turn on *Enforce
   HTTPS*. The domain was verified unregistered on 2026-08-02 — registering it
   is a human action and nothing in this repository can do it.
