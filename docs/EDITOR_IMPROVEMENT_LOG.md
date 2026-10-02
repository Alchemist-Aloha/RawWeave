# Editor improvement log

## Priorities

Ranked by impact × frequency × confidence / cost:
1. Numeric typing creates intermediate graph commands/history entries and preview requests (measured).
2. Text-field Undo/Redo triggers graph history instead of native editing (code-confirmed).
3. Whole-pixel controls advertise fractional steps (baseline regression failure).
4. Keyboard library result navigation; selection deletion batching.
5. Profile native preview full-frame probe + regional evaluation and source-byte copying; cache memory ceiling.

## Iteration 1 — numeric edit transactions

Problem: entering one precise value publishes intermediate numbers.
Evidence: running Chrome browser adapter, `2` → `2.2` → `2.25` caused three workflow-hash updates. Component code commits every valid input event.
Impact: redundant commands, graph refreshes, history snapshots, and potential preview cancellations/restarts; Undo steps through typing.
Proposed change: numeric local draft, commit once on Enter/focus loss, Escape cancels. Keep pointer sliders and nonnumeric controls unchanged.
Expected improvement: three graph updates become one; abandoned/invalid/unchanged drafts become zero.
Measurement/test: failing component regressions verified before implementation; numeric transaction tests pass. Browser observation: zero hash updates while typing, one after trusted Enter key (previously three). Build passes. Existing tests updated to current perceptual labels and a nondefault reset fixture; two existing whole-pixel step failures remain for iteration 2.
Files: `apps/desktop/frontend/src/components/GraphNode.tsx`, `GraphNode.test.tsx`.
Change: validated numeric drafts commit on blur/Enter, Escape discards, unchanged values do not publish; native title explains the transaction. No timer or backend changes.
Remaining concern: text/prompt fields retain existing per-keystroke behavior; Rust preview latency is not measured by browser adapters.

## Iteration 2 — whole-pixel control precision

Problem: pixel coordinates and integral filter radii offer fractional spinner/slider increments that validation rejects.
Evidence: existing crop regression reports step 0.01 instead of 1; blur reports 0.1 instead of 1. `ux.step` overrides `wholeNumber` in the shared field.
Impact: keyboard arrows/pointer slider can do apparently nothing; valid whole-pixel values require extra editing.
Proposed change: whole-number fields always use step 1 before the UX fractional default.
Expected improvement: every arrow increment on those fields is a valid pixel step.
Measurement/test: existing failing crop/blur tests now pass; focused GraphNode suite 9/9. Both range and numeric fields use the same corrected step.
Files: `apps/desktop/frontend/src/components/GraphNode.tsx`, `GraphNode.test.tsx`.
Root cause/change: `wholeNumber` now takes precedence over `ux.step`. No parameter schema or processing changes.

## Iteration 3 — editing focus owns Undo/Redo

Problem: Undo while typing in a field removes/changes unrelated graph nodes.
Evidence: browser search focused, Ctrl+Z changed the graph from two nodes to one. Global handler permits modifier shortcuts through editable fields. New resolver regression fails before fix.
Impact: accidental graph changes when correcting search, prompts or numeric drafts; violates native editing expectations.
Proposed change: shortcut resolver leaves Undo/Redo to focused native fields and editable descendants, while retaining global Save/Open/Search.
Expected improvement: zero graph mutations from field Undo/Redo; graph shortcuts unchanged on canvas.
Measurement/test: resolver regression failed before fix and passes afterward for both platform modifiers and all native field types. Live browser: search Undo leaves node count unchanged; canvas Undo gives 0 nodes and Redo restores 1. Native numeric/Escape/Enter/field-Undo regression passed (476 ms).
Files: `apps/desktop/frontend/src/ui/shortcuts.ts`, `shortcuts.test.ts`, `apps/desktop/frontend/e2e/native/parameter-editing.spec.mjs`.
Change: editable controls own Undo/Redo; Save/Open/Search retain global behavior.

## Iteration 4 — search shortcut reveals its target

Problem: Ctrl/Cmd+K and canvas “Add node…” silently do nothing when the library is collapsed.
Evidence: running browser, collapsed dock + Ctrl+K leaves no search input and the dock collapsed. Native regression fails after 15 s waiting for search to appear.
Impact: keyboard acceleration fails exactly when maximizing graph space; users must manually expand the dock first.
Proposed change: shared search action switches to Build, expands library if necessary, then focuses the mounted input on the next frame.
Expected improvement: manual expand + search click become one shortcut; context-menu creation also recovers.
Measurement/test: native visibility/focus regression failed before fix; rebuilt native regression suite now passes 2/2 (958 ms). Shared callback used by both callers.
Files: `apps/desktop/frontend/src/App.tsx`, `apps/desktop/frontend/e2e/native/parameter-editing.spec.mjs`.

## Iteration 5 — large-graph overview

Problem: Fit View cannot show a moderately large workflow.
Evidence: production browser build, a synthetic 200-node 20×10 grid loads in ~163 ms, but default zoom floor 0.5 clips outer columns; only 60/200 nodes fully visible.
Impact: graph orientation and organization require repeated panning; fit command breaks its promise.
Proposed change: lower React Flow minimum zoom, retaining existing fit/navigation implementation. A 0.1 floor still clipped nodes at 1100px; final floor is 0.05.
Expected improvement: 200/200 nodes fit at desktop size; no per-frame layout or new React state.
Measurement/test: actual browser bounding-rectangle counts improved 60/200 → 200/200 at 1440px (fit zoom ~0.130). Resizing to 1100px and fitting shows 200/200 at zoom ~0.077. Browser E2E regression covers both widths, but standard browser runner remains blocked by driver download; equivalent scenario executed through Chrome DevTools. Production-build load-to-graph baseline ~163 ms; after-change load plus an explicit 300 ms settle wait ~400 ms (not comparable timings, no load-speed claim). Idle rAF median/p95 ~16.7 ms; this is not a drag FPS benchmark.
Files: `apps/desktop/frontend/src/App.tsx`, `apps/desktop/frontend/e2e/browser/large-graph.spec.mjs`.
Change: one React Flow prop; no processing, layout algorithm or drag-time state changes.

## Iteration 6 — fixed library heading and search

Problem: scrolling the node library also scrolls its title, search and filters out of view.
Evidence: `.panel--library` owned scrolling; the node list had no bounded scroll area. New native regression failed before the change.
Impact: users must return to the top to search or collapse the panel.
Change: CSS-only flex column with a shrinking, independently scrolling node list; heading, search, filters and selection actions remain fixed. No sticky overlay or scroll-time JavaScript.
Files: `apps/desktop/frontend/src/styles.css`, `apps/desktop/frontend/e2e/native/parameter-editing.spec.mjs`.
Measurement/test: title/search coordinates remain identical after scrolling the list to its end at requested window heights 900 and 600; panel scroll remains zero. All 195 frontend tests and three focused native regressions pass; native binary rebuilt, screenshot inspected. Detector reports an existing node-card pseudo-element accent warning outside this change.
Remaining concerns: unrelated full-suite failures listed below remain unresolved.

## Iteration 7 — portrait workbench

Problem: a tall window spends its limited width on three columns, squeezing the graph.
Change: portrait media query retains the library on the left, stacks the graph above a preview/source row, and rotates the existing splitters and their keyboard/ARIA axes. Source retains its preferred width instead of consuming spare preview space. Collapsed preview/source become narrow rails. Portrait dock height is stored separately from landscape width; older preferences receive the default height without changing their width. No graph remount, processing or dependencies added.
Files: `apps/desktop/frontend/src/App.tsx`, `styles.css`, `src/ui/layout.ts`, `layout.test.ts`, `e2e/native/portrait-layout.spec.mjs`, `e2e/browser/portrait-layout.spec.mjs`.
Evidence/measurement: new native layout regression failed before implementation (preview top 86 versus graph bottom 976). Chrome viewport measurements at 900×1200 now give the graph and preview/source the same 663px width, instead of sharing that width side by side. Portrait → landscape restores the original 838px graph and 360px right dock at 1440×900. At 600×1000 the library remains left and there is no document-width overflow. These are layout measurements, not performance benchmarks.
Verification: 196 frontend tests pass; frontend/native build passes. Two native portrait tests, three editing tests and four selected native preview tests pass. Native tests cover actual image display, keyboard divider resizing, library/preview/source collapse and unchanged workflow hash. Native portrait screenshot inspected. Browser round-trip/overflow assertions executed through Chrome DevTools; standard browser runner still lacks its matching driver.
Boundary: native window size requests were constrained/ignored by the current desktop/WebKitGTK environment (1440×900 request returned an unchanged 960×1023 outer window). Native coverage therefore verifies portrait behavior; orientation round-trip is verified in Chrome, not claimed as a native resize round-trip. Existing full-suite failures remain unresolved.

## Iteration 8 — minimum workflow width

Problem: the workflow could shrink to 363px in the narrow portrait case, squeezing its controls and overview.
Change: a 480px minimum in both landscape flex and portrait grid layouts. Excess dock width scrolls inside the workbench rather than clipping panels or scrolling the document. No controller, graph or processing changes.
Evidence: regression assertion failed at 363px before the fix; Chrome measurements now show 480px at 600×1000 and 960×640. Controls and overview remain within the workflow and do not overlap. Scrolling makes the complete 480px workflow reachable at a 600px viewport; screenshot inspected.
Verification: 196 unit tests, native build and five focused native layout/editing checks pass. Extended browser regression covers minimum width and widget bounds; its equivalent assertions were executed through Chrome DevTools because the standard browser driver remains unavailable. `git diff --check` passes. Existing graph-node accent-stripe detector warning remains outside scope.

## Iteration 9 — fit portrait workflow to the window

Change: portrait overrides the workflow minimum to zero and uses a shrinkable grid track, so graph and preview/info fit the width beside the library. Landscape retains its 480px minimum.
Evidence: the new viewport-bound assertion failed before the fix (workflow right edge 717px in a 600px viewport); afterward the workflow spans 237–600px, with controls and overview contained and separated. Chrome checks also cover 900×1200, landscape restoration and the retained landscape minimum. Screenshot inspected.
Verification: 196 unit tests and frontend/native build pass; browser assertions executed through Chrome DevTools. Native portrait tests could not enter portrait orientation in the current desktop environment and timed out at their orientation waits; native validation is not claimed. Standard browser driver limitation remains. `git diff --check` passes.

## Iteration 10 — image-input helpers and precise crop/resize controls

- Reused existing graph parameter transactions and geometry editing, with no new processing or IPC APIs. Nodes with image inputs expose a Preview input action and actual full evaluated resolution; unresolved inputs remain honestly unknown. Direct source dimensions are used only when explicitly supplied, not inferred through resize/crop chains.
- Crop provides full-frame, square, 3:2, 4:3 and 16:9 centered, integer-pixel presets, output dimensions and the existing draw action. Resize shows output dimensions and quarter/half/three-quarter/original-size actions. Linear/radial gradients retain their existing drawing helpers. Presets commit as one grouped undoable edit; exact numeric fields remain available.
- Full evaluated A/B metadata replaces requested-region guesses at 100%. Live helper subscriptions mount only while a node's parameter fold is open, avoiding progress-driven updates across collapsed nodes.
- Native crop test now edits the visible Crop Width field to refresh direct backend connections. The previous test attempted to blur an unfocused hidden advanced field, so no commit/reset button appeared. This was reproduced and corrected without weakening gesture/undo assertions.

## Iteration 11 — transparent crop guides and display-sized previews

- SVG guide shapes use `fill: none` with their existing visible outline; the photograph stays visible beneath crop drawing.
- Viewport/zoom/DPR-aware mip requests use full-resolution geometry and separate bitmap dimensions. Panning and same-mip changes do not render again; 100% upgrades detail, retaining the old frame until replacement. Output-size correction applies in every zoom mode. A failing delayed-cancellation regression prevented an old acknowledgement from clearing a newer frame.
- Frontend: 230 tests and production build pass (existing bundle warning remains). Controller coverage includes DPR, hidden/unmeasured stages, metadata correction, stale results, URL release, revisions and cancellation; viewer coverage checks full-size attributes and geometry at reduced bitmap resolution.
- Rebuilt native binary: six selected preview tests pass, including transparent crop drawing with grouped undo, fit/scopes, pan/clipping and comparisons. This is real WebKitGTK evidence on the 1349×2023 fixture, not 24 MP end-to-end latency.

## Iteration 12 — measured RAW result reuse and shared demosaic search

- Node-level profiling found persistent caching excluded multi-output RAW/color results. Extended the existing bounded, revision-aware render cache rather than adding a separate cache. Default logical payload ceiling is 2 GiB / 64 entries; 512 MiB was tested and rejected because it evicted the 24 MP JPEG chain. Payload accounting is conservative and is not a peak RSS measurement.
- Demosaic searches each neighbor ring once for missing channels; Bayer/X-Trans comparisons preserve original sample ordering and bitwise results. Scene-linear HDR values and CPU fallback remain unchanged.
- Final release mip-2 backend measurements: JPEG warm ~24 ms (unchanged), Nikon RAW ~30 ms versus ~701 ms, Sony RAW ~29 ms versus ~588 ms. First RAW previews remain hundreds of milliseconds. Conditions, individual timings, cache boundaries and remaining costs are in `docs/LARGE_IMAGE_PERFORMANCE.md`.
- Root tests/format/Clippy and desktop tests/Clippy pass. A broad native attempt had 13 passes / 10 failures; workflow/batch/restoration gates are not cleared by the six focused preview passes. Standard browser E2E is still blocked by matching chromedriver availability. No dependency additions or commits.

## Earlier performance investigation and remaining work

- Existing optimized RAW preview corpus benchmark run unchanged: 933.37, 901.14, 895.64, 903.48 ms for four repeated 512×512, mip-2 requests. Median ~902 ms. Debug run ~7.1 s/request is recorded only to distinguish build modes, not a product latency claim. This narrow single-camera synthetic request is not a RAW-open benchmark or camera matrix.
- Source inspection: RAW open decodes for metadata, while previews reconstruct contexts from compressed bytes; contexts copy the source byte vector. Persistent graph cache reconstruction excludes RAW values. Repeated preview latency warrants decode/hashing/conversion/PNG stage timings before choosing a fix.
- Graph parameter invalidation already targets edited nodes and descendants; do not replace it with new global invalidation. Upstream work and pixel hashing occur before some cache lookups and need profiling.
- Ordinary browser metadata and thumbnails decode separately; directory pagination rescans/sorts. RAW browser inspection decodes without producing a thumbnail. Measure folder loading and disk I/O independently.
- Workflow history serializes graph configuration, not image buffers. Numeric transactions reduce repeated serialization requests; payload/lock time for 200 nodes remains unprofiled.
- Desktop previews currently attach no GPU render context. Generic GPU paths upload/read back per operation and block, but changing them would not prove a desktop-preview improvement.
- Startup config/registry loading, idle CPU, total process memory, image switching, rapid slider latency, export throughput and actual graph-drag frame time still need native profiling. Browser heap/rAF values must not substitute for these.

## Earlier verification / stopping boundary

- Final frontend tests: 41 files / 195 tests pass. Final production frontend build passes (existing ~670 kB bundle-size warning remains).
- Native E2E build passes. Final focused native regression: 2/2 pass on rebuilt binary (~958 ms test time).
- Full native suite attempted: 8 pass / 10 fail across preview/workflow specs. Examples: old `Reset X` selector, batch error, restored graph/node-count expectations and comparison preview timeout. These are not declared pre-existing without a native baseline; full native regression clearance is still outstanding.
- Browser standard E2E blocked by unavailable matching chromedriver. Actual browser workflows exercised through DevTools: precise numeric typing/commit, field Undo, canvas Undo/Redo, collapsed search, graph load/fit and resizing. Desktop screenshots inspected. Native test driver uses synthetic input/focus where WebKitGTK cannot deliver pointer/key events.
- Impeccable detector on changed UI files: no findings. `git diff --check` passes.
- Changes recorded here rather than committed; no dependency additions, schema changes, processing changes, or broad visual redesign.
- This is five implemented iterations, **not proof of diminishing returns across the whole application**. Continue by restoring full native/browser regression gates and profiling the ~902 ms repeated RAW preview. Avoid speculative decode/cache/GPU rewrites and new debounce layers until phase measurements identify the dominant cost.


## Environment / baseline

- Clean Git worktree at start.
- Vitest baseline: 189 passed, four GraphNode tests failed (stale labels and fractional whole-pixel step).
- Standard browser E2E initially timed out; explicit Chrome binary fails downloading matching chromedriver 154.0.8037.92. No E2E pass claimed.
- Application runs in Chrome via Vite and direct DevTools protocol on an isolated browser profile. Screenshots are temporary, not committed. Browser adapters do not measure Rust processing or native WebView behavior.
