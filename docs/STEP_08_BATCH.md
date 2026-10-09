# Step 08 — Batch Processing

## Objective

Make the application reliable for applying one exact workflow across many queued images.

## Preflight

Validate before launch:

- missing source files;
- invalid graph;
- missing dependencies;
- missing plugins/node packs;
- output directory access;
- naming collisions;
- unsupported formats;
- checkpoint policy;
- unavailable external providers;
- disk-space risk where practical.

Classify:

```text
Error
Warning
Info
```

## Dry Run

Support:

- current preview item;
- Test Set;
- first N;
- selected subset.

Dry run must use the exact workflow revision and output recipes intended for the real batch.

## Job Model

A batch job captures:

```text
workflow revision/hash
selected queue items
per-image overrides
checkpoint policy
output recipes
dependency versions
job state
```

Per-item states:

```text
Waiting
Running
Completed
Skipped
Failed
Cancelled
```

## Job Operations

Implement:

- start;
- pause where practical;
- cancel;
- retry failed;
- retry selected;
- skip;
- open failed item in preview;
- resume interrupted jobs.

## Output Recipes

Recipe fields:

```text
format
resolution
bit depth
color space
ICC/OCIO transform
metadata policy
output sharpening
quality/compression
destination
filename template
collision policy
```

Initial formats:

- JPEG;
- PNG;
- TIFF;
- OpenEXR.

## Reproducibility

Pin:

- workflow revision;
- node-pack versions;
- plugin versions;
- output recipe;
- overrides;
- checkpoint artifact hashes where applicable.

## Acceptance Criteria

- processing 100+ images does not require keeping all decoded images resident;
- failures do not stop unrelated items unless policy says so;
- completed outputs are not recomputed after restart if job state validates them;
- running batch is unaffected by subsequent workflow edits;
- interactive preview remains responsive during background batch work.

## Exit Deliverable

A production-oriented batch engine suitable for real photographer workloads.

## Implemented in the native GPUI shell

The **Batch** surface runs the queue through the existing engine
(`rawweave_batch::BatchEngine` with `ImageFileProcessor`); the shell only builds and
reports the job (`apps/desktop-gpui/src/batchqueue.rs`):

- queue a folder or a single frame from Browse; duplicates by source path are refused;
- output folder, format (JPEG/PNG/TIFF/OpenEXR) and quality (1–100);
- the job pins the open workflow's revision and hash, so later graph edits cannot
  change a running job;
- preflight runs before any worker starts, and its diagnostics are listed with the
  queue; a job the processor cannot run is refused with the reason;
- Run / Pause / Cancel, per-item state stamps (Waiting, Running, Completed, Failed,
  Skipped, Cancelled) and a progress readout, polled from the engine's own snapshot;
- the queue keeps paths, not decoded images.

Not implemented yet: dry-run subsets, retry/skip actions, per-image overrides,
resume after restart (the job store is in memory, so the queue is rebuilt by
re-queueing), a persisted queue, and the remaining recipe fields (resolution,
bit depth, colour space, ICC/OCIO, metadata policy, sharpening, filename template,
collision policy) which stay at their engine defaults.
