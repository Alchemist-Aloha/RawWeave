# Large-image loading and preview performance

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

## Final measured backend results

Same fixtures, whole-frame mip 2, release build, one test thread, diagnostics enabled and filesystem caches not flushed. Two unchanged repeats are averaged. This is one final observation, not a statistical latency guarantee or native open-to-display timing.

| Source | Open | First preview | Unchanged | Parameter edit | Repeat after edit |
|---|---:|---:|---:|---:|---:|
| 24 MP synthetic JPEG | 125.71 | 225.10 | 23.76 | 165.63 | 22.89 |
| Nikon D70s RAW | 48.31 | 546.42 | 29.50 | 447.58 | 29.67 |
| Sony ILCE-7S RAW | 10.22 | 457.42 | 28.84 | 389.25 | 28.83 |

Compared with the preceding implementation, RAW warm previews fell from ~701/~588 ms to ~30/~29 ms. JPEG warm reuse is preserved. A separate mip-3 diagnostic run under different load produced slower cold RAW timings; do not compare different mip/load conditions as an algorithm speedup.

## Verification

- `cargo test --locked --workspace --all-targets --all-features`: passes.
- Desktop backend `cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features`: 78 library tests and one binary test pass; two timing benchmarks ignored by default.
- Root and desktop all-target/all-feature Clippy with `-D warnings`: pass.
- Root `cargo fmt --all -- --check`: passes. Desktop full formatting check reports existing untouched formatting in `src/ai.rs` and `src/lib.rs`; changed `preview.rs` is formatted. Unrelated files were not reformatted.
- Frontend: 230 unit tests and production build pass; existing chunk-size warning remains.
- Native E2E binary rebuilt. Six selected real-WebView tests pass: restored-source PNG display, frame retention, preview/scope fitting, transparent crop drawing/grouped undo, pointer pan/clipping and comparisons. This native fixture is 1349×2023, not the synthetic 24 MP benchmark source. The crop regression edits the visible Crop Width field to refresh backend connections; hidden advanced inputs do not reliably focus/blur through the native driver.
- `git diff --check`: passes.
- No dependency additions, persisted identifier changes, JPEG/RAW path merging or frontend processing.

## Remaining work / stopping boundary

RAW open still decodes for metadata and first preview decodes again. Source contexts still copy/hash compressed RAW bytes, and RGB-to-RGBA conversion remains measurable. Parameter edits still recompute affected processing stages. The 2 GiB conservative payload budget is not a measured peak-memory bound; allocator overhead, source state, temporary buffers and GPU allocations are separate. Broader cameras, high-resolution noisy photos, native end-to-end latency and peak RSS remain unmeasured.

GPU/codec rewrites and progressive demosaic were deliberately skipped: current desktop previews attach no GPU render context, and no native transfer/decode bottleneck was established. The unrestricted native suite still has workflow/batch/restore failures (initial broad run: 13 pass/10 fail); it is not cleared by six focused preview passes. Standard browser E2E remains blocked by the missing matching chromedriver. Changes are recorded, not committed.
