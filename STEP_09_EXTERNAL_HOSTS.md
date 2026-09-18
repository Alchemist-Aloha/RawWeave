# Step 09 — External Node Host Framework

## Objective

Create the generic compatibility boundary for foreign plugins, command-line tools, and external processes.

## Protocol

Define the External Node Host Protocol around:

```text
discover
describe
instantiate
set parameters
evaluate / submit
status
result
cancel
serialize state
destroy
capability query
```

Use a versioned protocol.

## Process Isolation

Foreign binary plugins should run out-of-process by default.

Requirements:

- host process launch;
- lifecycle monitoring;
- crash detection;
- restart;
- timeout;
- cancellation;
- stderr/log capture;
- host-version negotiation.

## Data Plane

For local binary hosts:

- shared-memory image buffers;
- explicit pixel format;
- color-domain metadata;
- region descriptors.

Avoid JSON image payloads.

## Capability Negotiation

Represent:

```text
pixel formats
bit depths
ROI support
full-frame requirement
multi-input
multi-output
thread safety
GPU support
custom UI
determinism
```

## Adapter Order

### 1. CLI Host

Use first because it exercises:

- process execution;
- parameter mapping;
- file/pipe exchange;
- failure handling.

### 2. GEGL

Map:

- pads;
- GObject properties;
- process operation.

### 3. OFX

Map:

- clips;
- parameters;
- render action;
- capabilities.

### 4. GIMP

Prefer direct GEGL for filter-like operations.

Use dedicated GIMP process integration only for higher-level document operations.

## Acceptance Criteria

- external process crash does not crash editor;
- external node behaves like an ordinary graph node;
- node descriptors render generic inspector UI;
- unsupported precision/conversion is visible to the user;
- host protocol can be versioned independently of the graph format.

## Exit Deliverable

A generic bridge making future compatibility work additive rather than architectural.
