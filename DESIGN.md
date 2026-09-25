---
name: RawWeave
description: Graph-first RAW photo editor — the grease-pencil edit bench
colors:
  room-ground: "#14110d"
  room-sunk: "#100e0a"
  room-strip: "#171310"
  room-panel: "#1a1611"
  room-raise: "#241e16"
  room-hover: "#302820"
  room-select: "#2f2a20"
  room-line: "#332a21"
  room-line-strong: "#443a2d"
  room-line-field: "#3a3126"
  room-ink: "#f5f0e4"
  room-ink-body: "#ded6c4"
  room-ink-dim: "#a89d88"
  room-ink-faint: "#847a66"
  room-spill: "#241d14"
  bench-ground: "#e0e0d5"
  bench-raise: "#eceade"
  bench-hover: "#f6f4ea"
  bench-line: "#b7b4a2"
  bench-grid: "#cecbbb"
  bench-line-strong: "#8d8a77"
  bench-glass: "#c9c6b6"
  bench-mount: "#c9c6b6"
  bench-ink: "#17170f"
  bench-ink-body: "#37362c"
  bench-ink-dim: "#55534a"
  judge-ground: "#1c1c1c"
  judge-sunk: "#141414"
  judge-raise: "#262626"
  judge-line: "#3a3a3a"
  judge-ink: "#f0f0f0"
  judge-ink-dim: "#a6a6a6"
  wax-white: "#f2efe6"
  wax-white-bright: "#fffdf7"
  wax-white-dim: "#948b77"
  wax-white-tint: "#2f2a20"
  wax-white-ink: "#14110d"
  wax-amber: "#e8a33d"
  wax-amber-dim: "#8a6a2a"
  wax-amber-tint: "#332817"
  wax-red: "#d94a38"
  wax-red-dim: "#8f4033"
  wax-red-tint: "#3a1a16"
  wax-red-ink: "#f0938a"
  wax-red-ink-soft: "#f0c0b6"
  wax-blue: "#7ba0e8"
  wax-blue-dim: "#3f5590"
  wax-blue-tint: "#1c2540"
typography:
  display:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 800
    lineHeight: 1.15
    letterSpacing: "-0.005em"
    fontStretch: "75%"
  heading:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 800
    lineHeight: 1.25
    letterSpacing: "-0.01em"
  title:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 800
    lineHeight: 1.3
  body:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.55
  control:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 700
    lineHeight: 1.2
  label:
    fontFamily: "Martian Mono, ui-monospace, monospace"
    fontSize: "9px"
    fontWeight: 500
    fontStretch: "87.5%"
    letterSpacing: "0.16em"
  edge-code:
    fontFamily: "Martian Mono, ui-monospace, monospace"
    fontSize: "9px"
    fontWeight: 500
    fontStretch: "87.5%"
    letterSpacing: "0.14em"
  readout:
    fontFamily: "Martian Mono, ui-monospace, monospace"
    fontSize: "10px"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  frame: "3px"
  field: "3px"
  chip: "3px"
  card: "3px"
  circle: "50%"
spacing:
  2xs: "3px"
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  2xl: "20px"
  3xl: "24px"
  4xl: "48px"
  lattice: "4px"
  row: "28px"
  row-tight: "24px"
  row-loose: "32px"
components:
  button-primary:
    backgroundColor: "{colors.wax-white}"
    textColor: "{colors.wax-white-ink}"
    typography: "{typography.control}"
    rounded: "{rounded.card}"
    padding: "8px 13px"
  button-primary-hover:
    backgroundColor: "{colors.wax-white-bright}"
    textColor: "{colors.wax-white-ink}"
    rounded: "{rounded.card}"
    padding: "8px 13px"
  button-default:
    backgroundColor: "{colors.room-raise}"
    textColor: "{colors.room-ink}"
    typography: "{typography.control}"
    rounded: "{rounded.card}"
    padding: "8px 13px"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.room-ink-dim}"
    typography: "{typography.control}"
    rounded: "{rounded.card}"
    padding: "8px 13px"
  button-danger-hover:
    backgroundColor: "{colors.wax-red-tint}"
    textColor: "{colors.wax-red-ink}"
    rounded: "{rounded.card}"
    padding: "8px 13px"
  icon-button:
    backgroundColor: "transparent"
    textColor: "{colors.room-ink-dim}"
    rounded: "{rounded.chip}"
    size: "25px"
  input-field:
    backgroundColor: "{colors.room-sunk}"
    textColor: "{colors.room-ink}"
    typography: "{typography.readout}"
    rounded: "{rounded.field}"
    padding: "8px"
  readout-row:
    textColor: "{colors.room-ink-dim}"
    typography: "{typography.edge-code}"
  status-stamp:
    backgroundColor: "{colors.room-raise}"
    textColor: "{colors.room-ink-dim}"
    typography: "{typography.edge-code}"
    rounded: "{rounded.chip}"
    padding: "3px 5px"
  status-stamp-fresh:
    backgroundColor: "{colors.wax-white-tint}"
    textColor: "{colors.wax-white}"
  status-stamp-stale:
    backgroundColor: "{colors.wax-amber-tint}"
    textColor: "{colors.wax-amber}"
  status-stamp-failed:
    backgroundColor: "{colors.wax-red-tint}"
    textColor: "{colors.wax-red-ink}"
  status-stamp-generating:
    backgroundColor: "{colors.wax-blue-tint}"
    textColor: "{colors.wax-blue}"
  graph-frame:
    backgroundColor: "{colors.bench-raise}"
    textColor: "{colors.bench-ink}"
    rounded: "{rounded.frame}"
    padding: "10px 11px 9px"
  contact-sheet-frame:
    backgroundColor: "{colors.bench-raise}"
    textColor: "{colors.bench-ink-body}"
    rounded: "0"
    padding: "9px"
  lamp-switch:
    backgroundColor: "{colors.room-raise}"
    textColor: "{colors.room-ink-dim}"
    typography: "{typography.edge-code}"
    rounded: "{rounded.chip}"
    padding: "7px 10px"
  dock-rail:
    backgroundColor: "{colors.room-panel}"
    textColor: "{colors.room-ink-dim}"
    typography: "{typography.label}"
    width: "32px"
---

# Design System: RawWeave

## Overview

**Creative North Star: "The Grease-Pencil Edit Bench"**

RawWeave is an edit bench: a dark room with one lit sheet of acrylic in it. Frames are laid out on that sheet, read by the codes printed along their edges, and marked in wax by the person doing the work. Nothing in the chrome is decoration — every rule, tone step, and stamp is either a measurement or a piece of state, and the photograph is always the only thing with real light in it.

The world is built on **four casts** and **four waxes**, and both sets are closed.

A cast is what a surface is *for*, and it decides its temperature:

- **Room** — warm near-black. Handling chrome: docks, panels, batch, integrations, menus, the topbar. You work in this dark.
- **Bench** — the one plane a frame is laid out on: the node workflow panel, the contact sheet. It is **dark by default**, a neutral instrument surface, and it becomes lit acrylic (`#e0e0d5`) when you throw the lamp. One plane per workspace, never two.
- **Plane** — the bench with the lamp lit: same structure, warm-neutral, backlit. You switch to it to handle frames on paper rather than to read a machine.
- **Judge** — neutral grey, always dark. The measuring station: viewer panes, scopes, the histogram. A photograph is judged in a neutral surround, never on a warm one.

A wax is a meaning, assigned to exactly one thing:

- **Wax White** — selection, and the one primary action per view.
- **Wax Amber** — focus, hold, caution, and the target you are about to drop onto.
- **Wax Red** — fault, destructive affordances, rejected frames, clipped highlights.
- **Wax Blue** — work in flight: generation, external hosts, a running batch.

Mint is retired. In the previous world one mint carried all four of those meanings at once, which is why it could never be read confidently; the four waxes are the fix, not a palette swap.

**Key Characteristics:**
- One plane inside a dark room, dark by default and lit when you throw the lamp; the image is always the brightest thing on screen.
- Two casts that mean something: warm where you handle, neutral where you measure.
- Four waxes, one meaning each; colour never appears as decoration anywhere in the app.
- Four corner radii: everything is 3px, `50%` for status dots and port handles. There are no pills.
- Density carried by rules and case: hierarchy comes from full-width hairlines and uppercase edge codes, not from cards, shadows, or spacing.
- A 4px lattice: panel edges, row heights, and rules all land on it.
- No glow, no blur, no glass, no gradient text. State is a tone step, a border, or a stamp.

## Colors

### The Room
- **Room Ground** (`#14110d`): the application ground, with **Room Spill** (`#241d14`) as the glow the bench throws onto the wall above it.
- **Room Sunk** (`#100e0a`) is every text field and inset box; **Room Panel** (`#1a1611`), **Room Strip** (`#171310`) and **Room Raise** (`#241e16`) are the three working surfaces, read by lightness alone.
- **Room Hover** (`#302820`) and **Room Select** (`#2f2a20`) are the only two hover/selection fills on the dark side.
- **Room Ink** (`#f5f0e4`), **Room Ink Body** (`#ded6c4`), **Room Ink Dim** (`#a89d88`), **Room Ink Faint** (`#847a66`): one ink ladder, warm, four steps, no fifth.

### The Bench
One plane with two states, and the recessed field inverts between them: in the dark, a field is *darker* than the plane it sits in, and on lit acrylic it is *brighter*, because the light is behind the sheet.

Dark, the default:
- **Bench Ground** `#1b1c1d`, **Bench Raise** `#24262a`, **Bench Hover** `#2c2f33`, **Bench Sunk** `#101112`.
- **Bench Grid** `#333538` rules the glass faintly; **Bench Line** `#3b3d40` and **Bench Line Strong** `#55585c` rule and frame it; **Bench Mount** `#34363a` is the frame the strip sits in.
- **Bench Ink** `#f0f0ec`, **Bench Ink Body** `#d6d6d0`, **Bench Ink Dim** `#9d9d97`.

Lit:
- **Bench Ground** `#e0e0d5` (the backlit acrylic), **Bench Raise** `#eceade` is a sheet resting on it, **Bench Hover** `#f6f4ea`, **Bench Sunk** `#f4f2e6`.
- **Bench Grid** `#cecbbb` under the glass, **Bench Line** `#b7b4a2`, **Bench Line Strong** `#8d8a77`, **Bench Mount** `#c9c6b6`.
- **Bench Ink** `#17170f` (black wax), **Bench Ink Body** `#37362c`, **Bench Ink Dim** `#55534a`.

### The Judge Station
- **Judge Ground** (`#1c1c1c`), **Judge Sunk** (`#141414`), **Judge Raise** (`#262626`), **Judge Line** (`#3a3a3a`), **Judge Ink** (`#f0f0f0`), **Judge Ink Dim** (`#a6a6a6`). Deliberately neutral: a warm surround biases how a photograph is read, and this is the one part of the app whose job is to not have an opinion.

### The Waxes
- **Wax White** (`#f2efe6`) on the dark side, black wax (`#1a1a12`) on the lit plane — the same mark, because white wax is invisible on lit paper and black wax is invisible in the dark. It is the selection mark, the current frame, the active tab, and the single primary action.
- **Wax Amber** (`#e8a33d` / `#6b4f12` on the plane) — focus rings, hold and caution, the target of a drop.
- **Wax Red** (`#d94a38`), with **Wax Red Ink** (`#f0938a`) as its text role and **Wax Red Ink Soft** (`#f0c0b6`) for error body copy. Text never uses the mark value: a red dark enough to be a wax is too dark to be read.
- **Wax Blue** (`#7ba0e8`) — the only colour allowed to mean "in flight".
- Ports and edges use the four wax families with tone steps inside a family (`data-type-colors.ts`), so a wire says which *kind* of data it carries and its label says which type. Ten arbitrary hues told you neither.

### Named Rules
**The Closed Wax Rule.** Colour in this app means one of four things and nothing else. A new hue is not a design decision, it is a bug: if something needs a colour it does not have, it needs a different wax or it needs no colour at all. Rose, amber, and blue are meanings, never decoration.

**The Measured-Surround Rule.** Anything whose job is to help judge an image is neutral. Warmth is for handling and is never allowed in the viewer, the scopes, or a histogram.

**Two Waxes, One Mark Rule.** A mark is the same mark on both sides of the lamp: it inverts with the cast rather than changing meaning. Nothing in the app is marked only in one cast.

**The AA-Per-Cast Rule.** Every ink is checked against the worst surface of its own cast, not against a single flat list of dark grounds — `src/styles.test.ts` derives the surfaces from the token table, fails on any colour that is not a declared token, and fails any ink under 4.5:1. Adding a cast means adding its surfaces, and the test refuses a cast with none.

## Typography

**Human voice:** Archivo (variable, width 62.5–125, weight 400–900), self-hosted from `src/fonts/`.
**Instrument voice:** Martian Mono (variable, width 75–112.5, weight 300–700), self-hosted from `src/fonts/`.

Both faces ship inside the application as woff2 subsets with their OFL licenses (`src/fonts/OFL-*.txt`), imported through `src/fonts.css`. They were loaded from a font CDN before, which meant an offline install fell back to the platform's own sans — a different design, not a degraded one, in a product whose truth assumes no cloud dependency.

**Character:** Archivo is compressed to 75% for the display tier and left near its natural width for the things a user acts on. Martian Mono is condensed to 87.5% wherever it prints a code, and left alone wherever it reports a value. One family gives both the loud structural line and the quiet UI text, which is the same economy a technical data sheet uses.

### Hierarchy
- **Display** (Archivo 800, 17px, compressed to 75%): the surface title — the workflow's own name on the bench, the empty-leader heading, crash screens. The loudest type in the app, and it is spent on the name of the thing you are working on.
- **Heading** (Archivo 800, 16px): panel headings. Sentence case, always: user data keeps its own case.
- **Title** (Archivo 800, 12px): node titles, library rows, card headings, dialog headings.
- **Body** (Archivo 400, 12px, 1.55): empty states, hints, error sentences.
- **Control** (Archivo 700, 12px, 1.2): buttons, field labels, menu items. Never uppercased: a control says what it does, in words.
- **Label** (Martian Mono 500, 9px, 87.5% width, `0.16em`): eyebrows and section kickers.
- **Edge Code** (Martian Mono 500, 9px, 87.5% width, `0.14em`, uppercase): node types, file identities, statuses, source keys — anything the system prints about a thing.
- **Readout** (Martian Mono 400, 10px): every measured value — dimensions, timings, counts, EXIF, ids, paths.

### Named Rules
**The Two-Voice Rule.** Archivo speaks to the user; Martian Mono speaks about the data. Anything the system reports is mono. Anything the user acts on is Archivo. Never swap them for variety, and never set a sentence a user must read at the chrome size.

**The Legibility Floor Rule.** No type below 9px, enforced by `src/styles.test.ts`. 9px is for badges, edge codes, and identifiers; 11–12px for prose, names, labels, and control text. Density is spent on row heights and panel layout, never on shrinking words.

**The Case Rule.** Uppercase belongs to the system's own voice — edge codes, stamps, section kickers, control toggles. User data (workflow names, node names, file names) is never uppercased, and neither is any label a user clicks.

## Layout

The shell is a fixed frame: `.app-shell` is a flex column at 100% width and height with `overflow: hidden`, and the document never scrolls. Panes scroll inside it, and their scrollbars read the cast they are scrolling in.

- **The workbench** is three zones divided by 5px splitters: the ruled node index on the left, the bench in the centre, the judging station on the right.
- **The bench** is one panel with three rows — a ruled toolbar, the glass, and a footer rail — and the whole panel wears the plane's cast, frame included, so it reads as the top of a light box rather than as chrome above a canvas.
- **The judging station** sits in the right dock: viewer panes above, scopes below, all in the neutral cast.
- **Working strips** (browse/queue, batch, integrations) replace the workbench rather than stacking inside it. Browse puts the contact sheet on the bench and keeps the working queue dark, because the queue is the short list you have already made.
- **Batch and integrations have no bench at all.** They are room surfaces, and their quiet is the point: no plane, no frames, just ruled sections.
- **Rhythm** is 3–4px inside chips and marks, 6–8px between related controls, 12–16px panel padding, 18–24px between panel sections, 48px only on crash and empty screens. A heading always has more space above it than below it.
- **Narrow widths reflow, they do not hide.** At 1100px the topbar wraps to two rows (brand and actions, then the mode switcher) rather than truncating a mode name; at 820px the strips go single-column and the scopes drop to two columns. The 640px branch is inherited from the web build and is **not** a target of this desktop-only product.
- **Grids over fixed widths**: the contact sheet is `repeat(auto-fill, minmax(106px, 1fr))` with small/medium/large/list variants, and the batch recipe auto-fills at a 200px minimum.

### Named Rules
**The Fixed Frame Rule.** The application is exactly one viewport. Any new panel accepts `min-height: 0` and scrolls internally rather than growing the shell.

**The One Plane Rule.** A workspace has at most one plane, and it is always the surface frames are laid out on. Whether it is dark or lit, two of them would make neither read as *the* place you are working.

## Elevation & Depth

Depth is tonal. Planes are separated by 1px hairlines and small lightness steps, and the app is nearly shadowless by design: on a dark ground a shadow is invisible, and on a lit plane it reads as dirt on the acrylic.

- **Frames on the glass** cast one short, real shadow (`0 7px 14px -8px`, warm-black) because a frame physically sits on the pane. That is the only resting shadow in the app.
- **Floating layers** — menus, dialogs, popovers, toasts — carry the offset-and-blur shadows they need to leave the shell.
- **Selection is a ring, not a halo**: an outline in wax with the negative offset that keeps it inside the frame's edge, plus the frame's own lift. A zero-offset coloured halo is decoration and was removed, including from the lamp switch's lens.
- **Hover is a tone change.** Nothing translates, scales, or grows a shadow on hover.
- **The state band.** A frame's run state is carried in its bottom perforations — the ticks take the state's wax — and printed in words on its badge. It is deliberately *not* a stripe down the side of the card; that was the first version and it was the most recognizable generic tell in the interface. Nothing in this app is marked with a side stripe, on a card, a row, or a list item.

### Named Rules
**The Floating Layer Rule.** A shadow means the element genuinely leaves the shell. Everything inside a pane states itself with tone, border, and rule.

**The Band-Edge Rule.** State is carried on the edge of the thing it describes — the strip's perforations, the stamp in the corner — never in a chip floating away from it.

## Shapes

Rectilinear and square-cut. There is exactly one radius in the system — **3px** — used for frames, fields, chips, stamps, and buttons alike, plus `50%` for status dots, port handles, and the lamp lens. The old 3–9px ladder is gone: a per-size radius ladder is a way of making one system look like nine, and a film frame has one corner.

Hairlines are always 1px, in the cast's line colour, and a region border uses the stronger line so it stays visible against the surface it sits on. The transparency checkerboard behind image surfaces is the one 16px pattern in the app, and the perforated strip along a frame's bottom edge is the one repeating tick pattern.

### Named Rules
**The 1px Rule.** Separation is a 1px line or a 1px gap showing the parent ground through. Never a heavier divider, never a shadow standing in for a border.

**The Cut-Corner Rule.** One radius, everywhere. A new radius is a new system.

## Components

### The Lamp
The signature control, and the only state control in the shell. A bordered switch with a lens: **lit** means the table is on and you are handling frames; **off** means the plane goes dark and neutral because you are judging a photograph. It is `aria-pressed`, keyboard reachable, and persisted in `localStorage` — view state only, never graph state.

Throwing it is the app's one authored moment: the plane's own cast — its ground, its rules, its frames' ink and borders — ramps over **160ms on an exponential ease-out**, so a light switched at night does not flash. Nothing else in the app eases. Hover is a tone change, and nothing translates, scales, or grows a shadow.

The app opens with it **off**, because an editor is a dark instrument and the image should be the only lit thing on screen until you decide otherwise.

### Buttons
- **Shape:** 3px radius, 1px border, `padding: 8px 13px`, Archivo 700 12px.
- **Primary:** solid Wax White on Wax White Ink. One per view.
- **Default:** Room Raise with the room ink; hover steps the fill and the border up one tone.
- **Quiet:** transparent, dim ink — tertiary actions in dense docks.
- **Danger:** default until hovered, then a rose border and rose ink.
- **Icon buttons:** 25px square, transparent, dim at rest.
- **Disabled:** `opacity: .4`, `cursor: not-allowed`. Never recoloured, never hidden.

### Stamps
Every state the machine reports is the same object: a 1px box, 3px radius, Martian Mono 9px uppercase, tracked. Neutral stamps are room-raise; a state stamp takes its wax tint and its ink. The treatment is identical in the node badge, the batch state, the checkpoint state, the host and provider status, and the diagnostic severity — so a state is never decoded twice.

### Frames
A node on the graph is a frame on the strip: Bench Raise on the glass, 3px radius, a 1px frame line, a short real shadow, the node type printed as an edge code beneath the title, and a perforated strip along the bottom edge whose ticks carry the frame's state. Selecting one draws a wax outline and recedes the rest of the strip to 72% opacity — commit, then clarify.

### Fields
Room Sunk ground, 1px field border, 3px radius, 8px padding, mono readout text, no inner shadow. Focus turns the border amber. **Checkboxes, radios, ranges, and file inputs stay native** (`accent-color` only); text inputs, textareas, and selects drop the system appearance so the cast actually reaches the screen on WebKit, with the select's chevron drawn in the field ink. Native select behaviour, keyboard handling, and the picker are untouched.

### Control marks
Every chevron, close mark, stepper, and sort arrow is an authored SVG from `src/ui/Icon.tsx`, drawn on one 12px grid at 1.5px stroke with round caps, inheriting the ink of the control it sits in. There are no unicode arrows in the interface: a glyph comes from whichever font resolves it, at that font's weight, and changes shape when the font falls back.
### Cards and Lists
Cards are 3px radius on Room Raise with a hairline border, and they are the exception: the node index, the queue, the source metadata, and the batch sections are **ruled lists** — a full-width hairline between rows, no container around them. Nested cards are always wrong.

### The Contact Sheet
Tiles are frames butted on the plane: no border and no radius, the photograph inside a 1px mount line, its filename and dimensions printed in a ruled ledger row beneath it, and the plane's ground showing through the 11px gaps. A selected tile lifts its mount line to wax. This is the same frame device the canvas uses, so the two planes read as one material rather than as a canvas and a grid of cards.

### The Minimap
The minimap is a **window cut into the plane**, not a panel laid over it: the recessed ground, the frames inside it drawn in the plane's line tone, a hairline mask outline showing where you are, and a footprint (148×100) small enough to stay out of the way on a narrow window. React Flow's own light-theme defaults painted a 60%-white mask over the plane; its consumed variables are set from the plane's tokens instead, so the window follows the lamp.

### Viewer Pane and Scopes
8px-radius containers in the neutral cast over a checkerboard that stands in for transparency, with a header (target selector + label) and a control rail. Comparison modes overlay panes in one box, and difference mode uses `mix-blend-mode: difference`. Scopes are the darkest neutral in the app (`#141414`) with a pixelated canvas, so nothing sits between the trace and the eye. Everything the system measured here is mono.

### Error Surfaces
Every failure renders in the same grammar: red tint, rose border, a rose heading, softer rose body text, and — where retry is possible — a small bordered rose button. Errors appear inline in the pane that owns them or as a single toast; batch and checkpoint failures never open modals. Identifiers and reasons are mono; the human sentence is Archivo.

## Do's and Don'ts

### Do:
- **Do** keep the room dark and the image the brightest thing on screen, and let the plane be dark unless the lamp is thrown.
- **Do** decide the cast first, then the values: warm where you handle, neutral where you measure.
- **Do** spend colour only as wax — selection, focus/hold, fault, in flight — and only on the thing that has that state.
- **Do** carry hierarchy with full-width rules, uppercase edge codes, and tone steps before reaching for a card or a shadow.
- **Do** land every edge, row, and rule on the 4px lattice, and every corner on 3px.
- **Do** give a heading more space above it than below, and a panel heading its own rule.
- **Do** print the unit beside the value and set both in mono.
- **Do** keep the memo: every ink token must survive the AA check for its own cast, every rule below 9px fails the build.

### Don't:
- **Don't** add a fifth hue, or reuse a wax for a second meaning. Rose, amber, and blue are meanings, not decoration.
- **Don't** let warmth into the judging station, or a second plane into a workspace.
- **Don't** fill a control with solid wax unless it is the primary action for that view.
- **Don't** glow, blur, glass, or gradient anything. State is a tone step, a border, or a stamp.
- **Don't** put a shadow on an element inside a pane, or a zero-offset halo anywhere.
- **Don't** mark a card with a stripe down its side; state belongs on the thing's own edge.
- **Don't** stack an eyebrow above a heading. The heading carries its weight and the edge code reads beside it as a caption.
- **Don't** uppercase user data, or a label a user clicks.
- **Don't** rebuild a native control the platform already draws — checkboxes, ranges, and the picker stay native.
- **Don't** let the document scroll or grow past the viewport; a new pane scrolls inside itself.
- **Don't** use solid black or an untinted grey: every neutral here belongs to the room's family, and the judge station's grey is neutral on purpose and only there.
- **Don't** treat the 640px branch as a canonical layout; this is a desktop-only product and that branch is inherited.
