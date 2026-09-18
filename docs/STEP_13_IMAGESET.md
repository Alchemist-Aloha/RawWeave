# Step 13 — ImageSet and Computational Photography

## Objective

Add true graph operations whose algorithmic input is a collection of images rather than one photo processed repeatedly.

## Core Type

Implement:

```text
core.ImageSet
```

Distinguish it clearly from Working Queue.

Working Queue:

- job/session list of independent sources.

ImageSet:

- one typed graph value representing a collection required by one algorithm.

## ImageSet Metadata

Represent:

- ordered/unordered semantics;
- member identities;
- shared metadata;
- per-member metadata;
- alignment state where relevant.

## Nodes

Initial:

- ImageSet Input;
- HDR Merge;
- Focus Stack;
- Alignment;
- Exposure Set;
- Select;
- Filter;
- Map;
- Group.

Panorama can follow once alignment/geometry infrastructure is mature.

## Scheduler

Support:

- collection-level node evaluation;
- bounded concurrency;
- reuse of per-image upstream caches;
- cancellation;
- progress across set members.

## Example

```text
Bracketed RAW Set
   ↓
Per-Image RAW Develop
   ↓
Alignment
   ↓
HDR Merge
   ↓
Tone Map
   ↓
Output
```

## Acceptance Criteria

- HDR/focus-stack workflow is representable without special application mode;
- Working Queue remains independent;
- ImageSet graph values serialize cleanly;
- collection evaluation can reuse per-image nodes/cache;
- failures identify offending member(s).

## Exit Deliverable

Computational photography becomes a normal extension of the graph model.
