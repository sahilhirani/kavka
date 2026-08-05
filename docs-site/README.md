# docs-site

The Kavka website. Two files: `index.html` and `styles.css`.

## Running it

Open `index.html` in a browser. That is the whole workflow — there is no build
step, no dependency, no dev server, no `npm install`. Every path in the page is
relative, so it renders identically from `file://`, from a project site under a
path prefix (`https://sahilhirani.github.io/kavka/`, or a custom domain later)
and from a domain root. The single exception is `og:image`, which Open Graph
requires to be absolute — if the site ever moves, that one tag moves with it.

**One thing does not come for free from `file://`: the screenshots.** They live
in `docs/screenshots/` and the page references them as `screenshots/<name>`,
because that is the layout the Pages workflow builds — it copies them into the
site root rather than duplicating them in git. To preview locally, do the same
copy once:

```sh
cd docs-site
cp -r ../docs/screenshots .     # or: ln -s ../docs/screenshots .
```

`docs-site/screenshots/` is gitignored, so the copy can never be committed by
accident. The alternative — pointing the page at `../docs/screenshots/` — would
preview beautifully and 404 on the deployed site, where there is no parent
directory to climb into.

That constraint is the point rather than an omission. A marketing page for a
desktop app is edited a few times a year, usually in a hurry, often by someone
who has not touched it since the last release. A page with a toolchain is a page
whose toolchain has rotted by then.

## Design

It is built on the app's own tokens — see `docs/DESIGN.md`. The rules that
apply here as much as they do in the product:

- **Soft raised panels.** The feature grid and the download columns are real
  surfaces — `--bg-panel`, one hairline, `--r-md` and `--shadow-1` — because
  that is what a panel is in the app now. There is exactly one shadow token in
  use here; if a rule reaches for a second elevation, it is wrong.
- **Two signatures, both real.** The page opens with a **Perch**: the bird, the
  screen and state in words, one sentence of verdict and the caveat printed
  beside it — the same shape `Perch.tsx` renders. And the **ledger rule** still
  runs down the left of every section, a mono address in a fixed gutter with a
  1px rule beside it, exactly as the app's tables carry offsets and broker ids.
  It collapses below 900px and the rule stays, the same behaviour the app has
  when a table runs out of width.
- **The accent means nothing.** Brass appears on the wire, the primary button,
  link underlines and one word in the headline. In the product it marks only
  what you can click and what is selected, which is what makes the accent
  picker in Settings safe; nothing on either surface is spelled in it.
- **Both themes.** Dark warm brown-grey is the default and the light block is
  the app's warm paper, keyed on `prefers-color-scheme`. Every colour token is
  declared exactly twice, once per theme — a token declared in only one of them
  is a half-themed control waiting to happen.
- **Almost nothing moves.** Two colour transitions on hover, and a
  `prefers-reduced-motion` block that removes even those. The one exception is
  the live-tail GIF, and it is handled rather than excused — see below.
- **No panel around the screenshots.** A capture already has a window frame
  around it and the app's own panels drawn inside it; a third surface with its
  own radius and shadow is a frame around a frame around a frame. Top hairline,
  whitespace, caption.

The token values are **copied** into `styles.css` rather than imported. The site
has to render from a static host with no relationship to the app's build, so it
cannot reach into `apps/desktop/src/styles.css`. If the audited values are ever
re-derived there, this file is the second place to change.

## Screenshots

The placeholders are gone; the page now carries real captures from
`docs/screenshots/`:

| Where | File | Why that one |
|---|---|---|
| Features | `06-message-browser.png` | The product in one frame: the table, the inspector, a decoded payload. |
| Features | `live-tail.gif` | The one thing a still cannot show — records arriving. |
| Features | `live-tail-still.png` | Not shown by default: the GIF's own last frame, served instead of it under `prefers-reduced-motion`. |
| AI & MCP | `08-about.png` | The About panel's MCP section, with the path filled in and the copy button. A screenshot of the thing you press beats a screenshot of somebody else's assistant. |

Rules for any capture added later:

- **Capture on the dev cluster** (`dev/docker-compose.yml`), dark theme,
  brass accent, comfortable density. A production screenshot puts a real
  bootstrap address on the internet, and a protected environment's warm
  substrate and lit wire make the page look like something is wrong.
- **Every `<img>` carries `width`, `height`, `loading="lazy"` and real `alt`
  text.** The dimensions reserve the box so nothing below it jumps; the alt
  text describes what is on the screen, not what the section is about. Both are
  the same discipline the app is audited to (`docs/A11Y-AUDIT.md`).
- **Anything that moves needs a still.** The live-tail GIF loops for about 16
  seconds, which is past WCAG 2.2.2's five-second line for auto-playing motion,
  and a GIF has no pause control. `<picture>` therefore serves
  `live-tail-still.png` — its own last frame — to anyone whose OS asks for
  reduced motion. No script; the media query does it.

Still open:

| Marker | What belongs there |
|---|---|
| `TODO(human)` (`<head>`) | `og-card.png`, 1200×630, purpose-built. `og:image` currently points at the message-browser screenshot, which is 1296×839 (≈1.54:1) against the card slot's 1.91:1 — every platform crops it differently and the inspector is what goes. A real card would carry the name, the one line and the licence at a size that survives a timeline. |

## Publishing to GitHub Pages

Wired up and live at **<https://sahilhirani.github.io/kavka/>**.

[`.github/workflows/pages.yml`](../.github/workflows/pages.yml) does it on every
push to `main` that touches `docs-site/**` or `docs/screenshots/**`. It is the
artifact-upload route rather than *Deploy from a branch*, because Pages only
serves the repository root or `/docs`, and `/docs` is taken by the project's
specification — so the workflow assembles a `_site/` instead:

```sh
cp -r docs-site/. _site/       # the page, minus this README
cp docs/screenshots/*.png docs/screenshots/*.gif _site/screenshots/
```

That copy is the whole "build". It is also why the page says
`screenshots/<name>` and why a local preview needs the same copy by hand
(see *Running it* above).

`.nojekyll` is here so Pages serves the files as they are instead of running
them through Jekyll.

**Still a human action:** a domain of Kavka's own. `kavka.io` was verified
unregistered on 2026-08-02; if it is ever registered, add a `CNAME` file here
containing it, point its DNS at GitHub, turn on *Enforce HTTPS* — and update the
absolute `og:image` in `index.html`, which is the one URL in this directory that
does not move by itself.
