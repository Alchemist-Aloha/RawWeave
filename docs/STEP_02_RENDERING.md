# Step 02 — Rendering and Image-Data Infrastructure

## Objective

Replace the prototype image path with a scalable rendering architecture suitable for large photographs and future GPU processing.

## Core Work

Define `core.Image` and the image-buffer abstraction.

Support:

- dimensions;
- pixel format;
- color-domain metadata;
- CPU-backed images;
- GPU-backed resources;
- view/region semantics;
- immutable or revision-safe data ownership.

Introduce:

- wgpu initialization;
- WGSL shader infrastructure;
- CPU fallback execution;
- tile coordinates;
- tile requests;
- graph revisions;
- invalidation;
- intermediate cache;
- preview quality levels.

Initial tile implementation may remain simple, but APIs should assume region-based rendering.

## Viewer

Implement:

- pan;
- zoom;
- fit-to-window;
- 100% view;
- intermediate-node preview;
- Viewer A;
- Viewer B;
- split or side-by-side comparison;
- loading/progress indicator.

Do not transfer full-resolution float images through JSON IPC.

## Node Execution

Extend node descriptors with execution capabilities:

```text
CPU
GPU
TileLocal
RegionAware
FullFrame
```

GPU nodes should use a common resource/context abstraction.

## Nodes

Implement as node-pack functionality:

- Resize;
- Crop;
- Blur;
- Levels;
- Curves;
- Color Matrix.

Use these to validate mixed CPU/GPU paths.

## Cache

Cache key should include at least:

```text
node id
node implementation version
parameter hash
upstream hash
region/tile
mip level
quality level
```

The first cache can be memory-only.

## Acceptance Criteria

- 20+ MP images can be opened and navigated without copying the full image on each parameter change;
- graph edits invalidate only required downstream work;
- Viewer A/B can point at different intermediate node outputs;
- at least one GPU node runs through wgpu;
- at least one CPU node can coexist in the same graph;
- stale render results are rejected through graph revision checks;
- image data does not transit through Tauri JSON commands.

## Tests

- cache hit/miss correctness;
- revision invalidation;
- CPU/GPU output equivalence within tolerance;
- crop/resize region requests;
- viewer request cancellation;
- old-revision result rejection.

## Out of Scope

- production RAW;
- OCIO/ICC;
- full disk cache;
- sophisticated scheduling;
- external plugin processes.

## Exit Deliverable

A scalable image rendering backbone that later RAW, mask, plugin, and AI nodes can share.
