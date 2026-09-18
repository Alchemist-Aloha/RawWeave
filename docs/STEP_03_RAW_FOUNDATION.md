# Step 03 — RAW Photography Foundation

## Objective

Make the application a genuine RAW processing environment while keeping RAW stages compatible with the generic graph architecture.

## RAW Decoder Abstraction

Create a decoder interface so the project is not permanently tied to one library.

Initial candidates:

- RawSpeed;
- LibRaw.

The decoder should expose:

```text
sensor dimensions
mosaic data
black/white levels
CFA layout
camera metadata
embedded preview
camera matrices/profiles where available
EXIF
```

## Core Data Types

Add:

```text
raw.Mosaic
raw.CameraProfile
raw.LensProfile
color.SceneLinearRGB
color.DisplayRGB
```

## Color Foundation

Introduce:

- explicit working-space metadata;
- scene-linear float representation;
- display-transform abstraction;
- OCIO integration foundation;
- ICC/LittleCMS compatibility boundary where required.

Do not bury all color conversion inside the viewer.

## RAW Nodes

Prefer nodes/node packs for:

- RAW Decode;
- Black Level;
- White Balance;
- Highlight Reconstruction;
- Demosaic;
- Camera Transform;
- Lens Correction;
- Display Transform.

A convenience `RAW Develop` subgraph may wrap these later.

## Metadata

Expose EXIF and RAW metadata as structured graph data even before logic nodes arrive.

Minimum metadata:

```text
camera
lens
ISO
aperture
shutter
focal length
capture time
orientation
dimensions
```

## Acceptance Criteria

- supported RAW input loads through the graph;
- scene-linear processing survives exposure changes without premature clipping;
- RAW stages can be individually represented in the graph;
- RAW output can be displayed correctly through a display transform;
- EXIF metadata is available to later logic nodes;
- ordinary JPEG/PNG input remains supported through separate input nodes.

## Tests

Use a small curated RAW corpus covering:

- Bayer;
- multiple camera vendors;
- different bit depths;
- orientation;
- highlight clipping;
- unusual black levels.

Tests:

- metadata extraction;
- decode determinism;
- white-balance application;
- demosaic dimensions;
- color transform sanity;
- workflow save/reload with RAW-specific nodes.

## Exit Deliverable

A usable but basic RAW workflow that validates the graph-first processing model on real camera data.
