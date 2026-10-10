# Large-image loading and preview performance

Historical Tauri measurements below refer to the removed legacy application; its
benchmark command is no longer runnable. Retained GPUI sections describe the native
path, but the old PNG/IPC measurements are not evidence of GPUI performance.

## Reproduce

From the repository root:

```sh
RAWWEAVE_PREVIEW_DIAGNOSTICS=1 cargo test --release --locked \
  --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features \
  large_image_loading_benchmark -- --ignored --nocapture --test-threads=1
```

The ignored benchmark creates a deterministic 6000×4000 JPEG in a temporary directory and uses two existing licensed RAW fixtures. Creation is excluded from timing. No fixture downloads or new dependencies are needed. Optional diagnostics split source-context setup, probe evaluation, display conversion, regional evaluation, preview selection and PNG encoding.

Open measures disk read, decode, metadata and default workflow construction. Preview measures the real Rust `render_preview` path through encoded PNG storage. Requests cover the whole image at mip 2: JPEG output 1500×1000, Nikon output 760×504, Sony output 704×468. An exposure node is inserted for the JPEG; the edit case changes exposure or RAW red gain. Each source runs first preview, two unchanged repeats, one parameter edit and an unchanged edited repeat on the same editor/cache.

These are optimized backend timings on this host, **not native UI/IPC/browser-decode latency**, cold filesystem measurements, or a camera compatibility matrix. Filesystem caches were not flushed. The RAW fixtures are approximately 6.1 MP and 5.3 MP, not 24 MP. JPEG content is a compressible synthetic pattern, not photographic noise. Timings are observations rather than CI thresholds.

## Before / after the initial three iterations (milliseconds)

The baseline and final rows below use the first and final recorded runs; unchanged-repeat columns average two samples. Two additional final-condition runs gave similar results (24 MP first preview 227/229 ms, unchanged repeats ~23 ms).

| Source / operation | Before | After |
|---|---:|---:|
| 24 MP JPEG open | 126 | 123 |
| 24 MP JPEG first preview | 2,146 | 222 |
| 24 MP JPEG unchanged preview | 2,028 | 23 |
| 24 MP JPEG parameter edit preview | 2,192 | 161 |
| 24 MP JPEG repeat after edit | not recorded | 23 |
| Nikon RAW open | 47 | 47 |
| Nikon RAW first preview | 900 | 699 |
| Nikon RAW unchanged preview | 912 | 701 |
| Nikon RAW parameter edit preview | 904 | 701 |
| Sony RAW open | 20 | 13 |
| Sony RAW first preview | 746 | 588 |
| Sony RAW unchanged preview | 754 | 588 |
| Sony RAW parameter edit preview | 747 | 592 |

Opening/decode itself was not materially improved. The large gain is time to rendered output: JPEG open plus first encoded preview went from ~2,272 ms to ~345 ms in this benchmark. Do not interpret the small open-time differences, especially Sony's, as proven improvements.

## Iterative improvement log

### 1. Repeated immutable image hashing

Problem: unchanged large-image previews were almost as slow as the first preview.
Evidence: baseline JPEG probe ~1,053 ms, regional evaluation ~1,067 ms; unchanged repetitions still ~2 seconds. Runtime graph hashing scanned all RGBA pixels repeatedly for context/output keys.
Impact: repeated preview requests and edits spent most time reading unchanged source buffers.
Change: immutable `Image` backing has a shared lazy `OnceLock` pixel-bit fingerprint; runtime graph keys retain revision, dimensions, origin, format and color metadata. Distinct pixels sharing a revision remain distinct. Fingerprints are not serialized or trusted from files. Durable checkpoint content hashing remains unchanged.
Files: `crates/image/src/lib.rs`, `crates/image/tests/step2_image.rs`, `crates/graph/src/lib.rs`.
Result: JPEG first preview ~365 ms, unchanged ~23 ms, edit ~304 ms after this iteration.
Test: regression failed before the API existed; clone/content-change/same-revision/serialization assertions pass. Full graph cache and checkpoint suites pass.

### 2. RAW/color buffer cloning

Problem: graph result, input and memo clones duplicated large decoded sample and RGB vectors.
Evidence: RAW evaluation remained ~900/~746 ms after the image hash change; pointer-sharing regressions failed for cloned RAW frames and color buffers.
Impact: full-resolution memory copying during every RAW preview and graph evaluation.
Change: reuse the ordinary-image pattern: immutable `Arc<Vec<_>>` backing for mosaics, scene-linear RGB and display RGB. Mapping operations still allocate validated new buffers; serializers still emit the same arrays. Scene-linear highlights and negative values are preserved.
Files: `crates/raw/src/lib.rs`, `crates/raw/tests/raw_decoder.rs`, `crates/color/src/lib.rs`, `crates/color/tests/color.rs`.
Result: Nikon previews ~702 ms and Sony ~590 ms in the next benchmark; JPEG warm-preview performance unchanged.
Test: pointer-sharing regressions verified failing first, then passing; independent mapped storage, HDR values and wire round trips covered. Decoder/color/graph suites pass.

### 3. Redundant whole-image regional evaluation

Problem: a whole-image request evaluated the same ordinary processing chain twice.
Evidence: after iteration 2, JPEG probe ~200 ms plus regional pass ~140 ms. Regression found four cache entries rather than the two from a single input/output pass.
Impact: first previews and parameter edits did avoidable duplicate computation and allocation.
Change: reuse the probe only when the requested region exactly equals its global bounds and tile is default (or when the target already requires full-frame execution). Probe already uses the requested mip and quality. Partial regions and nondefault tiles keep regional evaluation.
Files: `apps/desktop/src-tauri/src/preview.rs` (also holds diagnostics and reusable benchmark).
Result: JPEG first preview ~222 ms; edit ~161 ms; unchanged preview ~23 ms.
Test: failing cache-entry regression verified first. PNG byte equality, mip dimensions, non-zero global origins, partial-region output and nondefault tile identity verified afterward.

### 4. Bounded RAW/color result reuse

Node diagnostics (`RAWWEAVE_NODE_DIAGNOSTICS=1`) identified repeated demosaic, display transforms and output hashing; the persistent cache reconstructed only single Image/Mask outputs. The existing `MemoryRenderCache` now retains typed, multi-output RAW/color results under the same FIFO, revision and dependency invalidation rules. Supported payloads are conservatively charged; unsupported mixed outputs are not retained. Default limits are 64 entries and 2 GiB of logical payload, not an RSS guarantee. Explicit `new(capacity)` retains its historical unlimited-payload behavior; callers can choose a smaller payload limit. A tested 512 MiB default evicted the 24 MP JPEG chain and regressed warm previews to ~160 ms, so it was rejected.

Regression coverage includes decoder reuse, shared output storage, parameter/source/mip/quality changes, stale revisions, targeted invalidation, type mismatch, clearing and memory pressure. Diagnostic timers run only when enabled.

### 5. Shared demosaic neighbor search

The CPU demosaic now searches each neighbor ring once for missing RGB channels rather than once per channel. Search is bounded by the CFA pattern period. Original sample order and first-matching-ring behavior are preserved; Bayer/X-Trans border, narrow-image and HDR sample comparisons against the original implementation assert bitwise equality. No interpolation algorithm, processing space or GPU behavior changed.

### 6. Display-sized preview requests

The frontend requests mip `floor(log2(1 / (zoom × DPR)))`, bounded to 0–6, with mip 0 when no viewport is known. Full-resolution dimensions remain separate from PNG dimensions so fitting, crop gestures and global origins remain correct. Pan and same-mip navigation stay local; detail changes upgrade the frame while retaining the previous image. Metadata corrections work at 100% as well as fit. Stale arrivals are released and delayed cancellation cannot clear a newer frame.

The earlier cached mip-0 JPEG measurement was ~371 ms warm, including ~285 ms PNG encoding and ~86 ms selection. At mip 2 it is ~24 ms. This comparison changes requested resolution, not processing correctness or codec speed. `RAWWEAVE_BENCHMARK_MIP=0` (or 3) selects another benchmark mip; the default remains 2.

## Previously measured backend results

Same fixtures, whole-frame mip 2, release build, one test thread, diagnostics enabled and filesystem caches not flushed. Two unchanged repeats are averaged. This is one final observation, not a statistical latency guarantee or native open-to-display timing.

| Source | Open | First preview | Unchanged | Parameter edit | Repeat after edit |
|---|---:|---:|---:|---:|---:|
| 24 MP synthetic JPEG | 125.71 | 225.10 | 23.76 | 165.63 | 22.89 |
| Nikon D70s RAW | 48.31 | 546.42 | 29.50 | 447.58 | 29.67 |
| Sony ILCE-7S RAW | 10.22 | 457.42 | 28.84 | 389.25 | 28.83 |

Compared with the preceding implementation, RAW warm previews fell from ~701/~588 ms to ~30/~29 ms. JPEG warm reuse is preserved. A separate mip-3 diagnostic run under different load produced slower cold RAW timings; do not compare different mip/load conditions as an algorithm speedup.

## Previous verification

- `cargo test --locked --workspace --all-targets --all-features`: passes.
- Desktop backend `cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features`: 78 library tests and one binary test pass; two timing benchmarks ignored by default.
- Root and desktop all-target/all-feature Clippy with `-D warnings`: pass.
- Root `cargo fmt --all -- --check`: passes. Desktop full formatting check reports existing untouched formatting in `src/ai.rs` and `src/lib.rs`; changed `preview.rs` is formatted. Unrelated files were not reformatted.
- Frontend: 230 unit tests and production build pass; existing chunk-size warning remains.
- Native E2E binary rebuilt. Six selected real-WebView tests pass: restored-source PNG display, frame retention, preview/scope fitting, transparent crop drawing/grouped undo, pointer pan/clipping and comparisons. This native fixture is 1349×2023, not the synthetic 24 MP benchmark source. The crop regression edits the visible Crop Width field to refresh backend connections; hidden advanced inputs do not reliably focus/blur through the native driver.
- `git diff --check`: passes.
- No dependency additions, persisted identifier changes, JPEG/RAW path merging or frontend processing.

## Additional backend iterations

The next baseline was remeasured on this host rather than compared against the earlier table. Node diagnostics showed Bayer demosaic taking 378–595 ms, scene/display output hashing taking ~45–61 ms per buffer, and the serial display transfer taking ~150–174 ms in typical samples. Ambient host load caused large timing excursions; these are observations, not latency guarantees.

### 7. Exact interior Bayer interpolation

Recognize only the four ordinary 2×2 Bayer arrangements once per node evaluation. Interior pixels use their fixed axial/diagonal neighbors, preserving the reference's row-major floating-point addition order and initial zero. Borders, narrow images, non-Bayer 2×2 arrangements and X-Trans keep the existing bounded search. No demosaic quality or RAW working-space changes. Tests cover all four phases, small/narrow dimensions, signed zero, negative values and HDR samples, comparing float bits. The fast-path regression first failed because the implementation was absent, then passed. File: `node-packs/raw/src/lib.rs`.

### 8. Batched runtime sample hashing

Use the already-installed workspace `bytemuck` dependency to feed validated contiguous mosaic/scene/display float bits to the runtime hasher in one slice, instead of millions of tiny writes. Metadata still contributes to keys. A recording-hasher regression first failed the batch-size assertion, then verified both batching and byte-for-byte equivalence to the original native-endian bit stream, including signed zero. No durable checkpoint encoding or hash format changed. Typical RGB hash stages fell to ~14–17 ms. Files: `crates/graph/Cargo.toml`, `crates/graph/src/lib.rs`, both lockfiles; no dependency version changes.

### 9. Shared compressed sources and sampled display expansion

`EvaluationContext.source_bytes` now uses `Arc<Vec<u8>>`; the existing builder accepts either owned bytes or an existing Arc. Context cloning no longer copies compressed files at every graph stage. Both desktop source-context builders pass the existing shared source. The pointer-sharing regression failed before the change and passes afterward. This is a Rust runtime API field-type change, not a persisted workflow change.

Keep DisplayRGB values in their three-channel shared storage while probing full dimensions; select the requested region/mip before allocating RGBA. Ordinary images and other visualization types retain their existing conversion paths. Full dimensions, origins, clipping, tile handling and PNG transport remain unchanged. The new regression first failed because the helpers were absent, then checked retained RGB storage, exact selected pixels and PNG equality for full/partial/clipped regions and mips 0/1/2/6. Out-of-bounds regions still fail. Files: `crates/node-api/src/lib.rs`, `crates/node-api/tests/raw_values.rs`, `apps/desktop/src-tauri/src/{lib,preview}.rs`.

### 10. Bounded CPU display workers

After demosaic and hashing improved, serial sRGB transfer was the largest typical RAW evaluation stage. The color crate now encodes disjoint chunks with scoped standard-library workers, preserving per-pixel arithmetic. Small images stay serial; large buffers use available CPU parallelism, capped at eight workers with at least ~262k pixels per worker. Thread creation failure recomputes the output serially after joining started workers. No GPU availability is simulated, and no thread-pool dependency was added. On this six-CPU host, typical display stages fell from ~150–174 ms to ~33–56 ms (loaded outliers remain). The regression first failed because the helper was absent, then compared serial/reference output bits against worker counts 0/1/2/8/usize::MAX, empty/small/large buffers, transfer-boundary values, signed zero and HDR. File: `crates/color/src/lib.rs`.

### Measured outcome

Same deterministic/licensed fixtures and benchmark entry point as above: release, whole frame, mip 2, Preview quality, one test thread, both diagnostic flags enabled, filesystem caches not flushed. Baseline: one run with two unchanged samples/source. Final: three fresh-editor runs of the same built test executable, each with first preview, two warm repeats, one edit and one edited repeat; no other validation builds were launched during these three runs. Final columns are medians across runs (warm repeats averaged within each run). Fixture creation remains outside timing; preview ends at encoded PNG storage, not WebView display.

| Source | Operation | New baseline (ms) | Final median (ms) |
|---|---|---:|---:|
| JPEG 6000×4000 → 1500×1000 | Open | 204.84 | 184.41 |
| | First preview | 729.75 | 515.77 |
| | Unchanged | 46.25 | 45.00 |
| | Exposure edit | 375.37 | 319.96 |
| | Edited repeat | 54.80 | 42.81 |
| Nikon 3040×2014 → 760×504 | Open | 203.21 | 62.27 |
| | First preview | 1406.56 | 413.15 |
| | Unchanged | 36.72 | 26.29 |
| | Red-gain edit | 1473.36 | 458.68 |
| | Edited repeat | 39.70 | 37.36 |
| Sony 2816×1872 → 704×468 | Open | 30.95 | 32.83 |
| | First preview | 1386.44 | 703.98 |
| | Unchanged | 35.76 | 25.16 |
| | Red-gain edit | 1072.70 | 321.23 |
| | Edited repeat | 34.78 | 21.65 |

Final first-preview ranges: JPEG 514–680 ms, Nikon 397–529 ms, Sony 532–741 ms. Edit ranges: JPEG 313–428 ms, Nikon 442–655 ms, Sony 265–513 ms. Warm per-run means: JPEG 43–46 ms, Nikon 18–41 ms, Sony 25–28 ms. Open was not optimized; its large fluctuations, especially Nikon, are not attributed to these changes. Intermediate builds also showed load excursions; the speedups of isolated stages are stronger evidence than any single end-to-end observation.

Cached RAW backend previews are near frame-time at the measured mip, but full RAW parameter recomputation is **not near realtime**. The JPEG edit path is largely unchanged. No native large-image latency or peak-RSS claim is made. A final rerun through the documented Cargo command also passed; loaded Nikon edit/demosaic stages reached 659/231 ms, reinforcing that the medians are not latency guarantees.

### Verification for these iterations

- `env -u DISPLAY cargo test --locked --workspace --all-targets --all-features`: passes, including actual optional GPU checks, graph/cache/checkpoint tests and the new regressions. The initial inherited SSH-forwarded DISPLAY run hung in the existing GPU color-matrix test and was stopped; removing DISPLAY resolved adapter initialization. No GPU behavior was changed to bypass the test.
- `cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features`: passes, 79 library tests and one binary test; two timing benchmarks ignored by default.
- Desktop `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features`: passes.
- Root and desktop `cargo clippy --locked` with all targets/features and `-D warnings`: pass. The initial root Clippy run found the new inline hashing test module before production items; moving it to the file end fixed the finding.
- `cargo fmt --all -- --check` and `git diff --check`: pass. Only the single source-context line was changed in the desktop `lib.rs`; unrelated desktop formatting was not changed.
- Focused release color, RAW-node, source-context and preview tests pass. Batched hashing, context-sharing and pixel-equivalence regressions were verified failing before implementation.
- `pnpm run build:e2e:native`: passes, including frontend build (existing chunk-size warning). `DISPLAY=:0 pnpm run test:e2e:native --spec e2e/native/preview.spec.mjs --mochaOpts.grep 'reattaches a saved image|fits the preview image'`: two real-WebView tests pass. The first run inherited SSH DISPLAY and timed out starting the embedded driver; the local display resolved startup. These tests verify native source restoration, PNG display and scopes on the existing portrait fixture, not large-image latency or RAW-camera coverage. The full native suite was not rerun/cleared.
- Build-generated E2E capability schemas were restored; no application permission changes are part of the optimization.

## Remaining work / stopping boundary

RAW open still decodes for metadata and first preview decodes again. Source contexts still hash compressed RAW bytes, and SceneLinearRGB target conversion still expands full-resolution buffers (DisplayRGB targets now select first). Parameter edits still recompute affected processing stages. The 2 GiB conservative payload budget is not a measured peak-memory bound; allocator overhead, source state, temporary buffers and GPU allocations are separate. Broader cameras, high-resolution noisy photos, native end-to-end latency and peak RSS remain unmeasured.

This earlier stopping boundary is superseded by the CPU reduced-resolution iteration below. The unrestricted native suite still has workflow/batch/restore failures (initial broad run: 13 pass/10 fail); focused preview passes do not clear it. Standard browser E2E remains blocked by the missing matching chromedriver. Changes are recorded, not committed.

## CPU reduced-resolution and coarse previews

Implemented origin-anchored sampled RAW demosaic without downsampling the CFA. Camera/display transforms preserve full-size sampling metadata, lens correction uses full-resolution coordinates, and Final quality stays full resolution. Mip-invariant RAW stages reuse cached results between coarse/refined requests. Ordinary-image source proxies are limited to verified pointwise whole-frame chains; geometry, unknown operations, partial regions and nondefault tiles retain full-resolution inputs. Proxy sampling participates in context and memo cache identity.

Large-image viewer requests now render one extra mip (one-quarter of the requested pixels), then refine after 100 ms. Cancellation/revision/target changes stop obsolete refinements. Coarse images remain usable during refinement; resizing can reuse an already-sufficient coarse frame, and crop gestures survive bitmap refinement without changing full-size coordinates.

Three fresh-editor release runs, whole frame, mip 2, Preview quality, CPU, same deterministic JPEG/licensed RAW fixtures. Filesystem caches were not flushed. Columns are medians of one sample per run; timing ends at encoded PNG storage, excluding transport/WebView display. Open/decode and preview timing are separate.

| Source | Open (ms) | First preview (ms) | Warm repeat (ms) | Parameter edit (ms) |
|---|---:|---:|---:|---:|
| JPEG 6000×4000 → 1500×1000 | 329 | 187 | 32 | 48 |
| Nikon 3040×2014 → 760×504 | 79 | 218 | 17 | 117 |
| Sony 2816×1872 → 704×468 | 31 | 355 | 17 | 123 |

Edits are exposure for JPEG and red gain for RAW. Separate three-run progressive medians (initial mip 3, refinement mip 2): JPEG first coarse/refinement 108/117 ms; Nikon 203/39 ms; Sony 344/63 ms. These are individual backend durations, not cumulative UI latency; the benchmark does not include the controller's 100 ms delay. Reproduce with the benchmark command above, adding `RAWWEAVE_BENCHMARK_PROGRESSIVE=1` for coarse/refined requests and optionally `RAWWEAVE_BENCHMARK_MIP=2`.

Changed areas: color sampling metadata, image mip sampling, graph/source cache identity, pointwise node descriptors, RAW CPU stages, desktop preview selection/benchmark and viewer controller/overlay regressions. Tests caught proxy/full-input cache aliasing, redundant coarse rerenders after viewport resizing, and crop-overlay remounts mid-gesture.

The GPU experiment was stopped and removed at user request: no added demosaic shader/pipeline, GPU RAW capability, desktop GPU initialization/environment flags, or GPU-specific tests remain. Pre-existing rendering/GPU infrastructure is unchanged; desktop previews remain CPU-based. The RAW-node rendering dependency is used only for CPU preview-quality selection.

Verification after removal: RAW-node tests (4 unit + 12 integration), desktop tests (81 library + 1 binary, two benchmarks ignored), desktop all-target/all-feature check, workspace Clippy with `-D warnings`, formatting and diff checks pass. Frontend tests: 293 pass. Rebuilt native binary: five focused preview regressions pass, including actual coarse-to-fine images, source restoration, image/scopes fit and crop/undo. No full native/browser suite clearance or native large-image latency claim. Full CFA preprocessing, visible-region scheduling, peak memory and camera-wide coverage remain unresolved.

## Native GPUI display branch

`feat/gpui-desktop` adds a separate native frontend. It evaluates the same CPU Rust graph, then uploads bounded BGRA bytes directly to GPUI's native image atlas for GPU scaling/composition. The new display path removes PNG encoding, URI transport and WebView decoding; it does not add a GPU RAW-processing chain or reuse the removed demosaic shader.
Verification was a debug build on local X11, NVIDIA GeForce GTX 1070/NVIDIA driver (`software=false`), whole-image Preview quality, Fit with viewport-derived mip. The licensed 1349×2023 portrait visibly displayed in the real native window; RAW mip-2 regression checks 3040×2014 full bounds and 760×504 sampled upload dimensions. Source opening and graph evaluation run off the UI thread. Obsolete results are rejected, prior atlas entries released, and uploads limited to 128 MiB/8192 pixels per axis. Processing retains source buffers and graph caches.
No GPU/native before-after latency, first-useful-frame timing, warm-cache sample series, peak RSS or frame-rate measurement was made. Therefore this proves hardware-backed native display, not near-realtime RAW editing or a measured speedup over Tauri. Larger 100% views need tiled GPU display; broad native feature parity and benchmarks remain outstanding. Reproducible build/run and test commands are in `apps/desktop-gpui/README.md`.
