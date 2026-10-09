# RawWeave native desktop

GPUI is the default application selected by `scripts/build-all-in-one.sh` and
`scripts/run-rawweave.sh`. It has **not reached complete Tauri feature parity**.
The legacy Tauri application remains in the repository for migration coverage,
but is not part of the default desktop build or launcher.

## Run

```sh
cargo run --locked --manifest-path apps/desktop-gpui/Cargo.toml -- \
  test-data/images/common/gracie-allen-portrait.jpg
```

Release build and launch:

```sh
./scripts/build-all-in-one.sh  # installs GPUI as bin/rawweave-desktop
./scripts/run-rawweave.sh
```

These scripts require neither pnpm nor a Tauri/WebKitGTK runtime.

The optional command-line argument opens an image. Otherwise use **Open Image**. On Linux this requires a working X11/Wayland session, Vulkan driver, fonts and a desktop file-picker portal. For the local display used during verification: `DISPLAY=:0 GPUI_FORCE_X11=1`.

## Implemented

- GPUI Kit 0.7.1 application, native controls, search and file dialogs; no Tauri or WebView dependency.
- The committed design world, not a generic dark shell: `src/native_theme.rs` mirrors `DESIGN.md` as a token table (room / bench / judge casts, the four waxes, the 4px lattice, the 3px corner) and every surface draws from it. Warm near-black handling chrome (room ground `#14110d`), the plane in the bench cast (ground `#1b1c1d`, grid `#333538`, frames on `#24262a`), and the viewer/scopes in neutral judge grey (`#1c1c1c` / `#141414`). Selection is wax white, focus and hold are amber, faults are wax red, in-flight work is wax blue; a wire and its port are toned by the kind of data they carry, using the same four-family palette as `data-type-ui`. Node frames carry the title face, the node type printed as an uppercase edge code beneath it, and the perforated strip along the bottom edge. The node index is a ruled list: full-width hairlines, uppercase mono category codes, trailing counts, and no container around the list. The toolbar heading carries its own weight with the scope code beside it as a caption, never as a kicker above it.
- Both design faces ship with the binary: Archivo (400/700/800), the 75%-width Archivo Compressed cut for the display role, and Martian Mono (400/500) at 87.5% width, instanced from the same OFL sources the web build uses and registered into GPUI's text system at startup. Measurements, edge codes and field values are mono; handling labels are Archivo. A failed registration is reported on stderr instead of silently falling back to a system face. Colour and ink contrast is asserted per cast in `native_theme` tests.
- The plane's overview is a window cut into it: footprint fitted to the viewport aspect inside 148×100, recessed bench-sunk ground, frames in the plane's line tone, and a hairline mask outline, with the palette supplied by the shell rather than hardcoded in the widget.
- gpui-flow canvas with named typed ports, selection, dragging, pan/zoom, connections and deletion. Canvas moves form one document-history entry per completed gesture; no-op/selection gestures do not. Undo/Redo restores positions, including moves released outside the canvas; invoking history mid-drag finishes the pending move first. Moving nodes does not invalidate processing or request a preview. Connection release is resolved at the current pointer and always notifies the authoritative editor, with 24px socket hit targets and canonical type-compatible snapping in both directions. Reconnecting an occupied input atomically replaces its wire in one history entry; cycles are still rejected by the engine and the old graph is restored. Port anchors have fixed world-space offsets, so opening/editing node controls does not move sockets. Socket rows name the port, its direction and its data type, and every edge anchor/hit test resolves a port by id **and** direction — an outgoing wire always leaves the output side even when a node names its input and output the same. Display names replace internal node IDs, including ordinal names for duplicate node types and named upstream input references. Persisted identifiers are unchanged.
- Task-grouped node library shows disclosure icons/counts, Collapse all/Expand all and two-line name/type-ID entries with bundled Lucide add icons. It shares Tauri's taxonomy (`node-categories.json`), preserves browsing collapses while showing all search matches, supports name/type/category search and Enter-to-add, and defaults to compatible-input filtering when a node is selected. Compatibility uses the graph engine's canonical wildcard/numeric/boolean rules. Typed GPUI library drag-and-drop adds at the pointer in embedded canvas coordinates, including pan/zoom, and one Undo/Redo restores the insertion and position. New image-output nodes also appear in viewer target menus. Unknown families keep honest fallback labels; IDs and processing stay unchanged.
- Registered node library and transactional controls embedded in the selected node (bounded scrolling, Collapse/Edit controls and Focus node): exact numeric text commits on Enter/blur, native sliders committing on release, repeated Up/Down text-field steps committing on key release, checkboxes, named choice menus, Reset, units/descriptions and recommended-range notices. Escape discards text drafts; opening controls does not round saved float values. Undo keeps the edited node selected when it still exists. Presentation metadata is shared with Tauri via `parameter-ux.json`, including percentage/degree display factors; node constraints remain authoritative. Multiline prompt/expression/workflow/point fields use native auto-growing textareas (Shift+Enter inserts a line, Enter submits without a newline), and shared metadata drives the Show/Hide Advanced group. Parameter input-port exposure/hiding is undoable and persisted, with dynamic canvas handles. Native input text undo is separate from graph history; empty Undo/Redo controls are disabled.
- Tauri-compatible Ctrl/Cmd shortcuts: O loads a workflow, Shift+O opens an image, S saves a workflow, K expands Nodes and focuses search, A selects all graph nodes/links outside text fields, Z undoes, Shift+Z/Y redoes. Delete/Backspace deletes selected nodes/edges outside text fields. View → Keyboard shortcuts / `?` shows inline help; startup and dock collapse give the editor keyboard focus.
- Independent A/B viewer-output menus list displayable node/port pairs. Explicit choices survive comparison-mode changes and node inspection; deletion/history repair unavailable targets. Single view defaults to Output; comparison defaults to Image Input / Output (RAW: Demosaic / Display Transform). Switching outputs clears the old texture so failures cannot mislabel the previous image.
- Horizontal and stacked A/B, adjustable wipe (A left / B right), 450-ms blink with visible-source label, and absolute display-space difference. Wipe, blink and difference share one view across both sources (one zoom and one pan), so a wipe cuts one registration and a blink cannot jump; the wipe edge itself can be dragged in the image and writes through the same value as the slider. Side-by-side A/B deliberately keeps one view per pane. Each pane has independent Fit/100%, pan, wheel/button zoom, cancellation and refresh; the active pane determines Export and the global Fit/100% buttons. Hidden pane jobs/textures are released. Wipe/blink use native image composition; difference rasterizes bounded BGRA display surfaces on a background CPU task, with independent transforms, bilinear sampling, alpha/background composition and stale-result rejection. This is a comparison of displayed values, not scene-linear processing or GPU difference computation.
- All eight scope views: Histogram, Waveform, RGB Parade, Vectorscope, False Color, Gamut Warning, Pixel Inspector and Zebra, with hide/show and a named active-viewer source. Shared `rawweave-rendering::scopes` analyzes the displayed BGRA preview on background workers: scope sampling is capped at 100,000 pixels and the inspection raster at 256 pixels on its longest axis. Pixel Inspector reports the center preview pixel from that sampled raster; values are display codes, not RAW/HDR measurements. False Color maps display luma; Gamut Warning is explicitly a display-saturation heuristic, not color-managed gamut detection. Scope textures follow panel/device scale within a 1024-pixel-per-axis limit, and obsolete results are rejected.
- Clipping overlay follows each pane's image through pan, zoom and comparisons. Display-channel maxima ≥98% are marked red; maxima ≤2% are marked blue. Difference includes the same straight-alpha clipping marks. Clip/scopes toggles do not mutate graph state, and stale/hidden textures are released.
- Spatial viewer outputs: Mask/MaskSet, LabelMap, ConfidenceMap, DepthMap and RegionSet, alongside ordinary/scene/display images. Masks offer independent grayscale/colored-alpha modes in A and B; confidence stays grayscale, labels use categorical colors, depth is min/max-normalized (flat maps use midgray), and regions show colored borders/fills. Non-zero origins are sampled in global coordinates; each output is fitted to its own local extent. Sparse unions are sampled directly into a budget-checked mip upload rather than allocating their full bounding raster. Empty MaskSet errors; empty RegionSet is transparent. These are visualization values, not exportable image outputs. Linear/radial gradient parameters can be drawn in their node's connected-input thumbnail; freehand painted-mask editing is not ported.
- Background full-resolution export of the selected graph output using the existing batch encoders, independently of preview mip/zoom. Filename extensions select PNG (8-/16-bit sRGB), JPEG (adjustable quality, sRGB), TIFF (16-bit sRGB), or OpenEXR (32-bit linear sRGB). Export settings offer original resolution or a bounded 1–8192-pixel long edge, PNG compression and Off/Low output sharpening. The shared encoder applies resizing/sharpening after mip-0 evaluation. EXR preserves scene-linear highlights; exporting a display-transformed output cannot recover clipped values. Metadata is stripped; custom profiles, exact dimensions, other sharpening recipes and batch management are not implemented. The export captures graph/source/target when clicked, disables duplicate requests, reports errors separately from previews, and atomically replaces the chosen file only after successful encoding. The desktop save portal may append an extension; the actual chosen path is reported.
- JPEG/PNG and RAW source paths remain distinct. Runtime sources are not serialized. Workflow load clears the source; the next compatible image attaches without rebuilding the saved graph.
- Compatible version-1 frontend workflow envelope (`graph` string and `positions`) and bare engine graph JSON. Atomic workflow saves avoid truncating an existing file on failure.
- Export selected canvas nodes as an engine-compatible blueprint (version `1.0.0`, filename supplies ID/name), retaining exposed parameters and crossing input/output declarations. Instantiate a validated blueprint as the active graph; successful import clears runtime sources/history/layout and waits for a compatible source, while failed import preserves the current document. These actions do not retain a blueprint-authoring session, provide workflow-level boundary bindings or create/navigate nested subgraphs; those remain migration gaps.
- Background CPU graph evaluation, shared immutable source buffers, mip-aware/coarse-to-fine previews, obsolete-result rejection, Fit/100%, pan and wheel zoom.
- Ordinary image previews honor `ColorDomain::LinearSrgb` through the shared `SrgbDisplayTransform`, while sRGB-coded images are uploaded without a second transfer. Scene-to-Image output therefore displays consistently with scene RGB at full resolution; preview mips may differ because image mips filter pixels while RAW scene previews sample the sensor grid. Alpha is retained and processing buffers remain unchanged.
- Direct BGRA preview upload to GPUI's native GPU image atlas. GPUI performs textured image scaling/composition; there is **no PNG encoding, JSON pixel IPC, preview URI fetch or GPU readback** in this display path. Old atlas entries are explicitly released.
- Actual adapter/software status is reported, not simulated. Local Linux verification selected NVIDIA GeForce GTX 1070, NVIDIA driver, `software=false`; a real JPEG was visibly displayed. RAW sampling/bounds are regression-tested. No native frame-time benchmark or camera-wide color claim.

## Working surfaces

The toolbar's trailing switch selects one of three surfaces; the graph, the source and
the queue are the same objects in each.

- **Browse** lays the images of one folder out as a contact sheet on the bench. Each
  frame shows a bounded real thumbnail and a ledger row with its file code and either
  its pixel dimensions or, when the file carries no preview, its encoded size. A RAW
  file whose container has no embedded preview says so instead of inventing an image.
  Nothing is fabricated: the sheet is a contact sheet, not a development.
- **Batch** queues those frames and runs them through the existing engine. The job
  pins the open workflow's revision and hash, so editing the graph afterwards cannot
  change a running job; preflight runs before any worker starts.
- **Workflow** is the graph editor, as before.

Decoding stays in the Rust image/RAW boundaries: this shell lists a folder, asks for
one bounded thumbnail per file, uploads it, and hands the queue to `rawweave_batch`.

Exposure, Local Exposure, Blur, Resize, Color Matrix, Levels, Curves, Invert and
Output offer explicit `scene` sockets alongside unchanged `image` sockets. All
pro-tools filters/analysis and image-derived/gradient/painted masks accept scenes;
Detail Separation exposes `base_scene`/`detail_scene`. Connected-input thumbnails
and gradient helpers follow scene wires as well as Image wires. Use one input
family at a time. Levels' authored transfer diagram switches to the signed,
unclipped scene response when its scene socket is connected. Scene processing
preserves working-space metadata and unclipped float values; its current raster is whole-frame, not region-origin-aware.
Add **Scene Linear RGB to Image** (`core.scene-linear-to-image`) after Camera
Transform/scene adjustments when entering image-only nodes. It converts known
primaries to linear sRGB, adds alpha 1, and requests full-resolution inputs;
it does not apply display gamma or clip highlights. Keep Display Transform last
on the typed scene chain. See `docs/STEP_03_RAW_FOUNDATION.md` for boundaries.

RAW development and color processing remain in Rust nodes on CPU. This is native GPU **display**, not a GPU-resident RAW processing chain and not a restoration of the removed WebGPU demosaic experiment. Uploads are bounded to 128 MiB and 8192 pixels per axis; larger 100% previews require tiled display support.

- Canvas wheel-up/down zooms in/out at the pointer without a modifier; Shift+wheel and middle-drag pan. Zoom stays out of graph history. Click a wire to select it (blue highlight), then Delete/Backspace or the contextual Disconnect button removes the connection. Disconnect keeps both endpoint nodes; Undo/Redo restores/removes the link through graph history. Connection anchors and selection geometry follow canvas zoom.
- The plane carries a 24px line grid, corner zoom/fit controls, a pannable overview with a viewport mask, and a footer with a status dot plus the drag hint. Selecting a node recedes the unselected frames.

## Workflow UX comparison

| Tauri Workflow behavior | Native GPUI implementation / boundary |
| --- | --- |
| Empty graph arrival and obvious next action | Empty workflow at startup; Open Image constructs the appropriate source graph. A centred arrival block (dashed add mark, "Start weaving", one-line instruction) sits on the plane while the graph is empty and retires as soon as a node exists. Node-library search is the authoring alternative. |
| Resizable/collapsible handling and judging docks | Native divider dragging and View-menu keyboard size adjustments for Nodes and Viewer; collapse/reopen, reset and bounded absolute-size restoration, including initial window-manager tiling. Parameters now scroll inside the selected node; the former inspector is an optional short Node help rail. Old inspector-height preferences remain compatible but do not determine inline controls. |
| Compact windows without overlapping controls | Viewer contents scroll within their own dock when short. All six comparison layouts keep controls, image surfaces and scopes separate. Very narrow workspaces scroll horizontally rather than dropping controls. This is not Tauri's portrait dock reflow. |
| Canvas navigation and creation feedback | The plane draws a 24px line grid, carries a bordered zoom-in/zoom-out/fit-workflow control stack in its lower-left corner and a pannable overview (minimap with a viewport mask) in its lower-right. Node/link counts and the zoom readout sit in the scope header. Scaled node contents/chrome keep stable world-space edge/hit-test dimensions; initial fit runs on source/workflow load; click/Enter insertion reveals the new node at a readable zoom after control layout and Focus node restores readable controls after a wider workflow Fit; Workflow Fit supports a 25% minimum zoom; explicit drops preserve the viewport. Selecting a node recedes the unselected frames, as Tauri does. Canvas navigation, overview panning and layout actions stay out of graph history and do not request processing; viewer resizing can request a different preview mip when fit scale changes. Canvas/nodes/wires still have no right-click menus and no wire-endpoint reconnect handles. |
| Clear command hierarchy | Primary Open Image; grouped Workflow files and View menus; separate export/history actions. Image Fit/100% stay in their own panes, distinct from Fit workflow. Source filename appears in status. The trailing **VIEW** switch moves between the Browse, Workflow and Batch surfaces; the live one carries a wax-white rule under it, and the chosen surface is persisted. |
| Parameters beside their graph context | In-node exact/draft/reset/advanced/port controls, sliders and bounded scrolling. Point-curve plots support dragging, add/delete, selection, exact x,y pair text and one-entry commits; gamma has a live draft plot and existing field/slider editing. Applied levels/map-range/clamp transfer diagrams, RGB qualifier swatches and circular hue references are presentation-only, not evaluated/color-managed results. Native curve coordinate fields/plot-based gamma manipulation and full integration automation still differ. |
| Persistent workspace and session | Dock dimensions/visibility and the last surface restore from `gpui-workspace.json` in RawWeave's platform config directory (`XDG_CONFIG_HOME/rawweave` or `~/.config/rawweave` on Linux). Invalid/unreadable files fall back to defaults with a startup diagnostic; an unavailable config location disables persistence. Graph documents, history, sources and viewer sessions are separate; A/B/session restoration and source-metadata docks remain open. The bench is drawn dark by default; the lamp switch that lights the plane is not ported. |

## Still to port

| Tauri surface | GPUI status / remaining gate |
| --- | --- |
| Graph editing, library, workflow files | Native wheel zoom, selectable/deletable connections with contextual Disconnect and undoable graph history implemented; parameter-port exposure and gesture-grouped canvas move history implemented; left-aligned two-line library rows indented under categories with trailing counts, collapse/expand, compatible filtering, keyboard insertion, library drag/drop and selection-blueprint export/graph instantiation implemented; grouping/subgraph composition, blueprint-authoring sessions, canvas/node/edge context menus, wire-endpoint reconnect and full native automation remain. |
| Parameter editing | In-node text/sliders/booleans/enums/reset/units/ports, multiline/advanced controls, interactive point curves, gamma plots and color/transfer references implemented. Rich parameter suggestions and full native automated coverage remain. |
| Viewer targets and navigation | Independent image/spatial-output selectors, grayscale/colored mask modes, Fit/100%, zoom/pan, refresh/cancel implemented; tiled 100% display and full viewer controls remain. |
| Viewer B / comparison | Independent targets, horizontal/stacked A/B, wipe, blink and display-space difference implemented. Wipe, blink and difference draw both sources into one viewport, so those three share a single view: zooming or panning either side moves both, and the wipe edge is one draggable 1px mark (12px pointer target) that stays in step with its slider. Horizontal and stacked A/B keep an independent view per pane on purpose. Persistent viewer-session restoration and automated native integration coverage remain. |
| Scopes and overlays | All eight scope views, scoped pixel readouts, hide/show, bounded sampling and clipping overlays implemented; native automated coverage remains. |
| Crop and masks | Connected upstream image thumbnails and dimensions are available in image-input nodes. Crop rectangle drawing/centered full/square/3:2/4:3/16:9 presets, resize presets and linear/radial gradient drawing update existing Rust parameters atomically. Thumbnails evaluate input metadata at mip 0 on workers, then use bounded direct BGRA uploads; gestures account for letterboxing and global mask origins. Exposed geometry ports disable direct drawing/presets. Freehand masks, persistent editable overlay handles and a full native GPU/portal automation suite remain. |
| Browser and image sets | **Browse** surface: folder prompt (or the folder of an opened image), a contact sheet of the supported files in that folder with bounded real thumbnails (ordinary decode or the RAW container's embedded preview), click-to-select, Develop, and Queue frame/folder. Image-set (HDR/focus/panorama) composition, breadcrumbs, ratings/flags, sort/filter, multi-selection, file operations and EXIF/summary panels are not ported. |
| Export and batch | Single-image export with JPEG quality, long-edge sizing, PNG depth/compression and Off/Low sharpening implemented. **Batch** surface: a queue built from a folder or a single frame, pinned to the open workflow's revision/hash, with output folder, format (JPEG/PNG/TIFF/OpenEXR), quality, preflight diagnostics, Run/Pause/Cancel, per-item state stamps and a progress readout. Dry-run subsets, retry/skip, per-image overrides, a persisted queue, resume after restart and the remaining recipe fields remain. |
| Checkpoints and subgraphs | Selection-blueprint export and graph instantiation implemented; checkpoint management, blueprint authoring/bindings and nested-subgraph navigation remain unported. |
| External hosts / providers | Engine available; connection/provider UI not ported. |
| Workspace and menus | Workflow command grouping, canvas controls, collapsible/resizable docks, keyboard size alternatives and layout persistence implemented; compact controls no longer overlap. The room/bench/judge casts, the four waxes, the 4px lattice, 3px corners and both shipped faces are implemented through a token table, and the bench defaults to dark. The lamp switch, portrait reflow, viewer/session restoration, source metadata, workspace switching and OS application menus remain. |
| Workflow scope, health and subgraphs | Not ported: Tauri's scope breadcrumbs, nested-subgraph launchers, dependency-health chip with workflow hash, and per-node state badges (checkpoint/fresh/stale/failed/generating) with their perforation strip. The dependency chip needs a flat blueprint/hash accessor on the project crate; node badges need checkpoint/analysis state. |
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
