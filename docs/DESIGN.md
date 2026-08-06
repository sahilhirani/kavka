# Kavka Design System — "Jackdaw"

The rules every Kavka screen follows. Read this before changing anything under
`apps/desktop/src/`. It is written to be usable by a human contributor or an AI
agent with no other context.

**Status:** both themes ship. Dark is the default ground; **warm-paper light is
a real, fully-tokened theme**, not a stub — see §10 for the runtime that
resolves them. Implemented in `apps/desktop/src/styles.css`.

**Jackdaw replaces Ledger.** Ledger was correct and unreadable: every number was
honest, every ratio was measured, and every screen arrived as one unbroken field
of 13px text with no landmarks, so there was nothing for the eye to grab and
nothing telling you what you were looking at. Jackdaw keeps every number and
every caveat and adds the three things that were missing — **landmarks** (soft
raised panels, 15px body type, a grouped rail instead of ten equal tabs), **a
spoken answer before the data** (§5.12, the Perch), and **a floor under the
jargon** (glossary terms, expert tables folded behind disclosure).

Section numbers are unchanged from the Ledger revision. Source files across the
repo cite them.

---

## 1. The four laws

Everything else in this document follows from these. If a proposal violates one,
the proposal is wrong, not the law.

### Law 1 — A panel is a surface; an overlay is a surface with a shadow

**This law reverses Ledger's.** Ledger said "Kavka ships zero cards" and grouped
by whitespace plus a top hairline. On a screen with nine sections that asked the
eye to infer every boundary from spacing alone, and it is most of why the app
read as hard to follow.

A `.panel` is now a real surface: `--bg-panel`, one `--line` hairline, `--r-md`
and `--shadow-1`. Overlays — modal, command palette, popover, toast — take
`--shadow-2`, and nothing else may. There are exactly three shadow tokens, and
if you are reaching for a fourth you are inventing an elevation the app does not
have.

The loudest pixels on screen are still always topic names, offsets and payloads —
never chrome. A panel is a container; it is not decoration.

### Law 2 — No state is ever encoded by colour alone

Unchanged, and the most important thing in this document.

Every dot has a word. Every severity has a glyph. Every lag figure has a bar
length **and** a number **and** a trend arrow. The protected-environment signal
has four independent channels. The Perch's tone is an edge colour **and** a
state spelled out in its kicker. The rail's current screen is a tint **and** a
weight change **and** a spine **and** `aria-current`. If you can only tell the
difference by hue, it is broken.

**"Every dot has a word" includes the quiet states.** A status map with
`disconnected: ""` is the bug this law is written to prevent: the one state
rendered as a hollow ring — the state hardest to read at a glance — was also
the only one with no word anywhere in its row. Every status produces text, so
a switcher row's second line reads *"address · state"* in **every** state, and the
dot stays `aria-hidden` decoration.

This is WCAG 2.2 SC 1.4.1, but it is mostly just correct: a support engineer
with deuteranopia has to be able to tell production from dev at a glance.

### Law 3 — Nothing inside a scrolling table gets a `transition`, `filter`, `backdrop-filter`, `border-radius` or `box-shadow`

Unchanged. `--row-h` is a fixed token the virtualizer reads at runtime
(`src/virtual.ts`). At 10,000 rows, a 120ms hover transition is visible jank.

Law 1 gave panels a radius and a shadow. **Neither reaches inside a scroll
well.** A panel wrapping a virtualized table carries the radius on the panel;
the rows stay flat.

### Law 4 — Every screen answers before it reports

Every screen opens with a Perch (§5.12): one line of plain English, derived from
live state, that says what you are looking at and what Kafka can actually tell
you here. A screen that opens with a stat dump is asking the reader to do the
interpretation the product exists to do.

The honesty rules that govern what a Perch may claim are in §7, with the rest of
the voice.

---

## 2. The signature: the ledger rule, and the Perch

Jackdaw has two signature elements. One is inherited and one is new, and they do
different jobs: the rule is how you know **where** you are, the Perch is how you
know **how it is going**.

### The ledger rule — kept in full

A fixed left gutter carries the row's address **in Kafka's own vocabulary** —
offset, broker id, partition index, line number — then a 1px vertical rule runs
the full height of the data, then the payload.

The rule is coloured by environment. **A production cluster is therefore
visible in every data view in the app without a single banner.** Raised panels
did not replace this and could not: a panel says "these things belong together",
the rule says "this row is at offset 8 412 on a production cluster".

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

### The Perch — the new one

Specified in full at §5.12, with its honesty rules at §7. In one line: a warm
banner at the top of every screen, with the bird sitting on it, answering *what
am I looking at* and *what can Kafka actually tell me here*.

### The accent means clickable or selected. Nothing else.

**This replaces Ledger's "teal means live".** Ledger spent its one accent on a
meaning — connected, live-tailing, selected — and paid for it by making the
primary button near-white.

Jackdaw's accent (`--brass`, and its three alternatives) carries **no meaning of
its own**. It appears on exactly two kinds of thing: something you can click,
and something that is currently selected. That is precisely what makes the
accent picker in Settings safe to ship — a user who prefers plum cannot thereby
hide a warning, because no warning was ever spelled in the accent.

**What took over "live":** `--ok` with its word, exactly as Law 2 requires. A
connected cluster reads *"Connected"* beside a green dot; a live tail says
*"Tailing"* beside a pulse. The colour was never the signal.

**The status-adjacency law survives, re-pointed:** in any table with a health or
lag column, the accent may not appear in an adjacent column, and lag fills use
`--ok`/`--warn`/`--danger` only.

---

## 3. Token reference

Full source of truth: the theme blocks at the top of
`apps/desktop/src/styles.css`.

**Ratios below are measured, not estimated** — WCAG 2.1 relative luminance,
computed against **every surface the token can land on**, in both themes,
including the six protected surfaces. Every figure quoted is a **worst case**.

- Body text and any text carrying meaning: **≥ 4.5:1**.
- Operable boundaries, meaningful graphics, large text: **≥ 3:1**.
- Purely decorative hairlines: **no floor** (SC 1.4.11 exempts them), and they
  are labelled as such below so nobody re-derives a gate that does not exist.
- `:disabled` controls: **no floor** (SC 1.4.3 exempts inactive components).
  Quoted anyway, because "exempt" is not the same as "invisible".

### The two-block rule

**Every colour token is declared exactly twice: once in the dark block, once in
the light block.** That is what makes flipping `data-theme` safe — a token
declared in only one of them is a half-themed control waiting to happen.

Everything that is *not* colour — type, space, metrics, shape, motion — is
declared once on `:root`, because it does not vary with theme.

### The legacy alias layer

Ledger's token names (`--text-primary`, `--bg-canvas`, `--accent`,
`--border-control`, …) are still declared, as `var()` references to their
Jackdaw equivalents. **This is why ~5,000 lines of component CSS re-themed
without being edited.** An alias resolves against whichever theme block won,
because both land on the same element.

> **Never give an alias a literal colour.** That is exactly how a token escapes
> the theme system, and it will be wrong in light.

### Naming traps

Two pairs are one character apart and mean unrelated things. Both have already
caused a bug in a design system this one is descended from.

| Looks like | Actually is |
|---|---|
| `--ser0`…`--ser5` | chart series. **Not** `--s0`…`--s5`, because `--s-1`…`--s-12` is the spacing scale, and at equal specificity `4px` would have silently erased a chart's lines. |
| `--t1` / `--t2` / `--t3` | the **text ramp** (primary/secondary/tertiary ink). `--t-xs` / `--t-sm` / `--t-base` / `--t-md` are the **type scale** (font sizes). |

### Surfaces

A warm brown-grey ladder in dark; warm paper in light. Each step is a real
elevation, not a shade.

| Token | Dark | Light | Use |
|---|---|---|---|
| `--bg-desk` | `#0A0908` | `#DCD3C4` | behind the app window |
| `--bg-app` | `#100E0B` | `#EAE3D8` | rail, status bar — recedes |
| `--bg-canvas` | `#191512` | `#F6F1E8` | the working ground |
| `--bg-panel` | `#221D18` | `#FFFDF8` | **every raised panel** |
| `--bg-panel-hi` | `#2A241E` | `#F0E9DC` | hover on a raised surface |
| `--bg-inset` | `#14110E` | `#F1EBE0` | inputs, payloads, code — pressed in |
| `--bg-row-hover` | `#2A241E` | `#F0E9DC` | table row hover |
| `--bg-row-sel` | `#33291C` | `#FBEFD6` | selected row (accent-tinted; moves with the accent) |
| `--bg-prod-row` / `-hi` / `-sel` | `#2A1815` / `#33201C` / `#3A241F` | `#FAE7E3` / `#F5DCD6` / `#FADFD6` | protected-environment rows |
| `--bg-scrim` | `rgba(8,6,5,.62)` | `rgba(60,50,38,.42)` | modal scrim — no blur, for perf |
| `--skeleton` | `#332B24` (1.31:1) | `#E7DECE` (1.19:1) | a placeholder, **never** content |
| `--selection` | `#3E3320` (`--t1` 10.80:1) | `#F3E1B8` (`--t1` 12.58:1) | `::selection` |

The two `--bg-canvas` literals are duplicated in **one** other place: the inline
pre-paint script in `index.html`. See §10.

### Text — three inks plus two specialists

Hierarchy lives in weight and these values, not in boxes.

| Token | Dark | worst | Light | worst | Use |
|---|---|---|---|---|---|
| `--t1` | `#F4EFE8` | 12.45:1 | `#23201B` | 12.73:1 | values, headings, topic names |
| `--t2` | `#C6BCAF` | 7.60:1 | `#564F45` | 6.34:1 | prose, help text |
| `--t3` | `#A0958A` | 4.86:1 | `#6A6155` | 4.77:1 | labels, column heads, metadata, placeholders, `∅` |
| `--t-mono` | `#E6DED3` | 14.11:1 | `#2E2A24` | 12.02:1 | literals from Kafka (on `--bg-inset`) |
| `--t-off` | `#8A7E70` | 3.59:1 | `#867C6F` | 3.21:1 | `:disabled` controls **only** |
| `--t-inv` | `#1A1206` | — | `#FFFDF8` | — | ink on a near-solid fill |

Dark's worst case is a selected row; light's is the rail. `--t3` carries the
placeholder and `∅` roles as well: both are text a sighted user is expected to
read, so both clear 4.5:1 rather than 3:1.

### Lines

| Token | Dark | Light | Floor | Use |
|---|---|---|---|---|
| `--line` | `#342D26` | `#DED5C6` | **none** | decorative separation — 1.4.11 exempt |
| `--line-mid` | `#463D34` | `#CDC3B2` | **none** | heavier decoration: dock edges, wells |
| `--line-strong` | `#8A7D6E` (3.55:1) | `#82786A` (3.40:1) | 3:1 | **required on every operable boundary** |
| `--focus` | `#FFD489` (10.19:1) | `#7A4B00` (5.81:1) | 3:1 | always with `outline-offset` |

`--line-strong` is the only thing satisfying SC 1.4.11 on a transparent-filled
control, and the figures above are on a selected row (dark) and the rail
(light) — the two surfaces that bind.

### Accent — four choices, both themes

The accent carries no meaning (§2), which is what makes it user-selectable.
Every accent redeclares the **same** token set in **both** themes, so the picker
cannot leave a half-themed control behind.

| Accent | Dark ink | worst | Light ink | worst | On-fill pair |
|---|---|---|---|---|---|
| **brass** (default) | `#E9A94E` | 6.95:1 | `#82540C` | 5.11:1 | dark 9.05:1 · light 5.92:1 |
| moss | `#7FCB9B` | 7.40:1 | `#1A6039` | 5.94:1 | dark 9.55:1 · light 7.56:1 |
| sky | `#8FBFEA` | 7.33:1 | `#155C8C` | 5.60:1 | dark 9.65:1 · light 7.14:1 |
| plum | `#C7A2EF` | 6.67:1 | `#59389F` | 6.61:1 | dark 8.90:1 · light 8.43:1 |

**There is deliberately no coral/clay accent**, though the mockup drew one. An
accent the eye reads as the danger colour puts a decorative hue and a guardrail
hue in the same family, which is the one composition §6 forbids.

Supporting tokens per accent: `--brass-tint` (ink on it ≥ 5.72:1),
`--brass-fill`, `--brass-hover`, `--brass-quiet` (hairline-weight, ≥ 3.80:1),
`--on-brass`, plus `--bg-row-sel`, `--focus` and `--selection`, which move with
the accent because selection *is* the accent's second meaning.

### Semantic

| Token | Dark | worst | Light | worst | On its tint |
|---|---|---|---|---|---|
| `--ok` | `#6FCB92` | 7.21:1 | `#186639` | 5.50:1 | 8.22 / 6.09 |
| `--warn` | `#E4B85E` | 7.67:1 | `#7E5209` | 5.32:1 | 8.87 / 5.85 |
| `--danger` | `#F49182` | 6.25:1 | `#A8351F` | 5.16:1 | 7.49 / 5.48 |
| `--info` | `#8CBEE4` | 7.19:1 | `#1A5F87` | 5.44:1 | 8.60 / 5.88 |

Danger fills: `--danger-solid` (`#A8412F` / `#A93223`, white on it 6.07:1 /
6.63:1) and `--danger-solid-hover` (4.97:1 / 8.40:1).

> **`--danger-edge` is not optional.** The solid danger fill measures **2.35:1**
> against a selected row in the warm dark ground, so a filled danger button's
> **border** — `--danger-edge`, `#B0685A` / `#A9584A`, 3.37:1 / 3.94:1 — is what
> satisfies SC 1.4.11, not the fill. `.btn-danger-confirm` is written this way.

`--track-empty` (`#4E463C` / `#E6DCCB`) is the empty half of a lag meter. It is
**not** the indicator — the fill is — so the gate is *fill against track*: ok
4.69 / 5.16, warn 5.00 / 4.99, danger 4.07 / 4.84, accent 4.52 / 4.80. The
track's own extent is carried by a `--line-strong` border, which is why it does
not need to clear 3:1 against the panel behind it.

### The Perch tokens

| Token | Dark | Light | Note |
|---|---|---|---|
| `--perch-bg` | `#2A2214` | `#FCF2DC` | the ground |
| `--perch-line` | `#6A5227` | `#C9A961` | **decorative** — the Perch is identified by its ground, its bird and its kicker, never by this line |
| `--perch-ink` | `#EBD5A6` (10.92:1) | `#5E4712` (7.91:1) | the verdict |
| `--perch-title` | `#F7E7C4` (12.85:1) | `#3F3009` (11.52:1) | the kicker and the bird |

Semantic inks stay legible on the perch ground in both themes: ok 7.95 / 6.29,
warn 8.46 / 6.09, danger 6.89 / 5.91.

### Payload syntax

Held to 4.5:1 on `--bg-inset` **and** on `--syn-match`, in both themes.

| Token | Dark | worst | Light | worst |
|---|---|---|---|---|
| `--syn-key` | `#8CBEE4` | 7.70:1 | `#1F5F86` | 5.47:1 |
| `--syn-string` | `#9FD5A8` | 9.12:1 | `#1C6B3F` | 5.16:1 |
| `--syn-number` | `#E0B36A` | 7.86:1 | `#8A5510` | 4.91:1 |
| `--syn-atom` | `#BBA0F0` | 6.82:1 | `#5B3FA8` | 6.11:1 |
| `--syn-punct` | `#A6998B` | 5.48:1 | `#655C50` | 5.20:1 |
| `--syn-match` | `#2E2412` | — | `#F7E3B8` | — |

> **Syntax colours are fixed and are NOT derived from the accent.** A plum
> accent must not recolour every JSON number in the app. `--syn-number` is a
> warm amber in dark and stays one under every accent.

A search hit is `--syn-match` **plus a 2px accent underline**: the hit is never
colour-only.

### Data series

`--ser0`…`--ser5` — one lightness band in each theme, so no series shouts.
Dark: `#E9A94E` `#7FC8A9` `#F4897A` `#84B9EA` `#BBA0F0` `#CBBFAF`.
Light: `#8A5A0E` `#1A6039` `#A93223` `#1B5F94` `#5B3FA8` `#655C50`.
`--grid` is `#2E2822` / `#E4DACA`.

Series must be direct-labelled or dash-patterned regardless — see §5.11.

### Type

Two families, one scale, one multiplier.

```
--font-ui    Segoe UI Variable Text, Segoe UI, system-ui, -apple-system, …
--font-mono  Cascadia Mono, ui-monospace, SF Mono, Segoe UI Mono, …
```

**`--fs` is the font-size preference and it multiplies one number.** Every step
is `calc(Npx * var(--fs))`, so nothing in the app hard-codes a pixel font size
and the whole interface scales together.

| Step | px at `--fs: 1` | Use |
|---|---|---|
| `--f11` | 11 | `kbd`, footnotes, status bar |
| `--f12` | 12 | secondary UI, chips |
| `--f13` | 13 | **data** — table cells, mono literals |
| `--f15` | 15 | **body** — prose, labels, controls, rail items |
| `--f17` | 17 | panel titles |
| `--f20` | 20 | page titles |
| `--f26` | 26 | screen headings |
| `--f34` | 34 | hero stat numerals |

**15px body is the single largest legibility change in the direction.** Ledger
set `body` to 13px and everything inherited it. Jackdaw sets `body` to `--f15`;
tables opt back down to `--t-base` (13px) because a table is scanned, not read.

Ledger's scale (`--t-micro` … `--t-display`) is re-expressed on these steps, so
every existing rule answers the font-size preference without being edited.

### Space, metrics, shape, motion

4px base. **One ladder: `--s-1`…`--s-12`** (2 · 4 · 6 · 8 · 12 · 16 · 20 · 24 ·
32 · 40 · 56 · 72).

> **There used to be two, and the second one was dead.** The mockup's
> `--s1`…`--s9` was declared beside this one and used **zero** times, while all
> 504 spacing declarations in the app used the hyphenated set — a second live
> name for the same nine values, with nothing to say which was canonical. The
> audit classified that as maintenance debt that "will silently generate drift
> in every future component", and the fix is to delete the unused one rather
> than to keep documenting the choice. **Reading a Jackdaw measurement:** the
> mockup's numbers map exactly onto rungs of this ladder — `--s1`→`--s-2`,
> `--s2`→`--s-4`, `--s3`→`--s-5`, `--s4`→`--s-6`, `--s5`→`--s-7`, `--s6`→`--s-8`,
> `--s7`→`--s-9`, `--s8`→`--s-10`, `--s9`→`--s-11`. `--s-1` (2px), `--s-3` (6px)
> and `--s-12` (72px) are this ladder's own; the mockup had no rung there and
> the app needs all three.
>
> **The type tokens are a different case and stay.** `--t-xs`…`--t-display` are
> *aliases* — each is declared as `var(--f11)`, `var(--f12)`, … — so there is
> one source of truth and two names for it, which is the legacy alias layer
> above working as designed. Folding those would mean rewriting 231 call sites
> to gain nothing. Prefer `--t-*` in new rules.

`--rail-w` 254 (the app rail — there is only one) ·
`--topbar-h` 44 · `--statusbar-h` 26 · `--tabstrip-h` = `--control-h-lg`
(inline tabs only) · `--gutter-w` 72 · `--gutter-w-code` 40 · `--inspector-w`
480 · `--palette-w` 580 · `--measure` 74ch · **`--hit-min` 24px** (SC 2.5.8 —
nothing operable may be smaller).

Radius: `--r-sm` 7 · `--r-md` 11 · `--r-lg` 15, aliased as `--r-control`,
`--r-popover`, `--r-overlay`; `--r-pill` 999. **Panels have a radius now**
(Law 1). Rows inside a scroll well still do not (Law 3).

Motion: `--dur-instant` 0 (row hover, cell select — data answers *now*) ·
`--dur-fast` 120 · `--dur-base` 180 · `--dur-slow` 260. `--ease-settle` is for
the **palette and toasts only**.

### Density — and the mechanism virtualized rows use

**Two steps, persisted, on `<html>`: `data-density="comfortable"` (default) and
`"compact"`.** Ledger's third step (`relaxed`) is gone; comfortable is what it
was reaching for.

| Token | comfortable | compact |
|---|---|---|
| `--row-h` | **44px** | **30px** |
| `--row-h-head` | 34px | 28px |
| `--control-h` | 32px | 28px |
| `--control-h-lg` | 36px | 32px |
| `--pad-panel` | 20px | 13px |
| `--stack` | 18px | 12px |

**Compact is exactly the 30px row Ledger shipped.** That is the point of it: the
one fair criticism of the Jackdaw mockup was that comfortable rows cost a reader
of 10,000-row tables real screen, and the answer is a preference, not a
compromise on the default.

Density changes **rhythm only**. No colour, no type weight, no affordance and no
information moves with it.

#### The mechanism, pinned

> `MessageGrid` and any other virtualized surface must get its row height from
> `--row-h` and from nothing else. `src/virtual.ts` already implements the read:
> `useRowMetrics` calls `getComputedStyle(el).getPropertyValue("--row-h")`, and a
> `MutationObserver` on `document.documentElement` and `document.body` watching
> `data-density` and `data-theme` re-measures whenever either changes. **The
> shell writes those attributes on `<html>`, which is inside the observer's
> scope, so a density change re-measures with no further wiring.**
>
> **`--row-h` is fixed px and is NOT multiplied by `--fs`.** The virtualizer
> places every windowed row by arithmetic on this token, and its dev-mode
> assertion (§9 gate 2) compares the token against a laid-out row. A row height
> that moved with the font preference would need the observer to watch
> `data-fontsize` too — a third coupling for no benefit, because the largest
> font step (13px → 14.95px, line box ~22px) still fits a 30px compact row.
>
> A row that needs more height at a larger font size is a row with padding it
> should not have.

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

**One shell. One rail.** The window is a two-column grid — a 254px rail and the
stage — plus the environment wire above and the status bar below.

> **This section used to describe a different app, and that is the point.**
> Kavka shipped Jackdaw's *components* on top of Ledger's *shell*: a permanent
> 248px "Clusters" sidebar, a second 224px cluster rail inside the workspace,
> and screens with no header. A connected user spent 472px on chrome before any
> content, and the message table had to scroll horizontally to show a key. The
> fidelity audit called that inherited-structure drift and found this section
> presenting it as the design. It was not the design; it was what was left when
> nobody deleted the old thing. The sidebar is gone. If a future PR proposes a
> second permanent navigator, this paragraph is the reason to say no.

```
┌────────────────────┬──────────────────────────────────────────────────────┐
│ 🐦 Kavka    0.4.0  │  ┌────────────────────────────────────────────────┐  │
│ ┌────────────────┐ │  │ 🐦 HOME · LOOKS FINE                           │  │
│ │ orders-prd ▮PRD│ │  │ Connected to 3 brokers …                       │  │
│ │ 10.0.4.19:9093 │ │  └────────────────────────────────────────────────┘  │
│ │ ● Connected ·3 │ │  ┌────────────────────────────────────────────────┐  │
│ │ Switch cluster▾│ │  │ OFFSET │ TIME  │ KEY    │ VALUE                │  │
│ └────────────────┘ │  │1204882 │ 12:04 │ A-102  │ {"id": …             │  │
│ SET UP             │  │ …virtualized…                                  │  │
│   Connections      │  └────────────────────────────────────────────────┘  │
│ CLUSTER            │                                                      │
│  ▸ Home            │                     ↑ 1px --rule, coloured by env    │
│    Topics          │                                                      │
│    Consumer groups │                                                      │
│    Brokers         │                                                      │
│ OBSERVE            │                                                      │
│    Monitoring      │                                                      │
│    Alerts       ⑴ │                                                      │
│ APPLICATION        │                                                      │
│    Settings        │                                                      │
│ ────────────────── │                                                      │
│ 🛡 Read-only: off  │                                                      │
│ Kavka can produce  │                                                      │
├────────────────────┴──────────────────────────────────────────────────────┤
│ ● Connected · orders-prd · 10.0.4.19:9093 · READ-ONLY      ⌘K commands     │
└───────────────────────────────────────────────────────────────────────────┘
```

- **`.app-wire`** — 2px, painted across the very top of the window, inside the
  webview and below the native title bar. Native window chrome is untouched.
- **The rail** (`--rail-w` 254) — brand lockup, cluster card, and every
  destination in the app. Two render modes; see below.
- **Stage** — the current screen: a Perch, then panels, spaced by `--stack`.
  It is the scrollport, and **every navigation returns it to the top**.
- **Inspector dock** — 480px, resizable 320–60%, `Ctrl/Cmd+I`.
- **Status bar** (`--statusbar-h` 26) — spans the whole window. Live
  operational state only: progressive search progress, tail rate, connection
  latency, the always-visible bootstrap address (§6 layer 3), the read-only and
  masking chips, and the `⌘K` hint. Phase 2's "search never silently truncates"
  gate depends on it existing. **The version is not here** — see the brand
  lockup.

> **One token, one meaning.** There used to be two: `--rail-w` meant the 248px
> sidebar while the *mockup's* `--rail-w` meant the 254px screen rail, and
> `--nav-w` was the one that actually corresponded. `--nav-w` is deleted and
> `--rail-w` is 254px. Anyone reading a mockup measurement can now trust the
> name.

#### The rail renders in two modes

| | Always | Also, when `conn.status === "connected"` |
|---|---|---|
| **Identity** | brand lockup · cluster card | — |
| **Groups** | Set up → Connections<br>Application → Settings | Cluster · Observe · Safety · Integrations (the ten screens) |
| **Foot** | the read-only readout | — |

Everything in the *always* column has to work with **nothing connected**. That
is the whole reason Settings is a rail item rather than a cluster screen: on
first launch there is no cluster, and the two preferences a new user wants are
the theme and the font size.

`App.tsx` owns the rail and therefore owns which screen is on. `ClusterView` is
controlled — `tab` arrives as a prop, its first value read from
`initialTab(profileId)` — and still owns everything below the tab (which topic,
which pane, which broker) and the persistence of the whole record.

**A rail click always opens that section's ROOT.** Restoring a placement is a
promise about *reconnecting* — come back tomorrow and you are where you left
off. It is not a promise about pressing "Topics", which means *show me the
topics*, not *show me the message browser I had open three screens ago*. The
reset lives in `ClusterView`'s `lastNav` effect — keyed on the tab **and** the
nav nonce, so pressing the rail item you are already on counts as a navigation
and resets the placement too — and it lives there so the shell stays ignorant of
what a placement contains.

#### The brand lockup — and the version

`.brand` is the rail's first child: a 26px brass jackdaw, **Kavka** at
17px/680/-.015em, and `.brand-ver` right-pushed at 11px in `--text-tertiary`.

There was no brand lockup at all. The top-left of the window was the micro-cap
eyebrow `CLUSTERS`, the wordmark was a small grey word in the sidebar's footer
with no bird beside it, and the version sat in the far *right* end of the status
bar — the diagonally opposite corner, on a strip specced for live operational
state. A build number is not operational state. As an annotation on the name it
reads once and recedes. It also appears in the About dialog, and nowhere else.

#### The cluster card and the switcher

`.cluster-card` is a raised panel pinned between the brand row and the first
group: name + `EnvChip`, the mono bootstrap address, then a `status-dot` and a
sentence — *Connected · 3 brokers* / *Connecting…* / *Not connected*. **Prod
guardrail layer 3 lives here**: name, environment and address are pinned beside
every screen rather than above one of them, because most production accidents
are right-action-wrong-cluster. Law 2 holds — the connected line carries the
broker count, because "Connected" with no number is a claim nobody can check.

Its last child is `.cc-switch`: full width, `space-between`, the word *Switch
cluster* and a 13px chevron-down. **A down-caret on a full-width trigger reads
as a menu, not a drawer**, and that is what let the permanent 248px list go.

`ClusterSwitcher.tsx` owns the menu, which the mockup asserts and never draws —
so its anatomy is ours:

```jsx
<button className="cc-switch" aria-haspopup="menu" aria-expanded>…</button>
<div className="cc-menu" role="menu" aria-label={t("switcher.menuLabel")}>
  <div className="cc-menu-scroll" role="none">
    <div className="cc-menu-group" role="group" aria-label={env.name}>
      <div className="cc-menu-label" aria-hidden="true">{env.name}</div>
      <div className="cc-menu-item cc-menu-item-protected cc-menu-item-selected" role="none">
        <button role="menuitem" className="cc-menu-row" aria-current="true">
          <span className="cc-row-line">
            ·dot· name <EnvChip/> <PadLock/> <span className="sr-only">Protected</span>
          </span>
          <span className="cc-row-meta">{address} · {status}</span>
        </button>
        <button role="menuitem" className="btn btn-sm cc-menu-act"
                aria-disabled={busy || undefined}>Disconnect</button>
      </div>
    </div>
  </div>
  <div className="cc-menu-foot" role="none">
    <button role="menuitem" className="cc-menu-add">Add connection</button>
  </div>
</div>
```

- **The row is a container with two controls.** The row selects; the trailing
  button connects or disconnects. Choosing a cold cluster used to swap the whole
  workspace for a connection form — the sharpest single finding in the audit.
  It no longer does anything of the kind.
- **`position: fixed`, measured from the trigger.** The rail is a scrollport, so
  an absolutely positioned popover would be clipped. Fixed positioning escapes
  the clip without leaving the `.app` subtree, so the protected substrate and
  the danger damper still cascade into it (same reasoning as `Overlay`).
- **Rows are grouped by environment, in registry order** — the order the user
  put their environments in, so the protected group does not move around
  between openings. Profiles pointing at an environment the registry has never
  heard of are gathered at the end under their literal name.
- **The protected treatment moved over as the old `.profile-row` rules under
  `.cc-menu-item*` names** — warm ground, a 2px `--danger` left border, the chip
  beside it saying which environment — **and then gained the two channels those
  rules never had.** Ground, border and chip fill are all colour, and the chip's
  text names the *environment*, not its protection, so an org whose protected
  environment is called `UAT` read nothing at all here. The row now also carries
  `<PadLock/>` (the same glyph as the environments list, `aria-hidden`, inheriting
  the row's own text colour so it clears SC 1.4.11 on the warm ground) and an
  `sr-only` **Protected** — §6's third and fourth channels. The word is `sr-only`
  rather than printed because the visible line is already the cluster's name and
  its address, and guardrail layer 3 is the address staying legible. The
  Connections screen's *Saved clusters* rows get the identical pair, since they
  are the same markup.
- **It is a menu, not a dialog.** It borrows `Overlay`'s two promises — Esc
  closes, focus goes back to the trigger — and none of its furniture: no scrim,
  no `aria-modal`, no focus trap. Click-outside closes on `pointerdown`; Tab
  closes, because a menu you can Tab out of while it is still painted is a menu
  that lies about where focus is.
- **Roving focus over a FLAT list of menu items in DOM order** — row, its
  action, the next row, …, *Add connection*. `↑`/`↓` alone reach every control,
  which is the property that matters for rows with two jobs. `Home`/`End` jump
  the ends. Opening lands on the cluster you are already on.
- **A connecting row's action is `aria-disabled`, never `disabled`.** A disabled
  `<button>` is not focusable, and roving focus walks this list by *index* — it
  reads `document.activeElement` to find where it is. Focusing an unfocusable
  item is a silent no-op, so the index never advances and the next `↓` lands on
  the same dead control: arrow navigation stuck at whichever cluster is
  connecting, which is exactly when you open this menu to go somewhere else. The
  `onClick` guard is what makes the press do nothing; `.btn[aria-disabled="true"]`
  in `styles.css` (and its forced-colors `GrayText` twin) is what makes it *look*
  like the state it is. Any button inside a roving-focus widget owes the same.
- **Three wrappers carry `role="none"`** — `.cc-menu-scroll`, `.cc-menu-item` and
  `.cc-menu-foot`. ARIA 1.2 requires a `menu` to own its `menuitem`s (or a
  `group` of them), and an unroled generic div in between breaks that
  relationship — some AT responds by mis-counting the menu or dropping
  position-in-set. `.cc-menu-group` was already `role="group"`; these three are
  presentational, which is what `none` is for.

#### Where every former sidebar duty lives now

| Sidebar did | Now |
|---|---|
| the profile list | the switcher menu's rows **and** the Connections screen's 330px *Saved clusters* panel, which is the canonical enumeration — one `.cc-menu-row` markup, two homes |
| connect / disconnect | the trailing `.btn-sm` on each menu row (and `⌘K`) |
| **Add connection** | the menu's foot strip, `⌘K`, and the empty states |
| env chip + status | the cluster card, and each menu row |
| Settings | a rail item, in the Application group |
| About · Support Kavka | **Settings → About** — two rows, beside each other |
| the word *Kavka* | the brand lockup, with the bird back beside it |

About and Support have no home in the mockup at all, and inventing chrome for
them would be re-drifting. Settings → About already existed and already had an
*Open About* button; Support is now the row under it. Both remain in `⌘K`.

#### The rail foot — the safety readout

`.crail-foot` is pinned by `margin-top: auto` under a hairline and carries a
15px shield plus **read-only stated in BOTH directions**, with the consequence
on the next line:

> 🛡 Read-only: **off** — *Kavka can produce and delete here.*
> 🛡 Read-only: **on** — *Kavka will not produce or delete here.*

The app previously spoke only in the safe direction: a chip when read-only was
ON, and silence when it was off. **A guardrail that is silent in its dangerous
state is not a guardrail** — a user who wants to confirm that Kavka *cannot*
delete here had nothing to read. The foot used to hold a bare Disconnect button;
Disconnect is the trailing action on the cluster's own menu row now, **and it is
also the second of Cluster home's two stage actions**, where the mockup has it.

#### Cluster home's two actions — Refresh, then Disconnect

`stage.overview` is the only screen the shell binds actions for; every other
screen's actions arrive with the screen. It gets two, in the mockup's order:

| | What it is | What it can and cannot do |
|---|---|---|
| **Refresh** | `App`'s `topicsNonce`, the same handle the palette's *Refresh topics* pulls | Remounts the workspace, so everything read **live** is read again: the consumer groups, the alert log, the quorum, the broker settings behind the fold. It **cannot** re-read the connect-time metadata snapshot — the four tiles and the broker list came back with `cluster_connect` and change only on reconnect. |
| **Disconnect** | `onDisconnect(profile.id)` | Exactly what the switcher menu's trailing button does. |

**Refresh is not primary and it says its own limit.** Its `title` states which
half of the screen it re-reads, and Home's `.panel-foot` under the broker table
states the other half — *"A broker that has joined or left since then appears
here only after you reconnect."* A Refresh button whose promise is bigger than
its behaviour is the same failure as a verdict from partial data.

#### The cluster groups — grouping, and why they cover more than the mockup drew

Ledger showed the ten cluster screens as a strip of ten equal tabs. Ten peers in
a row tell you nothing about which one answers the question you arrived with,
and past ~900px of workspace the last tabs were unreachable by pointer *and* by
keyboard.

Jackdaw groups them, and **names each group after what its screens are about
rather than after Kafka's own nouns**:

| Group | Screens | `TabKey` |
|---|---|---|
| **Cluster** | Home · Topics · Consumer groups · Brokers | `overview` `topics` `groups` `brokers` |
| **Observe** | Monitoring · Alerts · Streams | `monitoring` `alerts` `streams` |
| **Safety** | ACLs · Masking | `acls` `masking` |
| **Integrations** | Connect | `connect` |

Someone who does not yet know what an ACL is can still guess that it lives under
*Safety*. That is the whole of the change.

> **FULL COVERAGE IS A CONTRACT.** Every screen reachable before this redesign is
> reachable here. **The Jackdaw mockup drew a four-item rail and left ACLs,
> Connect, Masking and Streams with no home at all.** That was an execution gap
> in a static drawing — a mockup only has to look right — and not the bet the
> direction is making. Reproducing it would have deleted four working surfaces
> from the product in the name of fidelity.
>
> `ClusterView.tsx` builds `TABS` by flattening `CLUSTER_RAIL`, so a screen that
> is not in a group is not in the app, and the omission is a compile-visible
> fact rather than a UI someone has to notice is missing.

> **`TabKey` values are PERSISTED and must not change.** They are written into
> every user's `kavka.cluster.<id>.view` record. Renaming one silently moves
> people off the screen they were last on. Only the presentation moved.

**The mockup titled its cluster-scoped group with the live cluster** — a
`state-dot` plus `local · DEV` instead of a static word — and called that its
biggest wayfinding idea. Kavka keeps the four concept words instead, and takes
the identity from the cluster card directly above them, which says the same
three facts with the address as well. The concept naming is load-bearing here in
a way it was not for a four-item rail: it is what teaches that ACLs live under
Safety. **This is a deviation, and it is owned in §11.**

**It is not a `role="tablist"`.** A tablist may not contain group headings, and
the headings are the entire point. The rail is a `<nav>` whose current item
carries `aria-current="page"`; Tab walks it, as it does in every other sidebar.
Arrow-key roving is a tablist affordance and went with the tablist.

The current screen is marked in **four** channels (Law 2): an `--accent-tint`
ground, a weight change, a 3px accent spine, and `aria-current`. In forced
colors the tint and the spine collapse, so the spine re-declares `Highlight`
with `forced-color-adjust: none`.

Below 900px the rail stops being a column and becomes a band above the stage,
capped at 45vh. That is why the old "make the tab strip scrollable" reflow rule
is gone: there is no strip left to scroll.

#### Markup the sweep agents should reuse

```jsx
<nav className="rail" aria-label={t("rail.navLabel")}>
  <div className="brand">
    <BrandBird />
    <span className="brand-name">Kavka</span>
    <span className="brand-ver">{version}</span>
  </div>

  <div className="cluster-card">
    <div className="cc-top"><h1 className="cc-name">…</h1><EnvChip /></div>
    <div className="cc-addr">…</div>
    <div className="cc-state cc-state-connected">·dot· Connected · 3 brokers</div>
    <ClusterSwitcher … />
  </div>

  <div className="crail-group">
    <h2 className="crail-label">{t("rail.group.observe")}</h2>
    <button className="crail-item" aria-current="page">
      <svg className="crail-icon" />
      {t("rail.item.alerts")}
      <span className="crail-badge" title={…}>3<span className="sr-only"> firing</span></span>
    </button>
  </div>

  <div className="crail-foot">…the read-only readout…</div>
</nav>
```

The `crail-` prefix is historical: these rules were written when the cluster
rail was a second navigator inside the workspace. There is one rail now, and
these are its groups.

Inline tabs (`.tab` / `.tab-active`) survive for genuine sibling panes inside one
surface — the export/import pair, the inspector's panes, the producer's modes.
`.tabstrip` and `.tab-badge` are gone.

#### Still missing from this shell

Named here so the next reader does not mistake absence for intent. The list is
short now; the three items that used to be on it — the stage head, the
Connections split, the panel feet — have all landed and are documented above.

- **The three full-height panes draw no stage head.** Messages, search and SQL
  own the whole stage: their own scrollport, their own status line, their own
  docked inspector. A head above them would take that height from the table and
  duplicate the `← orders` breadcrumb they already draw. They are the first
  screens that should own their heads outright — the trail *local · DEV ·
  Topics · orders · Messages* is exactly what repairs the
  rail-says-Topics-while-you-read-messages confusion (§11) — and that is a
  change inside those components, not a default the shell can guess.
- **No screen passes `chips` yet.** `StageHead` accepts them; Cluster home is
  the only screen passing `actions` (Refresh · Disconnect, above), and the rest
  keep their controls in their panel heads where they were already discoverable.
- **The 14 files in `docs/screenshots/` all show the deleted sidebar** and the
  pre-alignment Cluster home and Settings. They need re-shooting against a
  running app with a live cluster; nothing in this document describes what they
  show.

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
  so nothing reflows as data streams in — and so a px width tuned to today's
  data does not overflow tomorrow's. The messages offset column is the worked
  example: at `--gutter-w`'s 72px, minus 12px of cell padding each side, it
  fits about six mono characters — and a space-grouped offset
  (`1 234 567 890`) is 13, so every busy topic spilled the address across the
  ledger rule. It is now `calc(13ch + var(--s-5) * 2)` (12ch compact) with an
  `overflow: hidden` + ellipsis backstop on the **data** cells only — the
  header cell holds a focusable `<Term>` and §5.3's clipping rule applies.
  Two things a `ch` column width must not get wrong: `ch` resolves against
  the element's **own** font, so the `<col>` carries the mono family and size
  its cells do; and `box-sizing: border-box` makes the width a border-box
  width, so the cell's padding is part of the number. **Alignment selectors must be
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
- **A bare fieldset has no border** — it is a `--t-micro` uppercase eyebrow plus
  12px of space, with `<fieldset>`/`<legend>` semantics kept via `border: 0`.
  Jackdaw promotes the *connection form's* fieldsets to panels
  (`.editor > .fieldset`), and nothing else. **If a border is ever put back on
  `.fieldset` globally, the legend must be fixed in the same commit.** A
  `<legend>` whose computed `float` is `none` and whose computed `position` is
  `static` is the fieldset's *rendered legend*: the browser cuts a notch out of
  the top border for it and lays it out against the border box, not the padding
  box — so a bordered fieldset gets a gap punched through its top edge and a
  heading sitting outside its own padding. `float: left` fails the rendered-legend
  test and takes the notch away; inside a `display: flex` fieldset nothing
  actually floats, so it costs nothing. The scoped fix in
  `styles/jackdaw-shell.css` covers the editor only, by design.
- **Environment picker** — a **wrapping row of chips**, one per defined
  environment, `role="radiogroup"` with a roving `tabIndex`, followed by a
  `Manage environments…` ghost button *outside* the group (a radiogroup with a
  non-radio child is a broken promise about what the arrows reach). Each chip
  is `--control-h`, `--r-pill`, `--bg-sunken`, `--border-control`. The chosen
  one takes `--env-tint` fill / `--env-ink` text — or the badge pair when the
  environment is protected, so the form previews the switcher — **plus a `✓`
  glyph and `--w-medium`**, because Law 2 applies to a picker too.
  **Picking a *protected* environment swaps the substrate live** and turns the
  rule its colour. That is the single best moment in the product to teach the
  guardrail. It was a fixed three-segment control; nine segments in a
  non-wrapping row is a horizontal scrollbar inside a form.
- **The environments manager** (`EnvironmentsManager.tsx`) is a `modal-wide`
  overlay reached from that button. It lists every environment with its chip,
  the word *protected* or *not protected* (never the colour alone) and how many
  connections use it; adding takes a name, one of the seven colour swatches —
  each swatch labelled with its colour's **word**, selected state shown as a
  near-white inset ring rather than a hue — and a protected checkbox whose hint
  states in one sentence what flipping it changes, because two of the four
  things it changes are in other processes (the CLI and the MCP server).
  **Deleting is refused while a connection still names it**, and the dialog
  turns that refusal into a reassignment: pick where those connections go,
  Kavka rewrites them one `profiles_save` at a time, *then* removes the
  definition. Removing protection — from an environment that has it, or by
  deleting one — takes the same type-to-confirm as any other destructive
  action, which is §6 layer 4 applied to the thing that defines §6 layer 4.

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

All `--control-h`, `--r-control`, `--t-base`, sentence case, no transform on
press.

| Variant | Fill | Border | Text | Use |
|---|---|---|---|---|
| Primary | `--brass-fill` | same | `--on-brass` (dark 9.05:1 · light 5.92:1) | **Exactly one per surface** |
| Secondary (`.btn`) | `--bg-panel` | `--border-control` (3.55 / 3.40:1) | `--text-primary` | Everything else |
| Ghost | transparent | none | `--text-secondary` | Toolbars, dismissals |
| Danger | transparent | `--danger-border` (3.37 / 3.94:1) | `--danger` | *Opens* a confirmation |
| Danger-confirm | `--danger-fill` | **`--danger-border`** | `--on-danger-fill` (6.07 / 6.63:1) | **Only one click from the action** |
| Latched toggle | `--accent-tint` | `--accent` | `--accent` (≥ 5.72:1) | Live tail ON, saved filter active |

**The primary button is the accent now, not near-white.** Ledger could not spend
its accent on a button because teal meant *live*; Jackdaw's accent means
"clickable or selected" and nothing else (§2), so the one thing per surface you
are meant to click is exactly what it is for.

**Danger-confirm's border is `--danger-border`, not its own fill.** The solid
fill measures 2.35:1 against a selected row in the warm dark ground, so the edge
is what satisfies SC 1.4.11 (§3).

**The guardrail rule: a filled red button only ever exists one click away from
the destructive action happening.** Everywhere else destructive controls are
outlined.

**Latched state must read as ON across the room** — a live-tail toggle is a
physical switch, not a link.

**Loading keeps the label and cannot change the button's width** — `.btn-swap`,
with `aria-busy` on the button. A button that swaps `Connect` → `Connecting…`
resizes mid-click and shifts everything after it. Anything over 2s gets a
visible Cancel.

```jsx
<button className="btn btn-primary btn-swap" aria-busy={busy}>
  <span className="btn-swap-face">Connect</span>
  <span className="btn-swap-face btn-swap-busy">
    <span className="spinner" aria-hidden="true" />
    Connect
  </span>
</button>
```

Both faces occupy **one `inline-grid` cell**, so the cell is sized by the wider
of them and the width is fixed by construction. The inactive face is
`visibility: hidden`, which keeps its layout box and removes it from the
accessibility tree — which is why the label is duplicated rather than
conditionally rendered, and why neither face carries `aria-hidden`.

> **This replaced `.btn-busy-slot`, and the replacement is the field report.**
> The old rule reserved a permanently rendered 16px box beside every
> busy-capable label. It bought the no-resize property honestly, but it paid for
> it with an **empty box on every idle button** — which the owner saw as "a weird
> UI expansion, or an icon blending into the background" on the connection
> editor's Connect button, and which pushed 33 labels off centre app-wide. All
> 33 call sites are converted, the rule is deleted, and two of them turned out
> to be buttons that could never spin at all (Search and Run, each swapped for
> Stop the instant work starts) — those simply lost the box. `aria-busy` is now
> load-bearing rather than decorative: `.btn-swap` keys its face swap on it, so
> a busy button that forgets the attribute is a busy button that never spins.
> One was found that way during the migration.

**Disabled** — `opacity: .45`, `cursor: default`, and **always a `title`
explaining why**. Never a dead control with no reason. This includes
busy-disabled buttons (*"Kavka is saving this connection"*) and **disabled
`<option>`s** — the four not-yet sign-in methods each carry the same sentence,
because an unexplained greyed-out choice reads as a broken build, not a
roadmap.

**Toolbar labels** ≤7 characters in English; budget ~40% width growth for the
Phase 6 languages, and ship a `⋯` overflow menu from day one.

### 5.6 Chips

- **Env** — 18px pill, `--t-micro`/600 uppercase tracked, carrying the
  environment's **name as the user typed it** (uppercased in CSS, never
  translated — it is the same string as the forced-colors wire label and the
  CLI's refusal). Unprotected → `--env-ink` on `--env-tint`, a **tag**.
  **Protected → `--env-badge-ink` on solid `--env-badge-fill`**, the only
  *filled* chip, so it reads as a badge. Ratios per colour in §3.1. The chip
  carries its own `data-env-color` / `data-env-protected`, which is how a
  switcher full of different environments paints correctly underneath one
  app-level colour: the nearest declaration wins.
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

**The placeholder is a promise about the engine, not a label.** "key, value or
headers" means the raw-byte prefilter reads all three — header **names** and
header **values** included. It was written before the prefilter did, and for one
phase it read two of the three: the records it silently skipped were
indistinguishable from records that never matched, which is the kind of wrong no
screenshot catches. If the copy and the engine ever disagree again, the engine
is what changes.

**The cheatsheet teaches `value_text`, never `string(value)`.** The CEL
activation binds `value` with the *shape* of the payload — a map for JSON, a
string for text — which is what makes `value.status == "failed"` possible and
what makes `string(value)` an **error** on every JSON record, because CEL has no
map→string conversion. The cheatsheet's "look anywhere in the body" line taught
exactly that broken expression. `value_text` is bound for every record whatever
its shape (a tombstone's is `""`), and it holds the same text the table is
showing, so the filter matches what the user is looking at. The full activation
is documented once, on `CelFilter` in `crates/kavka-core/src/search.rs`; the
cheatsheet is a view of it and must not grow a second opinion.

**Search says what it could not judge, and where it stopped early.** Two counts
ride the progress contract for the same reason the buffer cap does (§7 rule 5,
and the phase's "never silently truncates" gate): `unevaluated` is records the
expression could not be evaluated against — read, not judged, and not matches —
and `assumed_complete` names the partitions that finished because nothing more
arrived rather than because they reached the end offset captured at the start.
Both render as one honest sentence apiece; neither is folded into `scanned` or
into a bar's length.

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

**Protected de-collision (required):** on a protected cluster a danger banner
drops its red tint for a neutral `--bg-raised` shell with a `--danger` left bar,
and the env rule dampens while it is visible
(`[data-env-protected="true"][data-alert="danger"]`). Otherwise the warm
substrate plus a coral banner is an undifferentiated red wash and **both**
signals die.

> **The damper moves chroma, not lightness.** The undampened red rule is
> already only **3.09:1** on a selected protected row, so there is no darker
> coral left to dampen *to*: the first implementation used `#6E3A33`, which
> measured **2.09:1** on the canvas and **1.83:1** on a selected row — the most
> load-bearing guardrail (layer 1, the one that needs no banner) switched
> itself off at exactly the moment an error was on screen. That is the worst
> possible failure of the two-signal composition this section exists to
> protect. The damper is now `#9C8079`: the same rule at roughly a third of
> the chroma (channel spread 35 vs the coral's 96) and *more* luminance
> contrast, **4.55:1 worst case** (4.61 on a selected protected table row). It
> reads as muted; it never reads as gone. Light has its own damper
> (`#8A6F6A`, 4.09:1 on a selected light row, 3.41:1 on a selected light
> protected switcher row) or it inherits the dark one.
>
> **One damper serves all seven colours, and it keys on PROTECTION rather than
> on the colour that collides.** The figures above are measured on the warm
> protected surfaces, which is the only place a dampened rule ever renders, and
> a neutral warm grey reads as *muted* behind a coral banner whatever hue it
> replaced. Seven damped variants would be seven more tokens to sweep for a
> composition nobody can tell apart — and keying the damper on `red` would
> re-introduce exactly the name-shaped special case §3.1 removes.
>
> `data-alert="danger"` must be set for **any** danger on screen, including an
> inline banner inside a form — not just the global one. Miss the inline case
> and a protected cluster shows a coral rule behind a coral banner, which is
> the single composition §9 gate 4 exists to catch.

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
wire as the protected-environment bar, so the two guardrails rhyme. The confirm button restates
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
- On a protected cluster the palette inherits the warm substrate, the profile
  row's context line gains the words `protected cluster`, and destructive
  commands render their subtitle as `payments-prod · asks for confirmation` in
  `--danger`.

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

### 5.12 The Perch — the answer-first banner

Jackdaw's signature element and Law 4's implementation.
`apps/desktop/src/Perch.tsx`, styled at `.perch` in `styles.css`.

One warm note at the top of **every** screen, with the bird sitting on it,
answering two questions in the product's own voice: *what am I looking at*, and
*what can Kafka actually tell me here*. It is where the honesty culture lives.
The rules that govern what it may claim are in §7 — they are voice rules, not
component rules, and they are non-negotiable.

#### Props

```ts
interface PerchProps {
  screen: string;              // "Cluster home", "Topics" — translated by the caller
  tone: "ok" | "watch" | "problem" | "unknown";
  children: ReactNode;         // THE VERDICT. One line, from live state.
  loading?: boolean;           // outranks everything — see §7
  error?: string | null;       // RAW; classified inside, never by the caller
  caveat?: ReactNode;          // what the verdict does not cover
  errorContext?: ErrorContext; // passed through to classifyError
  actions?: ReactNode;         // at most one or two
}
```

**Precedence is resolved inside the component, in one place:** `loading`
outranks `error` outranks the caller's `tone`. Every screen would otherwise get
it subtly wrong in a different way.

**`error` takes the RAW string.** The classification happens inside `Perch`, so
one screen cannot accidentally put a librdkafka sentence in the banner the whole
app is judged by. See §7's error library.

#### Anatomy

| Part | Class | Rule |
|---|---|---|
| bird | `.perch-bird` | decorative, `aria-hidden` |
| kicker | `.perch-kicker` | `{screen} · {state}` — **the word for the tone edge** (Law 2) |
| verdict | `.perch-verdict` | one line, `--f15`, from live state |
| caveat | `.perch-caveat` | what the verdict does not cover — **never behind a disclosure** |
| actions | `.perch-actions` | optional, at most two |
| hide | `.perch-hide` | *Hide*, or *Show the whole note* in one-line mode — see below |
| restore | `.perch-restore` | the `.btn-sm` Hide leaves behind |

Tone paints a 4px left edge (`--perch-edge`) in `--ok` / `--warn` / `--danger` /
`--line-strong`. **The edge is decoration.** The kicker spells the same state in
words on every screen, which is why forced-colors mode can drop the edge
entirely and lose nothing.

#### Visibility — the Hide control, and the rule it does not break

This component used to refuse the mockup's *Hide* pill outright: a verdict the
user can switch off is a verdict the app stops being accountable for. That
argument is sound about a **verdict** and wrong about a **note**, and this one
component is both — it absorbed the mockup's teaching note and its separate
`.verdict` card. So the refusal is replaced by a distinction rather than
dropped.

- **What can be hidden** is the standing note: the sentence that reads the same
  on the fortieth visit as it did on the first.
- **What can never be hidden** is a screen that is still reading, or a screen
  whose read failed. Both force the full form back, **in every mode, on every
  screen**, and they take the Hide control away while they hold it — a control
  that would have to do nothing is worse than one that is not there. *"Do not
  tell me the cluster is fine"* has never meant *"do not tell me the numbers
  are missing"*.

Two controls express that, and the precedence between them is resolved in one
expression in `Perch.tsx`: **forced** (loading or error) outranks **this
screen's own Hide/Show** outranks **the stored preference**.

| Control | Scope | What it does |
|---|---|---|
| `.perch-hide` | this screen, this session | *Hide* → the note is replaced by `.perch-restore`, a `.btn-sm` reading *Show the note for this screen*. In one-line mode the same pill reads *Show the whole note* and expands instead. This is the mockup's own behaviour, restore button included. |
| `appearance.perch` | durable, all screens | `full` (default) · `line` · `hidden`. Settings → Appearance. |

**A preference of `hidden` renders nothing at all** — not even the restore
button — while a Perch the user hid *by hand* always leaves the way back on
screen. The asymmetry is the point: somebody who pressed Hide a minute ago
needs the door, and somebody who turned Perches off last month does not need a
button on all ten screens reminding them they did.

**Hide and Show hand focus to each other.** They are two elements in two
branches — pressing one unmounts the other's parent — and React moves nothing,
so an unguarded press drops focus to `<body>` and the next `Tab` restarts at the
top of the document, which on this shell is the rail. `Perch.tsx` holds a ref on
each button and focuses the newly-rendered one in an effect keyed on the local
mode, gated by a *just toggled* ref so it fires on a **press** and never on
mount — every screen renders a Perch, and an effect without that gate would
yank focus to the Hide pill on arrival at all ten.

**One-line mode drops exactly one thing: the caveat.** That is why the control
beside it reads *Show the whole note* rather than *Expand* — nothing is
silently missing, and the sentence that says so is one visible click away. A
caveat that came from `classifyError` never reaches one-line mode, because an
error is forced back to full. The verdict itself truncates with an ellipsis;
it is the only sentence in the app that is allowed to.

#### One thing it deliberately does not have

- **No `role="status"`.** This is standing content that happens to change, not
  an announcement. `role="status"` would make a screen reader read every
  screen's verdict on arrival, over the heading the user came for. It is a
  `<section>` with an `aria-label`.

#### The reference implementation

`OverviewPerch` in `ClusterView.tsx`. Every other screen's verdict is written
against it. Three things make it honest and all three are worth copying:

1. It is derived from **live** state — the broker list the cluster actually
   answered with, and the set of alert rules firing right now. No constant, no
   "looks good" that is true by construction.
2. **Its worst case is a real case.** A cluster that connects and reports zero
   brokers is a real failure mode of a load balancer in front of Kafka, and the
   verdict says so instead of rendering a cheerful "0 brokers".
3. It carries a caveat it would have been easy to omit: those counts came back
   at the moment of connection and do **not** track the cluster. A banner that
   let a user believe otherwise is precisely the failure §7 forbids.

### 5.13 The stage head — how a screen introduces itself

`apps/desktop/src/StageHead.tsx`, styled at `.stage-head` in `styles.css`.
The first child of the stage, **above the Perch**.

Three registers, in this order:

| Part | Class | Rule |
|---|---|---|
| where you are | `.whereami` | 12px tertiary trail, a 13px right-arrow then middot-joined crumbs. Opens with the cluster identity or the rail group, ends with the screen. |
| what this is | `.sh-title` | 26px/`--w-semi`/`--track-tight`, a flex row so `chips` sit on its baseline |
| — | `.sh-sub` | **one** sentence, 82ch. This is where the screen says what it cannot tell you. |
| what you can do | `.stage-actions` | `margin-left: auto`, two or three, rightmost usually `.btn-primary` |

The order is the argument: the head says what this screen **is**, and then the
Perch says what Kafka can actually **tell** you about it. A verdict that
arrives before its subject is a verdict about nothing.

**The trail is plain text, not links.** Nothing in it navigates today, and a
breadcrumb whose crumbs are dead should not claim `role="navigation"`. It reads
out in order immediately before the heading, which is where a screen-reader
user wants it. The first crumb to earn a destination is *Topics* on the message
browser.

**It is an `<h2>`.** The rail's cluster card carries the document's `<h1>` — the
cluster is what the whole window is about and the screen is a view of it. Panel
titles inside the screens are `<h2>` as well, which is flat and was flat before
this component existed; demoting them to `<h3>` belongs with the per-screen
work below.

**It scrolls with the screen,** which is the one deliberate departure from the
mockup. There the head is a fixed band above a `.stage-body` that scrolls under
it; here the stage **is** the scrollport (`.workspace-body`), so the head is the
first child of `.cluster-view` and takes the stage's own horizontal padding
rather than a second inset that would have to agree with it. Every navigation
puts the scrollport back at the top (§5.14), so the head is on screen on
arrival — which is what a fixed band buys, without a second scroll container.

#### The registry

```ts
export const CLUSTER_HEADS: Record<TabKey, ScreenHead>
interface ScreenHead { titleKey: MessageKey; subKey: MessageKey; ownHead?: true }
```

`Record<TabKey, …>` is the guard: **adding a screen to `CLUSTER_RAIL` without
writing its title and its sentence is a compile error**, the same shape `TABS`
gives the rail. No screen can ship headless by omission.

- **Titles are usually the rail's own label.** The rail and the title agreeing
  *is* the wayfinding, not a duplication to optimise away. `overview` is the
  exception: "Home" does not read as a title on its own, so it gets the noun
  back — *Cluster home*.
- **Actions are not in the registry and never will be.** A registry can hold a
  message key; it cannot hold a handler bound to the cluster on screen without
  becoming a second copy of the screen's props.
- **`ownHead` is the handover.** While it is absent, `ClusterView` draws the
  head. A screen that has grown its own — with the chips, the actions and a
  trail that knows which topic is open — sets it and renders `<StageHead>`
  itself. It is a per-screen switch rather than a big-bang migration precisely
  so the ten screens can convert one at a time without a frame that renders two
  heads or none.

**The three full-height panes get no head.** The message browser, search and
SQL own the whole stage — their scrollport, their status line, their docked
inspector — and a head above them would take that height from the table and
duplicate the `← orders` breadcrumb they already draw. They are the first
screens that should own their heads outright: the trail
`local · DEV · Topics · orders · Messages` is exactly what repairs the
rail-says-*Topics*-while-you-read-messages confusion (§11).

### 5.14 Navigation — the stage goes back to the top, and a rail press opens the root

Two behaviours, both invisible in review because both only misbehave when the
screen you left was longer or deeper than the one you arrived on.

**One scrollport, one mechanism** (`src/stage.ts`). The stage registers itself
(`ref={registerStage}`); anything that knows it has moved the user calls
`useStageTop(token)` with a string describing where "here" is. Two components
own navigation and neither can see the other's state — `App` owns the screen
and the rail item, `ClusterView` owns the placement below it (which topic,
which pane, which broker, which connector) — so the **scrollport** is what they
share rather than the state. No screen scrolls itself: ten copies of
`useEffect(() => scrollTo(0))`, nine of them right, is the shape this exists to
prevent.

**A rail press always opens the section's root.** Restoring a placement is a
promise about **reconnecting** — come back tomorrow and you are where you left
off. It is not a promise about pressing *Topics*, which means "show me the
topics", not "show me the message browser I had open on one of them three
screens ago". `ClusterView`'s `lastNav` effect clears `topic`, `pane`, `group`,
`broker`, `connect`, `connector` and `streamsGroup` on every press.

> **It keys on the press, not on the tab.** Watching `tab` alone missed the
> case users hit most: pressing *Topics* while already on Topics — the plainest
> "take me back to the list" there is, and the one that changes no state at
> all. `App` folds a counter (`session.nav`) into the props and the scroll
> token, so the press itself is the event. A new cluster session resets it to
> zero, because arriving on a restored placement must not look like a press.

**The exception, and it is the only one.** Something can ask for a specific
thing *inside* a section on the way — the alert toast's *View group
demo-checkout* is the one caller. It parks the selection in `pendingGroup` and
then presses the rail; the `lastNav` effect adopts it once instead of clearing,
then forgets it. Without that hand-off the navigator would `setPlace` and the
effect would wipe it one tick later, which is a bug neither file looks like it
has on its own. Any future deep link goes through the same door.

### 5.15 The panel foot — the sentence a table ends on

**Law 4 says every screen answers before it reports. The panel foot is the
other half: every table says what it cannot tell you, under itself, in plain
English.** The Perch is the verdict for the screen; the foot is the caveat for
one table, and it is where the qualification lives that would otherwise be
either missing or buried in a tooltip nobody opens.

```html
<section class="panel">
  …head, table, optional <details class="disclose">…
  <p class="panel-foot">What the rows above cannot tell you.</p>
</section>
```

- **Full-bleed.** `margin: 0 calc(var(--pad-panel) * -1) calc(var(--pad-panel) *
  -1)`, so its top hairline spans the panel's whole width. A foot that floats
  inside the padding reads as a box in a box.
- **It must be the panel's last child.** `.disclose` full-bleeds too, and two
  stacked full-bleeders each pull the next up over the panel's own edge.
- **13px `--text-tertiary`.** Quieter than the table, never a warning. A foot
  that shouts is a foot people stop reading.
- **It is translated** even though the table above it is not. That is a
  deliberate seam and `docs/I18N.md` §1 states the rule that draws it: a caveat
  is translated when it stands alone as its own block, and stays inline English
  when it is one clause in an already-English line.

**What a foot must contain: a limit, not a summary.** *"Shape only — this is
metadata read when the screen opened"* is a foot. *"5 topics, 12 partitions"* is
a count, and counts belong in the panel head. The test that has worked: write
the sentence a support engineer would have to add out loud if somebody quoted
this table at them in an incident review.

**Where they are today:** Topics, a topic's partitions and its config, schema
versions, consumer groups and their members and lag, brokers and a broker's
config, Monitoring's lag / health / throughput / no-endpoint / no-series states,
Streams' topology, ACLs, Masking, both Connect tables, share groups, Alerts'
rules and channels, Cluster home's triage list and broker table, and the
Connections screen's saved-clusters panel. 28 catalog keys. The audit called
this "the highest fidelity-per-effort item in the whole product"; it was right,
and the reason is that it is prose, so it carries no layout risk at all.

### 5.16 Cluster home — the one screen that triages

The audit's sharpest sentence was about this screen: **"The app's Home reports
and never triages."** It had three regions stacked full width — a floating row
of four numbers, a broker table, a quorum panel. It now has five, and the two
that carry the direction's whole thesis are the two that were absent.

1. **Four tiles with a third tier** (`.stat-grid.stat-grid-tiles`). *Brokers 1*
   is a number; *Brokers 1 / as the cluster named them when you connected* is an
   answer **and** its own caveat. The fourth tile counts consumer groups and
   says how many are settled; it takes the attention spine when any is not, and
   the spine never carries that meaning alone (Law 2) — the sub-caption says the
   word.
2. **The cluster id, outside the tile row**, as a mono literal. It is a string
   Kafka handed over, not a quantity Kavka counted, so it is not a card and it
   is not sans (§4).
3. **"Needs a look" — the triage list.** Every row is something Kavka can
   **prove** from data it holds: an alert rule firing right now (from this
   connection's own alert log, `resolved_ms === null`), and a consumer group
   Kafka itself reports as anything other than Stable. The panel head says so
   out loud, because a triage list that looks exhaustive and is not is worse
   than no list. Each row carries a deep link to the screen that answers it —
   and when no navigator is threaded down, it states the screen in words rather
   than drawing a button that cannot navigate.
4. **Brokers beside the quorum** (`.home-split`), not above it. Both are short
   tables on a 1320px stage, and stacking them pushed the quorum — the thing you
   read when the brokers all look fine and nothing works — below the fold. It
   wraps to one column under the split's own min-width.
5. **A `.panel-foot` on both tables** (§5.15).

**Two things Home deliberately does not have.**

- **No Topics table.** The mockup draws one; Topics is its own rail screen with
  filtering, creation and per-topic drill-down, and a second poorer copy here
  would be two places to look for one answer. The audit agrees — *"do not
  restore that one"*.
- **No worst-lag tile.** The mockup's fourth tile is *Worst lag 82 /
  demo-checkout on payments*. Computing it means asking **every** consumer group
  for its committed offsets — one admin round trip per group, on the screen that
  opens the moment you connect. On a cluster with two groups that is invisible;
  on one with four hundred it is a Home screen that hangs, and the number would
  still be a snapshot the instant it arrived. Reporting a number Kavka did not
  measure is the one thing this redesign exists to refuse, so the tile counts
  groups instead and the lag question is answered where it is asked — the
  Consumer groups screen, and any lag rule the user actually asked Kavka to
  watch, which **does** appear in "Needs a look" the moment it fires.

**Both sources, or neither.** The triage list is built from the alert log *and*
the group list, so it reads as "still reading" while either is in flight, says
which half is missing when one failed, and refuses to render *"Nothing needs a
look"* while half the evidence is outstanding. A cheerful verdict from partial
data is the exact failure this product exists to prevent, and Home is where it
would have been most expensive.

---

## 6. Protected-environment guardrails — nine layers, ordered by survivability

**Every layer below is gated on the connection's environment being marked
`protected` (§3.1), never on it being called `prod`.** The flag is the gate;
the name is user data. An org whose production environment is called
`PRODUCTION`, `PRD` or `live` gets all nine, and an org that marks UAT
protected gets them there too.

1. **The ledger rule takes the environment's colour** in every table in the
   app. No banner required; the cluster is identified wherever data is. (This
   layer is *identity*, and it applies to every environment — but it is what
   makes the other eight legible, so it stays first.)
2. **The 2px `--env-wire`** across the very top of the window. Zero vertical cost,
   permanently peripheral, no text to habituate to. In forced-colors it thickens
   and gains **the environment's own name, uppercased** — `PROD`, `PRODUCTION`,
   `UAT` — via `content: attr(data-env-label)`.
3. **The bootstrap address is always on screen** — in the rail's cluster card,
   on every switcher menu row, and in the status bar. *Most production accidents are
   right-action-wrong-cluster.*
4. **Type-to-confirm on every destructive modal, environment-gated not
   action-gated.** A protected environment always asks; an unprotected one
   never does. Friction where the stakes are, nowhere else.
5. **Warm substrate.** Nobody will name it; everybody will feel it on switch.
   This is the *bonus*, not the guardrail — layers 1–4 are load-bearing. Escape
   hatch: `[data-env-intensity="rule-only"]`. **One warm-danger substrate for
   every protected environment, whatever colour its chip is.**
6. **Protected rows stay tinted in the cluster switcher** whether selected or
   not, plus a 2px `--danger` left border.
7. **Write actions change class in a protected environment** — Produce, Delete,
   Reset render as danger-outlined even when routine, and the produce form
   carries an undismissable warning banner.
8. **Read-only defaults ON** when a protected environment is picked in the
   connection form, with the reason stated as it happens. — *specified, not
   shipped.*
9. **The window title carries it** — `Kavka · orders-prod · PROD`, the last
   part being the environment's own uppercased name, so the taskbar, Dock and
   `Cmd+Tab` warn too. `src/windowTitle.ts`, called once from the shell.

> **Layer 9 is the one layer not gated on `protected`.** Every connection puts
> its environment in the title, protected or not — which is the mockup's own
> titlebar (`Kavka · local · DEV`) and which weakens nothing, because the
> warning here *is* the word: `PROD` appears in the taskbar only on a prod
> cluster whether or not `DEV` appears on the others. The environment's
> spelling comes from the registry rather than from `profile.environment`, so
> a renamed environment renames the title and the chip together; the state
> (connecting / connected) is deliberately **not** in it, because a title that
> flickers is a title people stop reading. It needs
> `core:window:allow-set-title` in `src-tauri/capabilities/default.json` —
> `core:default` grants only the read side — and that grant reaches the binary
> at build time, so a shell built before it landed rejects the call and the
> title silently stays "Kavka".

> **Layer 8 is specified and not shipped**, marked the same way the light theme
> is in §10 and for the same reason: a spec that reads as shipped is worse than
> one that admits it isn't, because the next person audits against it. It needs
> a rule for the edit that *removes* protection from an environment a
> connection already sits in — silently un-flipping somebody's deliberate
> `read_only: false` is a worse failure than not defaulting at all. **Layers
> 1–7 and 9 ship, and 1–4 are the load-bearing ones** (§6's own ordering), so
> the guardrail is not waiting on it.

Outside the app the same flag drives the two write gates: the CLI's
**`--yes-prod`** and the MCP server's **`KAVKA_MCP_ALLOW_PROD`**. Both keep
their names — they are in scripts, shell history and MCP client configs, and
renaming a flag to improve a sentence breaks somebody's cron job — but both are
documented as *"required when the connection's environment is marked
protected"*, which is what they now actually check.

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

### The Perch's four honesty rules

The eight rules above govern every sentence in Kavka. These four govern the one
sentence per screen that claims to summarise a cluster, and they are stricter
because a verdict is quoted. `Perch.tsx` enforces the precedence; a reviewer has
to enforce the wording.

1. **While loading, say so.** `loading` outranks tone, verdict and caveat: the
   Perch renders *"Still checking — Kavka will say what it finds as soon as the
   cluster answers."* A banner that renders "Everything looks healthy" from an
   empty response is worse than one that renders nothing at all.
2. **On error, speak through the library.** A raw string is never the verdict.
   `Perch` calls `classifyError` itself, so *"what happened"* becomes the
   verdict and *"the next click"* becomes the caveat, in the vocabulary the rest
   of the app already uses.
3. **Never invent certainty.** `tone="unknown"` exists precisely so a screen
   that cannot answer has somewhere to sit. Reach for it. "Not sure yet" is a
   complete and respectable verdict.
4. **Never be cheerful about data you don't have.** If the verdict rests on a
   partial page, a sampled window, or numbers fetched a while ago, pass
   `caveat`. It renders beside the verdict, never behind a disclosure, because a
   qualification one click away is a qualification that gets quoted without it.

The failure this set exists to prevent has a shape: a green banner reading
*"All 12 brokers healthy"* above a table that only loaded 12 of 40 brokers
before the request timed out.

### DO / DON'T

| DON'T | DO |
|---|---|
| `Save Profile` | `Save` |
| `BOOTSTRAP SERVERS` | `Bootstrap servers` |
| `Invalid input.` | `Use host:port, e.g. broker-1:9092` |
| `Connection name is required.` | `Give this connection a name so you can find it in the cluster switcher.` |
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

**Two variants, and which one you want is decided by where the nothing is.**

| | `.empty-state` / `.empty-block` (`styles.css`) | `.teach` (`styles/jackdaw-shell.css`) |
|---|---|---|
| Where | the whole viewport | **inside a panel**, where the data would have been |
| Shape | centred block, 52ch, left-aligned text inside | left-aligned horizontal row: 56px `.teach-art` tile, then the words |
| When | genuine first run, a failed profile read — the app has nothing to show at all | one panel came back empty while the rest of the screen is fine |
| Today | `App.tsx`'s four states | Monitoring's *no metrics endpoint*, Connect's equivalent |

Same anatomy in both: one sentence naming the situation, one sentence naming
the action, at most one primary action. **Left-aligned text** (centred
paragraphs are harder to read). No shrug emoji. Where a visual helps, render a
6-line mono skeleton of the table that will appear here, in `--text-absent`.

**`.teach` adds the three-item list, and that is the reusable part.** It answers
the three questions every in-panel empty state gets asked, in this order — *what
you would get* · *how to switch it on* · *what still works without it* — as an
em-dashed `ul` (a list of three sentences, not three options). The governing
line is Monitoring's: **"Kavka would rather show you nothing than draw a line it
made up."** An empty panel that only says "no data" makes the user suspect the
app; one that names the mechanism makes them fix the cluster.

> **It is shell furniture and it lives in the shell sheet.** It was written
> beside Monitoring, the screen that needed it first, in
> `styles/jackdaw-ops.css`; Connect adopted it within the same sweep. An idiom
> two screens share is not one screen's, and a shared idiom left in a
> screen-scoped file is how the third screen forks it instead of reusing it. Its
> `@media (max-width: 900px)` reflow and its `forced-colors` edge moved with it —
> `jackdaw-shell.css` loads last, so an override left behind would have silently
> lost to the base rule.

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

- **Cluster switcher, no connections** — *No connections saved yet.*
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
> `classifyError(raw, ctx?) → { title, detail, known, cause }` is a pure, total
> function — no React, no imports, no I/O — so the whole library can be checked
> by calling it with a captured broker string. Every renderer of an error uses
> it; the raw librdkafka text is **never** the banner title. `known: false`
> means we did not recognise the cause, and only then does the raw string
> become the title, with the full text still under `Show details`. Keep it
> pure: the moment it reaches for component state it stops being testable and
> becomes a component.
>
> **Two rows are not derivable from a string, so they take context.** "Active
> group" needs the group's state and member count; "Timeout, prod" needs to
> know whether the connection's environment is **protected** — not what it is
> called. Both arrive in the optional second argument —
> `{ groupState?, memberCount?, environmentProtected? }` — which the *caller*
> passes, which is what keeps the function pure while still letting it answer
> them. `environmentProtected` is a **boolean**, not a name: comparing against
> the literal `"prod"` stopped being answerable the moment environments became
> user-definable, and an org whose production environment is called `PRD` got
> the dev wording at 3am. The caller resolves the name through the registry;
> `errors.ts` does not, and must not, know a registry exists.
> Every field is optional and every branch degrades to the string-only answer
> without it. The context-only inference (a terse failure on a group we happen
> to know is live) is tested **last**, after every branch that names its own
> cause in the text.
>
> **`cause` is the row that matched**, and it is how a renderer decides what
> else to offer without re-sniffing the raw string with its own regex — the
> reset modal reveals its "send it anyway" checkbox only on `active-group`.
> Two copies of that rule in two files is how the banner and the checkbox come
> to disagree about the same error.

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
| Unknown profile | That connection isn't on this machine any more | It may have been deleted in another window. Pick another connection from the cluster switcher, or add it again. |
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

**protected** — adds type-to-confirm and restates the cluster

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
`gg`/`G` top/bottom · `⌘I` inspector · `Esc` cancel the running
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
   protected surfaces and all six light surfaces; **every env `--env-ink` on
   its `--env-tint` and every `--env-badge-ink` on its `--env-badge-fill`, all
   seven colours, at ≥4.5:1** (surface-independent, because both members of
   every pair are opaque — that is the reason they are opaque); and
   `--border-control`, `--danger-border`, **`--rule` for all seven colours**
   (**both** its normal and its `[data-alert="danger"]` value), `--env-wire`
   and every lag-bar fill — against `--track-empty` as well as the surfaces —
   at ≥3:1 across all twelve dark/protected surfaces plus the three protected
   switcher row surfaces. `slate`'s rule is the one **documented exemption**:
   1.14–1.38:1, decorative by design (§3.1).
   **Chip fills are excluded on purpose** — the chip contains the
   environment's name, so the fill is a container and not a signal; §3.1
   records their 1.93–3.11:1 range so nobody mistakes them for a gate.
   **No alpha may appear in any of these tokens**: an `rgba()` border has no
   ratio until you know what is behind it, so it cannot be asserted at all.
   **Both themes are in scope, not just dark.** Light ships (§10.2), so
   every assertion above runs twice: six dark surfaces, six dark protected
   surfaces, six light surfaces, six light protected surfaces. The binding
   figures are usually a selected row in dark and the rail in light.
   **Also asserted, new in Jackdaw:** all four accents' ink against every
   surface and their `--on-brass` against their own fill; `--perch-ink` and
   `--perch-title` on `--perch-bg`; and every semantic ink on `--perch-bg`,
   because the Perch is a surface that carries verdicts.
   `--perch-line` and `--line`/`--line-mid` are the **documented exemptions**
   alongside slate's rule: decorative, 1.4.11-exempt, and the Perch is
   identified by its ground, its bird and its kicker rather than by its border.
   **Current state: zero failures**, computed with the WCAG 2.1 relative
   luminance formula over the token literals in `styles.css`. Wire it into CI2. **`--row-h` assertion** — dev-mode check that the token equals the first row's
   measured `offsetHeight`; fail loudly. Wire it before the message browser
   lands.
3. **Platform screenshots** — macOS and Windows, 100% and 125% DPI. SF Pro at 600
   is noticeably lighter than Segoe UI Variable at 600, and `--t-micro` at 10.5px
   plus `--row-h: 30px` are the most fragile. Budget a `[data-os="win"]` override
   dropping `--w-semi` to 550 if Windows reads heavy. Verify the 2px wire at
   100/125/150/175% Windows scaling.
4. **Protected + danger together** — the single most likely thing to get wrong
   in implementation. Screenshot a protected cluster with an error banner up
   and confirm both signals still read. Do it once with a `red` environment
   (where the rule and the banner share a hue) and once with a `blue` one
   (where the damper is doing nothing anyone can see, and must still not have
   erased the rule).
5. **Forced-colors** — Windows high-contrast with the wire and its uppercased
   environment-name label present, **and one unselected row next to one
   selected row next to one protected row.** The bug this catches is not "the
   indicator is missing"; it is "every row has the indicator", which looks fine
   in a screenshot of a single row. Check the manager's swatch grid in the same
   pass: it is the one place in the app whose *subject* is colour, so its
   selected state is an outline and each swatch carries its colour's word.
6. **`index.html`** hard-codes the anti-flash background. It must stay in sync
   with `--bg-canvas`, or the app flashes the old theme on every cold start.
7. **WCAG 2.2 AA** — the full audit, finding by finding with SC references, is
   `docs/A11Y-AUDIT.md`. It is the procurement deliverable, so it is kept
   current rather than dated: **a PR that changes a control changes that file
   too.** Five things in it are cheap to break and expensive to notice, so
   check them by hand before merging anything that touches a widget:
   1. **A declared role is a promise about the keyboard.** `role="tablist"`
      means one tab stop plus ←/→/Home/End; `role="radiogroup"` means the same
      with selection following focus. Four widgets shipped the role without the
      model. If you add either, copy `ClusterView`'s strip — it is the
      reference implementation — and give every tab an `aria-controls` that
      actually resolves.
   2. **Nothing operable may be a `<div>` or a `<tr>`.** A row that responds to
      Enter has the role `row`, which says nothing about being operable
      (SC 4.1.2). The pattern is a real `<button>` in the affordance cell
      (`.row-affordance`), the row's `onClick` kept for the pointer, and
      `stopPropagation` on the button so one click is not two.
   3. **A message that appears without focus moving needs a live region.**
      Validation the *form* owns focuses its control and needs nothing;
      validation a *caller* owns (`SeekBar`) and anything raised on blur needs
      `role="alert"`.
   4. **Reduced motion is per animation, not one blanket.** The blanket at the
      top of the sheet is right for an animation whose resting state is the
      answer, and it **deletes** an indeterminate one. See §11.
   5. **`--hit-min` is 24px in BOTH dimensions.** `height: auto` on a button
      and a bare glyph in narrow padding are the two ways it gets missed.

   Add to the manual sweep in gate 3: 400% zoom at the window's 960px minimum,
   `prefers-reduced-motion: reduce` with a fetch in flight and a toast up, and
   one keyboard-only pass end to end with no pointer at all.

---

## 10. Themes and environments

### 10.1 The theme runtime

`<html>` carries five attributes. All five are **resolved values** — none of
them is ever the string `"system"`:

| Attribute | Values |
|---|---|
| `data-theme` | `dark` · `light` |
| `data-density` | `comfortable` · `compact` |
| `data-accent` | `brass` · `moss` · `sky` · `plum` |
| `data-fontsize` | `s` · `m` · `l` |
| `data-motion` | `system` · `reduce` |

> **"System" is a preference, not a value.** `theme: "system"` is resolved
> through `matchMedia("(prefers-color-scheme: dark)")` and **re-resolved
> whenever the OS flips**, live, while the app is open. A stylesheet that had to
> handle a third `data-theme` value would need a `:root:not([data-theme])`
> fallback on every token block, and the one that got missed would be a
> half-themed control. `data-motion` keeps its `system` value because it is
> the *absence* of an override rather than a third behaviour: the OS media
> query wins on its own regardless, so a user who told the OS they get motion
> sick never has to find a second switch.

**A sixth axis has no attribute: `perch`** (`full` · `line` · `hidden`, §5.12).
The five above are answered by the stylesheet alone, which is why they have to
be on `<html>` before the first byte of CSS arrives. Perch visibility is
answered in React, because it is not absolute: a hidden Perch still renders
while a screen is **loading** and still renders when a read **failed**, and no
`[data-perch="hidden"] .perch { display: none }` can tell which of those it is
looking at. It is stored with the other five — one record, one key, one
Settings screen — and read through `useAppearance()`.

**Persistence:** one JSON record under `localStorage["kavka.appearance"]`,
through the safe helpers in `src/storage.ts`. Storage can throw — disabled,
full — and an appearance preference is never worth an exception in a render
path. `readAppearance()` degrades **field by field**, so one bad value cannot
cost the user the other five. A record written before an axis existed is
missing that field, which reads as its default — the behaviour those users
already have.

**The store** (`src/appearance.ts`) is a module-level value plus a listener set
with a `useSyncExternalStore` snapshot — the same shape `i18n/index.ts` uses,
for the same reason: appearance is one global fact and every subscriber must see
the same one.

#### Pre-paint

`index.html` carries an **inline script** that reads the same key, validates
against the same allow-lists, resolves `system` the same way, stamps the same
five attributes, and sets an inline `background` on `<html>` before the
stylesheet exists.

> **It is a deliberate duplicate.** A module import cannot run before first
> paint. Four things are shared and the two files must be changed together: the
> storage key, the five attribute names, the defaults, and the two
> `--bg-canvas` literals. Nothing else in either file needs to agree — and
> `perch` in particular is **not** in the inline script's `DEFAULTS`, because
> it stamps nothing and a sixth field there would be a duplicate that does
> nothing.

The stylesheet then takes `html { background: var(--bg-canvas) !important }`, so
there is one authority for the colour the moment there can be one.

`<meta name="color-scheme">` tracks the **resolved** theme, set by both the
inline script and `applyAppearance`. Form controls, scrollbars and the webview's
own chrome do not read our custom properties; that tag is the only thing that
makes them follow the app instead of the OS.

### 10.2 Both themes ship

Ledger specced light and did not expose it. Jackdaw ships it.

Every colour token is declared in both blocks (§3), every ratio in §3 is
measured in both, and **the nine production guardrail layers survive both**
(§6): the environment chip, the protected substrate, the top wire, the typed
confirmations and the danger banners all have measured light values. Identity is
never carried by colour alone in either theme, so a screenshot of light
production is as unmistakable as a screenshot of dark production.

### 10.3 Settings

`SettingsView.tsx`, a rail item in the **Application** group, reachable **with
no cluster connected**. That is the reason it is a view and not a dialog: the two
preferences people want on first launch are the theme and the font size, and on
first launch there is nothing to connect to. It is also the first branch in
`App.tsx`'s view selection, so it works even in the state where reading the
connection file failed.

**There is no Save button.** Every control applies on change and persists on
change — a preference you have to commit is a preference you cannot preview,
and appearance is the one category where previewing *is* the decision.

Sections: **Appearance** (theme, accent, density, text size, motion),
**Language** (the same picker the About dialog has, reading the same store —
two copies of one control, never two settings), and **About**, which *links* to
the About dialog.

> **Diagnostics and MCP did not move.** They live in the About dialog, every
> existing link and screenshot points there, and Settings gains a door rather
> than a landlord.

Every control carries its word: the segmented controls are `role="radiogroup"`
with text in each arm, and each accent swatch carries the accent's **name** in
its accessible name and its `title` — a colour picker whose options are only
colours is unusable to the people most likely to open it.

### 10.4 Environments

The environment reaches CSS through **two** attributes, not one — see §3.1 for
why. They are set on the app root (`apps/desktop/src/App.tsx`), on the
connection `<form>` while editing, and on the copy wizard's and offset-migrate
modal's bodies (where they carry the *destination's* environment, not the
workspace's). So **picking a protected environment in the picker swaps the
form's substrate live**.

```jsx
<div className="app" {...envAttrs(envDef)}>              {/* whole app  */}
<form className="editor" {...envAttrs(formEnv)}>          {/* live preview */}
<span className="env-chip" {...envAttrs(def)}>{def.name}</span>
```

`envAttrs` (`apps/desktop/src/environments.ts`) emits
`data-env-color="<token>"` always and `data-env-protected="true"` only when the
flag is set — **never `"false"`** — so the CSS keys on the attribute's presence
and a DOM screenshot says what it means.

> **Those attributes sit on `.app`, not on `<html>`.** The theme sits on
> `<html>`. So every light-theme environment override is a **descendant**
> selector — `[data-theme="light"] [data-env-color="red"]` — and not a compound
> one. Writing `[data-theme="light"][data-env-color="red"]` matches nothing, and
> it is the single easiest mistake to make in this file.

The registry itself is a module-level store with a `useSyncExternalStore`
snapshot, the same shape `i18n/index.ts` uses and for the same reason: about
twenty components ask *is this connection's environment protected?*, most of
them deep views that receive nothing but a `ConnectionProfile`, and there is one
registry per machine rather than one per subtree. **It starts full, not empty** —
the initial snapshot is the three defaults, so the first paint has correct
## 11. Deviations from the brief, and why

These are deliberate. Do not "fix" them without reading the reason.

### From the Jackdaw mockup

- **There is a status bar; the mockup has none.** It carries live operational
  state a static drawing never had to honour — progressive search progress
  (Phase 2's "search never silently truncates" gate depends on it), tail rate,
  connection latency, and §6 layer 3's always-visible bootstrap address,
  because most production accidents are right-action-wrong-cluster. It spans
  the whole window and carries **no version**: a build number is not
  operational state, and it lives in the brand lockup (§5.1).
- **The cluster-scoped rail group keeps its concept names.** The mockup titled
  that group with the live cluster — a `state-dot` plus `local · DEV` — and
  called it the biggest wayfinding change from the ten-tab strip. Kavka has ten
  screens where the mockup had four, so the four concept words (Cluster ·
  Observe · Safety · Integrations) are doing work the mockup's single group
  never had to do: they are what teaches that ACLs live under Safety. The live
  identity is not lost — it is in the cluster card directly above the groups,
  with the address as well, which is more than the mockup's label said.
- **Messages is a drill-down, not a top-level rail item.** The mockup reaches a
  message browser directly; Kavka reaches it through Topics → a topic → its
  messages, because a message browser is always *of* a topic and Kavka has a
  Topics list the mockup never drew. The cost is that the rail says *Topics*
  while you are reading messages, and the fix is `.stage-head`'s breadcrumb
  (§5.13) rather than a flat item that would need a topic already chosen —
  the component exists now, and the three full-height panes are the ones that
  still have to adopt it.
- **The rail carries ten screens, not four.** The mockup drew Cluster home,
  Messages, Monitoring and Alerts, and left ACLs, Connect, Masking and Streams
  with no home at all. Reproducing that would have deleted four working
  surfaces from the product in the name of fidelity to a drawing that only had
  to look right. The four groups in §5.1 cover everything that was reachable
  before. `TABS` is derived from `RAIL` by flattening, so a screen with no group
  is a screen with no app.
- **There is no `clay` accent.** The mockup offered five; Jackdaw ships four.
  Clay is close enough to `--danger` that a user could pick an accent the eye
  reads as the warning colour, and a decorative hue and a guardrail hue in the
  same family is the one composition §6 forbids.
- **The Perch's Hide button is back, and there is a three-state visibility
  preference beside it — BY OWNER RULING, overturning this document's own
  earlier no-dismiss doctrine.** The app shipped without a dismiss, on the
  grounds that a verdict the user can switch off is a verdict the app stops
  being accountable for. That is true of a *verdict* and not of a *note*, and
  this component is both — so §5.12 splits it. The mockup's HIDE control returns
  as the per-screen pill, and **Settings → Appearance** carries the durable
  preference `appearance.perch`: **Full** (default) · **One line** · **Hidden**.
  The guardrail that makes the ruling safe is stated once and enforced in the
  component rather than at each call site: **a screen that is still reading, or
  that failed to read, forces the whole note back in every mode, including
  Hidden.** Errors surface whatever the preference says. The mockup's restore
  button comes with it.
- **`data-motion` is `system | reduce`, not `full | reduced`.** The app-level
  preference is an *override*, and the OS media query wins on its own whatever
  it says — so the third state the mockup implied ("motion on, ignore the OS")
  is one this app deliberately cannot express.
- **A compact density ships.** The mockup's comfortable rows cost a reader of
  10,000-row tables real screen, which was the one fair criticism of it. The
  answer is a preference (§3), not a compromise on the default — and compact is
  *exactly* the 30px row Ledger shipped, so nobody loses what they had.
- **The mockup's `--f15` body type and `--row-h: 46px` became 15px and 44px.**
  15px is kept verbatim because it is the direction's central legibility bet.
  44px is a round number in the 4px space ladder; 46 was not.
- **The mockup's spacing ladder is deleted, not adopted — BY OWNER RULING on
  the audit's one open maintenance item.** `--s1`…`--s9` was declared and used
  zero times while `--s-1`…`--s-12` carried all 504 spacing declarations. Two
  live names for the same nine values is how the *next* component drifts, so
  the unused set is gone and §3 records the exact mapping for anyone reading a
  Jackdaw measurement. The type-token pair (`--t-*` over `--fN`) is **not** the
  same case and stays: it is an alias layer with one source of truth, which is
  §3's documented pattern rather than a second ladder.
- **Every table's foot is translated; the table above it is not.** This is the
  one seam that runs *through* a component rather than around it, and it is the
  price of the panel-foot idiom (§5.15) landing before the deep views are
  extracted. The rule that decides which side of the line a new caveat falls on
  is stated once, in `docs/I18N.md` §1: a caveat is translated when it stands
  alone as its own block, and stays inline English when it is one clause inside
  a line that is already English end to end. **Do not add a translated string
  inside a table without reading that rule** — an undocumented exception here is
  exactly the drift the fidelity audit was called in to find.

### Carried over from Ledger


- **The global `:focus-visible` rule does not set `border-radius`.** As specced
  it would round table rows and panels the moment they take focus. Controls carry
  their own radius, so the ring already follows it; radius-less elements get a
  correct square ring.
- **`--env-label` is a React prop (`data-env-label`), not a CSS custom
  property.** Only the forced-colors block ever needed the string, and
  `attr()` on a data attribute is better supported than `content: var()`.
- **Env tokens are applied via attribute selectors, not `:root[…]`.**
  Identical specificity, but they also work on the `.app` wrapper, on a nested
  `<form>` and on an individual chip, which is what makes the live substrate
  preview — and a switcher of mixed environments under one app-level colour —
  possible without lifting state.

- **`data-env` became `data-env-color` + `data-env-protected`, and that split
  is the whole feature.** Environments used to be the closed triple
  `dev | staging | prod`, in the Rust enum, in the TypeScript union, in the
  chip classes and in `[data-env="prod"]`. Enterprises run four or five, and
  every one of those places was a name check standing in for a policy. The
  migration, in full:
  - **`ConnectionProfile.environment` is a `string`.** Wire-compatible: the old
    enum serialized to exactly those lowercase strings, so every
    `profiles.json` on disk parses unchanged and no profile needed rewriting.
  - **Colour is identity, `protected` is the guardrail** (§3.1). Everything
    formerly keyed on `environment === "prod"` — substrate, damper,
    typed confirms, window title, forced-colors wire, `--yes-prod`,
    `KAVKA_MCP_ALLOW_PROD`, `errors.ts`'s context — re-keys onto the flag.
    `errors.ts`'s `ctx.environment?: string` became
    `ctx.environmentProtected?: boolean` for the same reason: an org whose
    production environment is called `PRD` was getting the dev wording.
  - **The CLI flag and the MCP variable keep their names.** They are in shell
    history, scripts and MCP client configs; renaming them to improve a
    sentence breaks somebody's cron job. Their *docs* now say "required when
    the connection's environment is marked protected", which is what they
    check.
  - **`.env-dev` / `.env-staging` / `.env-prod` are gone**, replaced by the
    chip's own `data-env-color` / `data-env-protected`. `.profile-row-prod`
    became `.profile-row-protected`.
  - **The one visible regression, accepted deliberately: `dev` looks
    different.** It shipped as `--accent` on `--accent-tint` with a neutral
    `#242B33` rule; it is now `green` with a green rule. Two reasons. The chip
    was spending `--accent` — *teal means live, only live* (§2) — on an
    environment tag, and `cyan` now exists for anyone who wants a teal-ish
    environment. And "no coloured rule" had to become a *different* statement:
    it now means `slate`, the token an **unknown** environment resolves to,
    which is a real state that needs its own appearance. `staging` and `prod`
    are pixel-identical to what they were.
  - **A profile can name an environment the registry does not hold** — after a
    delete in another window, or an import from a colleague. It renders slate
    and unprotected with a hint, never an error, and the connection form gives
    it a picker segment of its own so the current value still reads as
    selected.
- **`.data-table` uses `border-collapse: separate`.** With `collapse`, WebKit
  drops the border on a `position: sticky` header.

- **The forced-colors block contains no `*` selector.** See §10 — a blanket
  `border-color` there erases exactly the indicators it looks like it is
  helping. Indicators are restated positively, one system colour per meaning.

- **The protected danger-damper reduces chroma, not lightness — and there is
  exactly one of it, for all seven colours.** See §5.8. There is no darker
  coral available: the undampened red rule is already at 3.04:1 on its worst
  surface.

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

- **The row affordance is a `<button>`, not a `<span>`, and the row is not
  focusable.** §5.2 says "on hover the last column reveals a ghost
  `View messages →`. The whole row is also clickable." That shipped as a
  focusable `<tr>` with an Enter/Space handler and a decorative span — and a
  `<tr>` has the implicit role `row`, which never tells assistive technology
  the element is operable (SC 4.1.2, and `role="button"` is not available to a
  row). So the affordance carries the role and a name that says which row it
  opens, the row keeps its `onClick` for the pointer, and the row's `tabIndex`
  is gone: **still exactly one tab stop per row**, on an element that can
  describe itself. Two consequences worth knowing before touching it:
  `.row-affordance` is `opacity: 0` rather than `visibility: hidden`, because a
  hidden element cannot be focused, and it repaints at `:focus-visible` so it
  is never an invisible tab stop; and it calls `stopPropagation`, because the
  row's handler would otherwise run twice per click — harmless on five of the
  six tables, and on Share groups a **toggle**, where twice is the same as
  never.

- **A tab strip and a radio group are one tab stop, not N.** `role="tablist"`
  and `role="radiogroup"` are promises about the keyboard, and four widgets
  shipped the role without the model. Every strip in the app now carries a
  roving `tabIndex` plus ←/→/Home/End with selection following focus, and every
  tab's `aria-controls` names a panel that is actually mounted. `ClusterView`'s
  is the one to copy.

- **`Overlay`'s focusable selector excludes `[tabindex="-1"]` on every branch,
  and includes `details > summary`.** `button:not([disabled])` matches
  `<button tabindex="-1">`, which the roving strips above would have fed
  straight into the focus trap's first/last calculation; and a `<summary>` —
  which every in-dialog error banner ends in — is Tab-focusable while matching
  none of the original selectors, so `last` was sometimes not the last thing
  Tab reaches and the trap leaked.

- **Reduced motion is a per-animation pass, not only the blanket rule.** The
  block at the top of the sheet (`animation-duration: 1ms`,
  `animation-iteration-count: 1`) is right for an animation whose RESTING STATE
  is what the user should see — an arrived toast, a drawn chart. It is exactly
  wrong for an indeterminate one, whose resting state is the end of a loop that
  was supposed to repeat: `.table-loading`'s bar landed at `translateX(430%)`,
  i.e. off the end of its own box, so §5.2's "never a spinner over data"
  indicator **disappeared** for the users who asked for less motion; and
  `.toast-progress` drained to zero in 1ms and then contradicted its own 5s
  dismiss timer for five seconds. Both are now `animation: none` with a static
  appearance, in a block at the END of `styles.css` — the rules it overrides are
  declared hundreds of lines below the top block, and at equal specificity the
  later declaration wins.

- **18px checkboxes pass SC 2.5.8 on the SPACING exception, not the size
  test.** §5.3 reads as though 18px cleared the criterion. It does not — the
  floor is 24px — and it passes because a 24px-diameter circle centred on each
  box does not intersect another target's: stacked check-fields sit ≥30px
  apart, being the 18px box plus at least `--s-5` of fieldset gap. Measured,
  not assumed. **The thing that would break this is tightening a fieldset gap,
  not the checkbox.** Full working in `docs/A11Y-AUDIT.md` (A11Y-27).

- **The glossary popover takes pointer events while it is open.** §7 specced it
  `pointer-events: none`, which made it unreachable by the pointer — and SC
  1.4.13's *hoverable* clause requires the pointer to be able to rest on it. It
  sits 6px from its term, so moving towards it fired `mouseleave` and closed
  the thing being moved towards, which is the manoeuvre a magnifier user makes
  constantly. `.term-pop-open` now takes `pointer-events: auto` (only while
  open, so a hidden 260px panel can never swallow a click on the row beneath
  it) and `Glossary.tsx` holds a 120ms grace timer to carry the pointer across
  the gap — `mouseleave` on the term fires *before* `mouseenter` on the
  popover, so the close has to be cancellable rather than immediate.

- **The inspector dock has a `max-width: 60%`, which §5.10 already specified.**
  It shipped as a flat 480px with `flex-shrink: 0`, so at the window's own
  960px minimum the message table was left about 200px and the payload column —
  the loudest pixels on screen, by Law 1 — vanished before any chrome did.

- **The tab strip scrolls horizontally.** Ten cluster tabs in a non-wrapping
  flex row inside `.workspace { overflow: hidden }` were clipped out of
  existence past about 900px of workspace — and not merely hidden: a roving
  `tabIndex` cannot focus what a clip has removed, so the last four views were
  unreachable by keyboard too (SC 1.4.10).

- **Light-theme protected switcher rows fail AA and ship anyway — because the light
  theme itself doesn't ship.** On `--bg-row-prod-selected` `#F2D8D4` and
  `--bg-row-prod-hover` `#F7E2DF` (light block), `--text-tertiary`,
  `--text-placeholder`, `--text-absent` and `--border-control` all measure
  below their floors. The light block is a token-swap spec, not a shipped
  surface; re-derive these four values (or lighten the two row backgrounds)
  as the first task of the light-theme phase, and run the §9 gate 1 sweep
  over the light surface set before flipping it on.

- **The §7 error table's two context rows: half resolved.** "Timeout, prod"
  and "Active group" are not derivable from a raw broker string alone, and
  both now take the promised second argument —
  `classifyError(raw, { groupState?, memberCount?, environmentProtected? })`,
  caller context, never component state, so the function stays pure.
  **The resolved half is "Active group":** it has a caller. The reset modal is
  the only place that error can arise and it holds the group's state and
  member count, so it passes them and reads `cause === "active-group"` to
  decide whether to offer `force`. Its local `looksLikeActiveGroup` copy of
  the rule is gone.
  **The unresolved half is "Timeout, prod":** the branch exists and switches
  on `environmentProtected`, and the four call sites that *do* hold a profile —
  the produce panel, the reset modal, the copy wizard and the offset-migrate
  modal — now resolve it through the registry and pass the boolean. But
  **`ErrorBanner` still takes a raw string and nothing else**, and the profile
  is not in scope at most of *its* call sites, so a timeout surfaced through
  the banner still reads with the unprotected wording. Thread the profile
  through the banner in the phase that gives the banner an owner; do not do it
  by reaching into the environment store from `errors.ts` — that module's
  purity is the reason its whole table is testable without a broker.

- **Light-theme protected switcher rows: the two `--bg-row-prod-*` token names
  kept their `prod` spelling.** They are now applied by
  `.profile-row-protected`, so the names lie slightly. Renaming them touches
  four rules, two of them in the forced-colors block, for zero behaviour — and
  the audit trail in this file and in `docs/A11Y-AUDIT.md` refers to them by
  name. Rename them in the light-theme phase, together with the four values
  above that already fail there.

- **The payload inspector ships four tabs, not §5.10's five.** `Value · Key ·
  Headers · Raw` — **Hex is staged for Phase 2**, and is deliberately absent
  rather than present-and-disabled. The IPC contract hands the UI a decoded
  `text` and never the bytes, so a Hex tab today would render a hex dump of a
  lossy UTF-8 decode rather than of the record — worse than not offering it,
  because it would look authoritative. It lands when `DecodedPayload` carries
  the raw bytes (or a bounded prefix of them), which is the same change
  Phase 2's search needs to match on bytes. A disabled tab is not an
  acceptable placeholder here: §5.5's "every disabled control says why" would
  need a sentence explaining a data-model gap to a user who cannot act on it.
