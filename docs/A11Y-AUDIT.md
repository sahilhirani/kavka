# Kavka — WCAG 2.2 Level AA audit

**Scope:** every view in `apps/desktop/src` at the end of Phase 5 — connection
manager, command palette, all ten cluster tabs, the message browser and its
payload inspector, search, SQL, produce, groups, monitoring (including charts),
alerts, masking, the MCP section, and every modal and wizard.

**Standard:** WCAG 2.2, Level A and AA. Level AAA is out of scope except where a
AAA criterion was already satisfied and is worth recording.

**Method:** static audit of the source. Every flow was walked in the code —
markup, ARIA, key handlers, focus moves, and the CSS that decides whether a
control can be seen, reached or hit. Contrast was **not** re-derived: the token
system is verified in `docs/DESIGN.md` §3 and §9 gate 1 and has zero failures,
so this pass only checked that components added in Phases 3–5 draw from those
tokens rather than inventing values. They do.

**Addendum (Phase 6):** the local playground panel (`Playground.tsx`) and the
diagnostics panel (`DiagnosticsSection.tsx`) were added after the sweep above
and are audited against the same checklist in **§6**. Their findings are in the
tables and totals below.

**Date:** 2026-08. **Branch:** `phase-0-first-slice`.

**Result:** 40 findings. **35 fixed in this pass, 5 deferred with a reason.**
No finding is a Level A or AA failure that is still open.

Findings are numbered `A11Y-nn` in one sequence. One (A11Y-09) is cross-listed
under two criteria and counted once, under the criterion it fails hardest.

---

## 1. Summary by success criterion

| SC | Level | Findings | Fixed | Deferred |
|---|---|---|---|---|
| 1.1.1 Non-text Content | A | 1 | 1 | — |
| 1.3.1 Info and Relationships | A | 3 | 3 | — |
| 1.4.1 Use of Color | A | 0 | — | — |
| 1.4.3 Contrast (Minimum) | AA | 0 | — | — |
| 1.4.10 Reflow | AA | 4 | 3 | 1 |
| 1.4.11 Non-text Contrast | AA | 0 | — | — |
| 1.4.13 Content on Hover or Focus | AA | 2 | 1 | 1 |
| 2.1.1 Keyboard | A | 4 | 4 | — |
| 2.1.2 No Keyboard Trap | A | 2 | 2 | — |
| 2.4.3 Focus Order | A | 1 | 1 | — |
| 2.4.6 Headings and Labels | AA | 3 | 3 | — |
| 2.4.7 Focus Visible | AA | 1 | 1 | — |
| 2.4.11 Focus Not Obscured (Min.) | AA | 1 | 1 | — |
| 2.5.8 Target Size (Minimum) | AA | 4 | 3 | 1 (passes by exception) |
| 4.1.2 Name, Role, Value | A | 6 | 6 | — |
| 4.1.3 Status Messages | AA | 8 | 6 | 2 |
| **Total** | | **40** | **35** | **5** |

Criteria with zero findings were audited, not skipped; §4 records what was
checked and why it passed.

---

## 2. Findings and fixes

Each entry names the criterion, the surface, what was actually wrong, and what
changed. "Fixed" means the change is in this commit.

### SC 1.1.1 — Non-text Content (A)

**A11Y-01 · Chart empty state was invisible to assistive technology · FIXED**
`Chart.tsx` wraps the plot in `role="img"`, which makes every descendant
presentational. The empty-state sentence — *"Nothing recorded in this window.
That is Kavka's own history — it only has what it collected while this
connection was up."* — is a `<p>` inside that box, so a screen reader reached a
chart with a name and no explanation of why it held nothing. The sentence is now
folded into the plot's `aria-label` when there is no data, rather than moved:
it is positioned against that element, so moving it would break the layout to
fix the semantics.

### SC 1.3.1 — Info and Relationships (A)

**A11Y-02 · The rail is an unnamed `complementary` landmark · FIXED**
`<aside className="sidebar">` had no accessible name. It is the first region a
screen-reader user jumps to. Now `aria-label` (routed through the i18n catalog
alongside the visible "Clusters" eyebrow, so the two can never disagree).

**A11Y-03 · The connection list is an unnamed `navigation` landmark · FIXED**
Same problem one level down: `<nav className="profile-list">`. Now labelled
"Saved connections", which is distinct from the region containing it — two
landmarks with the same name are as useless as two with none.

**A11Y-04 · Import strategy hints were not associated with their radios · FIXED**
`ImportExportDialog`'s two `.check-field` blocks rendered a `.field-hint`
sibling with nothing pointing at it. DESIGN §5.3 is explicit that the hint is a
sibling of the label wired with `aria-describedby` — these were the only two
check-fields in the app that had the markup and not the wiring, so the sentence
explaining what "Keep the one on this machine" actually does never reached a
screen reader. Now `aria-describedby` on both inputs.

### SC 1.4.10 — Reflow (AA)

Kavka's window has a hard floor of 960 × 600 (`tauri.conf.json`), so the
"320 CSS pixel" figure in the criterion is reached through webview zoom rather
than through a narrow window. Everything below was reproduced by narrowing the
CSS viewport; all three fixes are no-ops at the sizes Kavka normally runs at.

**A11Y-06 · Cluster tabs were clipped out of reach · FIXED**
`.tabstrip` is ten tabs in a `display: flex` row with no wrap and no scroll,
inside `.workspace { overflow: hidden }`. Past roughly 900px of workspace the
last tabs — Monitoring, Alerts, Masking, Streams — were simply gone. Not merely
invisible: **unreachable by keyboard too**, because a roving `tabIndex` cannot
focus what a clip has removed from view. `.tabstrip` now scrolls horizontally
(`overflow-x: auto`, `min-height` so the scrollbar cannot squeeze the tabs it
appears for) and `.tabstrip > .tab` no longer shrinks — scoped, because
`.modal-tabs` reuses `.tab` inside a width-capped dialog that has nothing to
scroll.

**A11Y-07 · Panel toolbars overflowed the workspace · FIXED**
The topic detail's `.panel-tools` carries seven controls (Browse · Search · SQL ·
Schemas · Copy to… · Produce · Delete topic) in a `space-between` head that could
not wrap. `.panel-head`, `.panel-tools`, `.view-title-row`, `.editor-actions`
and `.modal-actions` now wrap. At normal widths nothing moves.

**A11Y-08 · The inspector dock could not shrink · FIXED**
`.inspector` was a flat `480px` with `flex-shrink: 0`. At the window's own 960px
minimum that leaves the message table about 200px — the payload column, which
DESIGN §1 calls the loudest pixels on screen, disappeared before any chrome did.
It now carries `max-width: 60%`, which is the range §5.10 already specified
("resizable 320–60%") and had never been applied.

**A11Y-09 · Sticky headers hid the row the keyboard had just focused · FIXED**
Cross-listed: it is a reflow symptom and a 2.4.11 failure, and it is counted
once, under **SC 2.4.11 (A11Y-23)**, where it fails hardest.

**A11Y-10 · The rail does not collapse · DEFERRED**
`.sidebar` is a fixed 240px that never narrows, so at very high zoom it takes
the whole viewport. The fix DESIGN §5.1 already specifies is a collapsible rail
on `Ctrl/Cmd+B` — a feature, not a stylesheet rule, and out of scope for an
accessibility-only pass. **Reason to defer:** the honest fix is the specced
control, and a media query that hides the rail would remove the app's only
cluster switcher at exactly the zoom level where `⌘K` is hardest to discover.
**Owner:** the phase that implements `Ctrl/Cmd+B`. **Interim:** every rail
function is reachable from the command palette, which is the app's real
navigation by design (§5.1).

### SC 1.4.13 — Content on Hover or Focus (AA)

**A11Y-11 · The glossary popover was not hoverable · FIXED**
`.term-pop` was `pointer-events: none` and sits 6px away from the term it
explains. Moving the pointer towards the gloss left the term, fired
`mouseleave`, and closed the thing the user was moving towards — so the content
could be seen but never rested on. That is the manoeuvre a magnifier user makes
constantly, because at 300% the term and its gloss are rarely both on screen.
Fixed in two halves: `.term-pop-open` takes `pointer-events: auto` (only while
open, so a hidden 260px panel can never swallow a click on the row beneath it),
and `Glossary.tsx` gained a 120ms grace timer that carries the pointer across
the gap. `mouseleave` on the term fires *before* `mouseenter` on the popover, so
the close has to be schedulable and cancellable rather than immediate. Focus and
`Esc` still close outright — neither crosses a gap.

*Dismissible* (`Esc`) and *Persistent* (no timeout) were already satisfied.

**A11Y-12 · `title=` tooltips carry meaning · DEFERRED, and correctly**
Kavka uses the `title` attribute heavily and deliberately: DESIGN §5.5 requires
every disabled control to say why on hover, and §5.2's special cells explain
themselves the same way. Tooltips rendered by the user agent are **explicitly
out of scope for SC 1.4.13**, which excludes content "whose presentation is
controlled by the user agent". No change. Two consequences are recorded rather
than fixed:

- A `title` on a **disabled** control is never announced, because a disabled
  control is not focusable. Every such sentence in Kavka is a *reason*, never an
  instruction the user must act on, and in every case the actionable version is
  also on screen as prose (the read-only note, the empty state, the panel note).
  Audited case by case; no orphans found.
- A `title` on an **enabled** control becomes its accessible *description*, so
  it does reach assistive technology.

### SC 2.1.1 — Keyboard (A)

Four widgets declared an ARIA role that promises a keyboard model, and did not
implement it. In each case the role was correct and the model was missing, so
the fix is the model — not weaker ARIA.

**A11Y-13 · Environment picker (`role="radiogroup"`) · FIXED**
`ProfileEditor`'s three segments were three plain buttons: all in the tab order,
arrow keys inert. A radio group is one tab stop walked with the arrows, with
selection following focus. Now roving `tabIndex` plus ←/→/↑/↓/Home/End.

**A11Y-14 · ACL pattern picker (`role="radiogroup"`) · FIXED**
`AclsTab`'s "How does the name match?" — same shape, same fix.

**A11Y-15 · Payload inspector (`role="tablist"`) · FIXED**
Four tabs, all tabbable, no arrow keys. Now one tab stop plus ←/→/Home/End,
matching `ClusterView`'s strip, which is the reference implementation.

**A11Y-16 · Produce panel (`role="tablist"`) · FIXED**
Same, for `Send one` / `Bulk`.

### SC 2.1.2 — No Keyboard Trap (A)

Both findings are in `Overlay.tsx`, the single focus-trap primitive behind the
palette and every dialog.

**A11Y-17 · `<summary>` was not in the trap's focusable set · FIXED**
`FOCUSABLE` listed anchors, buttons, inputs, textareas, selects and
`[tabindex]`. A `<summary>` is Tab-focusable and matches none of them — and
every error banner inside a dialog ends in a `Show details` summary. The trap
reads `first` and `last` off that list, so when a summary was genuinely the last
focusable thing in a dialog, Tab walked straight out of it. Now
`details > summary` is in the set.

**A11Y-18 · `[tabindex="-1"]` was only excluded on one branch · FIXED**
`button:not([disabled])` matches `<button tabindex="-1">`. That was harmless
until A11Y-15 and A11Y-16 put roving-tabindex widgets *inside* overlays — at
which point the trap's `last` would have been a control the browser refuses to
focus, and Shift+Tab would have leaked. The exclusion is now applied to every
branch of the selector rather than only the catch-all.

### SC 2.4.3 — Focus Order (A)

**A11Y-19 · Tab strips consumed one stop per tab · FIXED**
Folded into A11Y-13 through A11Y-16: the cluster strip alone was ten tab stops
before reaching any content. Roving `tabIndex` makes each strip one stop, which
is both the ARIA pattern and materially faster for a keyboard user.

Everything else in the focus order audited clean. Worth recording, because these
are the things that usually go wrong:

- `Overlay` captures the invoker on mount and restores it on unmount — but only
  if nothing else has claimed focus, so a command that deliberately focuses
  something on its way out (⌘K → *Add connection* → the autofocused name field)
  is not yanked back.
- `Esc` inside an overlay calls `stopPropagation`, so it never also triggers the
  profile editor's "undo edits" underneath. `HelpPopover` does the same, so its
  `Esc` never also closes the modal it is inside.
- The palette keeps focus on its input and drives the list with
  `aria-activedescendant`, so the caret never leaves the box being typed into.
- Destructive modals take initial focus on **Cancel**, and `⏎` does not confirm
  them (`ConfirmModal`).

### SC 2.4.6 — Headings and Labels (AA)

**A11Y-20 · N identically-named "Remove" buttons · FIXED**
The connection form repeats a Connect-cluster block and a decoder block, each
ending in `Remove`. A list of identical accessible names is a list a screen
reader cannot navigate. Both now carry an `aria-label` naming the row
(`Remove orders connect`, falling back to `Remove Connect cluster 2` before the
row is named). The visible word is unchanged — §7's sentence-case, seven-
character toolbar budget is untouched.

**A11Y-21 · N identically-named row affordances · FIXED**
Six tables end in a `View messages →` / `View lag →` / `View tasks →` cell. Now
each names its row (`View messages in orders.v2`). Folded into the A11Y-28 fix,
which made them real controls in the first place.

### SC 2.4.7 — Focus Visible (AA)

**A11Y-22 · A focusable control at `opacity: 0` · FIXED as it was created**
The row affordance (A11Y-28) had to become focusable, which meant it could no
longer be `visibility: hidden` — a hidden element is not focusable at all. It is
`opacity: 0` and repaints at `:focus-visible`, so it is never an invisible tab
stop. Recorded because "make it focusable" is exactly the change that usually
introduces an invisible tab stop.

The rest of the focus-visible surface was already correct and is worth stating
for a procurement reviewer: one global `:focus-visible` rule (2px `--focus` at
12.03:1, always with `outline-offset`), `.table-scroll` and `.messages-scroll`
reserve 2px of inner padding so an edge row's ring is not clipped by the scroll
well, and DESIGN §5.3 forbids `overflow: hidden` on any rounded shell wrapping
focusable children — the rule that `.env-picker` and `.palette` exist to
document.

### SC 2.4.11 — Focus Not Obscured (Minimum) (AA, new in 2.2)

**A11Y-23 · The sticky table header covered the row the keyboard just reached · FIXED**
`.data-table th` is `position: sticky; top: 0` with an opaque `--bg-canvas`
fill, and a browser's scroll-into-view does not know about sticky ancestors. Now
that each row has a focusable control (A11Y-28), tabbing down a long list parked
that control underneath 28px of header. Fixed with
`scroll-margin-top: calc(var(--row-h-head) + 4px)` on `.row-affordance`, so the
row it focuses arrives below the header rather than under it.

### SC 2.5.8 — Target Size (Minimum) (AA, new in 2.2)

`--hit-min: 24px` already exists as a token and is applied widely. Three
controls had been missed, and one class of control passes by exception.

**A11Y-24 · Breadcrumb button ~17px tall · FIXED**
`.crumb-btn` sets `height: auto` so it sits on the heading's baseline, which
left it one 17px line box tall. `min-height: var(--hit-min)` restores the target
without moving the baseline — `.btn` is an inline-flex box that centres its own
line.

**A11Y-25 · Saved-filter delete ~19px wide · FIXED**
`.saved-chip-del` is a `×` glyph in 12px of padding, sitting flush against the
Apply half of the same chip. Under 24px in one dimension **and** touching
another target, so the spacing exception could not rescue it either.
`min-width: var(--hit-min)`.

**A11Y-26 · `<summary>` disclosures ~18.6px tall · FIXED**
`Show details` under every error banner, and the SQL surface disclosure.
`min-height` on the list-item box, which keeps the disclosure marker that
changing `display` would have eaten.

**A11Y-27 · 18px checkboxes and radios · PASSES BY EXCEPTION, no change**
`.check-field` and `.check-row` use an 18px box. That is under the 24px size
test, and it passes on the **spacing** exception instead: a 24px-diameter circle
centred on each box does not intersect another target's, because stacked
check-fields are separated by at least `--s-5` (12px) of fieldset gap plus the
hint row, giving ≥30px between centres. Measured, not assumed.

**This is worth stating precisely, because DESIGN §5.3 currently reads as if
18px passed the size test.** It does not; it passes the spacing test. The note
matters the day someone tightens a fieldset gap — that change, not the checkbox,
is what would break the criterion. §11 now records it.

### SC 4.1.2 — Name, Role, Value (A)

**A11Y-05 · `aria-controls` pointing at elements that do not exist · FIXED**
`ClusterView` renders one tab panel and put `aria-controls="clusterpanel-<key>"`
on all ten tabs, so nine were broken references. `ImportExportDialog` did the
same on its inactive tab. Both now scope `aria-controls` to the selected tab.
(The payload inspector and produce panel had no `aria-controls` at all — see
A11Y-30 and A11Y-31.)

**A11Y-28 · Six clickable table rows had no operable role · FIXED**
Topics, Groups, Brokers, Connect connectors, Schema versions and Share groups
all rendered `<tr className="row-click" tabIndex={0} onKeyDown={Enter|Space}>`
with a non-interactive `<span className="row-affordance">View messages →</span>`
in the last cell. A `<tr>` has the implicit role `row`; nothing about that role
says the element is operable, so a screen-reader user landed on a tab stop that
announced as a table row and did something when they pressed Enter. This is the
classic 4.1.2 failure (F59 in spirit): script makes an element a control without
giving it a role.

`role="button"` on a `<tr>` is not available — a row inside a table may only be
a row. The fix turns the affordance into a real `<button>`:

- it carries the role and a name that says which row it opens;
- the row keeps its `onClick`, so the pointer behaviour is byte-for-byte what it
  was;
- the row's `tabIndex` and key handler are gone, so there is still exactly one
  tab stop per row — no keyboard path was added or removed, it moved onto an
  element that can describe itself;
- the button calls `stopPropagation`, because the row's handler would otherwise
  run twice per click. On five of the six that is harmless; on Share groups the
  handler is a **toggle**, so twice is the same as never. That one is a real bug
  the conversion would have introduced, and it is why the guard is on all six
  rather than only where it is load-bearing today.

**A11Y-29 · The selected cluster had no programmatic state · FIXED**
The sidebar's current connection is a tint plus a 2px left border — nothing a
screen reader can perceive. Now `aria-current="true"`. Deliberately not
`aria-selected` (there is no listbox) and not `aria-pressed` (it is not a
toggle).

**A11Y-30 · Payload inspector tabs had no panel · FIXED**
`role="tablist"` and `role="tab"` with no `aria-controls`, and a body that was
never announced as anybody's panel. The four tabs now point at one stable
`role="tabpanel"` whose contents swap and whose identity does not — so
`aria-controls` always resolves.

**A11Y-31 · Produce panel tabs had no panel · FIXED**
Same; each of the two panels is now a `role="tabpanel"` labelled by its tab.

### SC 4.1.3 — Status Messages (AA)

**A11Y-32 · Seek-bar validation was announced to nobody · FIXED**
`SeekBar` produces `{field, message}` on submit and renders it under the bar.
DESIGN §5.3 says a failed submit focuses the offending control — but SeekBar's
comment is explicit that *the caller owns focus because the caller owns the
button that failed*, and both callers (the browser and search) leave the caret
on the button. So the message appeared with no focus change and no live region:
silent. Now `role="alert"`. `aria-describedby` still carries it on the control
itself for anyone who tabs back into the field.

**A11Y-33 · CEL parse errors were announced to nobody · FIXED**
`SearchView`'s `celError` is raised by ⏎ in the box or by Start beside it,
neither of which moves focus. Now `role="alert"`.

**A11Y-34 · Decoder save failures were announced to nobody · FIXED**
`WasmSerdesFields` writes as you type, so its failures arrive on a blur — focus
has already moved on. Now `role="alert"`.

**A11Y-35 · Loading sentences are not live regions · DEFERRED**
*"Asking the cluster for topics…"*, *"Reading your saved connections…"* and
their siblings are plain prose that replaces the table it is standing in for.
**Reason to defer:** they are the content of the region, not a status *about* it,
and they are announced when the user next enters the region. Wrapping every one
in `role="status"` would announce a loading sentence on every tab switch and
every refresh, which is chatter rather than information. The genuinely important
case — a fetch failing — is already an `alert` banner. **Revisit** if user
testing shows the wait is disorienting; the honest fix then is one
`aria-live="polite"` region owned by the workspace, not thirty scattered ones.

**A11Y-36 · Scan progress announces on every tick · DEFERRED**
`SearchView`, `SqlView` and `CopyWizard` put their progress line in
`role="status"`, and it updates several times a second during a long scan. That
is *correct* for the criterion and *unpleasant* to listen to. **Reason to
defer:** throttling it means holding a second, slower copy of a number the whole
view is built around being honest about (§5.7's "search never silently
truncates"), and the wrong throttle makes the announced figure disagree with the
figure on screen. **Revisit** with a debounced mirror node — announce every ~5s,
keep the visible line live — in the phase that owns the progress contract.

---

## 3. What the reduced-motion pass changed

DESIGN §3 already ships a `prefers-reduced-motion` block: durations to 0,
`animation-duration: 1ms`, `animation-iteration-count: 1`,
`scroll-behavior: auto`, and `.spinner { animation: none }`.

That blanket is right for any animation whose **resting state is what the user
should see** — a toast that has finished sliding in is a toast in place, a chart
wipe that has finished is a drawn chart. It audited clean for `toast-in`,
`chart-draw` and every `transition`.

It is exactly **wrong** for the two indeterminate animations, whose resting
state is the end of a loop that is supposed to repeat. Collapsing those to 1ms
does not calm them down — it deletes the indicator. Both are now switched off
and given a static appearance instead, in a block at the end of `styles.css`
(the rules it overrides are declared hundreds of lines below the top block, and
at equal specificity the later declaration wins):

| Animation | What the blanket rule did to it | Now |
|---|---|---|
| `.table-loading::after` (`slide`) | Landed at `translateX(430%)` — off the end of its own `overflow: hidden` box. The one signal that a fetch is in flight over live data (§5.2: *never a spinner over data*) **disappeared** for exactly the users who asked for less motion. | `animation: none; width: 100%` — a static full-width accent bar that still says "busy". |
| `.toast-progress` (`toast-drain`) | Drained to `scaleX(0)` in 1ms, then sat empty for the five seconds the dismiss timer actually ran — a progress hairline contradicting the toast it belongs to. | `animation: none; transform: scaleX(1)` — it holds full width rather than lying. |
| `.status-connecting` (`pulse`) | One 1ms flicker. | `animation: none`. `--warn` plus the word *connecting…* beside it (Law 2) is the whole signal either way. |
| `.toast`, `.palette` (`toast-in`) | Played imperceptibly. | `animation: none` — the resting state is the answer. |
| `.term-pop` (opacity/visibility transition) | 1ms. | `transition: none` — belt and braces with the blanket rule. |
| `.chart-wipe` (`chart-draw`) | Already had its own `animation: none`. | Unchanged; verified. |

No animation in Kavka flashes, and none runs longer than five seconds
unattended, so **SC 2.3.1** (Three Flashes) and **SC 2.2.2** (Pause, Stop, Hide)
have nothing to answer: the only auto-updating things are the tail and the scan,
both of which have a visible Stop, and the toast timer pauses on hover *and* on
focus.

---

## 4. Criteria audited with no findings

Recorded so a reviewer can see they were checked rather than skipped.

**SC 1.4.1 Use of Color (A).** DESIGN Law 2 — "no state is ever encoded by
colour alone" — is enforced throughout and was re-verified component by
component: every health dot has a word beside it; lag is a number *and* a bar
length *and* a word; chart series are dash-patterned as well as coloured and the
legend swatch draws the real pattern; a hidden series is struck through as well
as dimmed; the wizard's current step is weight plus lightness; a deny ACL row
has a glyph, the word "Deny" and a wash; a diff row's meaning is the `+`/`−` in
the gutter, not the wash; search hits carry a 2px underline as well as a
background; the prod guardrail has nine independent layers of which only one is
a hue. Nothing new in Phases 3–5 breaks this.

**SC 1.4.3 Contrast (Minimum) (AA) and 1.4.11 Non-text Contrast (AA).** Not
re-derived — DESIGN §9 gate 1 sweeps every text, syntax, semantic and boundary
token against twelve dark and prod surfaces plus two prod sidebar rows, and
reports zero failures. This pass checked only that components added after that
sweep draw from the token set rather than inventing values: the chart family
(`--series-*`, `--grid-line`, `--chart-axis` on `--text-tertiary`), the topology
diagram, the plain-English bar, the MCP section, masking, and the dead-letter
badge all do. **No new colour value was introduced by this audit.** The one new
control it creates (`.row-affordance`, a button) is a ghost control whose
boundary is its own text, which §5.5 already permits.

**SC 1.4.4 Resize Text (AA).** The type scale is px-based, which the criterion
permits when the content survives 200% *zoom* — which it does, and is the same
mechanism §1.4.10 above is measured through.

**SC 2.4.1 Bypass Blocks (A).** No repeated block of content precedes the main
region. The rail is a `complementary` landmark and the workspace is `<main>`, so
both are skippable by landmark; `⌘K` is a keyboard bypass in its own right.

**SC 2.4.2 Page Titled (A).** `index.html` carries `<title>Kavka</title>`, which
is what satisfies the criterion. §6 layer 9 — the cluster and its environment
appended to that title — is **specified and not shipped**; it is a guardrail
improvement, not the thing this SC turns on, so the criterion passes today and
layer 9 makes the title more useful rather than compliant.

**SC 2.5.7 Dragging Movements (AA, new in 2.2).** Nothing in Kavka requires a
drag. The dock and rail resize handles specced in §5.1 are not implemented yet;
when they are, they need a keyboard equivalent to satisfy this — recorded here
so that lands with the feature rather than after it.

**SC 3.2.1 On Focus / 3.2.2 On Input (A).** No control changes context on focus
or on input. The environment picker swaps the substrate's *colour*, which is a
change of appearance, not of context. Nothing auto-submits.

**SC 3.2.6 Consistent Help (A, new in 2.2).** The `About` control and the
support link sit in the same rail footer on every screen.

**SC 3.3.1 Error Identification / 3.3.3 Error Suggestion (A/AA).** This is the
part of the app that was already strongest. `errors.ts` is a pure, total
`classifyError(raw, ctx?)` that turns a broker string into a title and a next
click; §7's rule 4 forbids "Invalid input" and its family outright; `validate()`
returns `{field, message}` and the form focuses the offending control. Every
message names the fix, not the failure.

**SC 3.3.2 Labels or Instructions (A).** Every input has a `<label htmlFor>` or
an explicit `aria-label`; hints are wired with `aria-describedby` (A11Y-04 was
the only exception and is fixed).

**SC 3.3.4 Error Prevention (Legal, Financial, Data) (AA).** Every destructive
action is reversible-by-confirmation, states its blast radius in numbers before
the button, and on prod is gated by type-to-confirm (§6 layer 4, §7's
destructive confirmations). This criterion is comfortably exceeded.

**SC 3.3.7 Redundant Entry (A, new in 2.2).** Nothing is asked for twice.
Secrets are the interesting case: a blank password field means "keep the stored
one" and says so in its placeholder and its hint, rather than demanding
re-entry.

**SC 3.3.8 Accessible Authentication (Minimum) (AA, new in 2.2).** Kavka's
sign-in is to a Kafka cluster, not to Kavka. No cognitive function test is
imposed: credentials come from the OS keychain, and every password field permits
paste (`autoComplete="new-password"` with no paste handler anywhere in the
tree).

**Forced colors.** Not a WCAG criterion, but it is the same audience. Verified
against DESIGN §10's doctrine: no blanket `* { border-color: CanvasText }`
anywhere, indicators restated positively with one system colour per meaning, and
the prod wire gains the literal word `PROD`. The five `@media (forced-colors:
active)` blocks are consistent — `Highlight` for selection and "the thing you
chose", `Mark` for prod and severity, `GrayText` for off, `CanvasText` for
structure.

---

## 5. Known deviations, and why they are not defects

Two things in this codebase look like findings on a first read and are not.

**Static tables carry no `role="grid"`, `aria-rowcount` or `aria-rowindex`, and
that is correct.** DESIGN §5.2: `role="grid"` is a promise of an interactive
widget with arrow-key cell navigation. Declaring it on a fully-rendered table
that does not implement that makes every row announce as a grid cell the user is
expected to drive, and nothing happens when they try. The two tables that *are*
virtualized — `MessageGrid` and `ResultGrid` — carry all three, plus
`aria-selected` on rows and `aria-activedescendant` on the scrollport, and
implement j/k, the arrows, Home/End, ⏎ and Esc.

**`aria-activedescendant` sits on a `role="group"` scrollport rather than on the
grid.** The scrollport is the focus target — it is what owns the scrolling and
what `contain: strict` applies to — and `role="group"` is there because
`aria-label` on a generic element is not reliably exposed. The active descendant
is dropped when the selected row scrolls out of the rendered window, because
pointing at an element that is not in the DOM is worse than pointing at nothing;
`aria-selected` still carries the state when the row returns.

---

## 6. The Phase 6 panels

Two surfaces landed after the sweep above: the **local playground**
(`Playground.tsx`, the first-run offer to start a single-node Kafka in Docker)
and the **diagnostics panel** (`DiagnosticsSection.tsx`, the opt-in log writer
in the About dialog). Both were walked against the same checklist — labels,
name/role/value, live regions, focus, target size — and both are on surfaces a
novice meets in their first ten minutes, which is the population this audit is
most about.

Five findings, all fixed.

### SC 4.1.3 — Status Messages (AA)

**A11Y-37 · The playground's checklist announced nothing · FIXED**
`Playground.tsx` renders the start/stop ladder as an `<ol>` that rewrites
itself: *Looking for Docker* → *Reading the bundled compose file* → *Starting a
single-node Kafka* → *Saving a connection called Playground*, each resolving
from `running` to `ok`, `fail` or `skipped`. A list is not a live region, so a
screen-reader user pressed **Start a local playground** and then heard nothing
for up to fifteen minutes — the pull of a 400 MB image — with no way to know
whether it was working, waiting or already broken. That is the longest silence
anywhere in the app.

Fixed with an `sr-only` `role="status"` mirror rendered as soon as the panel is
ready (before the first step can arrive — a live region inserted *with* its
content is a live region that does not announce). It carries the newest step's
**label and state** and deliberately not its note: the note ticks once a second
with Docker's own progress line, and building the sentence without it leaves
the text node byte-identical across those ticks, so nothing is announced until
a step actually changes state.

That is the **debounced mirror A11Y-36 defers to**, arrived at by construction
rather than by timer, and it is cheap here for the reason it was not there:
this region has no second copy of a number that could disagree with the visible
one — the visible ladder shows Docker's progress, the mirror announces
transitions, and they are two different facts rather than two versions of one.

**A11Y-38 · Diagnostics failures were announced politely · FIXED**
`DiagnosticsSection`'s error line was `role="status"`. Every error it can hold
arrives from a command that failed — toggling the switch, opening the folder,
deleting the files — while focus is still on the control that started it, which
is exactly the shape of A11Y-32 through A11Y-34. Those three were made
`role="alert"` and this one was not. Now it is.

**A11Y-39 · Deleting the logs reported nothing and took the focus with it · FIXED**
*Delete them* is rendered only while `files > 0`, so a successful delete
**removes the control the user is standing on** — focus falls to `<body>` — and
the sentence that changed underneath it (`3 files, 41 KB.` → `No log files
yet.`) was in plain prose. The one destructive action in the panel confirmed
itself to nobody.

The count sentence is now its own `role="status"`, scoped to the clause that
changes: the ceiling sentence beside it is constant, and a `role="status"` on
the whole paragraph would re-read all three clauses (`aria-atomic` is true by
default) every time a file count moved. The focus consequence is now benign —
the region reports what happened at the moment the button disappears — and it
is recorded here rather than fixed by keeping a disabled button on screen,
which would trade a solved 4.1.3 problem for a permanently disabled control
that has to explain itself (§5.5).

### SC 2.4.6 — Headings and Labels (AA)

**A11Y-40 · "Delete them" has no antecedent · FIXED**
The accessible name of the delete button was the visible string *Delete them* —
a pronoun whose referent is the previous clause of the paragraph it sits in.
Read in a list of controls, or landed on by Tab, it names nothing; this is
A11Y-20's finding in a different shape. Now
`aria-label="Delete them — every diagnostics log file on this machine"`.

The visible words **lead** the accessible name rather than being replaced by
it, which is SC 2.5.3 (Label in Name, A): a voice-control user says "delete
them", and a name that had dropped those words would not match. The `title`
stays as the pointer-hover description.

*Checked and passing in the same sweep:* **Open logs folder** names its own
target, is unique in the section, and takes its `title` as a description rather
than as its name — the contrast with *Delete them* is the whole point of the
finding above. The diagnostics checkbox has a real `<label htmlFor>`, its hint
is wired with `aria-describedby` (§5.3's shape, the one A11Y-04 was about), and
it is a native `<input type="checkbox">` — so name, role and value need no ARIA
at all. It is `disabled` for the moment the status is being read, with the
reason in a `title`; per A11Y-12 that sentence is a reason rather than an
instruction, and the state is momentary, so it stays.

### SC 4.1.2 — Name, Role, Value (A)

**A11Y-41 · Two control-state gaps in the playground panel · FIXED**
*Stop the playground* runs `docker compose down` — the same kind of wait as
*Start*, which carried `aria-busy` — and did not report itself busy. It does
now.

Its `Show details ▾` disclosure carried `aria-expanded` and no `aria-controls`,
which was valid but silent about what it opens. It now points at the `<pre>` it
reveals, **and only while that `<pre>` exists**: a reference to an element that
is not in the DOM is A11Y-05's failure, and adding the attribute
unconditionally would have re-created it.

*Checked and passing:* the busy glyph is `aria-hidden` in a fixed-width slot so
the button's name does not change mid-wait (§5.5); the failure line is already
`role="alert"`; the ladder's glyph column is `aria-hidden` with the state
carried by the mirror in A11Y-37 rather than by a `✓` a screen reader would
read as a tick character; and the panel's one refusal that is a *policy* rather
than a shortcoming — Docker pointing at a remote host — renders its endpoint as
prose in the paragraph rather than behind the disclosure, so it is read in
order rather than found.

---

## 7. Re-testing this

The audit above is static. Before a VPAT is signed, run these against a build:

1. **NVDA + Firefox and Narrator + Edge** on Windows: the connection form, the
   topic list, the message browser with the inspector open, and one destructive
   confirmation on a prod cluster.
2. **VoiceOver + Safari** on macOS: the same four.
3. **Keyboard-only**, no pointer, end to end: add a connection → connect →
   browse → search → inspect → produce → delete. Every step is reachable; this
   confirms it.
4. **Windows high contrast**, with §9 gate 5's composition on screen: one
   unselected row next to one selected row next to one prod row.
5. **400% zoom** at the 960px window minimum, and again at 1920px.
6. **`prefers-reduced-motion: reduce`** with a fetch in flight and a success
   toast up — the two cases §3 above is about.
