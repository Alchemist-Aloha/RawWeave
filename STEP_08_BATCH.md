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
