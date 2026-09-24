# AGENTS.md

Guidance for coding agents working in the RawWeave repository.

## Project overview

RawWeave is a graph-first, non-destructive RAW photo editor. The desktop application combines:

- a Rust workspace for graph execution, image processing, rendering, RAW decoding, and color;
- first-party node packs built on a common node API;
- a Tauri 2 desktop boundary;
- a React 19, TypeScript, Vite, and React Flow frontend.

Preserve the graph-first architecture. Processing belongs in Rust nodes and reusable crates, not in React components or viewer-only code.

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
apps/desktop/src-tauri/  Tauri commands, source state, previews, URI protocol
apps/desktop/frontend/   React editor, viewer, controllers, platform adapters
scripts/build-all-in-one.sh  release desktop binary with embedded frontend
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

## Rust conventions

- The root workspace uses Rust edition 2024; the Tauri crate currently uses edition 2021.
- Prefer workspace dependencies in the root `Cargo.toml` for workspace members.
- Avoid `unwrap`, `expect`, unchecked indexing, and unchecked numeric casts in production paths handling files, dimensions, regions, or user data.
- Validate finite floating-point parameters and resource limits before allocating or processing.
- Use shared immutable backing storage for large images. Do not clone full-resolution buffers on parameter changes.
- Region-aware processing must preserve global origins. Full-frame nodes must explicitly request full-frame inputs.
- Cache keys must include every result-affecting dimension: node/input identity, parameters, region/tile, mip, quality, backend identity, and output schema.
- Invalidate only affected downstream results. Do not use global revisions in a way that defeats targeted cache reuse.
- Serialize graph configuration, not runtime image buffers, decoder state, GPU resources, preview bytes, or source-file contents.
- New graph values require stable type IDs plus updates to hashing, matching, serialization behavior, and tests.

## Frontend and Tauri conventions

- Keep native calls behind `EditorPlatform` and `PreviewTransport` adapters so controllers remain testable in memory.
- Controllers own editor/viewer state transitions; React components should remain presentation-focused.
- Filter asynchronous preview events by request ID and graph revision.
- Register preview jobs before spawning workers. Cancellation before worker start must remain cancelled.
- Release preview URLs/bytes and bound preview storage by count and total bytes.
- Loading a workflow clears runtime source state. The next compatible source selection attaches to the loaded graph without rebuilding it.
- A normal Open Image action may construct the appropriate default ordinary or RAW graph.
- Use the official Tauri dialog plugin for filesystem path selection. Keep capabilities least-privileged.
- Do not rely on browser-only `File.path` behavior in the Tauri application.
- Keep `apps/desktop/src-tauri/gen/schemas/` synchronized when plugin capabilities change.

## Testing

Run focused tests first, then the relevant full gates.

### All-in-one desktop release

From the repository root, run:

```sh
./scripts/build-all-in-one.sh
```

The script uses pnpm to build the frontend, builds the Tauri release binary with `custom-protocol`, and copies it to `./bin/rawweave-desktop`. The `bin/` directory is ignored by Git. Use this script when delivering a desktop executable so the binary embeds the current frontend assets.

On Linux systems affected by WebKitGTK's DMABUF renderer, launch the built binary with `./scripts/run-rawweave.sh`. This sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` only for RawWeave and preserves an explicit value already set by the caller.

### Root Rust workspace

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

### Tauri desktop backend

```sh
cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features
cargo clippy --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
```

### Frontend

```sh
cd apps/desktop/frontend
pnpm install --frozen-lockfile
pnpm test
pnpm run build
```

Do not force dependency upgrades through an audit fix; breaking changes require deliberate review.

### Desktop end-to-end tests

Run these from `apps/desktop/frontend` after relevant frontend or desktop changes:

```sh
pnpm run test:e2e:browser
pnpm run build:e2e:native
pnpm run test:e2e:native
```

- Browser mode starts Vite and uses in-memory platform adapters in headless Chrome. Cover each workspace surface and its relevant controls here for fast UI regression checks.
- The native suite runs the debug Tauri binary through `@wdio/tauri-service`. Use it for Rust command bridge, file/source restoration, preview protocol, image display, scopes, and other behavior that depends on the real WebView or backend.
- Rebuild the native E2E binary after changing Rust, Tauri configuration, or frontend code that the binary embeds. Keep the `wdio-e2e` plugins and permissions gated to E2E builds.
- Browser mode does not prove native integration, and component tests do not prove the actual display path. Add a native regression when fixing a native-only bug.
- See `apps/desktop/frontend/e2e/README.md` for setup, coverage, and runner notes.

### Image dataset

```sh
python3 scripts/validate_image_dataset.py
cargo test -p rawweave-raw --test online_dataset
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml downloaded_common_image_dataset_decodes
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
- frontend controller behavior independently of React rendering.
- visible frontend workspace behavior in WebdriverIO browser mode and backend-dependent preview behavior in the native Tauri suite.

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
