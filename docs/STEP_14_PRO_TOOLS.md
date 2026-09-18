# Step 14 — Professional Photo Node Packs

## Objective

Expand photographic capability without bloating the core.

This phase should mostly create first-party node packs that exercise the same APIs available to third-party developers.

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
