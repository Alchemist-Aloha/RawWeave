# Step 10 — Manual Checkpoints

## Objective

Implement stateful manual evaluation barriers before adding AI.

The mechanism must work for any expensive or externally controlled computation.

## Evaluation Policy

Add:

```text
Automatic
ManualCheckpoint
```

Checkpoint state:

```text
Ungenerated
Generating
Current
Stale
Failed
Cancelled
```

## Dependency Hashing

Store:

```text
current dependency hash
committed dependency hash
committed artifact id
generation metadata
```

A checkpoint becomes stale when current and committed dependency hashes differ.

## Artifact Store

Committed results are not disposable cache.

Implement content-addressed storage for:

- output image;
- masks/spatial data later;
- generation metadata;
- upstream hashes;
- node version;
- external tool metadata where applicable.

## Scheduler Semantics

Upstream changes:

- do not trigger checkpoint evaluation;
- mark stale;
- preserve last committed output.

Downstream automatic nodes continue using the committed output.

## UI

Checkpoint node should show:

- state;
- generation revision;
- stale warning;
- Generate/Regenerate;
- Cancel;
- Input preview;
- Generated preview;
- Difference view.

## Batch Policy

Support explicit policies:

```text
use committed
generate if missing
regenerate all
fail if stale
```

## Test Node

Before AI, create a trivial manual external transformation node to prove semantics.

## Acceptance Criteria

- changing upstream does not automatically execute checkpoint;
- downstream continues from last committed result;
- changing graph while generation is running produces correct stale state;
- artifacts survive restart;
- export does not silently regenerate;
- batch requires explicit checkpoint policy.

## Exit Deliverable

A general stateful checkpoint system proven independently of any AI provider.
