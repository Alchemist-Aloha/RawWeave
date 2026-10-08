# RawWeave native desktop migration

Branch: `feat/gpui-desktop`. This is a working first native frontend, **not complete Tauri feature parity**. The existing Tauri application remains available during migration.

## Run

```sh
cargo run --locked --manifest-path apps/desktop-gpui/Cargo.toml -- \
  test-data/images/common/gracie-allen-portrait.jpg
```

Release build and launch:

```sh
cargo build --release --locked --manifest-path apps/desktop-gpui/Cargo.toml -p rawweave-gpui
./apps/desktop-gpui/target/release/rawweave-gpui
```

The optional command-line argument opens an image. Otherwise use **Open Image**. On Linux this requires a working X11/Wayland session, Vulkan driver, fonts and a desktop file-picker portal. For the local display used during verification: `DISPLAY=:0 GPUI_FORCE_X11=1`.

## Implemented

- GPUI Kit 0.7.1 application, native controls, search and file dialogs; no Tauri or WebView dependency.
- gpui-flow canvas with named typed ports, selection, dragging, pan/zoom, connections and deletion. Canvas moves form one document-history entry per completed gesture; no-op/selection gestures do not. Undo/Redo restores positions, including moves released outside the canvas; invoking history mid-drag finishes the pending move first. Moving nodes does not invalidate processing or request a preview. Mutations pass through the existing graph validator; invalid connections are rejected and the canvas is restored.
- Task-grouped node library shares Tauri's taxonomy (`node-categories.json`), preserves browsing collapses while showing all search matches, supports name/type/category search and Enter-to-add, and defaults to compatible-input filtering when a node is selected. Compatibility uses the graph engine's canonical wildcard/numeric/boolean rules. Typed GPUI library drag-and-drop adds at the pointer in embedded canvas coordinates, including pan/zoom, and one Undo/Redo restores the insertion and position. New image-output nodes also appear in viewer target menus. Unknown families keep honest fallback labels; IDs and processing stay unchanged.
- Registered node library and transactional parameter inspector: exact numeric text commits on Enter/blur, native sliders committing on release, repeated Up/Down text-field steps committing on key release, checkboxes, named choice menus, Reset, units/descriptions and recommended-range notices. Escape discards text drafts; opening an inspector does not round saved float values. Presentation metadata is shared with Tauri via `parameter-ux.json`, including percentage/degree display factors; node constraints remain authoritative. Multiline prompt/expression/workflow/point fields use native auto-growing textareas (Shift+Enter inserts a line, Enter submits without a newline), and shared metadata drives the Show/Hide Advanced group. Parameter input-port exposure/hiding is undoable and persisted, with dynamic canvas handles. Native input text undo is separate from graph history; empty Undo/Redo controls are disabled.
- Tauri-compatible Ctrl/Cmd shortcuts: O loads a workflow, Shift+O opens an image, S saves a workflow, K focuses node search, Z undoes, Shift+Z/Y redoes. Delete/Backspace deletes selected nodes/edges outside text fields. The Shortcuts button / `?` shows inline help; startup gives the editor keyboard focus.
- Independent A/B viewer-output menus list displayable node/port pairs. Explicit choices survive comparison-mode changes and node inspection; deletion/history repair unavailable targets. Single view defaults to Output; comparison defaults to Image Input / Output (RAW: Demosaic / Display Transform). Switching outputs clears the old texture so failures cannot mislabel the previous image.
- Horizontal and stacked A/B, adjustable wipe (A left / B right), 450-ms blink with visible-source label, and absolute display-space difference. Each pane has independent Fit/100%, pan, wheel/button zoom, cancellation and refresh; the active pane determines Export and the global Fit/100% buttons. Hidden pane jobs/textures are released. Wipe/blink use native image composition; difference rasterizes bounded BGRA display surfaces on a background CPU task, with independent transforms, bilinear sampling, alpha/background composition and stale-result rejection. This is a comparison of displayed values, not scene-linear processing or GPU difference computation.
- All eight scope views: Histogram, Waveform, RGB Parade, Vectorscope, False Color, Gamut Warning, Pixel Inspector and Zebra, with hide/show and a named active-viewer source. Shared `rawweave-rendering::scopes` analyzes the displayed BGRA preview on background workers: scope sampling is capped at 100,000 pixels and the inspection raster at 256 pixels on its longest axis. Pixel Inspector reports the center preview pixel from that sampled raster; values are display codes, not RAW/HDR measurements. False Color maps display luma; Gamut Warning is explicitly a display-saturation heuristic, not color-managed gamut detection. Scope textures follow panel/device scale within a 1024-pixel-per-axis limit, and obsolete results are rejected.
- Clipping overlay follows each pane's image through pan, zoom and comparisons. Display-channel maxima ≥98% are marked red; maxima ≤2% are marked blue. Difference includes the same straight-alpha clipping marks. Clip/scopes toggles do not mutate graph state, and stale/hidden textures are released.
- Background full-resolution export of the selected graph output using the existing batch encoders, independently of preview mip/zoom. Filename extensions select PNG (8-bit sRGB), JPEG (quality 92, sRGB), TIFF (16-bit sRGB), or OpenEXR (32-bit linear sRGB). EXR preserves scene-linear highlights; exporting a display-transformed output cannot recover clipped values. Metadata is stripped; recipe controls and profile transforms are not implemented. The export captures graph/source/target when clicked, disables duplicate requests, reports errors separately from previews, and atomically replaces the chosen file only after successful encoding. The desktop save portal may append an extension; the actual chosen path is reported.
- JPEG/PNG and RAW source paths remain distinct. Runtime sources are not serialized. Workflow load clears the source; the next compatible image attaches without rebuilding the saved graph.
- Compatible version-1 frontend workflow envelope (`graph` string and `positions`) and bare engine graph JSON. Atomic workflow saves avoid truncating an existing file on failure.
- Background CPU graph evaluation, shared immutable source buffers, mip-aware/coarse-to-fine previews, obsolete-result rejection, Fit/100%, pan and wheel zoom.
- Direct BGRA preview upload to GPUI's native GPU image atlas. GPUI performs textured image scaling/composition; there is **no PNG encoding, JSON pixel IPC, preview URI fetch or GPU readback** in this display path. Old atlas entries are explicitly released.
- Actual adapter/software status is reported, not simulated. Local Linux verification selected NVIDIA GeForce GTX 1070, NVIDIA driver, `software=false`; a real JPEG was visibly displayed. RAW sampling/bounds are regression-tested. No native frame-time benchmark or camera-wide color claim.

RAW development and color processing remain in Rust nodes on CPU. This is native GPU **display**, not a GPU-resident RAW processing chain and not a restoration of the removed WebGPU demosaic experiment. Uploads are bounded to 128 MiB and 8192 pixels per axis; larger 100% previews require tiled display support.

## Still to port

| Tauri surface | GPUI status / remaining gate |
| --- | --- |
| Graph editing, library, workflow files | Basic native port implemented; parameter-port exposure and gesture-grouped canvas move history implemented; category navigation, compatible filtering, keyboard insertion and library drag/drop implemented; grouping/subgraph composition, richer canvas actions and full native automation remain. |
| Parameter editing | Exact text, sliders, booleans/enums, resets, units/descriptions and parameter ports implemented; multiline widgets and advanced grouping implemented; curves, color/transfer references, parameter suggestions and full native automated coverage remain. |
| Viewer targets and navigation | Independent output selectors, Fit/100%, zoom/pan, refresh/cancel implemented; masks, tiled 100% display and full viewer controls remain. |
| Viewer B / comparison | Independent targets, horizontal/stacked A/B, wipe, blink and display-space difference implemented; persistent viewer-session restoration and automated native integration coverage remain. |
| Scopes and overlays | All eight scope views, scoped pixel readouts, hide/show, bounded sampling and clipping overlays implemented; native automated coverage remains. |
| Crop and masks | Rust nodes usable through parameters; interactive drawing/editing not ported. |
| Browser and image sets | Not ported: thumbnails, folders, selections and source metadata. |
| Export and batch | Single-image preset export implemented; recipe controls, batch queue, progress/cancel/resume and preflight UI remain. |
| Checkpoints and subgraphs | Engine available; management/navigation UI not ported. |
| External hosts / providers | Engine available; connection/provider UI not ported. |
| Workspace and menus | Basic shortcuts/help implemented; panel sizing/persistence, compact-window layout, themes, workspace switching and application menus remain. |
| Native integration gates | Session/engine tests and local X11 smoke verification only; no full GPUI end-to-end suite or Tauri-equivalent coverage. |

Do not remove Tauri or present this frontend as feature-complete until these gates are cleared.

## Check

```sh
cargo test --locked --manifest-path apps/desktop-gpui/Cargo.toml --workspace
cargo clippy --locked --manifest-path apps/desktop-gpui/Cargo.toml \
  -p rawweave-gpui --all-targets --no-deps -- -D warnings
cargo fmt --manifest-path apps/desktop-gpui/Cargo.toml --all -- --check
```

Headless session tests: append `--no-default-features` to the package test command. Existing project and Tauri backend tests still exercise the shared engine. Tauri WebdriverIO tests do not cover GPUI. Whole-workspace Clippy including upstream gpui-flow currently reports upstream style warnings; the owned application passes the scoped command above.

See `vendor/gpui-flow/UPSTREAM.md` for pinning and integration fixes.
