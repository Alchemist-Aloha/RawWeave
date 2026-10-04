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

The test loads and round-trips every example through the real editor core, renders ordinary examples against a deterministic image, checks monochrome and resize outputs, and renders the RAW chain with the deterministic test decoder. Synthetic RAW coverage is not a camera-compatibility claim.
