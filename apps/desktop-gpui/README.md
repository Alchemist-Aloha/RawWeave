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
- Tauri-compatible Ctrl/Cmd shortcuts: O loads a workflow, Shift+O opens an image, S saves a workflow, K expands Nodes and focuses search, A selects all graph nodes/links outside text fields, Z undoes, Shift+Z/Y redoes. Delete/Backspace deletes selected nodes/edges outside text fields. View → Keyboard shortcuts / `?` shows inline help; startup and dock collapse give the editor keyboard focus.
- Independent A/B viewer-output menus list displayable node/port pairs. Explicit choices survive comparison-mode changes and node inspection; deletion/history repair unavailable targets. Single view defaults to Output; comparison defaults to Image Input / Output (RAW: Demosaic / Display Transform). Switching outputs clears the old texture so failures cannot mislabel the previous image.
- Horizontal and stacked A/B, adjustable wipe (A left / B right), 450-ms blink with visible-source label, and absolute display-space difference. Each pane has independent Fit/100%, pan, wheel/button zoom, cancellation and refresh; the active pane determines Export and the global Fit/100% buttons. Hidden pane jobs/textures are released. Wipe/blink use native image composition; difference rasterizes bounded BGRA display surfaces on a background CPU task, with independent transforms, bilinear sampling, alpha/background composition and stale-result rejection. This is a comparison of displayed values, not scene-linear processing or GPU difference computation.
- All eight scope views: Histogram, Waveform, RGB Parade, Vectorscope, False Color, Gamut Warning, Pixel Inspector and Zebra, with hide/show and a named active-viewer source. Shared `rawweave-rendering::scopes` analyzes the displayed BGRA preview on background workers: scope sampling is capped at 100,000 pixels and the inspection raster at 256 pixels on its longest axis. Pixel Inspector reports the center preview pixel from that sampled raster; values are display codes, not RAW/HDR measurements. False Color maps display luma; Gamut Warning is explicitly a display-saturation heuristic, not color-managed gamut detection. Scope textures follow panel/device scale within a 1024-pixel-per-axis limit, and obsolete results are rejected.
- Clipping overlay follows each pane's image through pan, zoom and comparisons. Display-channel maxima ≥98% are marked red; maxima ≤2% are marked blue. Difference includes the same straight-alpha clipping marks. Clip/scopes toggles do not mutate graph state, and stale/hidden textures are released.
- Spatial viewer outputs: Mask/MaskSet, LabelMap, ConfidenceMap, DepthMap and RegionSet, alongside ordinary/scene/display images. Masks offer independent grayscale/colored-alpha modes in A and B; confidence stays grayscale, labels use categorical colors, depth is min/max-normalized (flat maps use midgray), and regions show colored borders/fills. Non-zero origins are sampled in global coordinates; each output is fitted to its own local extent. Sparse unions are sampled directly into a budget-checked mip upload rather than allocating their full bounding raster. Empty MaskSet errors; empty RegionSet is transparent. These are visualization values, not exportable image outputs or interactive mask drawing.
- Background full-resolution export of the selected graph output using the existing batch encoders, independently of preview mip/zoom. Filename extensions select PNG (8-/16-bit sRGB), JPEG (adjustable quality, sRGB), TIFF (16-bit sRGB), or OpenEXR (32-bit linear sRGB). Export settings offer original resolution or a bounded 1–8192-pixel long edge, PNG compression and Off/Low output sharpening. The shared encoder applies resizing/sharpening after mip-0 evaluation. EXR preserves scene-linear highlights; exporting a display-transformed output cannot recover clipped values. Metadata is stripped; custom profiles, exact dimensions, other sharpening recipes and batch management are not implemented. The export captures graph/source/target when clicked, disables duplicate requests, reports errors separately from previews, and atomically replaces the chosen file only after successful encoding. The desktop save portal may append an extension; the actual chosen path is reported.
- JPEG/PNG and RAW source paths remain distinct. Runtime sources are not serialized. Workflow load clears the source; the next compatible image attaches without rebuilding the saved graph.
- Compatible version-1 frontend workflow envelope (`graph` string and `positions`) and bare engine graph JSON. Atomic workflow saves avoid truncating an existing file on failure.
- Export selected canvas nodes as an engine-compatible blueprint (version `1.0.0`, filename supplies ID/name), retaining exposed parameters and crossing input/output declarations. Instantiate a validated blueprint as the active graph; successful import clears runtime sources/history/layout and waits for a compatible source, while failed import preserves the current document. These actions do not retain a blueprint-authoring session, provide workflow-level boundary bindings or create/navigate nested subgraphs; those remain migration gaps.
- Background CPU graph evaluation, shared immutable source buffers, mip-aware/coarse-to-fine previews, obsolete-result rejection, Fit/100%, pan and wheel zoom.
- Direct BGRA preview upload to GPUI's native GPU image atlas. GPUI performs textured image scaling/composition; there is **no PNG encoding, JSON pixel IPC, preview URI fetch or GPU readback** in this display path. Old atlas entries are explicitly released.
- Actual adapter/software status is reported, not simulated. Local Linux verification selected NVIDIA GeForce GTX 1070, NVIDIA driver, `software=false`; a real JPEG was visibly displayed. RAW sampling/bounds are regression-tested. No native frame-time benchmark or camera-wide color claim.

RAW development and color processing remain in Rust nodes on CPU. This is native GPU **display**, not a GPU-resident RAW processing chain and not a restoration of the removed WebGPU demosaic experiment. Uploads are bounded to 128 MiB and 8192 pixels per axis; larger 100% previews require tiled display support.

## Workflow UX comparison

| Tauri Workflow behavior | Native GPUI implementation / boundary |
| --- | --- |
| Empty graph arrival and obvious next action | Empty workflow at startup; Open Image constructs the appropriate source graph. Canvas instructions and node-library search provide an authoring alternative. |
| Resizable/collapsible handling and judging docks | Native divider dragging and View-menu keyboard size adjustments for Nodes, Viewer and Parameters; collapse/reopen, reset and bounded absolute-size restoration, including initial window-manager tiling (rather than proportional shrinking of saved sizes). Empty Parameters uses a short rail; authored fields get a resizable, independently scrolling inspector. |
| Compact windows without overlapping controls | Viewer contents scroll within their own dock when short. All six comparison layouts keep controls, image surfaces and scopes separate. Very narrow workspaces scroll horizontally rather than dropping controls. This is not Tauri's portrait dock reflow. |
| Canvas navigation and creation feedback | Workflow node/link counts, measured Fit workflow, scaled node contents/chrome with stable world-space edge/hit-test dimensions, zoom controls/readout and initial fit on source/workflow load. Click/Enter insertion reveals the new node after inspector layout; explicit drops preserve the viewport. Canvas navigation stays out of graph history and does not request processing. Dock layout stays out of history; viewer resizing can request a different preview mip when fit scale changes. Minimap and richer canvas actions remain unported. |
| Clear command hierarchy | Primary Open Image; grouped Workflow files and View menus; separate export/history actions. Image Fit/100% stay in their own panes, distinct from Fit workflow. Source filename appears in status. |
| Parameters beside their graph context | Named native inspector with existing exact/draft/reset/advanced/port semantics, bounded fields and sliders. Tauri's embedded node controls, interactive curve plots/color references and geometry gestures remain different/unported. |
| Persistent workspace and session | Dock dimensions/visibility restore from `gpui-workspace.json` in RawWeave's platform config directory (`XDG_CONFIG_HOME/rawweave` or `~/.config/rawweave` on Linux). Invalid/unreadable files fall back to defaults with a startup diagnostic; an unavailable config location disables persistence. Graph documents, history, sources and viewer sessions are separate; A/B/session restoration, lamp/theme parity and source-metadata docks remain open. |

## Still to port

| Tauri surface | GPUI status / remaining gate |
| --- | --- |
| Graph editing, library, workflow files | Basic native port implemented; parameter-port exposure and gesture-grouped canvas move history implemented; category navigation, compatible filtering, keyboard insertion, library drag/drop and selection-blueprint export/graph instantiation implemented; grouping/subgraph composition, blueprint-authoring sessions, richer canvas actions and full native automation remain. |
| Parameter editing | Exact text, sliders, booleans/enums, resets, units/descriptions and parameter ports implemented; multiline widgets and advanced grouping implemented; curves, color/transfer references, parameter suggestions and full native automated coverage remain. |
| Viewer targets and navigation | Independent image/spatial-output selectors, grayscale/colored mask modes, Fit/100%, zoom/pan, refresh/cancel implemented; tiled 100% display and full viewer controls remain. |
| Viewer B / comparison | Independent targets, horizontal/stacked A/B, wipe, blink and display-space difference implemented; persistent viewer-session restoration and automated native integration coverage remain. |
| Scopes and overlays | All eight scope views, scoped pixel readouts, hide/show, bounded sampling and clipping overlays implemented; native automated coverage remains. |
| Crop and masks | Rust nodes usable through parameters; interactive drawing/editing not ported. |
| Browser and image sets | Not ported: thumbnails, folders, selections and source metadata. |
| Export and batch | Single-image export with JPEG quality, long-edge sizing, PNG depth/compression and Off/Low sharpening implemented; remaining recipe/profile controls, batch queue, progress/cancel/resume and preflight UI remain. |
| Checkpoints and subgraphs | Selection-blueprint export and graph instantiation implemented; checkpoint management, blueprint authoring/bindings and nested-subgraph navigation remain unported. |
| External hosts / providers | Engine available; connection/provider UI not ported. |
| Workspace and menus | Workflow command grouping, canvas controls, collapsible/resizable docks, keyboard size alternatives and layout persistence implemented; compact controls no longer overlap. Portrait reflow, viewer/session restoration, source metadata, full theme/lamp/typography parity, workspace switching and OS application menus remain. |
| Native integration gates | Session/engine tests plus opt-in headless GPUI layout/input regressions and local X11 verification. No full GPUI OS-portal/GPU end-to-end suite or Tauri-equivalent coverage. |

Do not remove Tauri or present this frontend as feature-complete until these gates are cleared.

## Check

```sh
cargo test --locked --manifest-path apps/desktop-gpui/Cargo.toml --workspace
cargo test --locked --manifest-path apps/desktop-gpui/Cargo.toml --workspace \
  --all-features -- --test-threads=1
cargo clippy --locked --manifest-path apps/desktop-gpui/Cargo.toml \
  -p rawweave-gpui --all-targets --no-deps -- -D warnings
cargo fmt --manifest-path apps/desktop-gpui/Cargo.toml --all -- --check
```

Headless session tests: append `--no-default-features` to the package test command. The `ui-tests` feature enables GPUI Kit's existing test support for actual headless view layout, input/focus, visibility, sizing, navigation and comparison regressions; these do not inspect GPU pixels or prove OS file portals. Existing project and Tauri backend tests still exercise the shared engine. Tauri WebdriverIO tests do not cover GPUI. Whole-workspace Clippy including upstream gpui-flow currently reports upstream style warnings; the owned application passes the scoped command above.

See `vendor/gpui-flow/UPSTREAM.md` for pinning and integration fixes.
