# Step 14 — Professional Photo Node Packs

## Objective

Expand photographic capability without bloating the core.

This phase should mostly create first-party node packs that exercise the same APIs available to third-party developers.

## Implemented scene-linear support

Every registered pro-tools operation and alias accepts either `image` (`core.Image`)
or `scene` (`color.SceneLinearRGB`), not both. Old IDs, parameters and Image
algorithms remain unchanged. Scene inputs are borrowed directly by the existing
kernels rather than copied into a full-resolution RGBA input buffer.

- Detail: denoise, detail separation, deconvolution, sharpen, local contrast/texture.
- Optical: defringe, chromatic aberration, distortion/perspective, vignetting.
- Color: tone mapping, color zones, selective color, channel mixer, perceptual
  saturation, gamut compression, LUT/LUT Tools.
- Creative: film curve/simulation, grain, halation, bloom, dye layer, split toning.
- Analysis: histogram statistics, clipping, noise, sharpness, dynamic range.

Raster results use `scene`; detail separation uses `base_scene` and `detail_scene`.
Analysis retains its scalar/mask ports. Working space and preview sampling survive
all pro raster operations. Since scenes have no regional origin, scene filters
retain the whole sampled raster even when a region is requested. Filter radii are
in raster pixels; grain coordinates use the full-image sampling grid. Clipping
masks use full-image coordinates, including for sampled sources. Clipping statistics
use reference levels 0 and 1, not sensor black/white calibration.

Luminance-based operations use the working space's known Y coefficients. Unknown
camera/custom primaries are rejected by those operations; apply Camera Transform
first. Channelwise/geometric operations retain unknown-space metadata without
inventing a transform. Analysis and mask weights may be bounded even though RGB
outputs remain scene-valued; these are not display transforms.

Scene LUT/Film Curve linearly extrapolate the first/last segment outside authored
point domains rather than clipping input to 0–1. Reinhard remains signed; scene
Filmic/ACES sign-extend their existing shared rational curve, evaluate it in f64,
and omit its hard 0–1 clamp. This is an artistic tone operation, not production
ACES color management. Scene Color Zones temporarily lifts an achromatic negative
offset for HSV evaluation and restores it afterwards, preserving signed/tiny RGB
at neutral settings. Image versions of all these formulas remain unchanged.

Regression tests cover every alias, HDR/negative samples, finite results, working
spaces, ambiguous/wrong inputs, full-grid masks, sampling, legacy Image parity,
and saved RAW → pro detail → mask/local exposure → Levels/Curves/Film → Output
workflows with repeated mip cache reuse. GPUI reads the authoritative registry and
its connected-input helper follows both Image and scene wires.

## Candidate Node Packs

### Detail

- advanced denoise;
- wavelet/detail separation;
- deconvolution;
- advanced sharpening;
- texture/local contrast.

### Optical

- defringe;
- chromatic aberration;
- distortion correction;
- vignetting;
- perspective correction.

### Color

- advanced tone mapping;
- color zones;
- selective color;
- channel mixing;
- perceptual saturation;
- gamut compression;
- LUT tools.

### Film / Creative

- film curves;
- grain;
- halation;
- bloom;
- dye-layer simulation;
- split toning.

### Analysis

- histogram statistics;
- clipping analysis;
- noise estimate;
- sharpness estimate;
- dynamic range estimate.

These analysis nodes should output connectable control values where useful.

## Plugin Discipline

For each first-party node ask:

- can this be implemented through public node API?
- does it require a new generic core capability?
- is that capability genuinely generic?
- can other node packs use it?

Avoid adding private shortcuts merely for first-party nodes.

## Performance

Profile:

- tile boundaries;
- GPU occupancy;
- unnecessary conversions;
- memory reuse;
- cache pressure;
- mixed CPU/GPU pipelines.

## Acceptance Criteria

- representative professional workflow can be assembled entirely from node packs;
- first-party nodes do not depend on undocumented privileged APIs;
- workflows remain shareable with dependency metadata;
- performance remains interactive on realistic large RAW files.

## Exit Deliverable

A competitive photographic toolset built on top of, rather than inside, the graph platform.
