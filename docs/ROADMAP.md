# ROADMAP.md

Implementation note: the legacy Tauri/React app has been removed. Shell-specific
sections below describe the original plan, not the current desktop architecture.
The Rust/core and product requirements remain applicable; see
`../apps/desktop-gpui/README.md` for the native implementation and remaining gaps.

# Development Roadmap

This roadmap decomposes the implementation of the graph-first photographic workflow application defined in `SPEC.MD` into 15 incremental steps.

The development strategy is intentionally architecture-first:

- build a thin but complete vertical slice before adding breadth;
- keep the Rust core small and generic;
- implement photographic functions as node packs/plugins wherever possible;
- keep the graph authoritative;
- treat UI polish as continuous work, with a larger refinement pass after the system stabilizes;
- add AI only after manual checkpoint and external-host semantics are proven independently;
- make every milestone independently testable.

The guiding rule is:

> If a function can reasonably be implemented as a node or node pack, it should not become special-case application core logic.

## Epoch 1 — Foundation

### Step 01 — Backbone: Core Engine, Frontend Shell, and Minimal Plugin System

Build the smallest end-to-end application that proves the architecture.

Deliver:

- Tauri 2 shell;
- React + TypeScript + React Flow frontend;
- Rust authoritative graph core;
- typed node ports;
- node registration/loading;
- workflow serialization;
- frontend/backend API abstraction;
- basic viewer;
- first sample nodes implemented through the same plugin/node API intended for third parties.

Representative nodes:

- Image Input;
- Constant Float;
- Exposure;
- Invert;
- Output.

Exit criterion:

A user can open an image, construct a tiny workflow visually, change a parameter, preview the result, save the workflow, reload it, and run third-party-style nodes without privileged hard-coded processing paths.

See: `STEP_01_BACKBONE.md`

### Step 02 — Rendering and Image-Data Infrastructure

Establish the image buffer, GPU, cache, and preview backbone.

Deliver:

- `core.Image`;
- CPU and GPU execution paths;
- wgpu/WGSL support;
- tiling;
- cache;
- revision/invalidation;
- viewer zoom/pan;
- intermediate-node preview;
- Viewer A/B.

Representative nodes:

- Resize;
- Crop;
- Blur;
- Levels;
- Curves;
- Color Matrix.

See: `STEP_02_RENDERING.md`

### Step 03 — RAW Photography Foundation

Turn the graph engine into a real RAW photo processor.

Deliver:

- RAW decoder abstraction;
- RAW metadata;
- scene-linear working representation;
- RAW pipeline;
- color-management foundation;
- EXIF access.

Representative nodes:

- RAW Decode;
- White Balance;
- Demosaic;
- Camera Transform;
- Lens Correction;
- Highlight Recovery;
- Display Transform.

See: `STEP_03_RAW_FOUNDATION.md`

### Step 04 — Logic and Adaptive Workflows

Add typed scalar/control flow and metadata-driven behavior.

Deliver:

- connectable parameters;
- metadata outputs;
- cheap control-value evaluator;
- lazy conditional evaluation.

Representative nodes:

- Constant;
- Metadata;
- Compare;
- AND / OR / NOT;
- Switch;
- Select;
- Map Range;
- Clamp;
- Curve;
- Expression.

See: `STEP_04_LOGIC.md`

## Epoch 2 — Usable Photographer Application

### Step 05 — Subgraphs and Reusable Workflow Ecosystem

Make workflows composable and distributable.

Deliver:

- subgraph blueprints;
- exposed subgraph ports;
- workflow templates;
- node-pack manifests;
- dependency/version metadata;
- workflow snapshots.

See: `STEP_05_SUBGRAPHS.md`

### Step 06 — File Browser and Working Queue

Build the photographer-facing Browse → Queue → Workflow loop.

Deliver:

- filesystem browser;
- thumbnails;
- EXIF display;
- ratings/flags;
- basic file operations;
- Working Queue;
- Test Set;
- session persistence;
- per-image overrides.

See: `STEP_06_BROWSER_QUEUE.md`

### Step 07 — Mask and Spatial-Data System

Make masks and spatial data first-class graph values.

Deliver:

- `Mask`;
- mask rendering/cache;
- mask overlay;
- painting;
- gradients;
- qualifier;
- mask algebra.

Representative nodes:

- Painted Mask;
- Gradient Mask;
- Luminance Mask;
- Color Qualifier;
- Add/Subtract/Intersect;
- Feather;
- Expand/Contract;
- Blur.

See: `STEP_07_MASKS.md`

### Step 08 — Batch Processing

Turn the workflow engine into a reliable production tool.

Deliver:

- preflight;
- dry runs;
- batch scheduler;
- resumable jobs;
- retries;
- workflow revision pinning;
- Output Recipes;
- filename templates;
- checkpoint policies.

Representative output nodes:

- JPEG;
- PNG;
- TIFF;
- OpenEXR.

See: `STEP_08_BATCH.md`

## Epoch 3 — Extensibility and AI

### Step 09 — External Node Host Framework

Create the generic compatibility/process boundary.

Deliver:

- out-of-process external node protocol;
- shared-memory transport;
- capability negotiation;
- crash isolation;
- lifecycle management.

Initial adapters:

- CLI host;
- GEGL;
- OFX;
- GIMP compatibility.

See: `STEP_09_EXTERNAL_HOSTS.md`

### Step 10 — Manual Checkpoints

Add explicit stateful evaluation barriers independent of AI.

Deliver:

- `Automatic` vs `ManualCheckpoint`;
- checkpoint state machine;
- committed artifacts;
- stale dependency tracking;
- checkpoint UI;
- batch checkpoint policies.

See: `STEP_10_CHECKPOINTS.md`

### Step 11 — AI Provider Layer

Add AI without embedding a model runtime into the editor.

Deliver:

- generic `AiProvider`;
- ComfyUI provider;
- generic HTTP provider;
- polling/cancellation;
- persistent generated artifacts;
- AI color interchange.

Representative nodes:

- Img2Img;
- Inpaint;
- Generative Fill;
- Upscale.

See: `STEP_11_AI_PROVIDERS.md`

### Step 12 — AI Masks and Analysis

Use AI as a producer of normal graph data.

Deliver:

- `MaskSet`;
- `LabelMap`;
- `ConfidenceMap`;
- `DepthMap`;
- AI segmentation;
- prompt-guided masks;
- scene analysis.

See: `STEP_12_AI_MASKS.md`

## Epoch 4 — Advanced Ecosystem

### Step 13 — ImageSet and Computational Photography

Add true multi-image graph semantics.

Deliver:

- `ImageSet`;
- collection-level scheduling;
- HDR merge;
- focus stack;
- panorama/alignment primitives;
- ImageSet operators.

See: `STEP_13_IMAGESET.md`

### Step 14 — Professional Photo Node Packs

Expand photographic capability primarily through plugins/node packs rather than core changes.

Candidate node packs:

- advanced denoise;
- deconvolution;
- advanced sharpening;
- defringe;
- chromatic-aberration correction;
- local contrast;
- film simulation;
- grain;
- halation;
- LUT tools;
- advanced tone mapping.

See: `STEP_14_PRO_TOOLS.md`

### Step 15 — UI/UX Refinement Against Full Spec

Perform a dedicated product-quality pass after the architecture is proven.

Deliver:

- large-graph performance improvements;
- keyboard-first graph editing;
- better node search;
- thumbnails and previews;
- docking/layout polish;
- Browse/Workflow/Batch refinement;
- Test Set multi-preview;
- scopes;
- high-DPI;
- tablet input;
- accessibility;
- undo/redo polish;
- error and dependency UX.

See: `STEP_15_UI_UX.md`

# Recommended Public Milestones

## Internal Prototype

After Step 02:

- graph executes;
- plugins load;
- viewer works;
- CPU/GPU path exists.

## Technical Preview

After Step 04:

- real RAW support;
- adaptive metadata-driven workflows;
- reusable graph semantics proven.

## First Public Alpha

After Step 06 or 07:

- RAW pipeline;
- graph-first editing;
- logic;
- subgraphs;
- browser;
- Working Queue;
- Test Set;
- masks.

This is the first milestone that clearly demonstrates the product's identity.

## First Production-Oriented Beta

After Step 08:

- reliable batch operation;
- export recipes;
- resumable jobs;
- reproducible workflow revision binding.

## Extensibility/AI Beta

After Step 12:

- external plugin hosts;
- checkpoint semantics;
- ComfyUI/HTTP AI;
- AI masks.

# Core vs Plugin Boundary

Core should contain only infrastructure that many unrelated nodes require:

```text
graph
typed ports
scheduler
cache
image/spatial/value data types
workflow/project format
plugin/node registration
external-host protocol
artifact store
color-management interfaces
viewer/render interfaces
batch orchestration
```

Prefer node packs for:

```text
White Balance
Demosaic variants
Exposure
Denoise
Sharpen
HDR Merge
Focus Stack
AI Segmentation
Film Grain
Halation
LUT operations
Export encoders where practical
```

A useful architectural test is:

> Can the first-party node be implemented through the same API available to a third-party node?

If not, document why privileged access is unavoidable.

# Dependency Order

The intended dependency chain is:

```text
01 Backbone
    ↓
02 Rendering
    ↓
03 RAW
    ↓
04 Logic
    ↓
05 Subgraphs
    ↓
06 Browser/Queue
    ↓
07 Masks
    ↓
08 Batch
    ↓
09 External Hosts
    ↓
10 Checkpoints
    ↓
11 AI Providers
    ↓
12 AI Masks
    ↓
13 ImageSet
    ↓
14 Professional Nodes
    ↓
15 UI/UX Refinement
```

Some later work may overlap once interfaces stabilize, but architecture work should preserve this dependency direction.
