# Step 15 — UI/UX Refinement Against Full Spec

## Objective

Perform a dedicated product-quality pass after architecture and workflows are mature.

This is not the first time UI polish happens; it is the point where the whole product is optimized coherently.

## Graph Canvas

Improve:

- large-graph performance;
- viewport culling;
- keyboard-first node creation;
- compatible-node filtering;
- edge readability;
- node thumbnails;
- node status badges;
- checkpoint state visualization;
- subgraph navigation;
- minimap;
- alignment/snapping;
- multi-selection;
- copy/paste;
- duplicate;
- comments/groups.

## Browse

Improve:

- thumbnail virtualization;
- EXIF presentation;
- fast rating/flag shortcuts;
- sorting/filtering;
- drag to queue;
- file-operation confirmations;
- XMP status.

## Working Queue / Test Set

Improve:

- filmstrip/list modes;
- quick preview switching;
- test-set markers;
- multi-image preview/contact sheet;
- per-image override indicators;
- failure badges.

## Viewer

Add/refine:

- Viewer A/B;
- side-by-side;
- wipe;
- blink;
- difference;
- clipping overlays;
- pixel inspector;
- zoom/pan ergonomics;
- mask overlays.

## Scopes

Implement/refine:

- Histogram;
- Waveform;
- RGB Parade;
- Vectorscope;
- False Color;
- Gamut Warning;
- Zebra.

## Batch

Improve:

- preflight readability;
- estimated work;
- checkpoint cost warnings;
- queue progress;
- retry UX;
- logs/errors;
- output-recipe editor.

## Platform Quality

Address:

- high DPI;
- multiple monitors;
- touch/tablet input;
- keyboard shortcuts;
- accessibility;
- dark/light themes;
- localization readiness;
- drag-and-drop;
- file associations.

## Error UX

Every failure should answer:

```text
what failed?
where in the graph?
which image?
why?
can it retry?
what dependency/provider is missing?
```

Avoid modal error spam during batch processing.

## Undo/Redo

Refine coalescing for:

- slider drags;
- node moves;
- graph wiring;
- brush strokes;
- parameter edits;
- queue operations where appropriate.

## Acceptance Criteria

- app remains responsive with large workflows and large queues;
- common tasks are keyboard efficient;
- dependency/checkpoint errors are understandable without reading logs;
- browser, workflow, and batch modes feel like one coherent application;
- UI satisfies the interaction model defined in `SPEC.MD`.

## Exit Deliverable

A polished product surface on top of the completed graph platform.
