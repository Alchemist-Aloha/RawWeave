---
name: RawWeave
description: Graph-first RAW photo editor — the darkroom instrument
colors:
  darkroom-black: "#0b0d12"
  canvas-black: "#0d1117"
  panel-black: "#10141b"
  work-strip: "#0e131a"
  field-sunk: "#0c1016"
  surface-raised: "#151b24"
  surface-hover: "#1a2431"
  hairline: "#1c2430"
  border-control: "#2b3645"
  border-field: "#293543"
  text-primary: "#e8edf4"
  text-body: "#cdd8e3"
  text-muted: "#8a94a2"
  calibration-mint: "#6ee7c7"
  mint-border: "#58c6aa"
  mint-bright: "#8cebd3"
  mint-tint: "#13251f"
  mint-ink: "#07100e"
  danger: "#f39ba4"
  danger-tint: "#301c22"
  warning: "#e7c77c"
  warning-tint: "#2d2719"
  ai-lavender: "#aab9ee"
typography:
  display:
    fontFamily: "Manrope, ui-sans-serif, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 800
    lineHeight: 1.25
  headline:
    fontFamily: "Manrope, ui-sans-serif, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.25
    letterSpacing: "-0.02em"
  title:
    fontFamily: "Manrope, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 700
    lineHeight: 1.3
  body:
    fontFamily: "Manrope, ui-sans-serif, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.5
  control:
    fontFamily: "Manrope, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 700
    lineHeight: 1.2
  label:
    fontFamily: "'DM Mono', ui-monospace, monospace"
    fontSize: "9px"
    fontWeight: 500
    letterSpacing: "0.12em"
  mono:
    fontFamily: "'DM Mono', ui-monospace, monospace"
    fontSize: "9px"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  xs: "3px"
  sm: "4px"
  md: "5px"
  lg: "6px"
  xl: "7px"
  2xl: "8px"
  3xl: "9px"
  pill: "999px"
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
components:
  button-primary:
    backgroundColor: "{colors.calibration-mint}"
    textColor: "{colors.mint-ink}"
    typography: "{typography.control}"
    rounded: "{rounded.xl}"
    padding: "8px 13px"
  button-primary-hover:
    backgroundColor: "{colors.mint-bright}"
    textColor: "{colors.mint-ink}"
    rounded: "{rounded.xl}"
    padding: "8px 13px"
  button-default:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text-body}"
    typography: "{typography.control}"
    rounded: "{rounded.xl}"
    padding: "8px 13px"
  button-default-hover:
    backgroundColor: "{colors.surface-hover}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.xl}"
    padding: "8px 13px"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.text-body}"
    typography: "{typography.control}"
    rounded: "{rounded.xl}"
    padding: "8px 13px"
  button-danger-hover:
    backgroundColor: "{colors.danger-tint}"
    textColor: "{colors.danger}"
    rounded: "{rounded.xl}"
    padding: "8px 13px"
  icon-button:
    backgroundColor: "transparent"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.lg}"
    size: "25px"
  input-field:
    backgroundColor: "{colors.field-sunk}"
    textColor: "{colors.text-primary}"
    typography: "{typography.mono}"
    rounded: "{rounded.lg}"
    padding: "8px"
  status-chip:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text-muted}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "3px 5px"
  status-chip-active:
    backgroundColor: "{colors.mint-tint}"
    textColor: "{colors.mint-bright}"
    rounded: "{rounded.sm}"
    padding: "3px 5px"
  status-chip-error:
    backgroundColor: "{colors.danger-tint}"
    textColor: "{colors.danger}"
    rounded: "{rounded.sm}"
    padding: "3px 5px"
  graph-node:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.3xl}"
    padding: "10px 11px 9px"
  library-item:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text-body}"
    typography: "{typography.title}"
    rounded: "{rounded.2xl}"
    padding: "9px"
  tab-active:
    backgroundColor: "{colors.mint-tint}"
    textColor: "{colors.mint-bright}"
    typography: "{typography.label}"
    rounded: "{rounded.lg}"
    padding: "4px 8px"
  dock-rail:
    backgroundColor: "{colors.panel-black}"
    textColor: "{colors.text-muted}"
    typography: "{typography.label}"
    width: "32px"
---

# Design System: RawWeave

## Overview

**Creative North Star: "The Darkroom Instrument"**

RawWeave is a darkroom instrument: a dark, calibrated surface where the photograph is the brightest and most saturated thing on screen. Every pixel of chrome is spent on measurement, state, and control — never on decoration. The interface reads as equipment, not as a document: hairline rules, tight rows, and monospaced readouts sitting beside the tools they report on.

The system is calibrated rather than expressive. One accent carries every active, selected, and live signal; everything else is a neutral from the same blue-black family, differentiated by lightness alone. Restraint here is discipline, not absence — the mint is rare precisely so that it always means the same thing. The dark ground is functional too: it keeps the viewer honest about exposure, which is the entire job of a RAW editor.

Density is deliberate. Rows run 22–32px, type runs 9–12px, and panels are expected to pack information rather than breathe. This is the incumbent behavior and it suits a graph editor, where the canvas — not the chrome — is the subject. It is constrained from below by the Legibility Floor: a hard 9px minimum enforced by test, and an 11px target for anything a user must read to work. The density budget is spent on rows and panels, not on shrinking prose.

**Key Characteristics:**
- Near-black blue ground (`#0b0d12`) with a five-step surface ladder read by lightness, not hue.
- One accent family, Calibration Mint, reserved for primary action, active state, focus, and live status.
- Two type voices: Manrope for anything a user acts on, DM Mono for anything the system reports.
- Hairline 1px borders and tinted backgrounds carry state; nothing lifts or glows at rest.
- Fixed 100vh shell with independently scrolling panes; the document itself never scrolls.
- Shadows exist only under layers that genuinely float above the shell.

## Colors

A single blue-black neutral family, one mint accent family, and three status colors that only appear when something is wrong, stale, or generating.

### Primary
- **Calibration Mint** (`#6ee7c7`): the primary-action fill (`.button--primary`), the global focus ring (`outline: 2px solid #6ee7c7; outline-offset: 2px`), the status dot, the live-progress bar accent, and checkbox/range accents (`accent-color`). It is the brightest chromatic value the resting interface is allowed to spend.
- **Mint Border** (`#58c6aa`): borders that mean "you are here or you can go here" — splitter hover, input focus (`border-color: #58c6aa`), hovered node-library items, hovered panel toggles, active imageset cards.
- **Mint Bright** (`#8cebd3`): text on a tinted or dark surface for active/hover states (`.panel-toggles button.is-active`, `.scope-tab.is-active`, workflow-health readouts).
- **Mint Tint** (`#13251f`): the filled background behind every active mint state — tabs, mode buttons, drop targets, current queue rows.
- **Mint Ink** (`#07100e`): the text color placed on a solid mint fill; green-tinted near-black so the primary button stays inside the neutral family rather than reading as a second hue.

### Status
- **Fault Rose** (`#f39ba4`, tint `#301c22`): failed work, destructive affordances, error text. Borders that frame errors use a darker variant of the same rose.
- **Hold Amber** (`#e7c77c`, tint `#2d2719`): stale checkpoints, caution notices, batch warnings — work that is not wrong yet.
- **Generating Lavender** (`#aab9ee`): external/AI work in flight. It is the only color in the system that appears exclusively on a transient state.

### Neutral
- **Darkroom Black** (`#0b0d12`): the application ground and the outer shell gradient's outer stop.
- **Canvas Black** (`#0d1117`): the graph canvas and the node-handle ring color, one step up from the ground so the canvas reads as a distinct plane.
- **Panel Black** (`#10141b`): docks, panels, and popover surfaces.
- **Work Strip** (`#0e131a`): full-width working strips — browse/queue, batch, host and AI provider managers.
- **Field Sunk** (`#0c1016`): every text input, select, and textarea; darker than the panel it sits in, so fields read as recessed.
- **Surface Raised** (`#151b24`): controls, cards, and graph nodes — the lightest resting surface.
- **Surface Hover** (`#1a2431`): the one-step hover lift for controls; state changes by lightness, never by position.
- **Hairline** (`#1c2430`): every 1px divider between panes, rows, and sections.
- **Control Border** (`#2b3645`): button, toggle, chip, and node outlines.
- **Field Border** (`#293543`): input, select, and textarea outlines.
- **Text Primary** (`#e8edf4`): the root foreground and the highest-emphasis text.
- **Text Body** (`#cdd8e3`): default reading text in panels and cards.
- **Text Muted** (`#8a94a2`): the one secondary tier — eyebrow labels, metadata, identifiers, units, hints, empty-state guidance, inactive controls, third-level detail. There is no dimmer tier below it on purpose: everything the system says is said at AA or better.

### Named Rules
**The One Grey Rule.** Secondary text is a single value (`#8a94a2`), not a family of near-duplicates. It must clear 4.5:1 on the lightest surface it can sit on — including hovered rows and tiles, which are lighter than the panels they sit in — so the tier is safe wherever it lands, not merely where it currently happens to be used. `src/styles.test.ts` computes this for every text colour in the stylesheet and fails on a violation, with a named exemption list for the three cases where the text rule does not apply (text on a mint fill, a large placeholder glyph, and a control glyph).

**The One Emitter Rule.** Calibration Mint is the only accent the resting interface spends. It marks primary action, active state, focus, and live status, and nothing else — so its presence always means the same thing. Fault Rose, Hold Amber, and Generating Lavender are status colors, never decoration, and never a second brand accent.

**The Readout Rule.** State is shown by border color and tinted fill, not by filling a control with solid color. A solid mint fill is reserved for one thing per view: the primary action.

## Typography

**Display Font:** Manrope (with `ui-sans-serif, system-ui, sans-serif`)
**Body Font:** Manrope (with the same stack)
**Label/Mono Font:** DM Mono (with `ui-monospace, monospace`)

**Character:** Manrope is the human voice — labels, titles, buttons, prose. DM Mono is the instrument voice — the readout beside the dial. Both are loaded from Google Fonts in `styles.css`; DM Mono is never used for anything the user clicks, and Manrope is never used for a value the system measured.

### Hierarchy
- **Display** (800, 20px, 1.25): crash and full-surface empty screens only (`.crash-screen h1`).
- **Headline** (700, 16px, 1.25, -0.02em): panel headings (`.panel__heading h2`).
- **Title** (700, 11–13px, 1.3): node titles, library items, card headings, dialog headings. The workhorse emphasis size.
- **Body** (400, 11px, 1.5): empty states, explanations, hints. Keep lines under ~60ch.
- **Control** (700, 12px, 1.2): buttons and primary form text.
- **Label** (DM Mono 500, 9px, `letter-spacing: 0.12em`, uppercase): eyebrows and section kickers (`.eyebrow`) — the system's most distinctive texture.
- **Mono** (DM Mono 400, 9px, 1.45): every measured value — paths, node types, ids, dimensions, timings, EXIF, statuses.

### Named Rules
**The Two-Voice Rule.** Manrope speaks to the user; DM Mono speaks about the data. Anything the system reports (identifiers, node types, metadata, counts, states, timings, file paths) is mono. Anything the user acts on (labels, titles, buttons, prose) is Manrope. Never swap them for variety.

**The Legibility Floor Rule.** No type renders below 9px, and `apps/desktop/frontend/src/styles.test.ts` fails if any declaration does. Above that hard floor the targets are: 11px for prose, names, field labels and control text; 10px for mono values inside dense cards; 9px for badges, status chips, metadata keys and identifiers. Compact one-token controls in tight grids (browser-tile rating and flag buttons, the dock rail) are the deliberate exception and stay at 9px — they are glyph-sized, not sentences. The file still sets several names, breadcrumbs and field values at 9–10px (`.browser-tile__open strong`, `.queue-item__preview strong`, `.browser-breadcrumbs`, `.browser-panel__controls input`, `.parameter`, `.context-menu__item`); those sit below the 11px target and should be raised as their components are touched. Density is spent on row heights and panel layout, never on shrinking words.

## Layout

The shell is a fixed frame: `.app-shell` is a flex column at 100% width and height with `overflow: hidden`, and the document (`html, body, #root`) never scrolls. Only panes scroll internally. A topbar sits at a fixed 66px (`flex: 0 0 66px`, `padding: 0 20px`, hairline bottom border), and the workbench below it is a three-zone flex row: left dock, canvas, right dock, separated by 5px draggable splitters (`flex: 0 0 5px`).

- **Docks** carry a 32px collapsed rail (`.dock--rail`, vertical uppercase label, `writing-mode: vertical-rl`) and expand to user-resized widths via splitter drag. Docks are `Panel Black`; the canvas is `Canvas Black`.
- **Canvas panel** is `grid-template-rows: auto minmax(0, 1fr) 36px` — toolbar, canvas, status footer — so the footer stays pinned.
- **Working strips** (browse/queue, batch, integrations) replace the workbench entirely rather than stacking inside it; `.app-shell--browse`, `--batch`, and `--integrations` swap which region is displayed.
- **Browse/queue** is a 48px toolbar plus a two-column body at `minmax(0, 1fr) / minmax(285px, 31%)` with a 1px gutter drawn by the background showing through.
- **Integration strip** is `minmax(0, 1.6fr) / minmax(360px, 0.9fr)`; the host manager is `180px / 1fr / minmax(300px, 34%)`.
- **Spacing rhythm** is tight and near-uniform: 3–4px inside chips and marks, 6–8px between related controls, 12–16px panel padding, 18–24px between panel sections, 48px only on crash/empty screens. Vertical rhythm inside cards is 3–7px.
- **Section rhythm** in a long form is three-step and deliberate: 5px from a caption to its control, 12px between elements inside one section, 22px between sections. That contrast is what separates groups — not borders or cards.
- **Responsive behavior** collapses by reflow, not by hiding: at 1100px paddings and gaps tighten; at 820px the topbar wraps and the integration strip becomes single-column; at 640px the fixed frame is deliberately relaxed to document scroll. That 640px branch exists for browser-shaped viewing and is **not** a target of this desktop-only product — treat it as inherited, not canonical.
- **Scopes** run a 4-column grid (`.viewer-scopes`), collapsing to 2 columns at 820px and 1 at 640px; the compact dock variant is a tabbed single view capped at 168px tall so the panel never has dead space.
- **Grids over fixed widths**: the browser grid uses `repeat(auto-fill, minmax(106px, 1fr))` with small/medium/large/list variants (84/106/154px) rather than breakpoint-specific column counts.

### Named Rules
**The Fixed Frame Rule.** The application is exactly one viewport. The document never scrolls; panes, lists, and canvases scroll inside it. Any new panel must accept `min-height: 0` and scroll internally rather than growing the shell.

## Elevation & Depth

Depth is tonal, not physical. The system reads as a stack of flat planes separated by 1px hairlines and small lightness steps (`#0b0d12` → `#0d1117` → `#10141b` → `#151b24`), so a panel is distinguished by its tone and its border rather than by a shadow. Shadows appear only where a layer genuinely floats above the shell: graph nodes sitting on the canvas, and popovers, dialogs, menus, and toasts floating above everything. Inline controls never get one.

Hover does not lift. `.button:hover` changes border color to a lighter neutral and background to `Surface Hover`; hovered cards shift tone. Nothing translates, scales, or grows a shadow on hover. The two visible exceptions are truthful: the selected graph node gains a stronger shadow because it floats over other nodes, and connection feedback uses expanding rings around ports rather than movement.

This behavior is descriptive of the incumbent implementation, not a locked invariant — elevation on inline controls is open to revision if a future surface needs it. The one thing worth preserving is the *reason*: on a dark canvas, shadow is nearly invisible, so state must be legible through tone, border, and ring instead.

### Shadow Vocabulary
- **Float low** (`box-shadow: 0 10px 25px rgba(0, 0, 0, .18)`): graph nodes resting on the canvas.
- **Float selected** (`box-shadow: 0 0 0 1px rgba(110, 231, 199, .2), 0 10px 25px rgba(0, 0, 0, .25)`): selected node — a mint ring plus the node shadow.
- **Popover** (`box-shadow: 0 18px 45px rgba(0, 0, 0, .42)`): context menus, dialogs, shortcut panel, subgraph form.
- **Menu** (`box-shadow: 0 14px 30px rgba(0, 0, 0, .38)`): the topbar overflow menu.
- **Toast** (`box-shadow: 0 12px 28px rgba(0, 0, 0, .35)`): the error toast.
- **Ring** (`box-shadow: 0 0 0 3px rgba(110, 231, 199, .12)`): the footer status dot's halo — the only resting glow in the system.

### Named Rules
**The Floating Layer Rule.** Shadows are for layers that actually float — canvas nodes, menus, dialogs, toasts. If an element sits inside a pane, its state comes from border and tone. Hover never moves an element.

## Shapes

Rectilinear and quietly rounded: small radii (3–9px) that soften corners without producing pills, and a strict 1px hairline wherever two regions meet. Nothing is fully circular except status dots (6px), port handles (10px), and the invisible 999px hit area around a handle — hit targets may be round, visible geometry stays rectilinear.

The ladder is deliberate and reused: 3px micro chips and thumbnails, 4px badges and status pills, 5px dense inputs and imageset cards, 6px buttons and tiles, 7px cards and popover controls, 8px viewer panes and library items, 9px graph nodes, dialogs, context menus, and the 32px brand glyph. The dashed empty-state icon is the single 12px outlier, and a 16px conic-gradient checkerboard stands in for transparency behind every image surface.

### Named Rules
**The Hairline Rule.** Separation is a 1px line in `Hairline` (`#1c2430`), or a 1px gap showing the parent ground through — never a heavier divider, never a shadow standing in for a border. Borders on interactive geometry (buttons, inputs, chips) use `Control Border` / `Field Border` so they stay visible against the surface they sit on.

## Components

### Buttons
- **Shape:** gently rounded (7px radius), 1px border, `padding: 8px 13px`, `font-size: 12px`, `font-weight: 700`.
- **Primary:** solid Calibration Mint on Mint Ink, with a Mint Border outline. One per view.
- **Default:** `Surface Raised` on `Text Body`. **Hover:** `Surface Hover` background and a lighter border (`#526377`) — no lift.
- **Quiet:** transparent background, `Text Body` foreground; used for tertiary actions inside dense docks.
- **Danger:** default until hovered, then a rose border and rose text. `--danger` is a hover contract, not a resting fill.
- **Icon buttons:** 25px square (19–20px inside dense cards), transparent with a 6px radius, `Text Muted` at rest, white on `#242d38` on hover. They are the scrollbar-free way to put actions on every row.
- **Disabled:** `opacity: .4`, `cursor: not-allowed`. Never recolored, never hidden.

### Chips and Badges
- **Style:** 4px radius, `padding: 3px 5px` (2px 4px for graph badges), DM Mono uppercase, tiny type, 1px border.
- **Neutral:** `Surface Raised` background, `Text Muted` foreground.
- **Selected / active:** `Mint Tint` background with `Mint Bright` text (`#8cebd3`) — the same treatment as active tabs and mode buttons, so "selected" looks identical everywhere.
- **Status variants:** fresh/ready = mint; stale = amber (`#f0cf82` on `#2d2719`); failed = rose; generating = lavender. A status chip never uses a neutral tone when it carries state.

### Cards and Containers
- **Corner Style:** 7px for popovers and cards, 8px for viewer panes, 9px for graph nodes and dialogs.
- **Background:** `Surface Raised` for content cards, `Work Strip` for full-width strips, `Field Sunk` for the darkest insets.
- **Border:** 1px `Hairline` internally, `Control Border` when the card is interactive.
- **Shadow Strategy:** none when inline; see Elevation & Depth for the floating set.
- **Internal Padding:** 9px in dense cards (host/AI provider), 11px in checkpoint panels, 16–20px in dock panels.
- **Selected:** mint border plus a subtle mint ring; background may tint to `Mint Tint`.

### Inputs and Fields
- **Style:** `Field Sunk` background, 1px `Field Border`, 6px radius, `padding: 8px`, mono text, no inner shadow.
- **Focus:** the border turns Mint Border (`#58c6aa`). Inputs do **not** receive the global 2px outline — that is reserved for buttons and controls without their own border treatment.
- **Global focus:** `outline: 2px solid #6ee7c7; outline-offset: 2px` on buttons, inputs, and selects. Focus is never removed and never replaced by color alone.
- **Dense fields:** the host/AI forms drop to 4px radius, 5px padding, and 10px mono text; the same colors apply so a field always looks like a field.
- **Checkbox/range:** native controls with `accent-color: #6ee7c7` — never rebuilt as custom widgets.

### Forms and Field Grids
A form is a sequence of labelled sections, not a wall of inline fields. Fields lay out in a grid that auto-fills at a 200px minimum (`grid-template-columns: repeat(auto-fill, minmax(200px, 1fr))`), so one markup order yields six columns at 1440px, four at 1024px, and one in a narrow dock. Every field is a `<label>` with its caption above the control (5px gap), every control is at least 28px tall, and a single-decision control — a policy select — caps at 420px rather than stretching across the panel. Long lists of choices (per-item checkboxes) get a bounded scrolling box (`max-height: 216px`) so they cannot push the action below the fold. Section separation is rhythm, not containers: the recipe, checkpoint policy, dry run and diagnostics sections of the Batch workspace carry no borders or cards between them.

### Navigation
- **Topbar:** brand glyph (32px, 9px radius, mint border, mint initials) plus the product name, a workspace-mode switcher (Library / canvas workbench / Browse / Batch / Integrations), and a right-aligned action cluster with a `More` overflow menu (190px popover).
- **Mode and panel toggles:** bordered pills at 6px radius; active state is `Mint Tint` background with `Mint Bright` text and a Mint Border.
- **Breadcrumbs:** inline text buttons; the current scope is `Text Primary`, ancestors are `Text Body`, hovering an ancestor turns it mint. Identical treatment in the graph and the file browser, so navigation reads the same in both.
- **Dock rails:** 32px vertical strips with a rotated uppercase label; hovering tints them mint. They are the primary way to reclaim canvas space.

### Graph Node (signature)
A 168px-minimum card on the canvas: 9px radius, `Surface Raised` background, a 1px `Control Border` outline, and the `Float low` shadow. The title is 11px/800, the node type is 9px DM Mono beneath it, and the body is a two-column port grid (inputs left, outputs right) under a hairline with 11px port labels. Ports are 10px circles with a 2px `Canvas Black` ring, each wrapped in an invisible 999px hit area so grabbing a dot is never fiddly — a targeted accessibility decision worth preserving. Status badges sit top-right as 4px chips (checkpoint, fresh, stale, failed, generating, cancelled). Selected state replaces the border with mint and adds the mint ring. Connection feedback uses React Flow's own handle state classes to paint expanding pale-mint rings on valid targets and amber on the source — feedback is a ring, never a moving element.

### Viewer Pane and Scopes (signature)
Panes are 8px-radius containers over a 16px transparency checkerboard (`repeating-conic-gradient(#141a22 0% 25%, #11161d 0% 50%)`), with a header (target selector + label), a grab-cursor stage, and a bottom control row. Comparison modes overlay two panes in the same box; the difference mode applies `mix-blend-mode: difference`. Scopes are 4–5px-radius cards reading `#111720`, each with a DM Mono uppercase caption and a pixelated `<canvas>`. Inspector readouts are DM Mono key/value rows at 9px keys and 10px values.

### Error Surfaces
Every failure renders in the same visual grammar: `danger-tint` background, rose border, rose heading, lighter rose body, and — where retry is possible — a small bordered rose button. Errors appear inline in the pane that owns them (viewer, queue, provider card, checkpoint panel) or as a single toast; batch and checkpoint failures never open modals. Identifiers, provider names, and reasons are DM Mono; the human sentence is Manrope.

## Do's and Don'ts

### Do:
- **Do** keep the ground dark and let the image be the brightest thing on screen. `#0b0d12` ground, `#151b24` at the lightest for resting chrome.
- **Do** spend Calibration Mint on one primary action per view, plus active/focus/live state. Nothing else.
- **Do** use DM Mono for every measured or system-reported value and Manrope for everything the user acts on.
- **Do** reach for border color, tinted fill, and 1px hairlines to express state before reaching for shadow or motion.
- **Do** keep row heights tight (22–32px) and let panels scroll internally under a fixed 100vh shell.
- **Do** use the established radius ladder (3/4/5/6/7/8/9px) rather than inventing new values.
- **Do** pair every accent-tinted surface with `Mint Bright` text or a `Mint Border` outline so contrast survives the dark ground.
- **Do** hold the Legibility Floor: 9px is the hard minimum, 11px is the target for anything a user reads, and the test in `src/styles.test.ts` enforces the floor.

### Don't:
- **Don't** add a second accent hue. Rose, amber, and lavender are status colors; if something is neither an error, a warning, nor in-flight, it wears a neutral.
- **Don't** fill a control with solid mint unless it is the primary action for that view.
- **Don't** lift, translate, scale, or add a hover shadow to inline controls. Hover is a border and background change.
- **Don't** let the document scroll or grow past the viewport; a new pane scrolls inside itself.
- **Don't** put a shadow on an element that sits inside a pane.
- **Don't** use solid `#000` or a pure grey — every neutral here is blue-shifted and belongs to the `#0b0d12` family.
- **Don't** set prose, form labels, or error sentences at the 9px chrome size; that size is for badges, keys and identifiers.
- **Don't** replace native checkbox, range, or select behavior with custom widgets, or remove the 2px mint focus outline.
- **Don't** style the 640px mobile branch as a canonical layout; this is a desktop-only product and that branch is inherited.
