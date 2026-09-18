# Step 12 — AI Masks and Analysis

## Objective

Use AI as a producer of ordinary graph data, especially spatial data, rather than as a separate editing mode.

## Spatial Types

Fully implement:

```text
MaskSet
LabelMap
ConfidenceMap
DepthMap
RegionSet
```

## AI Nodes

Candidate nodes:

- Subject Segmentation;
- Semantic Segmentation;
- Prompt Segmentation;
- Face Detection;
- Skin Mask;
- Sky/Foreground convenience subgraphs;
- Depth Estimation;
- Scene Analysis.

## Generic Pattern

Prefer:

```text
Image
  ↓
Semantic Segmentation
  ↓
LabelMap
  ↓
Select Label("sky")
  ↓
Mask
```

over one hard-coded AI implementation per semantic class.

Convenience nodes can be subgraphs.

## Evaluation Policy

Allow node implementation to choose:

```text
Automatic
ManualCheckpoint
```

Examples:

Automatic:

- lightweight local face detector;
- fast local segmentation.

Manual:

- large model;
- ComfyUI workflow;
- paid remote API.

## Downstream Integration

AI output must connect directly to ordinary nodes:

```text
AI Subject Mask -> Feather -> Local Exposure
AI Depth Map -> Depth-selective Contrast
AI LabelMap -> Select Label -> Hue Shift
```

## Acceptance Criteria

- AI mask output is indistinguishable from manual mask output to downstream nodes;
- AI mask checkpoint can be stale while downstream mask operations remain live;
- label maps can be inspected;
- multiple semantic masks can be derived from one committed segmentation artifact;
- batch policy applies consistently.

## Exit Deliverable

AI becomes a reusable graph-data generator rather than a special tool mode.
