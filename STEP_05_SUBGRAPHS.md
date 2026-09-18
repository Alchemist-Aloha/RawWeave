# Step 05 — Subgraphs and Reusable Workflow Ecosystem

## Objective

Make complex graphs reusable, distributable, and approachable through progressive disclosure.

## Subgraph Model

Implement subgraphs with:

- internal graph;
- exposed inputs;
- exposed outputs;
- exposed parameters;
- version;
- stable identity;
- metadata;
- dependency list.

Support nested subgraphs.

## User Actions

Implement:

- create subgraph from selection;
- open subgraph;
- return to parent;
- expose/hide parameter;
- expose/hide port;
- save as blueprint;
- instantiate blueprint;
- expand/copy internals where appropriate.

## WorkflowDefinition

Stabilize the reusable workflow object:

```text
graph
default values
exposed workflow parameters
outputs
subgraph dependencies
node-pack dependencies
metadata
version/hash
```

## Templates

Add template metadata:

- name;
- author;
- version;
- description;
- thumbnail;
- tags;
- license;
- recommended input type;
- minimum app version.

## Node Packs

Define manifest structure:

```text
package id
version
nodes
subgraphs
workflow templates
dependencies
platform requirements
license
```

## Dependency UX

Opening a workflow should report:

```text
available dependencies
missing dependencies
version mismatches
disabled nodes
```

Do not silently substitute incompatible versions.

## Acceptance Criteria

- a RAW development graph can be saved as one reusable subgraph;
- exposed controls work without opening internals;
- workflows can be exported and re-imported;
- missing node-pack dependencies are detected;
- nested subgraphs save/reload;
- workflow hashes/revisions are stable enough to pin future batch jobs.

## Exit Deliverable

Users can share complete photographic workflows, not just parameter presets.
