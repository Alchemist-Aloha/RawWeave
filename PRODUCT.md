# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

## Users

**Primary:** mainstream, Lightroom-class photographers. Competent working and enthusiast photographers who expect familiar RAW development results and workflows, but who are not node-graph programmers. They judge the product by whether they can get their photos developed, compared, and exported without learning a computational model first.

**Secondary (confirmed):** workflow and node-pack authors — people who build reusable graphs and third-party nodes and distribute them to other users.

## Product Purpose

RawWeave is an open-source, cross-platform desktop RAW photo editor in which the workflow graph is the primary document and the primary interface. Photographers develop RAW files, reuse one workflow across many images with per-image overrides, and share workflows and node packs as the reusable unit of work.

Success means: a mainstream photographer can open a RAW file, work in a graph-first editor, and produce professional results with reusable workflows — without becoming a graph programmer to do it, and without the graph being hidden as an advanced panel.

## Positioning

A **graph-first visual computing environment for photography**. The workflow graph itself is the reusable computational artifact, not an optional advanced view behind panels and sliders.

Explicitly not: an open-source Lightroom clone; "GIMP with nodes"; another ComfyUI frontend.

Stated conceptual neighbors: ComfyUI-style composition + DaVinci Resolve/Fusion-style visual processing + professional RAW color management + Natron/GEGL/OFX-style extensibility.

Mechanistically, one workflow serves single images, image sets, and batch jobs; metadata, analysis, constants, expressions, and logic drive node parameters; masks (including AI-produced ones) are ordinary graph data.

## Operating Context

Desktop application, Linux / Windows / macOS — desktop only, no browser or mobile target. Native GPUI application without a WebView; desktop-class mouse and keyboard input, with tablet/touch and high-DPI as quality requirements, not as a touch-first design target.

The product is a **file- and workflow-based editor**: it works without a catalog or import step. Photography work runs through Browse → Working Queue / Test Set → Workflow → Batch export. The Rust backend is authoritative for graph state, evaluation, color, rendering, caching, and serialization; the frontend owns presentation-only state. Large RAW files are handled with tiled, resolution-aware processing; the application must stay responsive with large workflows and large queues.

No cloud dependency is assumed for ordinary editing. AI and external plugins operate through explicit adapter boundaries (ComfyUI, generic HTTP providers, native/OFX/GEGL/GIMP/CLI hosts) and are treated as data-export and checkpoint boundaries.

## Capabilities and Constraints

Confirmed functionality direction (see `docs/SPEC.MD`, `docs/ROADMAP.md` for the staged plan):

- Professional RAW development (decode, white balance, demosaic, camera transform, lens correction, highlight recovery, display transform).
- Node graph as the document: typed ports, subgraphs, workflow templates, node packs, workflow serialization.
- Logic/control values: metadata, expressions, comparison, switch/select, map range — used to drive parameters and branches.
- First-class masks and spatial data, including AI-generated segmentation and prompt-based masks.
- Manual checkpoints as explicit barriers for expensive or generative work, with committed artifacts.
- External node hosting with out-of-process isolation; AI providers as adapters, never embedded models.
- ImageSet semantics (multi-image operations: HDR merge, focus stack, panoramas).
- Batch processing with preflight, resumable jobs, output recipes, and revision-pinned workflows.

Constraints that future work must preserve:

- Processing belongs in Rust nodes and reusable crates, never in UI components or viewer-only code.
- The frontend presents; it does not compute. RAW bytes and float buffers never travel over JSON IPC.
- Scene-linear values are preserved until an explicit display transform; highlights are not clamped in intermediate stages.
- Ordinary JPEG/PNG and RAW input paths stay distinct where their graph values differ.
- Node types, ports, parameter schemas, and persisted identifiers are stable by product requirement: workflow compatibility is a promise.
- GPU paths must retain a tested CPU fallback; every accelerated path has a real fallback, not simulated availability.
- Camera/lens calibration data is never invented; unavailable profiles are marked unavailable until sourced and validated.
- Diagnostics must answer: what failed, where in the graph, which image, why, whether it can retry, and which dependency is missing.

Licensing: **AGPL** (version not yet pinned; no `LICENSE` file is committed yet).

## Brand Commitments

The name **RawWeave** is fixed. No logo, voice guide, tagline, or brand asset system exists yet — nothing beyond the name is binding today.

## Evidence on Hand

- `docs/SPEC.MD` — full product and architecture specification; the authority for scope and behavior.
- `docs/ROADMAP.md` — 15-step staged plan and public milestone definitions (Prototype → Technical Preview → Public Alpha → Beta).
- `docs/STEP_01`–`STEP_15` — per-step deliverables, including `STEP_15_UI_UX.md` for the product-quality UI pass.
- `AGENTS.md` — engineering invariants for agents working in the repo.
- Working implementation: Rust workspace (core, image, rendering, node-api, graph, project, raw, color), first-party node packs, and native GPUI frontend. The legacy Tauri/React application has been removed; see `apps/desktop-gpui/README.md` for implemented surfaces and remaining gaps.
- `test-data/images/` — a deliberately small, permissively licensed RAW and common-image corpus with a provenance/licensing manifest.

Absent — future work must not fabricate these: user research or testimonials, benchmarks or performance claims, a camera/lens compatibility matrix, a pricing or distribution model, screenshots or a public website, and any third-party endorsement.

## Product Principles

1. **The graph is the document.** Anything that can be a node or node pack should be one; special-case core logic requires justification. Reuse across images, sets, and batches follows from this.
2. **Mainstream photographers first.** A Lightroom-class user is the primary audience. Graph power must not be paid for by familiar results, legible defaults, or an entry path that works without understanding the graph.
3. **Non-destructive and reproducible.** Edits are parameters, not pixels. Outputs stay reproducible over years through a versioned workflow format, explicit checkpoints, and committed artifacts.
4. **Honest boundaries.** Unavailable profiles, unproven GPU paths, and missing providers are reported as such. Failures name their cause and whether they can retry.
5. **Replaceable shell, stable core.** Processing semantics survive frontend and shell replacement; every external system enters through an adapter.
