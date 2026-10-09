# Step 03 — RAW Photography Foundation

## Objective

Make the application a genuine RAW processing environment while keeping RAW stages compatible with the generic graph architecture.

## RAW Decoder Abstraction

Create a decoder interface so the project is not permanently tied to one library.

### Implemented decoder

The default `RawlerDecoder` uses [rawler 0.8 / dnglab](https://github.com/dnglab/dnglab).
Its pure-Rust backend supports modern compressed Fujifilm RAF and Canon CR2/CR3,
including C-RAW, while returning sensor mosaics rather than developed display RGB.
Legacy `RawloaderDecoder`, `RawLoaderDecoder`, and `RawloaderAdapter` names alias
the new default, so existing callers and persisted graph identifiers do not change.

Compared alternatives: LibRaw provides broad camera coverage but adds a native
library/FFI build dependency; RawSpeed also needs a native integration. Rawler fits
the existing Rust sensor-data boundary with less integration overhead.

Encoded-size and existing container-header limits remain enforced. A dummy sensor
decode validates actual dimensions/components before pixel decompression, including
CR3 and TIFFs whose primary EXIF dimensions describe a preview. The vendor parser
is not a sandbox: these limits do not bound every metadata allocation.

A narrow fallback to rawloader remains when RAF dummy decoding panics: rawler 0.8
accesses uninitialized pixels on rotated legacy Fuji Super CCD sensors (reproduced
with the FinePix S5000 fixture). Remove this fallback after an upstream fix.
Unsupported non-mosaic/extra-channel sensors still return typed errors.
Spatial black levels are averaged into the existing RGB channel schema (including
both green sites); spatially varying correction and sensor active-area cropping
are not newly implemented. Camera matrices use supplied D65 calibration, or a
deterministically selected supplied illuminant; none are fabricated.

Regression corpus: CC0 X-T4 lossless-compressed X-Trans RAF, EOS Kiss F CR2,
EOS-1D X Mark III CR3 RAW and C-RAW, plus the existing Nikon/Sony/legacy Fuji files.
This establishes those specific fixtures, not camera-wide compatibility. Run:

```sh
python3 scripts/validate_image_dataset.py
cargo test --locked -p rawweave-raw --all-targets
```

**Licensing:** rawler is LGPL-2.1 (not MIT like the workspace).
Distributing linked executables requires satisfying its license, including applicable
source and relinking requirements for static linking. See its
[license](https://github.com/dnglab/dnglab/blob/main/rawler/LICENSE).
The Rust adapter does not change the dependency's license.

Validation: the new camera regression failed first with rawloader's unsupported
X-T4 error, then passed with rawler. Root workspace tests (341) and clippy,
Tauri check/tests (82 passed, 2 ignored) and clippy, GPUI workspace tests (46)
and application clippy, root formatting, and the 15-file checksum validator passed.
No native display/E2E or image-quality equivalence claim is made by these decoder tests.

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

## Scene-linear interoperability (implemented)

Exposure, Local Exposure, Blur, Resize, Color Matrix, Levels, Curves, Invert, and
Output expose optional `scene` input/output ports of type `color.SceneLinearRGB`,
alongside existing `image` ports. All pro-tools operations and aliases also accept
scene RGB, including detail/optical/color/film filters and analysis. Detail
Separation produces `base_scene` and `detail_scene`; analysis keeps its existing
scalar/mask outputs. Luminance, Color Qualifier, Painted, Linear Gradient and
Radial Gradient masks accept scene sources and preserve full-image mask bounds. Connect one input family, not both. Only its corresponding output
is produced; scene samples retain their working space, finite validation, negative
values, and highlights above 1. Existing node IDs, versions, image algorithms,
parameter aliases, and saved image connections remain compatible.

```text
RAW Decode → Black Level → White Balance → Demosaic → Camera Transform
→ Exposure (scene → scene) → Display Transform
```

To enter ordinary image-only nodes, use `core.scene-linear-to-image`
(**Scene Linear RGB to Image**), connecting `scene` to its `image` output.
It uses the existing matrix transform to linear sRGB, adds straight alpha 1,
and declares `ColorDomain::LinearSrgb`. There is no gamma encoding or clamping.
Camera-native/custom spaces without a known transform are rejected rather than
mislabeled: connect Camera Transform with its supplied profile first.

Scene values currently have no regional origin. Scene processing therefore retains
the whole raster even when evaluation requests a region; the existing Image paths
remain region-aware. Sampling metadata survives pointwise operations, Blur, and
Output. Local Exposure samples masks in full-image coordinates on the preview grid.
Blur radius is in raster pixels. Resize uses the existing nearest-neighbor policy
and creates a new full-size coordinate grid, clearing prior sampling metadata.
The conversion node requests full-frame, mip-invariant upstream evaluation and
rejects directly supplied sampled previews so full-size geometry is not lost.
Its full-resolution conversion can be more expensive than the typed RAW preview path.

Color Matrix uses real CPU evaluation for scene RGB and rejects alpha-row edits
because the scene type has no alpha channel. Its existing RGBA GPU path is unchanged.
Levels and Curves extend their gamma response with signed powers and no 0–1
clamp on scene inputs; Invert uses reference-white subtraction (`1 - RGB`).
Their Image formulas are unchanged. Pro LUT/Film Curve extrapolate endpoint
slopes for out-of-domain scene samples; tone mapping uses signed analytic curves
without a hard display clamp. See `STEP_14_PRO_TOOLS.md` for exact boundaries.

Crop remains image-only because scene RGB cannot preserve its global crop origin.
ImageSet members and AI/provider/external-host payloads remain Image-typed: their
existing storage/normalized-RGB/provider contracts are not scene-linear contracts.
Sensor mosaics still require dedicated RAW stages. Luma-based scene operations
use known working-space Y weights and reject camera-native/custom spaces without
a known transform; geometry and channelwise operations need no guessed primaries.

Regression coverage includes unbounded linear values, working spaces, sampling,
full-origin masks, control errors, alpha-loss rejection, ordinary Image behavior,
RAW scene processing/workflow round trips, full-resolution conversion under mip
requests, and repeat cache reuse with alternative typed outputs.

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
