# RawWeave native desktop migration

Branch: `feat/gpui-desktop`. This is a working first native frontend, **not complete Tauri feature parity**. The existing Tauri application remains available during migration.

## Run

```sh
cargo run --locked --manifest-path apps/desktop-gpui/Cargo.toml -- \
  test-data/images/common/gracie-allen-portrait.jpg
```

The optional command-line argument opens an image. Otherwise use **Open Image**. On Linux this requires a working X11/Wayland session, Vulkan driver, fonts and a desktop file-picker portal. For the local display used during verification: `DISPLAY=:0 GPUI_FORCE_X11=1`.

## Implemented

- GPUI Kit 0.7.1 application, native controls, search and file dialogs; no Tauri or WebView dependency.
- gpui-flow canvas with named typed ports, selection, dragging, pan/zoom, connections and deletion. Mutations pass through the existing graph validator; invalid connections are rejected and the canvas is restored.
- Registered node library, typed parameter fields committing on Enter/blur, graph/parameter undo and redo buttons. Native input text undo is separate.
- JPEG/PNG and RAW source paths remain distinct. Runtime sources are not serialized. Workflow load clears the source; the next compatible image attaches without rebuilding the saved graph.
- Compatible version-1 frontend workflow envelope (`graph` string and `positions`) and bare engine graph JSON. Atomic workflow saves avoid truncating an existing file on failure.
- Background CPU graph evaluation, shared immutable source buffers, mip-aware/coarse-to-fine previews, obsolete-result rejection, Fit/100%, pan and wheel zoom.
- Direct BGRA preview upload to GPUI's native GPU image atlas. GPUI performs textured image scaling/composition; there is **no PNG encoding, JSON pixel IPC, preview URI fetch or GPU readback** in this display path. Old atlas entries are explicitly released.
- Actual adapter/software status is reported, not simulated. Local Linux verification selected NVIDIA GeForce GTX 1070, NVIDIA driver, `software=false`; a real JPEG was visibly displayed. RAW sampling/bounds are regression-tested. No native frame-time benchmark or camera-wide color claim.

RAW development and color processing remain in Rust nodes on CPU. This is native GPU **display**, not a GPU-resident RAW processing chain and not a restoration of the removed WebGPU demosaic experiment. Uploads are bounded to 128 MiB and 8192 pixels per axis; larger 100% previews require tiled display support.

## Still to port

Viewer B/comparison, scopes, crop/mask drawing, rich parameter controls, dock/panel persistence, browser/thumbnail workflows, batch/export UI, checkpoints/subgraph navigation, external hosts, menus/shortcuts and full native integration coverage. Canvas moves are persisted but are not yet separate undo transactions. Do not remove Tauri or present this frontend as feature-complete until those gates are cleared.

## Check

```sh
cargo test --locked --manifest-path apps/desktop-gpui/Cargo.toml --workspace
cargo clippy --locked --manifest-path apps/desktop-gpui/Cargo.toml \
  -p rawweave-gpui --all-targets --no-deps -- -D warnings
cargo fmt --manifest-path apps/desktop-gpui/Cargo.toml --all -- --check
```

Headless session tests: append `--no-default-features` to the package test command. Existing project and Tauri backend tests still exercise the shared engine. Tauri WebdriverIO tests do not cover GPUI. Whole-workspace Clippy including upstream gpui-flow currently reports upstream style warnings; the owned application passes the scoped command above.

See `vendor/gpui-flow/UPSTREAM.md` for pinning and integration fixes.
