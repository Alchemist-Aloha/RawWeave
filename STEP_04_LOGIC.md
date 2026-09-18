# Step 04 — Logic and Adaptive Workflows

## Objective

Turn the image graph into a photographic programming environment by allowing metadata, scalar values, analysis, and logic to drive processing.

## Core Types

Add:

```text
value.Float
value.Integer
value.Boolean
value.String
value.Enum
value.Color
value.Condition
core.Metadata
```

## Connectable Parameters

Any meaningful node parameter should be optionally exposable as an input port.

Define precedence:

```text
connected value
>
per-instance/per-image override
>
stored literal
>
default
```

A parameter should retain its literal value even when a connection temporarily overrides it.

## Control Scheduler

Control/value nodes should:

- evaluate cheaply;
- avoid image allocations;
- cache simple values;
- propagate invalidation immediately;
- participate in graph dependency analysis.

## Logic Nodes

Implement:

- Constant;
- Metadata;
- Compare;
- Equal;
- Greater Than;
- Less Than;
- AND;
- OR;
- NOT;
- Switch;
- Select;
- Map Range;
- Clamp;
- Curve;
- Expression;
- String Match;
- Enum Select.

## Lazy Branching

`Switch`/`Select` should avoid evaluating unselected expensive image branches unless another consumer needs them.

Do not implement conditionals by mutating graph topology at runtime.

## Example Workflow

```text
Metadata.ISO
   ↓
Curve
   ↓
Denoise.Strength

Metadata.Camera
   ↓
Select
   ↓
Camera Profile
```

## Acceptance Criteria

- ISO can drive denoise strength;
- metadata can select a camera-specific branch or profile;
- unselected image branches are not rendered unnecessarily;
- parameter-port connection/disconnection preserves stored literal values;
- workflows serialize logic/control state correctly;
- value-node failures produce understandable graph errors.

## Tests

- scalar type checking;
- lazy branch evaluation;
- expression determinism;
- missing metadata handling;
- override precedence;
- graph invalidation when metadata changes.

## Exit Deliverable

A workflow can adapt automatically to input image properties without scripting.
