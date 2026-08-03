# Kavka Design System — "Ledger"

The rules every Kavka screen follows. Read this before changing anything under
`apps/desktop/src/`. It is written to be usable by a human contributor or an AI
agent with no other context.

**Status:** dark theme ships. Light is specced, audited and **not exposed** —
see §10. Implemented in `apps/desktop/src/styles.css`.

---

## 1. The three laws

Everything else in this document follows from these. If a proposal violates one,
the proposal is wrong, not the law.

### Law 1 — Elevation is colour and one hairline

Shadows exist **only on true overlays**: modal, command palette, popover, toast.
Never on a card, a row, a chip, a button, a banner or a table. There are exactly
two shadow tokens, `--shadow-popover` and `--shadow-overlay`, and if you are
reaching for a third you are building a card.

**Kavka ships zero cards.** Grouping is whitespace plus a top hairline.
Emphasis is type weight plus one of five text lightness values. The loudest
pixels on screen are always topic names, offsets and payloads — never chrome.

### Law 2 — No state is ever encoded by colour alone

Every dot has a word. Every severity has a glyph. Every lag figure has a bar
length **and** a number **and** a trend arrow. The prod signal has four
independent channels. If you can only tell the difference by hue, it is broken.

**"Every dot has a word" includes the quiet states.** A status map with
`disconnected: ""` is the bug this law is written to prevent: the one state
rendered as a hollow ring — the state hardest to read at a glance — was also
the only one with no word anywhere in its row. Every status produces text, so
the sidebar's second line reads *"address · state"* in **every** state, and the
dot stays `aria-hidden` decoration.

This is WCAG 2.2 SC 1.4.1, but it is mostly just correct: a support engineer
with deuteranopia has to be able to tell prod from dev at a glance.

### Law 3 — Nothing inside a scrolling table gets a `transition`, `filter`, `backdrop-filter`, `border-radius` or `box-shadow`

`--row-h` is a fixed token the virtualizer reads at runtime. This is a
performance contract with the Phase 1 message browser, written into the token
names so it survives six phases. At 10,000 rows, a 120ms hover transition is
visible jank.

---

## 2. The signature: the ledger rule

A fixed left gutter carries the row's address **in Kafka's own vocabulary** —
offset, broker id, partition index, line number — then a 1px vertical rule runs
the full height of the data, then the payload.

The rule is coloured by environment. **Prod is therefore visible in every data
view in the app without a single banner.** It is the brand, the information
architecture and the guardrail, in six lines of CSS.

```css
.ledger-gutter {
  width: var(--gutter-w);
  padding-right: var(--s-5);
  text-align: right;
  font-family: var(--font-mono);
  color: var(--text-tertiary);
  border-right: 1px solid var(--rule);
}
```

| Table | Gutter carries |
|---|---|
| Messages | offset, space-grouped (`8 412`) |
| Partitions | partition index |
| Brokers | broker id |
| Payload inspector | line number (`--gutter-w-code`) |
| Diff views (payload / config / schema) | `+` / `−` |
| Topics, groups, ACLs, connectors, subjects | *nothing* — `--gutter-w: 0`, rule flush at the table's left edge (`.data-table-flush`) |

**The rule is always present, even when the gutter is empty.** It is the app's
constant.

**Collapse rule:** below 1100px of available table width, or whenever the
inspector is open, set `--gutter-w: 0` while keeping the rule; the offset
becomes a normal first column. Without this the signature element is the thing
that breaks the layout at 1280px.

> **The trigger is a container query on `.table-wrap`, not a viewport media
> query.** "Available table width" is not the window: the rail, the inspector
> dock and any split view all steal width without the window changing size, so
> a viewport query collapses the gutter at the wrong moments in both
> directions. Safari 15.6 — the same floor that rules out `color-mix()` — has
> no container queries, so a `@supports not (container-type: inline-size)`
> block keeps the old viewport rule as a fallback there.

### Teal means live. Only live.

`--accent` is connected status, live tail, and selection. **Never** a button
fill, never a heading, never a border for its own sake. The primary button is
near-white, which is what frees the accent to keep this meaning.

**The status-adjacency law:** in any table with a health or lag column,
`--accent` may not appear in an adjacent column, and lag fills use
`--ok`/`--warn`/`--danger` only.

---

## 3. Token reference

Full source of truth: the `:root` block in `apps/desktop/src/styles.css`. Ratios
below are measured, not estimated, and there are **zero contrast failures**
across 12 dark/prod surfaces and 6 light surfaces.

### Surfaces

| Token | Value | Use |
|---|---|---|
| `--bg-rail` | `#090B0D` | Sidebar — recedes behind the stage |
| `--bg-canvas` | `#0F1317` | The stage; all data lives here |
| `--bg-raised` | `#161B20` | Palette, modals, toasts, popovers **only** |
| `--bg-sunken` | `#0A0D10` | Inputs, payload inspector, code |
| `--bg-row-hover` | `#161C22` | |
| `--bg-row-selected` | `#152227` | Teal-tinted; primary text 13.57:1 |
| `--bg-row-prod-hover` | `#241816` | Prod sidebar rows (guardrail layer 6) |
| `--bg-row-prod-selected` | `#2A1B18` | Primary text 13.80:1 |
| `--skeleton` | `#28313A` | Skeleton bars, 1.41:1 — a placeholder, never content |
| `--bg-scrim` | `rgba(5,7,9,.55)` | No blur — perf, and blur is the tell of a templated design |
| `--selection` | `rgba(79,195,176,.28)` | Precomputed, **not** `color-mix()` |

> **Why no `color-mix()`:** macOS 12 ships WKWebView/Safari 15.6. Every alpha
> value in this system is precomputed.

### Text — six values. Hierarchy lives here, not in boxes.

| Token | Value | Ratio on canvas | Use |
|---|---|---|---|
| `--text-primary` | `#E6EBF0` | 15.55:1 | Values, headings, topic names |
| `--text-secondary` | `#A2AEBB` | 8.27:1 | Prose, help text |
| `--text-tertiary` | `#808D9A` | 5.50:1 | Labels, column heads, metadata |
| `--text-disabled` | `#657280` | 3.79:1 | **`:disabled` controls only.** Never a placeholder |
| `--text-placeholder` | `#7E8A97` | 5.30:1 | `::placeholder` — 4.63:1 worst case |
| `--text-absent` | `#7E8A97` | 5.30:1 | The `∅` glyph — 4.63:1 worst case |
| `--text-mono` | `#C9D4DF` | 12.41:1 | Literals copied out of Kafka |
| `--text-inverse` | `#0B0E11` | 16.13:1 | On the near-white primary fill |

> **`--text-absent` is TEXT, not a graphic.** It was `#6E7A86` and rated
> against the 3:1 graphics floor, which is the wrong floor: `∅` is a
> character a sighted user reads to learn the value is absent, so SC 1.4.3
> applies and it needs 4.5:1. On a selected row it measured **3.71:1** — the
> one place a user is most likely to be looking. Now 4.63:1 worst case
> across all twelve dark + prod surfaces.
>
> **`--text-placeholder` exists so `--text-disabled` can mean one thing.**
> A placeholder carries the example format (`broker-1:9092`) and is meant to
> be read; `--text-disabled` measured **3.53:1** on a raised surface. The two
> tokens currently hold the *same value*, because §9's twelve-surface sweep
> at 4.5:1 binds them both to the same floor. They stay separate tokens
> because they are separate decisions — a future theme may split them, and a
> shared literal must not let one drift by accident.

### Lines

| Token | Value | Use |
|---|---|---|
| `--hairline` | `#1B2127` | Decorative separation — SC 1.4.11 exempt |
| `--hairline-strong` | `#262E36` | Header underline, dock edges. **Decorative only** |
| `--border-control` | `#636F7C` | **Required on every operable boundary** |

> `--bg-sunken` is only 1.13:1 against the canvas, so `--border-control` is the
> **only** thing satisfying SC 1.4.11 on an input. It clears 3:1 on all twelve
> dark + prod surfaces, worst case 3.17 on a selected row. Do not remove it to
> make inputs "cleaner".
>
> **`--hairline-strong` is never a control boundary.** It is 1.18:1 on a
> selected row. `.btn` and `.new-connection-btn` used it and were therefore
> operable elements with no perceivable edge; both now take
> `--border-control`. The rule to apply when in doubt: *if the element
> responds to a click, its border is `--border-control` or
> `--danger-border` — never a hairline.*

### Accent, semantic, environment

| Token | Value | Notes |
|---|---|---|
| `--accent` | `#4FC3B0` | 8.67:1 — connected · live tail · selected |
| `--accent-quiet` | `#3C9788` | 5.31:1 — hairline-weight accent |
| `--accent-tint` | `#12241F` | Chip background; accent on it 7.52:1 |
| `--focus` | `#6FE3CD` | 12.03:1 — **always** with `outline-offset` |
| `--fill-primary-hover` | `#FFFFFF` | The primary button's hover fill |
| `--ok` / `--warn` / `--danger` | `#56C08A` / `#D9A441` / `#E5786B` | 8.26 / 8.29 / 6.42 |
| `--danger-fill` / `--danger-fill-hover` | `#A63C31` / `#B84439` | 6.34 / 5.35 with `--on-danger-fill` |
| `--on-danger-fill` | `#FFFFFF` | The only ink that goes on a danger fill |
| `--danger-border` | `#A85C50` | 3.34:1 worst (selected dark row); 3.56 raised — the outlined danger button's boundary |
| `--track-empty` | `#333C46` | The unfilled half of a lag meter |
| `--ok-tint` / `--warn-tint` / `--danger-tint` | `#101E18` / `#241D0F` / `#1B1315` | |
| `--rule` | `#242B33` dev · `#8C7036` staging · `#A65246` prod | Staging 3.98:1, prod 3.54:1 canvas / 3.12:1 on a selected prod table row / **3.09:1 on a selected prod sidebar row** — meaningful, so they clear 3:1 |
| `--env-ink` / `--env-tint` | accent / warn / danger | |
| `--env-chip-prod-ink` / `--env-chip-prod-fill` | `#FFE4DF` / `#8E3A31` | 6.22:1 — prod is the only *filled* chip |
| `--env-wire` | transparent · transparent · `#B8453A` | 3.57:1 — the bar is a meaningful indicator |

> **Every operable boundary is opaque and precomputed.** `--danger-border`
> replaced `rgba(229,120,107,.45)`, which composited to **2.20:1** on the
> canvas: an alpha border does not have a contrast ratio until you know what
> is behind it, so it cannot be audited and it silently fails SC 1.4.11 the
> moment the surface underneath changes. Same reason as `--selection`: no
> alpha anywhere a ratio has to be provable.
>
> **`--track-empty` is the adjacent colour inside a lag meter, and every lag
> fill clears 3:1 against it** (ok 4.96, warn 4.98, danger 3.86). It was
> `--hairline-strong` at 1.36:1 on the canvas, which made the meter's *extent*
> invisible — and a bar whose total you cannot see is not a proportional
> indicator, it is a coloured smear. `.lag-fill` also carries `min-width: 2px`
> so a small non-zero lag never rounds down to "caught up".

> **Dev's rule is decorative (1.30:1) by design.** Dev has no warning to give,
> and the absence of a coloured rule is itself the signal.

### Payload syntax

All AA on `--bg-sunken` **and** on `--syn-match`.

`--syn-key` `#8FA6BC` 7.74 · `--syn-string` `#9FD5A8` 11.65 ·
`--syn-number` `#E0B36A` 10.04 · `--syn-atom` `#B99BD4` 8.07 ·
`--syn-punct` `#7B8899` 5.40 (braces carry structure, so they are AA, not
decorative) · `--syn-match` `#132028`.

The search-hit background is neutral-cool so the worst syntax token still clears
4.60:1, and it is **always paired with a 2px `--accent` underline** — the hit
must not be colour-only.

### Data series

`--series-1…6`: `#4FC3B0` `#6FA8DC` `#B99BD4` `#D9A441` `#E5786B` `#86C98A` —
one lightness band, so no series shouts.

### Type

- `--font-ui`: `system-ui, -apple-system, "Segoe UI Variable Text", "Segoe UI", …`
- `--font-mono`: `ui-monospace, "SF Mono", "Cascadia Mono", "Segoe UI Mono", …`

| Step | Size / line-height | Use |
|---|---|---|
| `--t-micro` | 10.5 / 14 | Uppercase eyebrow, tracked |
| `--t-xs` | 11 / 15 | kbd, footnotes, status bar |
| `--t-sm` | 12 / 17 | Secondary UI, chips |
| `--t-base` | 13 / 19 | **Default — all data** |
| `--t-md` | 15 / 21 | Panel + modal titles |
| `--t-lg` | 19 / 25 | Page title |
| `--t-xl` | 24 / 30 | Empty-state headline |
| `--t-display` | 30 / 36 | Hero stat numerals |

Weights `400 / 500 / 600 / 700`. `--track-eyebrow: .08em` on uppercase micro
labels only. `--track-tight: -.011em` at ≥19px only. `--nums-tabular` on every
computed quantity.

### Space, metrics, shape, motion

4px base: `--s-1` 2 → `--s-12` 72.

`--rail-w` 240 · `--topbar-h` 44 · `--tabstrip-h` 30 · `--statusbar-h` 24 ·
**`--row-h` 30** · `--row-h-head` 28 · `--control-h` 28 · `--control-h-lg` 32 ·
`--gutter-w` 72 · `--gutter-w-code` 40 · `--inspector-w` 480 · `--palette-w` 580
· `--measure` 62ch · **`--hit-min` 24px** (SC 2.5.8 — nothing operable may be
smaller).

Radius: `--r-control` 4 · `--r-popover` 6 · `--r-overlay` 10 · `--r-pill` 999.
**Tables, panels, page sections, stat blocks and banners have no radius and no
border by design.** If you are reaching for a card, use space.

Motion: `--dur-instant` 0 (row hover, cell select — data answers *now*) ·
`--dur-fast` 120 · `--dur-base` 180 · `--dur-slow` 260. `--ease-settle` is for
the **palette and toasts only**.

### Density

Three steps, persisted: `[data-density="compact"|"relaxed"]` on the app root.
The message browser defaults **one step denser** than the app. The app-level
toggle sets the preference; the browser's default is only its initial value,
never a lock.

---

## 4. The mono/sans law

> **`--font-mono` + `--text-mono` = a literal string that came out of Kafka and
> could be pasted into a `kafka-topics.sh` command** — topic name, cluster id,
> broker host, principal, group id, message key, header value, config key, CEL
> expression, payload, timestamp.
>
> **`--font-ui` + `tabular-nums` = a quantity Kavka computed** — partition
> count, lag, throughput, message count, broker count.

**Never mono for a count.** One law, and it does more for scannability than any
colour decision.

```jsx
{/* DO */}
<td className="cell-mono">{topic.name}</td>
<td className="col-num cell-num">{topic.partitions}</td>

{/* DON'T — a count is not a literal, and mono makes it look copy-pasteable */}
<td className="cell-mono">{topic.partitions}</td>
```

---

## 5. Component rules

### 5.1 Shell

Four regions plus a wire:

```
┌──────────────┬───────────────────────────────────────────┬─────────────┐
│ CLUSTERS     │ orders.v2 ×   checkout-svc ×           +  │             │  30px tabs
│ ● orders-prd │───────────────────────────────────────────│  INSPECTOR  │
│ 10.0.4.19:…  │ ⌕ Find in key, value or headers  [chips]  │  Value      │  44px topbar
│              │───────────────────────────────────────────│  ─────────  │
│ BROWSE       │ OFFSET │ TIMESTAMP  │ KEY   │ VALUE        │  {          │
│  Topics  128 │ 1204882│ 12:04:19.2 │ A-102 │ {"ord"…      │   "id": …   │
│              │ …virtualized…                              │  }          │
│ ──────────── │───────────────────────────────────────────│─────────────│
│ Kavka        │ Connected · orders · localhost:9092 · 12ms│  core v0.1.0 │  24px status
└──────────────┴───────────────────────────────────────────┴─────────────┘
                ↑ 1px --rule, full height of tbody, coloured by environment
```

- **Rail** — 240px, resizable 200–360, `Ctrl/Cmd+B`. Sections are collapsible
  with persisted state **from the start**: the sidebar outgrows the viewport by
  Phase 3. **If the sidebar becomes primary navigation, the design has failed** —
  `⌘K` is the real navigation.
- **Workspace** — tab strip → top bar → content → status bar.
- **Inspector dock** — 480px, resizable 320–60%, `Ctrl/Cmd+I`.
- **`.app-wire`** — 2px, painted across the very top of the window, inside the
  webview and below the native title bar. Native window chrome is untouched.
- **Status bar** (24px) — the permanent home for progressive search progress,
  tail rate, connection latency, the read-only chip, the alert counter, and the
  two most relevant shortcuts for the current view. Phase 2's "search never
  silently truncates" acceptance gate depends on this existing.
- **Tab strip** (30px) — active tab is `--text-primary` + 2px `--accent` bottom
  rule; inactive is `--text-tertiary`.

### 5.2 Tables — the load-bearing component

**No card, no radius, no outer border, no shadow, no zebra, no row separators.**

- **Header** — `position: sticky; top: 0`, **opaque** `--bg-canvas` so rows never
  ghost through, `--row-h-head`, `--t-micro`/600 uppercase `--text-tertiary`,
  bottom `1px solid var(--hairline-strong)`. The sort caret occupies a reserved
  8px slot **always**, so sorting never reflows the header.
- **Rows** — `--row-h` fixed. Leading alone separates them; the gutter rule
  anchors the eye. Hover paints the whole row with `transition: none`. Selected:
  `--bg-row-selected` + the gutter's `border-right` swaps to `2px solid
  var(--accent)`.
- **Alignment** — identifiers left + mono; quantities right + sans +
  `tabular-nums`; timestamps left + mono. **Mono column widths are `ch`-based**
  so nothing reflows as data streams in. **Alignment selectors must be
  qualified** — `.data-table th` sets `text-align: left` at specificity
  (0,1,1), so a bare `.col-num` at (0,1,0) loses and every numeric column
  *head* sits left above a right-aligned column. Write
  `.data-table th.col-num, .data-table td.col-num`, and the same for
  `.ledger-gutter`, so the two halves of a column can never disagree.
- **Special cells** — null/absent → `∅` in `--text-absent`, **never the word
  "null"**. Tombstone → gutter tick `--warn`, value reads `∅ tombstone`.
  Internal topics → whole row `--text-tertiary` (dimmed, **never hidden**, so
  nobody wonders where they went) **plus an inline `internal` tag and a `title`
  on the row saying what that means.** Dimming without an explanation just
  moves the question from "where did they go" to "why is that one grey".
- **Health** — `<span class="health"><i class="dot"></i>Lagging</span>`. Dot
  **plus word**, always.
- **Lag** — number, then a 40×3px `--track-empty` track filled proportional to
  threshold, then a trend arrow. Three channels. The fill clears 3:1 against
  the track, and carries `min-width: 2px` so non-zero never renders as zero.
- **Row affordance for novices** — on hover the last column reveals a ghost
  `View messages →`. The whole row is also clickable.
- **Loading** — **never a spinner over data.** A 2px `--accent` indeterminate bar
  slides along the bottom edge of the header row (`.table-loading`); existing
  rows stay readable and interactive. Skeleton rows only where the layout is
  known, and **no shimmer** (paint cost, no benefit).
- **Live tail** — new rows insert at the top with a 400ms fade from
  `--accent-tint`. The only row animation permitted, disabled above 200 rows/sec
  (batch and swap instead).
- **Perf contract** — see Law 3. `contain: strict` on the scroll viewport,
  **feature-detected** via `@supports`; degrade to plain virtualization.
- **Semantics — non-negotiable, and staged.** A **virtualized** table takes
  `role="grid"`, `aria-rowcount` set to the **total** row count including the
  header (not the rendered count), and `aria-rowindex` on every row. Without
  this a screen reader announces "row 1 of 20" for a 12,480-row topic.
  A **static, fully-rendered** table — everything shipping today — takes
  **none of them**: the implicit `<table>` semantics are already complete, so
  add only `<caption class="sr-only">` and `scope="col"`.

  > `role="grid"` is a promise of an interactive widget with arrow-key cell
  > navigation. Declaring it on a table that does not implement that makes
  > every row announce as a grid cell the user is expected to drive, and
  > nothing happens when they try. `aria-rowcount` earns its keep the day the
  > rendered count stops matching the real one — **not before.** Turn all
  > three on in the same PR as the virtualizer.

- **A view that lists things you cannot act on yet says so**, in one sentence
  under the panel head. The Topics table carries *"Message browsing arrives in
  an upcoming release — this is the live topic inventory for now."* A user who
  has just connected, clicks a topic and gets nothing does not conclude
  "not built yet"; they conclude "broken". The dead end costs more than the
  missing feature.
- **`--row-h` is read at runtime by the virtualizer.** Guard it with a dev-mode
  assertion comparing the token against the first row's measured
  `offsetHeight`, and fail loudly. Any density change, font change or platform
  override must invalidate the virtualizer's cache.

**Stat blocks** replace bordered tiles: no border, no background, no radius. A
`--t-micro` uppercase tertiary label above a `--t-display`/700 number with
`tabular-nums`, siblings separated by 40px, floating on the canvas.

### 5.3 Forms

- **Labels are sentence case.** `Bootstrap servers`, not `BOOTSTRAP SERVERS`.
  All-caps labels read as system output; sentence case reads as someone talking
  to you. Uppercase micro-caps survive **only** on table column headers, stat
  labels and section eyebrows.
- **Input** — `--control-h-lg`, `--bg-sunken`, `1px solid var(--border-control)`,
  `--r-control`, 10px inline padding. Mono when the value is machine data
  (bootstrap servers, principals, CEL, cert paths); sans for prose (connection
  name). Focus: `border-color: var(--accent)` plus the global ring.
- **Validate on blur, never on keystroke.** A novice typing `broker-` must not be
  told they are wrong four times. Editing a field *clears* its message; nothing
  ever *adds* one mid-keystroke.
- **Invalid** — `--danger` border (`.input-invalid`) **plus** `aria-invalid`
  **plus** a `--t-sm` `.field-error` message beneath naming the fix, not the
  failure: *"Use host:port, e.g. broker-1:9092"*. `aria-describedby` carries
  both the hint and the error.
- **Validation never goes to the global banner.** `validate()` returns
  `{ field, message } | null`, the form holds it in state, and a failed submit
  **focuses the offending control**. A message at the top of the workspace
  about a field 400px further down is a scavenger hunt, and it is announced
  detached from the input it is about. The banner is for conditions the
  *system* is in; validation is a condition a *control* is in.
- **Checkbox** — 18px box in a `grid-template-columns: 18px 1fr` with the hint in
  row 2 / column 2 (`.check-field`). 14px fails SC 2.5.8. The hint is a
  **sibling** of the label, wired with `aria-describedby`, so it describes the
  control instead of renaming it.
- **Fieldsets lose their borders entirely** — a fieldset is a `--t-micro`
  uppercase eyebrow plus 12px of space. Keep `<fieldset>`/`<legend>` semantics
  with `border: 0`.
- **Segmented control (environment)** — `--control-h` `--bg-sunken` well,
  `--border-control` edge. Active segment: `--env-tint` fill, `--env-ink` text.
  **Picking `prod` swaps the substrate live** and turns the rule coral. That is
  the single best moment in the product to teach the guardrail.

> **Never `overflow: hidden` on a shell that wraps focusable children.** The
> focus ring is an `outline` drawn *outside* the box with `outline-offset: 2px`,
> so any clipping ancestor eats it — and it is the first and last child, the
> ones a keyboard user reaches first, that lose it. Give the **children** the
> shell's radius instead. This bit `.env-picker` and `.palette`; check any new
> rounded container before adding the property.
- Forms cap at `--measure` (62ch).

### 5.4 The connection ladder

The highest-leverage novice component in the product. `Test connection` streams
a live mono checklist instead of a spinner (`.ladder`):

```
Resolving broker-1.internal          ✓   12 ms
TCP 10.0.4.19:9092                   ✓   38 ms
TLS handshake (TLSv1.3)              ✓   61 ms
SASL SCRAM-SHA-512 as svc-orders     ✗   Authentication failed
Cluster metadata                     —   skipped
```

Steps stream in at `--text-tertiary` and resolve to `--ok`/`--danger` with a
glyph. It localizes the failure **before** the user has to interpret prose, and
it turns the error library in §7 from reactive into diagnostic.

### 5.5 Buttons

All `--control-h`, `--r-control`, `--t-sm`, sentence case, no transform on press.

| Variant | Fill | Border | Text | Use |
|---|---|---|---|---|
| Primary | `--text-primary` | none | `--text-inverse` (16.13:1) | **Exactly one per surface** |
| Secondary (`.btn`) | transparent | `--border-control` (3.17:1) | `--text-primary` | Everything else |
| Ghost | transparent | none | `--text-secondary` | Toolbars, dismissals |
| Danger | transparent | `--danger-border` (3.34:1 worst) | `--danger` | *Opens* a confirmation |
| Danger-confirm | `--danger-fill` | none | `--on-danger-fill` (6.34:1) | **Only one click from the action** |
| Latched toggle | `--accent-tint` | `--accent` | `--accent` (7.52:1) | Live tail ON, saved filter active |

**The guardrail rule: a filled red button only ever exists one click away from
the destructive action happening.** Everywhere else destructive controls are
outlined.

**Latched state must read as ON across the room** — a live-tail toggle is a
physical switch, not a link.

**Loading keeps the label and reserves a fixed 16px leading slot**
(`.btn-busy-slot`) with `aria-busy="true"`. A button that swaps
`Connect` → `Connecting…` resizes mid-click and shifts everything after it.
Anything over 2s gets a visible Cancel.

**Disabled** — `opacity: .45`, `cursor: default`, and **always a `title`
explaining why**. Never a dead control with no reason. This includes
busy-disabled buttons (*"Kavka is saving this connection"*) and **disabled
`<option>`s** — the four not-yet sign-in methods each carry the same sentence,
because an unexplained greyed-out choice reads as a broken build, not a
roadmap.

**Toolbar labels** ≤7 characters in English; budget ~40% width growth for the
Phase 6 languages, and ship a `⋯` overflow menu from day one.

### 5.6 Chips

- **Env** — 18px pill, `--t-micro`/600 uppercase tracked. dev → `--accent` on
  `--accent-tint` (7.52:1). staging → `--warn` on `--warn-tint` (7.42:1).
  **prod → `--env-chip-prod-ink` on solid `--env-chip-prod-fill` (6.22:1)** —
  prod is the only *filled* chip, so it reads as a badge, not a tag.
- **Read-only** — tertiary, transparent, `1px --hairline-strong`, `--t-xs`,
  **sentence case**. Permanently in the top bar and the status bar. Uppercase
  micro-caps are reserved for env chips, table column heads, stat labels and
  section eyebrows; `read-only` is a phrase Kavka says to you, not a system
  token like `PROD`, and uppercasing it made the two chips shout equally loudly
  when only one of them is a guardrail.
- **Filter** — 20px, `--r-control` (**not a pill** — pills read immutable,
  filters are editable), `--bg-raised`. Backspace on an empty input removes the
  last chip. A chip that fails to parse takes a `--danger` border with the parser
  message and caret position in its tooltip.
- **Schema** — `orders-value · v4 · id 217` in `--t-xs` mono tertiary, clickable
  through to the subject.
- **kbd** — `--t-xs` mono, `--bg-sunken`, `1px --hairline-strong`.

### 5.7 Search bar

`--control-h-lg`, `--bg-sunken`, `--r-popover`, `1px solid var(--border-control)`.

`[⌕] [chips…] [input, flex:1] [ƒx] [⏎ hint]`

Placeholder starts plain: `Find in key, value or headers`. **The CEL editor lives
behind a `ƒx` toggle at the right edge** — visible so an expert finds it in one
second, never the default so a junior never faces it. A plain-text search shows
the equivalent CEL as a hint underneath; that is how a support engineer
accidentally learns CEL.

### 5.8 Banners, toasts, modals — the three-surface rule

> **Toast** = something you did, finished.
> **Banner** = a condition you are in.
> **Modal** = something irreversible needs your consent.
>
> **An error the user must act on is never a toast.**

**Banner** — full-bleed above the content area. No radius, no icon-in-a-circle,
no shadow. 1px top and bottom in the semantic colour at 30% alpha, `*-tint`
background, a 2px left bar. Line 1 (`--w-medium`) = what happened. Line 2
(`--text-secondary`, capped at `--measure`) = what to do. Actions are ghost
buttons. `Show details ▾` discloses the raw librdkafka string in a selectable
mono block. **One banner at a time**; a second replaces it and the header gains
a `2 issues ▾` counter.

**Prod de-collision (required):** on a prod cluster a danger banner drops its red
tint for a neutral `--bg-raised` shell with a `--danger` left bar, and the env
rule dampens while it is visible (`[data-env="prod"][data-alert="danger"]`).
Otherwise the warm substrate plus a coral banner is an undifferentiated red wash
and **both** signals die.

> **The damper moves chroma, not lightness.** The undampened prod rule is
> already only **3.09:1** on a selected prod row, so there is no darker coral
> left to dampen *to*: the first implementation used `#6E3A33`, which measured
> **2.09:1** on the canvas and **1.83:1** on a selected row — prod's most
> load-bearing guardrail (layer 1, the one that needs no banner) switched
> itself off at exactly the moment an error was on screen. That is the worst
> possible failure of the two-signal composition this section exists to
> protect. The damper is now `#9C8079`: the same rule at roughly a third of
> the chroma (channel spread 35 vs the coral's 96) and *more* luminance
> contrast, **4.55:1 worst case** (4.61 on a selected prod table row). It
> reads as muted; it never reads as gone. Light has its own damper
> (`#8A6F6A`, 4.09:1 on a selected light row, 3.41:1 on a selected light prod
> sidebar row) or it inherits the dark one.
>
> `data-alert="danger"` must be set for **any** danger on screen, including an
> inline banner inside a form — not just the global one. Miss the inline case
> and prod shows a coral rule behind a coral banner, which is the single
> composition §9 gate 4 exists to catch.

**Toast** — bottom-right, 24px inset, 320px, `--bg-raised`, `--r-overlay`,
`--shadow-popover`, and a 4px left rule in the semantic colour (the ledger rule
at toast scale). Enter 180ms `translateY(10px)→0` on `--ease-settle`; exit 120ms
linear. Success auto-dismisses at 5s with a 1px progress hairline; **errors and
warnings never auto-dismiss.** Pause on hover *and* focus. Max 3, then `+2 more`.
`role="status"` for success/info, `role="alert"` for error. Copy is the past
tense of the button that caused it. Where an OS notification also fires, the
toast and the notification carry **identical text**.

**Modal** — 440px (560px for offset reset), `--bg-raised`, `--r-overlay`,
`--shadow-overlay`, scrim `--bg-scrim` with **no backdrop blur** (a measurable
WebView2 cost, and the tell of a templated design). Focus trap; **initial focus
on Cancel**; `Esc` always cancels; `⏎` confirms only non-destructive modals.
Destructive modals get a 2px `--danger-fill` rule across the top edge — the same
wire as the prod bar, so the two guardrails rhyme. The confirm button restates
the verb: `Delete topic`, never `OK`.

### 5.9 Command palette

580px, `top: 14vh`, `--bg-raised`, `--r-overlay`, `--shadow-overlay`. The only
element with real elevation, and one of only two using `--ease-settle`.

- Input 48px, `--t-md`, borderless, `⌘K` chip right in `--text-disabled`.
- Rows 36px: `[glyph] [label] [context tertiary] ······ [kbd]`. Active row:
  `--bg-row-selected` + `inset 2px 0 0 var(--accent)` — **identical to table
  selection, so the pattern is learned once.**
- **Never opens empty.** With no query it shows `Suggested`: last cluster, last
  three topics, `Add connection`, `Toggle read-only`, `Make rows more compact`,
  `Support Kavka ☕`.
- `⌘K` opens it; `⌘P` opens it pre-scoped to go-to — one palette, two doors (do
  vs go) without teaching two concepts.
- **Matching is bilingual.** `bootstrap` finds the field labelled *Bootstrap
  servers*; `lag` finds *Consumer groups*. Plain-language relabelling must never
  hide the Kafka term from search.
- On prod the palette inherits the warm substrate and destructive commands
  render their subtitle as `payments-prod · asks for confirmation` in `--danger`.

### 5.10 Payload inspector

Right-docked, `--inspector-w`, resizable via a 4px hit area over a 1px
`--hairline-strong`.

- Header 36px: `Partition 3 · Offset 8 412 · 14:02:11.482` in mono tertiary.
- Tabs `Value · Key · Headers · Raw · Hex`. Pretty JSON is the default; Raw and
  Hex are one click away and **never the landing state**.
- Body `--bg-sunken`, mono, **line numbers in a `--gutter-w-code` gutter with the
  same `border-right: 1px solid var(--rule)`** — the ledger device at document
  scale.
- Collapsed nodes summarise as `{…} 14 keys` in `--text-absent`.
- **Search hits: `--syn-match` background *plus* a 2px `--accent` underline.**
- Provenance pinned at the bottom: `Decoded as Avro · orders-value v4 · id 217`,
  or `Shown as plain text — no schema matched these bytes.` with a `Read as…`
  select.
- **Perf guard:** payloads over 256 KB render as raw text with a
  `Format JSON (1.4 MB — may take a moment)` action rather than blocking on a
  parse. Long payloads virtualize by line with `Showing the first 200 KB — load
  the rest` at the cut.
- **One diff component serves three features** — payload diff (`⌘D` with two rows
  selected), config diff-from-default, schema version diff. `+`/`−` in the ledger
  gutter, `--ok-tint`/`--danger-tint` line washes. Build it once in Phase 1.

### 5.11 Charts

1px strokes, no gradient fills (at most a 6%-alpha wash under a single series),
grid at `--grid-line`, axis labels `--t-micro` tertiary, no chart junk, no
shadows. Line-draw animation on first paint only, 260ms; **never on data
update**. **Series must be direct-labelled or dash-patterned** — a 6-series line
chart identified by hue alone fails SC 1.4.1 for deuteranopes.

---

## 6. Prod guardrails — nine layers, ordered by survivability

1. **The ledger rule turns coral** in every table in the app. No banner required;
   prod is visible wherever data is.
2. **The 2px `--env-wire`** across the very top of the window. Zero vertical cost,
   permanently peripheral, no text to habituate to. In forced-colors it thickens
   and gains the literal word `PROD`.
3. **The bootstrap address is always on screen** — line 2 of the sidebar row and
   in the status bar. *Most prod accidents are right-action-wrong-cluster.*
4. **Type-to-confirm on every destructive modal, environment-gated not
   action-gated.** Prod always asks; dev never does. Friction where the stakes
   are, nowhere else.
5. **Warm substrate.** Nobody will name it; everybody will feel it on switch.
   This is the *bonus*, not the guardrail — layers 1–4 are load-bearing. Escape
   hatch: `[data-env-intensity="rule-only"]`.
6. **Prod rows stay tinted in the sidebar** whether selected or not, plus a 2px
   `--danger` left border.
7. **Write actions change class in prod** — Produce, Delete, Reset render as
   danger-outlined even when routine, and the produce form carries an
   undismissable warning banner.
8. **Read-only defaults ON** when the environment is set to prod in the
   connection form, with the reason stated as it happens.
9. **The window title carries it** — `orders-prod · PROD — Kavka`, so the
   taskbar, Dock and `Cmd+Tab` warn too.

> **If a future PR proposes removing the wire "because the substrate already does
> it", that PR is wrong.**

### Read-only mode

Mutating controls are **disabled, never hidden** — hiding creates the "where did
the button go" support ticket and destroys discoverability of what the app can
do. But a dead control with no reason reads as a broken app, so every one carries
the same `title` **and** the toolbar shows a persistent one-line explanation:

> *This connection is read-only. Turn that off in the connection's settings to
> produce or edit.*

The read-only chip sits permanently in the top bar and the status bar. The
palette greys the same commands with the same sentence.

---

## 7. How Kavka speaks

### The eight rules

1. **Sentence case everywhere.** Buttons, labels, headings, menu items. The only
   exceptions: env chips (`PROD`) and table column headers. Title Case reads like
   a legacy Java app, and this product's entire positioning is *not that*.
2. **The verb survives the whole flow.** `Connect` → `Connecting…` → `Connected
   to orders — local`. `Delete topic` → `Deleting…` → `Deleted orders.v2`. Never
   `Submit`, `OK`, `Apply`, `Confirm`.
3. **Name things the way the user thinks, not the way Kafka's API does — but
   never hide the Kafka term.** The label is what a novice already owns; the
   Kafka term appears in the hint, the tooltip and palette matching, so an expert
   is never lost.
4. **Errors state what happened, then the next click.** No apologies, no
   exclamation marks, no "Oops", "Something went wrong", "Unfortunately",
   "Invalid input", or "Please try again later".
5. **Numbers are grouped and honest.** Prose rounds: *"about 4.2M messages."*
   Tables are exact: `4 218 907`. **Never round a lag figure someone is about to
   act on.**
6. **Never a full-screen spinner.** Loading is a 2px accent hairline under a
   table header, a skeleton row, or — wherever the wait exceeds ~400ms — **a
   sentence**: `Asking the cluster for topics…`, `Scanned 412,000 of ~2.4M · 0
   matches so far`. A spinner says *wait*; a sentence says *what for*.
7. **Every disabled control says why on hover.** No dead ends.
8. **Destructive copy states the blast radius before the button.** *"every
   message in it — about 4.2M records"* is the difference between an informed
   click and an incident.

### DO / DON'T

| DON'T | DO |
|---|---|
| `Save Profile` | `Save` |
| `BOOTSTRAP SERVERS` | `Bootstrap servers` |
| `Invalid input.` | `Use host:port, e.g. broker-1:9092` |
| `Connection name is required.` | `Give this connection a name so you can find it in the sidebar.` |
| `Meta data fetch error: BrokerTransportFailure` | `Can't reach kafka-1.internal:9092` + *The hostname didn't resolve. Check the spelling, or whether you need to be on the VPN.* + `Show details ▾` |
| `Are you sure?` | `This removes the topic and every message in it — about 4.2M records.` |
| `OK` | `Delete topic` |
| `Loading…` (full screen) | `Asking the cluster for topics…` + skeleton rows |
| `No results` (while still scanning) | `Scanned 412,000 of ~2.4M · 0 matches so far` |
| `null` in a cell | `∅` in `--text-absent` |
| Hiding write buttons in read-only | Disabling them with a `title` and a visible reason |

### Teaching without documentation

Every Kafka term that must appear as a label gets a **dotted 1px
`--text-disabled` underline** and a popover on hover *and* focus
(`aria-describedby`, keyboard-reachable). 260px, `--bg-raised`,
`--shadow-popover`, one sentence plus one concrete example.

> **The popover is portalled to `<body>` and positioned `fixed`**, from the
> term's `getBoundingClientRect()`. As a positioned child of the term it was
> clipped by whatever sat between them — and two of its three real call sites
> are inside a `position: sticky` `<th>` inside `.table-scroll { overflow:
> auto }`, so the gloss was cropped to a 28px header strip. The one component
> whose entire job is teaching a novice was the one component a novice could
> not read. (`container-type: inline-size` on `.table-wrap` would have made it
> a containing block too.) It flips above/below near the top of the window,
> clamps to the viewport, re-places on scroll and resize (capture phase, so
> table scrolling counts) and closes on `Esc`.
>
> **It stays mounted and is merely hidden**, never conditionally rendered: an
> `aria-describedby` target that only exists while the mouse is over the term
> is a description a screen reader never resolves. Directly-referenced hidden
> nodes *are* included in the accessible description, so `visibility: hidden`
> is both correct and quiet.

**Hard cap: 15 terms, in the shared registry at
`apps/desktop/src/Glossary.tsx`, one gloss per term per view.** If a term needs
more than a sentence, the label is wrong and should be rewritten instead.
Without the registry, Phases 2–5 each add their own and the app becomes a
tooltip farm.

Current registry (13 of 15): offset · partition · lag · consumer group · broker ·
bootstrap server · replication factor · internal topic · retention · live tail ·
tombstone · under-replicated · ISR.

```jsx
{/* DO */}
<Term name="partition">Partitions</Term>

{/* DON'T — the gloss must come from the registry, not the call site */}
<span title="a topic is split into partitions…">Partitions</span>
```

### Empty states

Same anatomy every time: one sentence naming the situation, one sentence naming
the action, at most one primary action. Centred block, `max-width: 44ch`,
**left-aligned text inside it** (centred paragraphs are harder to read). No
illustrations, no shrug emoji. Where a visual helps, render a 6-line mono
skeleton of the table that will appear here, in `--text-absent`.

**First launch** — the one screen that has to teach:

> ### Point Kavka at a broker
> A connection is a saved address for one Kafka cluster — a name, one broker to
> start from, and how to sign in. Kavka finds the rest of the cluster from there.
>
> A bootstrap server usually looks like `kafka-1.internal:9092`. Running this
> repo's dev cluster? Use `localhost:9092`.
>
> **[ Add connection ]**
>
> Passwords go to your operating system's keychain. Nothing about your clusters
> leaves this machine.

That last line is not filler. The target user has often just been handed
production credentials, and the biggest objection to any Kafka GUI is "where do
my creds go". Answering it unprompted on screen one is worth more than any
feature copy.

Other required empty states:

- **Sidebar, no connections** — *Nothing here yet. Add your first connection below.*
- **Connected, no topic chosen** — *Pick a topic to see its messages. `orders.v2`
  is the busiest one right now.* — **gate that second sentence on a fresh,
  successful metadata fetch and fall back silently**; it is embarrassing when
  stale or when the user lacks Describe.
- **Cluster with no topics** — *This cluster has no topics. They appear here as
  soon as something creates one.* `[ Create topic ]`
- **Only internal topics** — *This cluster only has Kafka's own internal topics.*
  `[ Show internal ]` `[ Create topic ]`
- **Topic with no messages** — *No messages in `orders.v2` yet. Start live tail
  and Kavka will show them as they arrive.* `[ Start live tail ]`
- **Live tail, quiet topic** — *Listening. Nothing has been produced to
  `orders.v2` in the last 30 seconds.* Silence is a state; saying so stops people
  wondering whether the app is broken.
- **Search running** — never say "no results" while scanning: *Scanned 412,000 of
  ~2.4M · 0 matches so far* `[ Stop ]`
- **Search found nothing** — *No messages matched `status = "failed"`. Checked all
  2,411,308 messages in the last hour — the default only looks back 60 minutes.*
  `[ Search all time ]` `[ Clear filters ]`
- **No consumer groups** — *No consumer groups yet. Groups appear here as soon as
  an application starts reading from this cluster. A group that has only ever
  produced won't show up.*
- **Profiles failed to load** — *Kavka couldn't read its connection file. Your
  connections are still on disk.* `[ Try again ]`

### Errors — the doctrine and the library

Three layers, always: **plain title → cause and fix → `Show details ▾` with the
raw librdkafka string, verbatim and selectable.** Experts get the truth one click
away; novices never have to read it.

**An error renders where its fix is.** A failed *connect* is stored on the
connection (`ConnState.error`) and rendered as a persistent inline banner in the
connection form, directly above the actions — the fields it is about are on the
same screen, and it stays up while the user edits them. It clears when the next
attempt begins, never on a timer. Only errors with no field to point at (the
profile file, a disconnect, a topic fetch) go to the global banner. **Capturing
an error into state and never rendering it is worse than not capturing it**: the
app knows exactly what went wrong and shows the user nothing.

**The generalizable rule: when the broker tells us the answer, put the answer in
the message.**

> **The table below is code, not prose: `apps/desktop/src/errors.ts`.**
> `classifyError(raw) → { title, detail, known }` is a pure, total function —
> no React, no imports, no I/O — so the whole library can be checked by calling
> it with a captured broker string. Every renderer of an error uses it; the raw
> librdkafka text is **never** the banner title. `known: false` means we did
> not recognise the cause, and only then does the raw string become the title,
> with the full text still under `Show details`. Keep it pure: the moment it
> reaches for component state it stops being testable and becomes a component.

| Cause | Line 1 | Line 2 |
|---|---|---|
| DNS | Can't reach `kafka-1.internal:9092` | The hostname didn't resolve. Check the spelling, or whether you need to be on the VPN. |
| TCP timeout | `kafka-1.internal:9092` didn't answer in 10s | The host is reachable but nothing is listening on that port. Check the port number, or whether the broker is running. |
| Refused | `localhost:9092` refused the connection | Nothing is listening there. If you're running Kafka in Docker, check the port is published to the host. |
| Not a broker | `localhost:8080` answered, but it isn't a Kafka broker | Something is listening there, but it doesn't speak the Kafka protocol. Kafka usually runs on 9092, or 9093/9094 with TLS. |
| Plaintext → TLS | This broker expects an encrypted connection | Turn on **Encrypt the connection (TLS)** and connect again. |
| TLS → plaintext | This broker isn't using TLS | Turn off **Encrypt the connection (TLS)** and connect again. |
| Untrusted cert | The broker's certificate isn't trusted | `broker-1.internal` presented a certificate signed by `Internal Corp CA`, which isn't in this machine's trust store. Add the CA certificate as a PEM file — Kavka doesn't need a keystore. |
| SASL rejected | The broker rejected these credentials | Check the username, then re-enter the password — Kavka can't tell whether the stored one is still valid. |
| Wrong mechanism | The broker doesn't accept SCRAM-SHA-256 | It offered `SCRAM-SHA-512`. Switch the mechanism and connect again. |
| Metadata timeout | Connected, but the cluster didn't answer in time | The broker accepted the connection but didn't return metadata within 15s. It may be overloaded, or a firewall may be blocking the address the broker advertises — which can differ from the one you typed. |
| Authorization | Connected, but this account can't list topics | It needs `Describe` on the cluster. Ask whoever issued the credentials for that permission. |
| Timeout, prod | Can't reach `payments-prod-1:9093` | Nothing changed on your machine — this is usually the VPN or a broker restart. |
| Unknown profile | That connection isn't on this machine any more | It may have been deleted in another window. Pick another connection from the sidebar, or add it again. |
| *(unrecognised)* | *the raw broker string, verbatim* | Kavka doesn't recognise this one. The broker's full reply is under Show details — it usually names the host or the setting at fault. |
| Read-only block *(toast, `role="alert"`, no auto-dismiss)* | Read-only connection — nothing was sent | This connection is marked read-only, so Kavka didn't produce the message. Turn read-only off in the connection's settings if you meant to write. |
| Active group | `checkout-service` is running | Offsets can't be reset while 3 members are consuming. Stop the application, then try again — Kafka will reject the reset otherwise. |

### Destructive confirmations

**dev**

> ### Delete `orders.v2`?
> This removes the topic and every message in it — about 4.2M records. It can't
> be undone, and the 3 consumer groups reading it will start failing.
>
> `Cancel`  **`Delete topic`**

**prod** — adds type-to-confirm and restates the cluster

> ### Delete `orders.v2` on `payments-prod`?
> This removes the topic and every message in it — about 4.2M records. It can't
> be undone, and the 3 consumer groups reading it will start failing.
>
> Type `orders.v2` to confirm  `[__________]`
>
> `Cancel`  **`Delete topic`** *(disabled until it matches exactly)*
> `Copy this as a CLI command`

**Offset reset — the modal that teaches**, because it is the one juniors get
wrong:

> ### Reset offsets for `checkout-service`
> `Earliest ▾` on `orders.v2`, all 12 partitions
>
> checkout-service will re-read from the beginning. Its 3 members will reprocess
> about **4.2M messages**. Anything the application does on each message will
> happen again.
>
> ⚠ This group is active. Reset while members are running and the broker will
> reject it — stop the consumers first.
>
> `Cancel`  **`Reset offsets`**

The "will reprocess about N messages" line is computed from committed vs earliest
offsets and **updates live as the mode changes**. That sentence is the entire
feature's UX.

### How experts get their speed back

Nothing above slows a platform engineer down, because the expert surface is
keyboard-shaped rather than screen-shaped:

`⌘K` every topic, group and action · `⌘P` go-to · `⌘1…9` clusters · `/` focus
search · `j`/`k` walk rows · `⏎` inspect · `y` copy cell, `Y` copy row as JSON ·
`gg`/`G` top/bottom · `⌘I` inspector · `⌘B` sidebar · `Esc` cancel the running
search · `ƒx` turn search into CEL.

`More options` and the column picker persist per profile. `Show details` yields
the verbatim broker error.

> **The disclosure is one-way ratcheting: the app starts simple and permanently
> becomes as complex as you have proven you want it.**

---

## 8. Build order

Tokens → base/reset → buttons, inputs, chips → **the ledger rule + table** →
status bar + tab strip → banner/toast → modal → connection ladder → command
palette → inspector (+ the shared diff component).

Anything not on that list — charts, config diff, topology, reassignment —
**inherits from these primitives. It does not get its own vocabulary.**

---

## 9. Verification gates before merge

1. **Contrast** — assert every `--text-*` (including `--text-placeholder` and
   `--text-absent`, which are **text**, not graphics), every `--syn-*`, and
   `--ok/warn/danger/accent` at ≥4.5:1 against all six dark surfaces, all six
   prod surfaces and all six light surfaces; and `--border-control`,
   `--danger-border`, `--rule` (**both** its normal and its
   `[data-alert="danger"]` value), `--env-wire` and every lag-bar fill —
   against `--track-empty` as well as the surfaces — at ≥3:1 across all twelve
   dark/prod surfaces plus the two prod sidebar row surfaces.
   **No alpha may appear in any of these tokens**: an `rgba()` border has no
   ratio until you know what is behind it, so it cannot be asserted at all.
   **Current state: zero failures.** Wire it into CI — this is the token set's
   only real defence.
2. **`--row-h` assertion** — dev-mode check that the token equals the first row's
   measured `offsetHeight`; fail loudly. Wire it before the message browser
   lands.
3. **Platform screenshots** — macOS and Windows, 100% and 125% DPI. SF Pro at 600
   is noticeably lighter than Segoe UI Variable at 600, and `--t-micro` at 10.5px
   plus `--row-h: 30px` are the most fragile. Budget a `[data-os="win"]` override
   dropping `--w-semi` to 550 if Windows reads heavy. Verify the 2px wire at
   100/125/150/175% Windows scaling.
4. **Prod + danger together** — the single most likely thing to get wrong in
   implementation. Screenshot a prod cluster with an error banner up and confirm
   both signals still read.
5. **Forced-colors** — Windows high-contrast with the wire and its `PROD` label
   present, **and one unselected row next to one selected row next to one prod
   row.** The bug this catches is not "the indicator is missing"; it is "every
   row has the indicator", which looks fine in a screenshot of a single row.
6. **`index.html`** hard-codes the anti-flash background. It must stay in sync
   with `--bg-canvas`, or the app flashes the old theme on every cold start.

---

## 10. Themes and environments

The environment substrate swaps via `data-env` on the app root
(`apps/desktop/src/App.tsx`), and — while editing a connection — on the
`<form>` itself, so **picking `prod` in the environment picker swaps the form's
substrate live**.

```jsx
<div className="app" data-env={env}>            {/* whole app  */}
<form className="editor" data-env={form.environment}>  {/* live preview */}
```

`data-alert="danger"` on the app root dampens the env rule while a danger banner
is up (§5.8, prod de-collision).

**Light theme is specced and audited but not shipped.** Every text token clears
AA on all six light surfaces and `--border-control` clears 3:1 on all six. Do
**not** expose a user toggle until every component composition has been verified
in it (Phase 6).

**Forced colors:** Windows high-contrast drops our tints, so the prod guardrail
must not depend on hue. The wire thickens and gains the literal word `PROD` via
`content: attr(data-env-label)`.

> **Never write `* { border-color: CanvasText }` in a forced-colors block.**
> Forced-colors mode already preserves `transparent` and repaints every other
> border with a system colour, so the blanket rule does nothing for real
> borders and **destroys every indicator built on a transparent one**. Kavka's
> selection, prod row, active tab and banner shell are all "transparent border
> → coloured border" — the rule painted their *idle* state in the same colour
> as their *active* state, so in high contrast every row read as selected and
> every tab as current.
>
> **Restate indicators positively instead**, one system colour per meaning:
> `Highlight` for selection (row, gutter, active palette row, active tab),
> `Mark` for prod and for danger severity — matching the wire, so the guardrail
> reads as one signal — and `CanvasText` for the neutral banner shell. This is
> also the only way two indicators stacked on one element stay tellable apart:
> a selected prod row is `Mark`, because the environment matters more than the
> selection.

---

## 11. Deviations from the brief, and why

These are deliberate. Do not "fix" them without reading the reason.

- **The global `:focus-visible` rule does not set `border-radius`.** As specced
  it would round table rows and panels the moment they take focus. Controls carry
  their own radius, so the ring already follows it; radius-less elements get a
  correct square ring.
- **`--env-label` is a React prop (`data-env-label`), not a CSS custom
  property.** Only the forced-colors block ever needed the string, and
  `attr()` on a data attribute is better supported than `content: var()`.
- **Env tokens are applied via `[data-env]` (attribute selector), not
  `:root[data-env]`.** Identical specificity, but it also works on the `.app`
  wrapper and on a nested `<form>`, which is what makes the live substrate
  preview possible without lifting state.
- **`.data-table` uses `border-collapse: separate`.** With `collapse`, WebKit
  drops the border on a `position: sticky` header.

- **The forced-colors block contains no `*` selector.** See §10 — a blanket
  `border-color` there erases exactly the indicators it looks like it is
  helping. Indicators are restated positively, one system colour per meaning.

- **The prod danger-damper reduces chroma, not lightness.** See §5.8. There is
  no darker coral available: the undampened rule is already at 3.12:1 on its
  worst surface.

- **`--text-placeholder` and `--text-absent` hold the same value.** Two roles,
  one contrast floor. They stay two tokens so a future theme can split them and
  so neither drifts silently. Do not "deduplicate" them.

- **The gutter collapse is a container query with a `@supports` fallback**, not
  a plain media query. The viewport is not the table's width. See §2.

- **`.term-pop` is portalled to `<body>` and `position: fixed`.** It cannot be
  a positioned child of the term: its most important call site is inside a
  sticky table header inside a scrollport. See §7.

- **`code` is `user-select: text` at the base rule**, rather than per component.
  Every `<code>` is a literal someone may need to copy; the opt-in list was
  always going to fall behind the components.

- **Static tables carry no `role="grid"` / `aria-rowcount` / `aria-rowindex`.**
  Those land with the virtualizer, in the same PR. See §5.2.

- **Light-theme prod sidebar rows fail AA and ship anyway — because the light
  theme itself doesn't ship.** On `--bg-row-prod-selected` `#F2D8D4` and
  `--bg-row-prod-hover` `#F7E2DF` (light block), `--text-tertiary`,
  `--text-placeholder`, `--text-absent` and `--border-control` all measure
  below their floors. The light block is a token-swap spec, not a shipped
  surface; re-derive these four values (or lighten the two row backgrounds)
  as the first task of the light-theme phase, and run the §9 gate 1 sweep
  over the light surface set before flipping it on.

- **The §7 error table overstates `errors.ts` by two rows.** "Timeout, prod"
  and "Active group" are not derivable from a raw broker string alone — the
  first needs the profile's environment, the second needs group state. Both
  need caller context threaded into `classifyError` (a second argument, not
  component state) when their features land in Phases 1–2.
