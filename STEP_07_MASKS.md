# Step 07 — Mask and Spatial-Data System

## Objective

Make masks and spatial outputs first-class graph data rather than special properties attached to adjustment nodes.

## Types

Add:

```text
core.Mask
core.MaskSet
core.LabelMap
core.ConfidenceMap
core.DepthMap
core.RegionSet
```

Initial implementation can focus on `Mask`.

## Rendering and Cache

Masks need:

- tile-aware storage;
- preview;
- overlay;
- viewer inspection;
- compositing with image operations;
- independent cache keys.

## Mask Nodes

Implement:

- Painted Mask;
- Linear Gradient;
- Radial Gradient;
- Luminance Mask;
- Color Qualifier;
- Invert;
- Add;
- Subtract;
- Intersect;
- Multiply;
- Threshold;
- Feather;
- Blur;
- Expand;
- Contract.

## Paint UX

Implement:

- brush size;
- hardness;
- opacity;
- add/subtract modes;
- zoom-aware input;
- stroke undo/redo;
- overlay toggle.

Painting state should serialize into the workflow/project appropriately.

## Downstream Use

Any suitable adjustment should accept a mask input rather than implementing its own private mask subsystem.

Example:

```text
Image ---------> Local Exposure
                    ^
                    |
Mask -> Feather ----+
```

## Acceptance Criteria

- masks can be produced, combined, and inspected;
- mask nodes can feed arbitrary compatible image nodes;
- mask cache/invalidation works independently;
- painting does not block the UI;
- graph save/reload preserves masks;
- masks can later be produced by AI without special downstream handling.

## Exit Deliverable

A general spatial-data foundation ready for both manual and AI-generated masks.
