# Example workflows

Ready-to-open graph documents using only built-in nodes. No source paths, image bytes, AI services, external plugins or calibration data are embedded.

## Use

1. In RawWeave, choose **Open Workflow** and select one of these JSON files.
2. Choose **Open Image** and select a compatible source: JPEG/PNG for the ordinary workflows, or a supported RAW file for RAW development. The source attaches to the loaded graph.
3. Preview the final **Output** node (`99-output`), or **Display Transform** (`99-display`) for RAW. Edit the intermediate nodes to tune the result.

Load the workflow before selecting its image. To switch examples, open another workflow and select its source again. These are editable starting points, not camera-specific presets or export recipes; opening them does not write an output image.

| File | Input | What it demonstrates |
| --- | --- | --- |
| [basic-tone.json](basic-tone.json) | JPEG/PNG | Exposure +0.35 EV followed by levels with black/white points 0.02/0.98 |
| [monochrome.json](monochrome.json) | JPEG/PNG | A color matrix with identical RGB rows (0.2126, 0.7152, 0.0722), preserving alpha |
| [highlight-mask.json](highlight-mask.json) | JPEG/PNG | Luminance mask drives local exposure −0.7 EV: brighter pixels receive more darkening |
| [web-resize.json](web-resize.json) | JPEG/PNG | Fixed 1200×800 output; change width/height to match your source aspect ratio to avoid stretching |
| [film-look.json](film-look.json) | JPEG/PNG | Film curve at 65% strength and subtle monochrome grain at 2%, with a fixed seed |
| [raw-development.json](raw-development.json) | RAW | Complete decode → black level → white balance → highlight reconstruction → demosaic → camera transform → lens correction → display transform chain |
| [expression-exposure.json](expression-exposure.json) | JPEG/PNG | Multiply requested EV by a strength control, clamp to −2…+2 EV, and drive an exposed Exposure parameter |
| [conditional-highlights.json](conditional-highlights.json) | JPEG/PNG | Compare requested EV with zero; AND with an enable toggle; Switch between that EV and zero before masked local exposure |
| [named-look-router.json](named-look-router.json) | JPEG/PNG | Enum Select translates a look name to an integer; Select lazily routes the unchanged, monochrome or film-curve image |
| [iso-adaptive-raw.json](iso-adaptive-raw.json) | RAW with ISO metadata | RAW camera metadata → scalar Curve → exposed highlight-recovery strength, retaining the full RAW development chain |

### Graphs

```text
Basic tone:     Image Input → Exposure → Levels → Output
Monochrome:     Image Input → Color Matrix → Output
Highlight mask: Image Input ─────────────────→ Local Exposure → Output
                     └→ Luminance Mask ─mask→       ↑
Web resize:     Image Input → Resize → Output
Film look:      Image Input → Film Curve → Grain → Output
RAW:            RAW Decode → Black Level → White Balance → Highlight Reconstruction
                                                                ↓
                Display Transform ← Lens Correction ← Camera Transform ← Demosaic
                RAW Decode also supplies camera/lens profiles to their transforms.
```

## Advanced logic walkthroughs

### Expression-driven exposure

```text
10-ev ───────a→ Expression (a * b) → Clamp (−2…+2) ─exposure→ Exposure → Output
11-strength ─b→                                           Image Input ─image→ ↑
```

- Edit `10-ev.value` (starts at +1.5 EV) and `11-strength.value` (starts at 0.5). The initial applied exposure is +0.75 EV.
- Try +10 EV: the product is +5 but Clamp limits the applied result to +2. Try −10: the applied result is −2.
- The connected `40-exposure.exposure` input overrides its stored 0-EV literal. Disconnect Clamp to restore that literal; changing the literal while connected does not override the logic.

### Conditional highlight darkening

```text
Requested EV < 0 ─condition→ AND ← Enable
                            ↓
Requested EV ─true→ Switch ←false─ Zero
                      ↓ exposure
Image Input → Local Exposure → Output
      └─────→ Luminance Mask ─mask→ ↑
```

- `11-enabled.value` is the master toggle. `12-requested-ev.value` starts at −0.7 EV.
- Darkening runs only when enabled **and** the requested value is negative. Disabling the toggle or entering a positive EV routes zero, leaving the image unchanged.
- Switch controls the exposure amount; the mask and Local Exposure still evaluate at zero. For lazy image processing, see the named look example.
- The luminance mask gives brighter pixels more of the adjustment. This is a teaching example, not an automatic clipped-highlight detector or a way to recover lost JPEG detail.

### Named look routing

```text
20-look (string) → Enum Select → integer index → Select → Output
                                             ↑ a: unchanged Image Input
                                             ↑ b: Color Matrix (monochrome)
                                             ↑ c: Film Curve (65% strength)
```

- Set `20-look.value` to exactly `neutral`, `monochrome` or `film` (case-sensitive). The default is `film`.
- Enum Select maps those names to 0, 1 and 2. Unknown names report an error; input D is intentionally unused.
- Enum Select currently evaluates its connected inputs eagerly, so its inputs here are only inexpensive integer constants. The following Select is lazy: it evaluates only the selected image branch. This distinction avoids doing every look's processing before choosing one.
- Edit `30-film` to shape the creative film look; its point curve is not a calibrated film-stock profile.

### ISO-adaptive RAW development

```text
RAW Decode ─camera→ Metadata ─iso→ Curve ─strength→ Highlight Reconstruction
      └──────────── existing RAW development chain ─────────────────→ Display Transform
```

- `15-recovery.points` maps ISO 100 → 1.0, 1600 → 0.9, 6400 → 0.75 and 25600 → 0.5; values beyond the endpoints hold the endpoint strength.
- These are illustrative control values, not camera-specific recommendations, noise reduction or exposure correction. Lower recovery strength compresses samples above the reconstruction threshold more; tune the threshold and curve for your own source.
- This example requires available ISO metadata. Missing ISO reports an error for the absent ISO value rather than inventing capture data. Disconnect the curve from `30-highlight.strength` to use its stored strength 1.0 and the ordinary RAW development chain.
- Preview `99-display`, not `99-output`. The source remains RAW/scene-linear until the final display transform; no ordinary-image adapter or calibration is invented.

## Boundaries

- The ordinary workflows use `core.Image`; they are not substitutes for a scene-linear RAW chain. Do not connect RAW scene/display ports to ordinary image ports without a compatible conversion.
- The highlight-mask example darkens bright areas; it cannot recover detail already clipped in a JPEG/PNG.
- The film look is a creative curve and grain example, not a calibrated film-stock simulation.
- RAW profiles depend on the selected file and available decoder/profile data. Missing profiles remain unavailable; the example supplies no invented camera or lens calibration. The explicit display transform stays last.
- Files use the current native saved-graph format, including registered node descriptors. They contain graph configuration only; node positions are arranged by the editor on load.

## Validation

From the repository root:

```sh
cargo test --locked -p rawweave-project --test example_workflows
```

The test loads and round-trips all ten examples through the real editor core, renders ordinary examples against a deterministic image, checks monochrome and resize outputs, and renders the RAW chains with the deterministic test decoder. Additional assertions check exposure limits/literal preservation, enable/negative-EV conditions, all named looks, lazy image-branch evaluation, unknown-name errors, ISO-driven parameter values and recovery to the stored literal when ISO is missing. ISO values supplied by the tests are synthetic metadata only and are not embedded in the workflows. Synthetic RAW coverage is not a camera-compatibility claim.
