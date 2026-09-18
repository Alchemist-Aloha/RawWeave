# Step 01 — Backbone: Core Engine, Frontend Shell, and Minimal Plugin System

## Objective

Build the smallest complete application that proves the architecture from UI to Rust graph execution to plugin-defined image processing.

The milestone is intentionally narrow. It should not attempt RAW processing, sophisticated GPU scheduling, masks, batch operation, AI, or compatibility hosts.

The goal is to answer one question:

> Can the entire product be built on a small authoritative Rust graph core with first-party processing implemented through the same node/plugin mechanism available to third parties?

## Scope

### Core Backend

Create a Rust workspace with clear boundaries:

```text
crates/
  core/
  graph/
  node-api/
  project/
  image/
```

Implement:

- stable internal node identifiers;
- graph node instances;
- typed input/output ports;
- parameter schema;
- edge validation;
- cycle detection;
- graph mutation API;
- graph serialization/deserialization;
- execution of a simple acyclic graph;
- error propagation;
- node registry;
- node-pack registration.

The initial scheduler can be synchronous and simple.

### Frontend

Create:

- Tauri 2 shell;
- React + TypeScript + Vite;
- React Flow canvas;
- node search/add dialog;
- node selection;
- inspector panel;
- connection creation/removal;
- basic workflow load/save;
- basic image viewer;
- minimal notifications/error surface.

Tauri-specific calls must be isolated behind a frontend platform adapter.

### Plugin/Node API

Design the first version of the node API around:

```text
NodeDescriptor
NodeInstance
PortDescriptor
ParameterDescriptor
EvaluationContext
NodeResult
```

The public ABI does not need to be frozen permanently yet, but the architecture must avoid assumptions that only built-in Rust nodes exist.

For the initial implementation, dynamically loaded binary plugins may be deferred. A "node pack" can initially be another Rust crate registered through a common interface.

### Initial Nodes

Implement through the node API:

- Image Input;
- Constant Float;
- Exposure;
- Invert;
- Output.

Do not give these nodes privileged processing access unavailable to future third-party nodes.

## Suggested Repository Layout

```text
apps/
  desktop/
    frontend/
    src-tauri/

crates/
  core/
  graph/
  image/
  node-api/
  project/

node-packs/
  core-image/
  core-values/
```

## User Flow to Support

```text
Launch
  ↓
Add Image Input
  ↓
Select image
  ↓
Connect Exposure
  ↓
Connect Output
  ↓
Change exposure
  ↓
Preview
  ↓
Save workflow
  ↓
Close/reopen
  ↓
Reload workflow
```

## Acceptance Criteria

- graph topology lives in Rust;
- React Flow is presentation only;
- invalid connections are rejected by typed ports;
- workflows save and reload without semantic changes;
- Exposure runs through the node API;
- replacing Exposure with an externally registered node would not require changing the graph engine;
- Tauri IPC is isolated from application logic;
- node errors appear in the UI without crashing the app;
- at least one automated backend integration test builds and evaluates a simple graph.

## Tests

Backend:

- graph add/remove/connect/disconnect;
- type mismatch rejection;
- cycle rejection;
- serialization round-trip;
- deterministic evaluation;
- missing node type behavior.

Frontend:

- create node;
- connect nodes;
- change parameter;
- workflow load/save;
- error display.

## Out of Scope

- RAW files;
- wgpu;
- tile cache;
- masks;
- node groups;
- batch processing;
- native binary plugin ABI;
- OFX/GEGL;
- AI.

## Exit Deliverable

A small application that already feels like a primitive graph editor and proves that "first-party functionality is plugin functionality."
