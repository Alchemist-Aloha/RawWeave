# AGENTS.md

Guidance for coding agents working in the RawWeave repository.

## Project overview

RawWeave is a graph-first, non-destructive RAW photo editor. The desktop application combines:

- a Rust workspace for graph execution, image processing, rendering, RAW decoding, and color;
- first-party node packs built on a common node API;
- a native GPUI Kit/gpui-flow frontend.

The legacy Tauri/React application has been removed at the user's request despite
incomplete GPUI feature parity. See `apps/desktop-gpui/README.md` for remaining gaps.

Preserve the graph-first architecture. Processing belongs in Rust nodes and reusable crates, not in UI components or viewer-only code.

## Repository map

```text
crates/core/             stable core identifiers
crates/image/            image buffers, metadata, views, CPU/GPU resources
crates/rendering/        render cache, regions/tiles, revisions, wgpu helpers
crates/node-api/         node descriptors, values, evaluation context, registry
crates/graph/            graph mutation, validation, evaluation, serialization
crates/project/          editor core and default node-pack registration
crates/raw/              RAW decoder boundary, metadata, profiles, test corpus
crates/color/            scene-linear/display color and transform boundaries
node-packs/core-values/  scalar/value nodes
node-packs/core-image/   ordinary image-processing nodes
node-packs/raw/          RAW development nodes
apps/desktop-gpui/       native GPUI Kit/gpui-flow frontend (separate workspace)
apps/desktop-gpui/assets/editor/  bundled taxonomy and parameter presentation metadata
scripts/build-all-in-one.sh  release native desktop binary
docs/                    staged development plans
test-data/images/        licensed image integration-test corpus
scripts/                 repository validation scripts
```

## Development principles

- Make minimal, focused changes and reuse existing abstractions.
- Follow strict test-driven development for behavior changes: add a failing regression test, verify the failure, implement the smallest fix, then refactor.
- Keep node types, ports, parameter schemas, and persisted identifiers stable. Workflow compatibility is a product requirement.
- Do not place processing logic in the frontend. The frontend edits graphs, requests previews, and presents results.
- Do not send RAW bytes or float image buffers through JSON IPC. Use runtime source state and binary preview transport.
- Treat image and RAW inputs as untrusted. Use checked arithmetic, bounded reads/allocations, validated deserialization, and typed errors rather than panics.
- Preserve scene-linear values until an explicit display transform. Do not clamp highlights in intermediate RAW stages.
- Keep ordinary JPEG/PNG and RAW input paths distinct where their graph values differ.
- Never invent camera/lens calibration data. Mark unavailable profiles honestly until data is sourced and validated.
- Keep CPU and GPU paths behaviorally equivalent within a documented tolerance. GPU absence must produce a real fallback, not simulated availability.

## Documentation and measured improvements

- Read the relevant implementation and current documentation before changing behavior. Keep documentation synchronized with the change; distinguish implemented functionality from plans and known limitations.
- For UI/UX work, consult and update `docs/EDITOR_IMPROVEMENT_LOG.md`. Iterate through a concrete problem, evidence, focused change, running-app verification, regression checks, and recorded result rather than an unsupported redesign.
- For image-loading or rendering performance work, consult and update `docs/LARGE_IMAGE_PERFORMANCE.md`. Profile first, fix the largest measured cost, rerun the same workload, and record files changed, before/after measurements, tests, and remaining concerns.
- Record build mode, source dimensions, region/mip/quality, cache state, sample counts, and timing boundaries. Separate open/decode, first preview, unchanged repeats, parameter edits, and end-to-end display latency. Backend benchmarks and browser adapters do not establish native UI performance.
- Keep benchmark commands reproducible and fixtures deterministic or licensed. Do not turn host-specific timings into CI thresholds, product guarantees, or claims of camera-wide compatibility. Report blocked or failing gates honestly.

The former Tauri large-image benchmark was removed with the legacy app. Historical
measurements in `docs/LARGE_IMAGE_PERFORMANCE.md` do not establish GPUI performance.

## Rust conventions

- The root workspace and native GPUI crate use Rust edition 2024.
- Prefer workspace dependencies in the root `Cargo.toml` for workspace members.
- Avoid `unwrap`, `expect`, unchecked indexing, and unchecked numeric casts in production paths handling files, dimensions, regions, or user data.
- Validate finite floating-point parameters and resource limits before allocating or processing.
- Use shared immutable backing storage for large images. Do not clone full-resolution buffers on parameter changes.
- Region-aware processing must preserve global origins. Full-frame nodes must explicitly request full-frame inputs.
- Cache keys must include every result-affecting dimension: node/input identity, parameters, region/tile, mip, quality, backend identity, and output schema.
- Invalidate only affected downstream results. Do not use global revisions in a way that defeats targeted cache reuse.
- Serialize graph configuration, not runtime image buffers, decoder state, GPU resources, preview bytes, or source-file contents.
- New graph values require stable type IDs plus updates to hashing, matching, serialization behavior, and tests.

## Native frontend conventions

- Keep UI components presentation-focused; session and engine logic must stay testable without a window.
- Filter asynchronous preview results by request ID and graph revision.
- Register preview jobs before spawning workers. Cancellation before worker start must remain cancelled.
- Release obsolete preview textures/bytes and bound preview storage by count and total bytes.
- Loading a workflow clears runtime source state. The next compatible source selection attaches to the loaded graph without rebuilding it.
- A normal Open Image action may construct the appropriate default ordinary or RAW graph.
- Use native file dialogs and preserve validation at filesystem boundaries.
- Bundle presentation-only metadata in `apps/desktop-gpui/assets/editor/`; Rust node constraints remain authoritative.

## Testing

Run focused tests first, then the relevant full gates.

### All-in-one desktop release

From the repository root, run:

```sh
./scripts/build-all-in-one.sh
```

The script builds the native GPUI release binary and copies it to `./bin/rawweave-desktop`. The `bin/` directory is ignored by Git. It does not build Tauri or the web frontend. Use this script when delivering the default desktop executable.

Launch it with `./scripts/run-rawweave.sh [image-path]`; arguments and environment are forwarded unchanged. No WebKitGTK renderer setting is needed for GPUI. Run `python3 scripts/test_desktop_scripts.py` to check the default target and launcher without compiling or opening a window.

### Root Rust workspace

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

### Native GPUI desktop

GPUI is the only desktop target, but is not yet feature-equivalent to the removed
Tauri app. See `apps/desktop-gpui/README.md` for implemented surfaces and remaining
parity gates. GPUI uses native GPU composition with direct BGRA image uploads;
RAW processing remains CPU-based. The legacy backend and WebdriverIO suites were
removed; session/headless tests do not prove native GPU pixels or OS file portals.

```sh
cargo test --locked --manifest-path apps/desktop-gpui/Cargo.toml --workspace
cargo clippy --locked --manifest-path apps/desktop-gpui/Cargo.toml -p rawweave-gpui --all-targets --no-deps -- -D warnings
cargo fmt --manifest-path apps/desktop-gpui/Cargo.toml --all -- --check
```

### Image dataset

```sh
python3 scripts/validate_image_dataset.py
cargo test -p rawweave-raw --test online_dataset
```

Every file under `test-data/images/raw/` or `test-data/images/common/` must have a matching entry in `test-data/images/manifest.json` containing provenance, licensing, byte size, and SHA-256. Only add files whose redistribution terms are compatible with this repository.

## Test expectations

Add regression coverage for every bug fix. Important areas include:

- graph validation, cycles, duplicate inputs, and workflow round trips;
- cache identity, targeted invalidation, revisions, CPU/GPU equivalence;
- non-zero image origins, regional chains, mip/quality propagation;
- malformed or oversized image/RAW inputs and validated deserialization;
- Bayer/X-Trans borders, black/white levels, highlights, color transforms;
- cancellation before/during preview work and stale-result rejection;
- RAW-to-ordinary transitions and workflow source reattachment;
- frontend session behavior independently of GPUI rendering;
- visible frontend workspace behavior in headless GPUI tests and running-app verification for native GPU/portal behavior.

Use deterministic synthetic fixtures for precise algorithm assertions. Use the licensed online-derived dataset only for decoder/loader integration smoke tests.

## Workflow and commits

- Inspect the relevant crates and existing tests before editing.
- Keep unrelated formatting or refactors out of feature commits.
- Run `git diff --check` before committing.
- Do not commit build output, frontend `node_modules`, temporary downloads, or generated previews.
- Use concise imperative commit messages, for example:

```text
fix(graph): preserve regional cache identity
feat(raw): add camera profile transform
test: add open image fixture dataset
```

- Report exactly what changed, which commands passed, and any remaining limitation.

## Current limitations

- The checked-in RAW corpus is intentionally small and permissively licensed; it is not a full camera-compatibility matrix.
- OCIO and ICC/LittleCMS are represented by explicit backend boundaries, not full production integrations yet.
- GPU acceleration currently covers selected operations; every accelerated path must retain a tested CPU fallback.

When a requested change conflicts with these invariants, preserve safety and persisted workflow compatibility, and document the tradeoff explicitly.
