# RawWeave

An open-source, cross-platform desktop RAW photo editor where the **workflow graph
is the document and the primary interface**.

Typed nodes are the editing surface, not a hidden advanced panel. One workflow
serves a single image, an image set, and a batch job; parameters can be driven by
metadata, expressions, and logic; masks are ordinary graph data. Results stay
scene-linear until an explicit display transform, and every edit is a parameter
that can be replayed years later from a versioned workflow file.

RawWeave is not an open-source Lightroom clone, not "GIMP with nodes", and not
another ComfyUI frontend. Conceptual neighbors: ComfyUI-style composition,
DaVinci Resolve/Fusion-style visual processing, professional RAW color
management, and Natron/GEGL/OFX-style extensibility.

## Status

Pre-1.0, no tagged release. The 15 roadmap steps (backbone, rendering, RAW
foundation, logic, subgraphs, browser/queue, masks, batch, external hosts,
checkpoints, AI providers/masks, image sets, pro tools, UI pass) are implemented
and covered by tests, but nothing has been validated against a wide camera
matrix or real user workflows. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the
milestone definitions.

**GPUI is the default desktop application.** Its native graph editor, RAW viewer,
and export path do not use Tauri or a WebView. Desktop feature parity is incomplete;
see [`apps/desktop-gpui/README.md`](apps/desktop-gpui/README.md) for remaining gates.
The legacy Tauri application is retained for migration coverage, not built by the
default desktop scripts.

## Architecture

Processing lives in Rust nodes and reusable crates. The frontend edits graphs,
requests previews, and presents results — it never computes pixels. RAW bytes and
float image buffers never travel over JSON IPC.

```text
crates/core/             stable core identifiers
crates/image/            image buffers, metadata, views, CPU/GPU resources
crates/rendering/        render cache, regions/tiles, revisions, wgpu helpers
crates/node-api/         node descriptors, values, evaluation context, registry
crates/graph/            graph mutation, validation, evaluation, serialization
crates/project/          editor core and default node-pack registration
crates/raw/              RAW decoder boundary, metadata, profiles
crates/color/            scene-linear/display color and transform boundaries
crates/batch/            batch jobs, output recipes, resumable execution
crates/external-protocol/ out-of-process node host protocol
crates/external-host/    external host supervision
crates/ai-provider/      AI provider adapters

node-packs/core-values/  scalars, metadata, expressions, comparison, switch/select
node-packs/core-image/   resize, crop, blur, levels, curves, exposure, invert, panorama
node-packs/raw/          decode, black/white levels, white balance, highlight recovery,
                         demosaic, camera transform, lens correction, display transform
node-packs/pro-tools/    denoise, sharpen, deconvolution, defringe, LUT, grain, bloom,
                         halation, vignetting, distortion, clipping, histogram
node-packs/ai/           img2img, inpaint, upscale (via provider adapters)

apps/desktop-gpui/       default native GPUI desktop (separate workspace)
apps/desktop/src-tauri/  legacy Tauri boundary retained during migration
apps/desktop/frontend/   legacy React editor and shared presentation assets
```

The first-party node packs use the same node API intended for third parties. If a
function can reasonably be a node or node pack, it is not special-cased into
application core.

## Getting started

Prerequisites: Rust 1.89 or newer and the native GPUI platform dependencies.
On Linux, use a working X11/Wayland session, Vulkan driver, fonts, and a desktop
file-picker portal. Node.js, pnpm, WebKitGTK, and a Vite server are not needed for
the default desktop build.

```sh
cargo run --locked --manifest-path apps/desktop-gpui/Cargo.toml -p rawweave-gpui -- \
  test-data/images/common/gracie-allen-portrait.jpg
```

Without a path argument, use **Open Image**. See the
[native desktop README](apps/desktop-gpui/README.md) for platform notes and
implemented surfaces.

### Example workflows

Six ready-to-open graphs are available in [`examples/workflows`](examples/workflows/README.md):
basic tone, monochrome, highlight masking, web resize, film look, and RAW development.
Open a workflow JSON first, then choose a compatible image to attach to it.

### Release build

```sh
./scripts/build-all-in-one.sh      # -> ./bin/rawweave-desktop
./scripts/run-rawweave.sh          # launch the native GPUI application
```

The script builds the GPUI release binary and copies it to the stable
`bin/rawweave-desktop` path (git-ignored). No Tauri backend or web frontend is
built. The launcher forwards image paths and caller environment unchanged.

## Testing

```sh
# Rust workspace
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

# Default native desktop
cargo test --locked --manifest-path apps/desktop-gpui/Cargo.toml --workspace
cargo clippy --locked --manifest-path apps/desktop-gpui/Cargo.toml \
  -p rawweave-gpui --all-targets --no-deps -- -D warnings
python3 scripts/test_desktop_scripts.py

# Legacy migration coverage (not the default app)
# Tauri backend
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features

# Frontend
cd apps/desktop/frontend && pnpm test && pnpm run build

# Desktop end-to-end (browser mode is fast; native proves real WebView/backend paths)
cd apps/desktop/frontend
pnpm run test:e2e:browser
pnpm run build:e2e:native && pnpm run test:e2e:native
```

The image corpus under `test-data/images/` is deliberately small and
permissively licensed, with provenance and SHA-256 recorded in
`test-data/images/manifest.json`; validate it with
`python3 scripts/validate_image_dataset.py`.

## Documentation

| File | Contents |
| --- | --- |
| [`docs/SPEC.MD`](docs/SPEC.MD) | Full product and architecture specification; the authority for scope and behavior |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | 15-step plan and public milestone definitions |
| `docs/STEP_01`–`STEP_15` | Per-step deliverables |
| [`PRODUCT.md`](PRODUCT.md) | Product positioning, users, principles, and evidence on hand |
| [`AGENTS.md`](AGENTS.md) | Engineering invariants, conventions, and gates |

## Contributing

Read [`AGENTS.md`](AGENTS.md) before changing anything. The invariants that
reviews check hardest:

- workflow compatibility is a product promise — node types, ports, parameter
  schemas, and persisted identifiers stay stable;
- no processing logic in the frontend, and no RAW/float buffers through JSON IPC;
- scene-linear values are never clamped in intermediate stages;
- GPU paths must retain a real, tested CPU fallback;
- camera and lens calibration data is never invented — unavailable profiles are
  reported as unavailable;
- behavior changes come with a regression test, and bug fixes with the test that
  would have caught them.

Use concise imperative commit messages, e.g. `fix(graph): preserve regional cache
identity`.

## License

Not settled. `PRODUCT.md` records the license as AGPL (version unpinned) while the
workspace manifests still declare `MIT`, and no `LICENSE` file is committed.
Pick one before treating this repository as distributable.
