# Step 06 — File Browser and Working Queue

## Objective

Build the everyday photographer workflow around filesystem browsing, lightweight metadata, a Working Queue, and representative Test Set images.

## File Browser

Implement:

- folder navigation;
- breadcrumb/path bar;
- thumbnail grid;
- selected image preview;
- EXIF summary;
- rating;
- flag/reject;
- sort;
- filter;
- multi-selection.

Basic file operations:

- rename;
- move;
- copy;
- reveal in system file manager;
- delete to OS trash.

The filesystem remains the source of truth.

## Metadata Persistence

Prefer:

- XMP or established metadata standards for portable ratings/flags where practical;
- app database only for cache/index/session acceleration.

## Working Queue

Queue item model:

```text
source identity/path
metadata
rating/flag
order
workflow binding
per-image overrides
processing status
output status
errors/warnings
test-set membership
```

Operations:

- add selection;
- remove;
- reorder;
- clear;
- select batch subset;
- choose current preview item.

## Test Set

Allow users to mark a small representative subset.

Support:

- previous/next test item;
- preview all at reduced quality;
- quick compare;
- retain one shared workflow;
- show per-image override indicators.

## Session

Persist:

```text
current folder
browser view settings
working queue
test set
current preview item
selected workflow
unsaved workflow working copy
viewer targets
panel layout
```

## Per-Image Overrides

Implement exposed workflow parameter overrides without cloning the graph.

UI actions:

- reset;
- copy to selected;
- apply to all;
- promote to workflow default.

## Acceptance Criteria

- user can browse a folder with hundreds of photos;
- thumbnails and EXIF populate incrementally;
- ratings persist portably where supported;
- selected images can be queued;
- current preview can switch without changing workflow;
- Test Set navigation is fast;
- session survives restart;
- queue does not become a hidden catalog.

## Exit Deliverable

A coherent Browse → Queue → Workflow loop suitable for daily photographic use.
