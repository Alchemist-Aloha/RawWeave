# SPEC.md

# Open-Source Node-Based RAW Photo Editor

## 1. Project Overview
This project is a cross-platform, open-source **graph-first visual computing environment for photography**.

The workflow graph is the primary document and primary user interface. RAW files, ordinary images, masks, metadata, image collections, plugins, analysis results, logic values, and AI services are data sources and processors within that graph.

The application is intended to provide:

- professional RAW development;
- a visual dataflow programming model for photography;
- reusable workflows and subgraphs;
- metadata-driven and analysis-driven processing;
- logic nodes that can select values or processing branches;
- first-class masks, including AI-generated masks;
- scene-referred color processing;
- fast interactive preview and export;
- broad third-party plugin and external-service compatibility;
- explicit manual checkpoints for expensive or generative operations;
- workflow reuse across single images, image sets, and batch jobs;
- a modern web-based desktop UI;
- a native Rust image-processing backend;
- Linux, Windows, and macOS support.

The product is not intended to imitate Lightroom's panel-and-slider philosophy.

The core conceptual model is:

```text
Workflow
   |
   +-- photographic inputs
   +-- metadata/control inputs
   +-- automatic processing nodes
   +-- logic/control nodes
   +-- masks/spatial data
   +-- manual checkpoints
   +-- outputs
```

The frontend is responsible for graph interaction and presentation. The Rust backend is authoritative for graph state, node evaluation, color management, external-node execution, rendering, cache management, artifact management, and project serialization.

## 2. Design Goals
### 2.1 Primary Goals

1. Make the workflow graph the primary document rather than an optional advanced view.
2. Provide a ComfyUI/Resolve-like visual programming environment specialized for photography.
3. Keep ordinary photographic edits live, automatic, and non-destructive.
4. Allow metadata, analysis, constants, expressions, and logic to drive node parameters.
5. Allow conditional selection of processing branches without mutating graph topology at runtime.
6. Make masks first-class graph data, including AI-generated segmentation and prompt-based masks.
7. Support reusable subgraphs, workflow templates, and node packs.
8. Reuse one workflow across many photos with per-image overrides.
9. Support large RAW files efficiently through tiled, resolution-aware processing.
10. Keep the image-processing core native, deterministic where possible, and high performance.
11. Support third-party plugins and external services through adapters rather than hard-coded integrations.
12. Isolate unstable or legacy plugins from the main process.
13. Preserve generated/manual checkpoint results as project-semantic artifacts.
14. Use a stable, versioned project/workflow format intended to remain reproducible for many years.
15. Keep the frontend shell replaceable without affecting processing semantics.

### 2.2 Non-Goals

The first versions do not need to compete directly with Photoshop in:

- pixel-perfect freehand painting;
- complex vector editing;
- typography;
- page layout;
- full PSD feature parity;
- frame-by-frame video editing.

The first versions also do not require a Lightroom-style catalog/import workflow. File-based and workflow-based operation must work without a catalog.

The initial focus is a visual photographic computation environment rather than a traditional asset-management application.

## 3. High-Level Architecture
```text
                         Tauri 2 Desktop Shell
                                  |
                    Frontend Platform Adapter
                                  |
             +--------------------+--------------------+
             |                                         |
     React + TypeScript                            Rust Core
             |                                         |
     Graph-First Canvas                           Workflow Graph
     React Flow                                   Scheduler
     Node Search                                  Typed Value Engine
     Viewer / Compare                             Tile Cache
     Inspector                                    RAW Pipeline
     Workflow Browser                             Color Engine
     Dynamic Node UI                              wgpu GPU Engine
             |                                    Artifact Store
             |                                         |
             +-------------- UI API -------------------+
                                                       |
                                          External Node Host Protocol
                                                       |
          +---------------------+----------------------+------------------+
          |                     |                      |                  |
      Native Host            OFX Host              GEGL Host          GIMP Host
          |                     |                      |                  |
       plugins              OFX plugins             GEGL ops          GIMP plugins

          +---------------------+----------------------+
          |                                            |
     ComfyUI Provider                            HTTP AI Provider
          |                                            |
   local/LAN/remote                           online AI services
```

The workflow graph is the authoritative computational document.

The Rust backend owns:

- graph topology;
- typed connections;
- parameter values and overrides;
- workflow definitions and workflow instances;
- metadata/control value propagation;
- RAW processing;
- color transforms;
- render revisions;
- cache state;
- checkpoint state;
- committed generated artifacts;
- external-node execution;
- project/workflow serialization;
- export state.

The frontend owns presentation-only state such as selection, viewport position, panel layout, and temporary interaction state.

Tauri is an application-shell dependency, not an application-architecture dependency.

## 4. Technology Stack
### 4.1 Desktop Shell

Use:

```text
Tauri 2
```

Reasons:

- stable production release;
- Rust-native backend;
- small application footprint;
- system WebView rather than bundled Chromium;
- Windows, macOS, and Linux support;
- clean separation between frontend and native backend.

Tauri-specific APIs must be isolated behind a thin platform adapter.

Frontend code should call project-owned interfaces such as:

```text
editor.openProject(...)
editor.setNodeParameters(...)
editor.renderViewport(...)
editor.generateCheckpoint(...)
```

rather than importing Tauri `invoke` calls throughout the UI.

On the Rust side:

```text
src-tauri/
    commands.rs
```

must translate shell commands into a Tauri-independent `EditorCore` API.

This minimizes the future Tauri 2 -> Tauri 3 migration surface and keeps replacement frontends possible.

GPUI may be reevaluated later as a native Rust frontend if its public API reaches the stability required by the project. It is not the initial UI dependency.

### 4.2 Frontend

Recommended stack:

```text
React
TypeScript
Vite
React Flow
Zustand
Tailwind CSS
Radix UI or equivalent accessible primitives
```

Responsibilities:

- node graph visualization;
- node interaction;
- parameter inspector;
- library/browser views;
- viewer chrome;
- toolbar;
- histogram/scopes presentation;
- notifications;
- dynamically generated node/plugin controls;
- AI checkpoint controls;
- preferences;
- keyboard shortcuts.

### 4.3 Backend

Primary language:

```text
Rust
```

Responsibilities:

- graph execution;
- tile scheduling;
- caching;
- RAW decoding and development;
- color transforms;
- image buffers;
- GPU processing;
- external-node management;
- compatibility hosting;
- AI checkpoint orchestration;
- committed artifact management;
- project serialization;
- export;
- metadata;
- history;
- undo/redo;
- revision tracking.

### 4.4 GPU

Use:

```text
wgpu
WGSL
```

Target native graphics APIs:

```text
Windows -> Direct3D 12
macOS   -> Metal
Linux   -> Vulkan
```

Browser WebGPU must not be a core requirement.

### 4.5 RAW Processing

Initial supported backends may include:

```text
LibRaw
RawSpeed
```

A Rust-native decoder may be evaluated later if its API and format coverage become sufficiently stable.

RAW decoding must be abstracted behind an internal interface so the decoder can be changed without affecting the node graph.

### 4.6 Color Management

Recommended:

```text
OpenColorIO
LittleCMS where ICC compatibility is required
```

The pipeline should be scene-referred internally wherever possible.

### 4.7 Metadata

Recommended:

```text
Exiv2
```

or another mature metadata library behind an abstraction layer.

### 4.8 Database

Use:

```text
SQLite
```

for:

- library metadata;
- thumbnails;
- catalog information;
- cache indexes;
- search;
- plugin registry;
- external-provider registry;
- application settings where appropriate.

## 5. Core Processing Model
The application is built around a typed dataflow graph.

The **workflow is the document**. Images are inputs to the workflow rather than the conceptual owner of a list of adjustments.

Example:

```text
RAW Input
   |
   +-----------> Metadata ---------> ISO ------> Curve ----+
   |                                                       |
   v                                                       v
RAW Develop -> Color -> AI Subject Mask -----> Local Grade -> Denoise -> Output
                    \                         /
                     +---- spatial mask ------+
```

Each node may consume and produce any supported typed graph values, not only images.

The graph must support:

- photographic image flow;
- RAW/mosaic flow;
- masks and spatial data;
- scalar/control values;
- metadata;
- image sets;
- multiple inputs and outputs;
- serial and parallel branches;
- lazy conditional routing;
- manual checkpoints;
- reusable subgraphs;
- workflow templates;
- multiple viewers/outputs;
- cached intermediate results.

The graph topology should normally remain static during evaluation. Runtime decisions are represented by typed values and lazy `Switch`/`Select` nodes rather than dynamically adding or removing graph nodes.

## 6. Workflow Definitions and Workflow Instances
Reusable computation is separated from concrete processing sessions.

The core objects are:

```text
WorkflowDefinition
WorkflowInstance
Session
WorkingQueue
```

A `WorkflowDefinition` contains reusable computation:

```text
graph topology
node types and versions
default parameters
subgraphs
exposed workflow inputs
exposed workflow parameters
outputs
dependencies
workflow metadata
```

A `WorkflowInstance` binds a workflow definition to one concrete source and optional overrides:

```text
workflow = Portrait-v3
input = DSC_9183.CR3

overrides:
    exposure_bias = +0.33
    skin_strength = 0.60
```

A `Session` contains transient working context:

```text
current folder
working queue
current preview item
test-set membership
per-image overrides
selected workflow + revision
viewer targets
batch/output settings
UI layout
batch job state
```

The same `WorkflowDefinition` may be reused for one photo or hundreds of photos without duplicating the full graph.

Workflow definitions must be independently shareable and versionable.

Sessions are not workflows. Saving a session must not silently modify the reusable workflow definition unless the user explicitly saves workflow changes.

## 7. Core User Journey
The primary user journey is:

```text
BROWSE
    |
    v
SELECT / RATE
    |
    v
WORKING QUEUE
    |
    v
LOAD OR BUILD WORKFLOW
    |
    v
PREVIEW / TINKER / COMPARE
    |
    v
PREFLIGHT
    |
    v
BATCH PROCESS
    |
    v
REVIEW RESULTS / RETRY FAILURES
```

The three main working modes are therefore:

```text
Browse
Build / Preview
Batch
```

These modes share one session and should transition quickly without forcing import/catalog operations.

The graph remains the primary computational document. The browser and queue are input-selection and job-management surfaces around that graph.

## 8. File Explorer and Photo Browser
The application should include a lightweight filesystem-based photo browser.

The browser must not require importing files into a proprietary catalog before use.

Minimum capabilities:

```text
folder tree / breadcrumb navigation
thumbnail grid
single-image preview
EXIF summary
rating
flag/reject state
basic filtering
sort by name/date/rating/metadata
multi-select
```

Useful EXIF display should include where available:

```text
camera
lens
capture date/time
ISO
shutter speed
aperture
focal length
dimensions
orientation
file type
```

Basic file operations should include:

```text
rename
move
copy
reveal/open in system file manager
delete to OS trash
```

Permanent destructive deletion should not be the default.

Ratings and basic portable metadata should use standard sidecars such as XMP where practical. Application-specific workflow/session data remains in the application's own formats.

The filesystem remains the source of truth.

## 9. Working Queue
Selected browser items can be added to a **Working Queue**.

The Working Queue is a first-class session object but is not a catalog.

Each queue item should track:

```text
source path / source identity
metadata snapshot
rating / flag
queue order
processing status
current workflow binding
per-image parameter overrides
checkpoint status
output status
warnings/errors
```

Queue operations should include:

```text
add selected files
remove
clear
reorder
filter queue
mark/unmark test images
select current preview item
select batch subset
```

A queue may contain:

```text
one image
hundreds of independent RAW images
explicit multi-image sets for HDR/focus-stack workflows
```

The queue should be recoverable as part of a saved session.

The queue is temporary processing context. It must not be required for simply opening one image or one workflow.

## 10. Test Set and Representative Preview Images
A queue may define a small **Test Set** of representative images.

Example:

```text
bright outdoor
dark indoor
high ISO
portrait
group photo
mixed lighting
```

The Test Set is used while designing or tuning a workflow.

Users should be able to:

```text
mark/unmark queue item as test image
switch previous/next test image quickly
render all test images at preview quality
compare workflow behavior across test images
```

A workflow edit should remain shared across all test images.

Per-image overrides remain isolated to the affected queue item.

The purpose is to verify that adaptive metadata/logic processing generalizes before a large batch is launched.

A future multi-preview mode may display several test images simultaneously or as a contact sheet with the same workflow revision applied.

## 11. Per-Image Overrides
The workflow remains shared across the queue, but queue items may override selected exposed workflow parameters.

Example:

```text
Workflow:
    Wedding-v4

Image DSC_1001.CR3:
    exposure_bias = +0.30

Image DSC_1002.CR3:
    exposure_bias = -0.15
```

Overrides must not fork or duplicate the graph.

Only parameters explicitly exposed by the workflow/subgraph should normally be offered as per-image overrides.

Override precedence:

```text
connected graph value
    >
per-image exposed-parameter override
    >
workflow-instance/session override
    >
workflow default
```

The UI must clearly indicate when the current preview image has overrides.

Users should be able to:

```text
reset override
copy override to selected images
apply override to all queue items
promote value into workflow default
```

## 12. Workflow Templates and Node Packs
Workflow templates are a major product concept, not merely parameter presets.

Examples:

```text
RAW Development
    Natural RAW
    High Dynamic Range
    Filmic Base

Portrait
    Natural Skin
    Studio Portrait

Landscape
    Golden Hour
    HDR Landscape

Technical
    Focus Stack
    Exposure Merge
    Panorama

AI
    Subject Isolation
    Object Removal
    Generative Fill
```

A workflow template should include:

```text
workflow definition
thumbnail
description
author
version
license
tags
required nodes/packages
minimum application version
recommended input types
```

Third-party extensions should commonly be distributed as **node packs**.

Example:

```text
FilmLab Node Pack
    Film Curve
    Halation
    Grain
    Dye Simulation
```

A node-pack manifest should contain:

```text
package id
package version
nodes
subgraphs/templates
dependencies
minimum host version
license
platform requirements
```

## 13. Subgraph Blueprints and Progressive Disclosure
Subgraphs are first-class reusable workflow components.

A complex photographic process can appear externally as one node:

```text
RAW -> [ Portrait Develop ] -> Image
```

while internally containing:

```text
Decode
  -> Black Level
  -> White Balance
  -> Demosaic
  -> Lens Correction
  -> Skin Tone
  -> Contrast
  -> Sharpen
```

A subgraph exposes only selected inputs, outputs, and parameters.

Users should be able to:

```text
Open Subgraph
Expand to Nodes
Save as Blueprint
Publish as Workflow Component
```

This provides progressive disclosure: common workflows remain approachable while advanced users can inspect and modify every stage.

Convenience nodes such as `RAW Develop` may therefore be implemented as standard subgraphs rather than opaque monolithic features whenever practical.

## 14. Typed Node Ports
The graph type system must be extensible and must treat images, masks, metadata, and control values as equally valid graph data.

Four primary data families are defined.

### Photographic Pixel Data

```text
core.Image
core.ImageSet
core.ImageBatch

raw.Mosaic

color.SceneLinearRGB
color.DisplayRGB
color.AIImageRGB
```

### Spatial Data

```text
core.Mask
core.MaskSet
core.LabelMap
core.ConfidenceMap
core.DepthMap
core.RegionSet
core.Geometry
```

### Control Data

```text
value.Float
value.Integer
value.Boolean
value.String
value.StringSet
value.Enum
value.Color
value.Condition
```

### Descriptive Data

```text
core.Metadata

metadata.CameraModel
metadata.LensModel
metadata.ISO
metadata.Aperture
metadata.ShutterSpeed
metadata.FocalLength
metadata.DateTime
metadata.Rating

raw.CameraProfile
raw.LensProfile
```

AI/external systems may register additional namespaced types:

```text
ai.Prompt
ai.Latent
ai.Conditioning
ai.Model
ai.VAE
ai.Control
ai.Seed

external.<namespace>.<type>
```

A type is identified by a stable namespaced identifier rather than a closed Rust enum.

Examples:

```text
Demosaic
raw.Mosaic -> color.SceneLinearRGB

AI Segmentation
core.Image -> core.LabelMap

Select Label
core.LabelMap + value.String -> core.Mask

Compare
value.Float + value.Float -> value.Boolean
```

Conversion adapters may exist where semantically valid.

## 15. Connectable Parameters
Meaningful node parameters should be optionally exposable as typed graph input ports.

Default form:

```text
Denoise
Strength = 0.30
```

After exposing the parameter:

```text
                 Strength
                    |
                    v
              +-----------+
              | Denoise   |
              +-----------+
```

A connected parameter value overrides the node's stored literal/default value. Disconnecting the input restores the stored value.

Examples of connectable parameters:

```text
exposure
white balance temperature
denoise strength
sharpen amount
opacity
LUT selection
blend amount
export quality
AI generation strength
```

This is the primary mechanism by which metadata, analysis, constants, expressions, and logic drive photographic processing.

Subgraphs may expose internal parameter ports as high-level external controls.

## 16. Logic and Control Nodes
Logic/control nodes operate on small typed values and should be part of the core node library.

Initial nodes should include:

```text
Constant
Compare
Equal
Greater Than
Less Than

AND
OR
NOT

Switch
Select
Map Range
Remap
Clamp
Curve
Expression

String Match
Enum Select
```

Example: ISO-driven denoise strength.

```text
RAW Input
   |
   +--> Metadata --> ISO --> Curve --------------------+
   |                                                  |
   v                                                  v
Develop ------------------------------------------> Denoise --> Output
```

Example: conditional branch selection.

```text
                    +--> Light Denoise --+
Image --------------+                    +--> Switch --> Output
                    +--> Heavy Denoise --+      ^
                                               |
Metadata --> ISO --> > 3200 ------------------+
```

Conditional routing must be **lazy** where possible. If `Switch` selects one branch, unselected expensive branches should not be evaluated unless another downstream consumer requires them.

Graph topology should not be mutated merely to express conditionals.

## 17. Metadata as First-Class Graph Data
Metadata is accessible through graph nodes rather than hidden application-only state.

A metadata node may expose:

```text
ISO
aperture
shutter speed
focal length
camera model
lens model
capture time
orientation
rating
tags
GPS when available
```

Example:

```text
Image Metadata
    |
    +--> ISO ----------> Denoise Curve
    |
    +--> Camera Model --> Profile Lookup
    |
    +--> Focal Length --> Sharpen Amount
```

Metadata-driven workflows should remain portable and deterministic given the same metadata inputs.

Image-analysis nodes may produce control values using the same system:

```text
Mean Luminance
Highlight Fraction
Face Count
Sharpness Estimate
Noise Estimate
Dynamic Range Estimate
```

These outputs may drive any compatible parameter port or logic node.

## 18. ImageSet and Batch Dataflow
`core.ImageSet` is a first-class type.

This enables graph-native multi-image and batch workflows:

```text
Folder Input
    |
ImageSet
    |
Workflow Map
    |
Resize
    |
JPEG Export
```

and computational photography:

```text
Exposure Set -> HDR Merge -> Develop -> Output
Focus Set    -> Focus Stack -> Sharpen -> Output
```

Future collection-oriented nodes may include:

```text
Map
Filter
Sort
Select
Group
Stack
Pair
```

Batch processing should use the same workflow semantics as single-image editing rather than a separate automation system.



The Working Queue and `core.ImageSet` are related but distinct.

```text
Working Queue
    session/job concept containing source items

core.ImageSet
    typed graph value used when a workflow intentionally processes a collection
```

Normal batch application of one workflow to 500 photos does not require converting those 500 sources into one `ImageSet` graph value. The batch scheduler evaluates the same workflow instance independently for each queue item.

`ImageSet` is used when the algorithm itself requires multiple images as one logical input, such as HDR merge, focus stacking, panorama, or collection-level processing.
## 19. Internal Image Representation

The main internal processing format should be:

```text
RGBA
32-bit floating point
scene-linear
unbounded
```

Values outside the nominal 0-1 range must be allowed.

Example:

```text
R = 3.52
G = 1.74
B = -0.03
```

This is important for:

- exposure operations;
- HDR processing;
- tone mapping;
- highlight recovery;
- gamut mapping;
- compositing;
- color-space transforms.

Lower-precision representations may be used for cached previews or compatibility nodes where necessary.

---

## 20. RAW Pipeline

A typical RAW path should be conceptually:

```text
RAW file
   |
RAW decode
   |
black-level correction
   |
bad-pixel correction
   |
highlight reconstruction
   |
demosaic
   |
lens correction
   |
camera color transform
   |
scene-linear working RGB
   |
creative node graph
```

Individual RAW stages may eventually become explicit nodes.

The initial implementation may expose a combined `RAW Develop` node containing several internal stages.

---

## 21. Color Pipeline

The processing model should separate:

1. input characterization;
2. working space;
3. creative processing;
4. output/display transform.

Example:

```text
Camera RGB
    |
Camera Profile
    |
Scene-Linear Working Space
    |
Creative Nodes
    |
Display / Output Transform
    |
Monitor / Export Space
```

Possible working spaces:

```text
Linear Rec.2020
ACEScg
custom OCIO-defined spaces
```

Possible outputs:

```text
sRGB
Display P3
Rec.2020
HDR PQ
HDR HLG
custom OCIO outputs
```

Nodes must either declare their expected color domain or operate in a color-space-independent manner.

---

## 22. Graph Evaluation
The engine uses demand-driven graph evaluation.

Every node declares an evaluation policy and locality.

Suggested model:

```rust
enum EvaluationPolicy {
    Automatic,
    ManualCheckpoint,
}

enum EvaluationLocality {
    TileLocal,
    RegionAware,
    FullFrame,
    Global,
    ExternalJob,
}
```

Normal photographic nodes use `Automatic`. When their dependencies change, the scheduler invalidates and reevaluates them as needed.

A manual checkpoint does not reevaluate when dependencies change. Instead, the node becomes stale and preserves its last committed output until the user explicitly requests generation/evaluation.

Typical mappings:

```text
Exposure              Automatic / TileLocal
Gaussian Blur         Automatic / RegionAware
Auto Exposure         Automatic / Global
AI Img2Img            ManualCheckpoint / ExternalJob
AI Inpaint            ManualCheckpoint / ExternalJob
Panorama Stitch       ManualCheckpoint / FullFrame
```

For ordinary render requests, evaluation includes:

```text
graph revision
output node
region of interest
scale / mip level
quality level
color transform
```

Example:

```text
RenderRequest:
    node = output
    region = x:2400..3900, y:1200..2300
    scale = 0.25
    quality = Interactive
```

The request propagates upstream only as far as required, stopping at committed manual-checkpoint artifacts rather than automatically reevaluating those checkpoints.

## 23. Tile-Based Processing

Use tiled image processing.

Suggested initial tile sizes:

```text
256 x 256
or
512 x 512
```

Tiles should be individually cacheable.

Cache keys should include at least:

```text
source identity
node identity
node implementation version
node parameters
upstream dependency hash
tile coordinates
mip level
processing quality
relevant color state
```

Changing a downstream node must not invalidate unrelated upstream tiles.

Example:

```text
RAW          cached
Demosaic     cached
Exposure     cached
Saturation   changed
Film Look    invalidated
Output       invalidated
```

---

## 24. Mip Pyramid and Preview Rendering

Large source images should use a mip pyramid.

Example:

```text
Level 0  8000 x 6000
Level 1  4000 x 3000
Level 2  2000 x 1500
Level 3  1000 x 750
Thumbnail
```

At normal fit-to-window zoom, the engine should render only the required lower-resolution representation.

At 100% zoom, only visible native-resolution tiles should be evaluated.

---

## 25. Progressive Rendering

Interactive editing should support progressive refinement.

While a user drags a slider:

```text
low-resolution render
        |
medium-resolution render
        |
full viewport-quality render
```

Full-resolution image evaluation is required only when appropriate:

- export;
- 100% viewing;
- pixel inspection;
- full-quality snapshot;
- explicit final render.

The scheduler should prioritize interaction latency over immediate final quality.

---

## 26. Viewer Architecture
The viewer is an inspection surface for graph outputs rather than the conceptual owner of the document.

Any compatible image-producing port should be viewable.

The application should support at least:

```text
Viewer A
Viewer B
```

and comparison modes such as:

```text
side-by-side
split/wipe
blink
difference
```

Examples:

```text
Demosaic -----> Viewer A
Final Grade --> Viewer B
```

The initial viewer remains inside the WebView.

The frontend must not receive full-resolution float images. Rust renders only required viewport tiles/preview representations.

A native GPU viewport may be introduced later only if profiling shows WebView presentation to be a material bottleneck.


The viewer also has a session-level **current preview item** drawn from the Working Queue.

Changing the current preview item must not change the workflow definition.

Recommended navigation:

```text
previous/next queue item
previous/next test image
jump to failed batch item
```

Graph changes should immediately render against the current preview item and may optionally trigger low-resolution previews for the Test Set.

Viewer targets and queue preview identity are separate concepts:

```text
queue item = which source instance is being evaluated
viewer target = which graph output/intermediate result is being displayed
```
## 27. Frontend State Model

The frontend must not own authoritative processing state.

Use:

```text
React state  -> UI presentation state
Rust state   -> authoritative editing state
```

React Flow represents the graph visually.

The Rust backend owns the actual graph.

Frontend graph operations are commands:

```text
add_node
remove_node
connect_nodes
disconnect_nodes
set_parameters
move_node_ui
group_nodes
```

Visual-only information such as node position may be persisted separately.

---

## 28. Revision Model
Every graph mutation increments a monotonically increasing graph revision.

Example:

```text
GraphRevision = 736
```

Every automatic render request is tagged with a revision.

If an older render finishes after a newer render:

```text
render revision 734
current revision 736
=> discard render 734
```

Manual checkpoint generation has an additional immutable input snapshot revision.

If generation starts from graph revision 187 and the upstream graph changes to revision 188 before generation completes, the completed result still records revision 187. If committed, it is immediately considered stale relative to the current upstream state.

This prevents stale asynchronous results from being represented as current.

## 29. Frontend-Backend Communication

Do not send high-frequency raw input events directly to Rust.

Example of undesirable behavior:

```text
mousemove -> IPC
mousemove -> IPC
mousemove -> IPC
mousemove -> IPC
```

Instead:

```text
UI interaction
    |
local temporary state
    |
throttled/coalesced update
    |
Tauri command
```

For parameter changes:

```text
set_node_parameters({
    node: 42,
    changes: {
        exposure: 0.73,
        contrast: 1.10
    }
})
```

Updates may be sent at an interaction-friendly rate such as 30-60 Hz, with coalescing where appropriate.

---

## 30. Node Types
Initial built-in node classes should include:

### Photographic processing

```text
RAW Input
RAW Develop
Exposure
White Balance
Curves
Color Wheels / Lift-Gamma-Gain
Color Balance
Channel Mixer
Saturation
Color Space Transform
LUT
Tone Mapping
Sharpen
Blur
Noise Reduction
Geometry
Crop
Transform
Mixer
Layer Mixer
Output
```

### Spatial and mask processing

```text
Painted Mask
Gradient Mask
Qualifier
Mask Combine
Mask Subtract
Mask Intersect
Mask Invert
Mask Blur
Mask Feather
Mask Expand
Mask Contract
Select Label
```

### Metadata and control

```text
Metadata
Constant
Compare
AND / OR / NOT
Switch
Select
Map Range
Curve
Clamp
Expression
Profile Lookup
```

### Multi-image

```text
ImageSet Input
HDR Merge
Focus Stack
ImageSet Map
```

### External and AI

```text
Plugin / External Node
AI Checkpoint
AI Segmentation
AI Prompt Mask
Node Group / Subgraph
```

Checkpoint behavior remains a general evaluation policy rather than an AI-only implementation.

## 31. Node Groups
Node groups evolve into first-class **subgraph blueprints**.

A subgraph can contain arbitrary image, mask, metadata, logic, and external nodes.

Example:

```text
Adaptive Denoise
+--------------------------------+
| Metadata -> ISO -> Curve       |
|                    |           |
| Image ----------> Denoise      |
|                    |           |
|                 Sharpen        |
+--------------------------------+
```

Externally it may expose:

```text
Image input
Image output
ISO response curve
Maximum denoise
Sharpen amount
```

Subgraphs should be:

- nestable;
- independently versionable;
- exportable/importable;
- publishable in node packs;
- expandable for inspection;
- usable as workflow templates.

A subgraph's external interface must remain stable even when its internal implementation changes, subject to normal versioning rules.

## 32. Mask System
Masks are first-class spatial graph values.

Initial mask sources:

```text
painted mask
linear gradient
radial gradient
luminance key
color key
external image mask
AI segmentation
AI prompt-guided segmentation
AI subject/face/sky masks
```

Mask operations remain independent from mask-generation methods:

```text
invert
blur
feather
expand
contract
threshold
combine
multiply
add
subtract
intersect
```

AI should generally act as another mask source rather than owning downstream cleanup.

Example:

```text
Image
  |
AI Subject Mask
  |
Expand
  |
Feather
  |
Local Exposure
```

AI segmentation may produce richer spatial outputs:

```text
Mask
MaskSet
LabelMap
ConfidenceMap
RegionSet
DepthMap
```

A generic semantic segmentation workflow is preferred over hard-coding one node per concept where possible:

```text
Image
  |
AI Semantic Segmentation
  |
LabelMap
  |
Select Label("sky")
  |
Mask
  |
Sky Grade
```

Convenience nodes such as `Sky Mask` may be packaged as subgraphs.

## 33. Scopes

The application should eventually provide:

```text
Histogram
Waveform
RGB Parade
Vectorscope
False Color
Clipping Overlay
Gamut Warning
Pixel Inspector
Zebra
```

Scopes should consume preview/render data efficiently and should avoid unnecessary full-resolution CPU readback.

---

## 34. External Node Host Architecture
Third-party compatibility and external computation are primary design goals.

The application must not hard-code each ecosystem into the graph engine.

Instead, define a canonical **External Node Host Protocol**.

Foreign plugin systems, command-line tools, local services, and remote AI services are translated to the internal node model by host/provider adapters.

Architecture:

```text
                             Rust Core
                                |
                     External Node Host Protocol
                                |
          +---------------------+----------------------+
          |                     |                      |
      Native Host            OFX Host              GEGL Host
          |                     |                      |
       plugins              OFX plugins             GEGL ops
                                |
                            GIMP Host
                                |
                          GIMP plugins

          +---------------------+----------------------+
          |                                            |
     ComfyUI Provider                            HTTP AI Provider
          |                                            |
   workflow execution                          online AI services
```

The core should need only a small common vocabulary:

```text
describe
instantiate
evaluate / submit
status
cancel
result
serialize_state
query_capabilities
```

This abstraction is intentionally broader than a conventional plugin API.

## 35. External Hosts and Compatibility Layers Are Modular
Compatibility and provider layers themselves should be modular.

Example directory structure:

```text
external/
    native/
    ofx/
    gegl/
    gimp/
    cli/
    comfyui/
    http-ai/
    photoshop8bf/
```

A third party should be able to implement a new host/provider without changing the main graph engine.

This allows future support for:

```text
binary plugin ecosystems
Python services
AI inference servers
remote rendering
research executables
model runtimes
cloud APIs
```

Host/provider implementations may be in-process only when trusted and appropriate. Untrusted binary plugins remain out-of-process by default.

## 36. Canonical External Node Descriptor
Every compatibility host/provider translates foreign operations into a shared internal description.

Conceptually:

```text
NodeDescriptor {
    id
    name
    vendor
    version

    inputs[]
    outputs[]

    parameters[]

    evaluation_policy
    evaluation_locality
    determinism
    execution_location

    capabilities

    supported_pixel_formats
    supported_color_spaces
    supported_bit_depths

    ui_metadata
}
```

Suggested determinism metadata:

```text
Deterministic
Seeded
Nondeterministic
```

Suggested execution locations:

```text
Native
PluginProcess
LocalService
RemoteService
```

The frontend dynamically generates parameter interfaces from this descriptor.

## 37. Plugin Parameter Schema

Example:

```json
{
  "name": "Film Grain",
  "parameters": [
    {
      "id": "strength",
      "name": "Strength",
      "type": "float",
      "min": 0,
      "max": 1,
      "default": 0.3
    },
    {
      "id": "size",
      "name": "Size",
      "type": "float",
      "min": 0.1,
      "max": 10,
      "default": 1
    },
    {
      "id": "monochrome",
      "name": "Monochrome",
      "type": "boolean",
      "default": false
    }
  ]
}
```

The frontend should generate appropriate controls automatically.

---

## 38. Plugin Capability Negotiation

Not all plugins support the same rendering model.

Capabilities may include:

```text
float32
float16
uint16
uint8

tiled rendering
full-frame only

ROI support
multi-input
multiple outputs
GPU support
thread-safe
sequential-only
custom UI
host-generated UI
```

Example:

```text
Modern Plugin
    float32
    tiled
    ROI
    multi-threaded

Legacy Plugin
    uint8
    full-frame only
    no ROI
```

The engine should automatically adapt.

Example legacy path:

```text
SceneLinear float32
       |
convert to required space
       |
convert to uint8
       |
legacy plugin
       |
convert to float32
       |
restore working-space representation
```

The UI should warn users when a compatibility conversion reduces precision or performance.

---

## 39. Plugin Isolation

Foreign plugins should run out-of-process by default.

Do not load arbitrary third-party binary plugins directly into the main editor process unless explicitly trusted.

Architecture:

```text
Editor Process
      |
      | IPC
      |
Plugin Host Process
      |
foreign plugin
```

Benefits:

- crash isolation;
- easier recovery;
- easier sandboxing;
- easier compatibility management;
- possible architecture translation;
- simpler plugin lifecycle control.

If a plugin crashes:

```text
Plugin host crashed:
"Example Sharpen 2.1"

[Restart]
[Disable]
```

The editor should remain running.

---

## 40. Plugin Control Plane and Data Plane

Use separate channels for control and pixel data.

### Control Plane

Recommended:

```text
Protobuf or another versioned binary RPC protocol
```

Operations:

```text
discover_plugins
describe_plugin
instantiate
set_parameter
render
save_state
load_state
destroy
query_capabilities
```

### Data Plane

Use:

```text
shared memory
```

for CPU image buffers.

Do not transmit large image arrays through JSON or ordinary RPC serialization.

Example render message:

```text
RenderRequest {
    input_buffer
    output_buffer
    region
    pixel_format
    color_space
    revision
}
```

---

## 41. Future GPU Plugin Transport

GPU resource sharing may be implemented later.

Possible platform mechanisms:

```text
Vulkan external memory
D3D12 shared resources
Metal IOSurface
```

This is not required for the initial plugin implementation.

Shared-memory CPU interoperability should be the initial compatibility baseline.

---

## 42. Native Plugin ABI

The application may expose a stable C ABI for high-performance native plugins.

Do not expose:

```text
Rust ABI
C++ ABI
```

as the primary long-term plugin interface.

A C ABI allows plugin implementations in:

```text
C
C++
Rust
Zig
Fortran
other native languages
```

The native ABI should map cleanly onto the same canonical node protocol.

---

## 43. OFX Compatibility

OpenFX should be an important compatibility target.

Translate:

```text
OFX clips       -> node ports
OFX parameters  -> node parameters
OFX render      -> node render
OFX metadata    -> node metadata
```

Custom interactive UI support may be limited initially.

Host-generated UI should be preferred where possible.

---

## 44. GEGL Compatibility

GEGL should be supported directly as a compatibility layer.

Translate:

```text
GEGL pads        -> node ports
GObject props    -> node parameters
GEGL operations  -> node implementation
```

GEGL is particularly suitable because its processing model is already graph-oriented.

GEGL compatibility should not require launching GIMP.

---

## 45. GIMP Compatibility

GIMP compatibility should exist but should not be the primary processing backend.

Where possible:

```text
GIMP/GEGL filter -> direct GEGL host
```

For higher-level GIMP plugins that manipulate:

```text
layers
selections
paths
canvas
documents
```

use a dedicated GIMP compatibility host.

Such a node may internally:

```text
create temporary document
run plugin operation
extract/flatten result
return image
```

These nodes should be marked as compatibility nodes and may require full-frame processing.

---

## 46. CLI Compatibility Host

Provide a generic compatibility host for arbitrary image-processing executables.

Examples:

```text
ImageMagick
custom Python script
custom command-line denoiser
research image-processing binary
```

The CLI host should define:

```text
input format
output format
arguments
parameter mapping
temporary-file or pipe behavior
timeout
environment
```

This is a compatibility feature, not the preferred high-performance path.

---

## 47. Compatibility Metadata Database

Support community-maintained compatibility overrides.

Example:

```yaml
plugin: "com.vendor.oldsharpen"

node:
  category: "Sharpen"

parameters:
  amount:
    name: "Amount"
    range: [0, 200]
    unit: "%"

  mode:
    type: enum
    values:
      0: Normal
      1: Fine
      2: Strong

hide:
  - internal_p1
  - internal_p2
```

Possible repository:

```text
plugin-compat-db/
    ofx/
    gegl/
    gimp/
    photoshop/
    legacy/
```

This allows compatibility quirks to evolve independently from the application.

---

## 48. Plugin Node Categories

Not every plugin is a simple image filter.

Canonical categories should include:

```text
ImageEffect
Generator
MaskGenerator
GeometryEffect
Composite
ImageLoader
ImageExporter
GraphOperation
```

Compatibility hosts must map foreign plugin semantics into the closest appropriate category.

---

## 49. Manual Checkpoint Semantics
Manual checkpoints are stateful graph barriers.

A normal node behaves conceptually as:

```text
output = f(current input, current parameters)
```

A checkpoint behaves as:

```text
committed_output = explicit_evaluate(input_snapshot, generation_parameters)
```

Checkpoint states:

```text
UNGENERATED
    |
 Generate
    v
GENERATING
    |
 success
    v
CURRENT
    |
 upstream/generation input changes
    v
STALE
```

Additional terminal/transient states:

```text
FAILED
CANCELLED
```

Suggested Rust model:

```rust
enum CheckpointStatus {
    Ungenerated,
    Generating,
    Current,
    Stale,
    Failed,
}
```

Changing upstream data does not automatically invoke the checkpoint.

If a committed result already exists, downstream automatic nodes continue evaluating from that committed result even while the checkpoint is stale.

Example:

```text
RAW -> Exposure* -> [ AI Inpaint: STALE ] -> Grade -> Sharpen -> Output
                              |
                    last committed result
```

This preserves a responsive photographic workflow and prevents accidental expensive or nondeterministic execution.

## 50. Checkpoint Dependency Tracking
A manual checkpoint stores both the hash of its current dependencies and the hash used to create its committed result.

Conceptually:

```rust
struct ManualCheckpointState {
    current_dependency_hash: Hash,
    committed_dependency_hash: Option<Hash>,
    committed_artifact: Option<ArtifactId>,
}
```

State is derived as:

```text
no committed artifact
    -> UNGENERATED

current_dependency_hash == committed_dependency_hash
    -> CURRENT

current_dependency_hash != committed_dependency_hash
    -> STALE
```

The dependency hash must include every value that affects generation, including upstream image/mask content and generation parameters.

For AI nodes this may include:

```text
input image hash
mask hash
prompt
negative prompt
model/model hash
workflow version
seed
sampler
scheduler
steps
guidance value
denoise strength
reference images
provider-relevant parameters
```

## 51. Checkpoint Generation Transaction
Pressing `Generate` or `Regenerate` creates an immutable generation transaction.

The transaction snapshots:

```text
graph revision
input image hash
input mask hash
generation parameters
workflow definition
provider
model identifiers/hashes where available
seed
```

The external operation runs against that snapshot.

If the graph changes while the job is running, the job is not silently retargeted.

Example:

```text
Generation starts from revision 187
User changes upstream graph -> revision 188
Generation 187 completes
Result may be committed
Result status is immediately STALE relative to revision 188
```

Generation should support cancellation where the provider supports it.

The editor must never automatically regenerate a manual checkpoint merely because an upstream node changed.

## 52. Committed Artifacts
A checkpoint result is a **committed artifact**, not ordinary disposable render cache.

Ordinary tile cache may be deleted without changing project semantics.

A committed generated result may not be reproducible later because:

```text
model disappears
remote API changes
workflow changes
plugin disappears
runtime implementation changes
nondeterministic behavior changes
```

Therefore checkpoint outputs should be persistable using a content-addressed artifact store.

Conceptual layout:

```text
project/
    project.json
    artifacts/
        <content-hash>/
            output.png or output.exr
            generation.json
            optional-input.png
            optional-mask.png
            submitted-workflow.json
```

The exact storage format may be sidecar-based, project-container-based, or backed by a managed global artifact store with project pinning.

A project must not treat committed AI output as safely reconstructible from a temporary cache.

## 53. AI Checkpoint Nodes
AI editing is implemented initially as a specialization of `ManualCheckpoint`.

Typical workflows:

```text
RAW -> Develop -> Crop -> [ AI Img2Img ] -> Grade -> Output
```

```text
Image -----------+
                 |
Mask ------------+--> [ AI Inpaint ] -> downstream nodes
                 |
Prompt -----------+
```

Generation-only parameters trigger checkpoint staleness:

```text
prompt
negative prompt
seed
model
workflow
denoise/strength
steps
sampler
reference image
mask
ControlNet/conditioning
```

Cheap result-side parameters may remain live automatic operations:

```text
opacity
blend mode
result mask feather
post-generation output transform
```

These may either be modeled as separate downstream nodes or as an explicit post-result section that does not trigger regeneration.

AI checkpoint UI should make state explicit:

```text
AI Inpaint
Status: STALE
Last generated from revision: 187

[ Input ] [ Generated ] [ Difference ]

[ Regenerate ]
```

Selecting an AI checkpoint should allow inspection of:

- current incoming image;
- last committed generated result;
- optional difference/comparison view.

## 54. AI-Generated Masks and Spatial Analysis
AI is not limited to image generation. AI-derived spatial data is a first-class input for ordinary photographic nodes.

Examples:

```text
Image -> AI Subject Mask -> Local Exposure
Image -> AI Sky Mask     -> Sky Grade
Image -> AI Skin Mask    -> Portrait Retouch
```

Text-guided segmentation should also be supported when the provider/model supports it:

```text
Image -----------+
                 +--> AI Prompt Segmentation --> Mask --> Hue Shift
"red car" -------+
```

Expensive segmentation may use `ManualCheckpoint`. Lightweight local analysis may use `Automatic`.

Example:

```text
EvaluationPolicy::ManualCheckpoint
    large segmentation model
    remote segmentation API

EvaluationPolicy::Automatic
    small local face detector
    lightweight local semantic model
```

If an upstream node changes, a manual AI mask checkpoint becomes stale but continues to serve its last committed mask until explicitly regenerated.

Downstream mask operations and photographic adjustments remain live.

The checkpoint artifact store must support non-image artifacts such as masks, label maps, confidence maps, and depth maps.

## 55. AI Color Boundary
Generative image systems generally should not receive the editor's unbounded scene-linear working buffer directly.

Define an explicit AI interchange domain:

```text
color.AIImageRGB
```

Conceptual conversion:

```text
scene-linear working RGB
        |
AI input transform
        |
bounded model-facing RGB
        |
AI provider
        |
declared model/API output RGB
        |
AI output transform
        |
editor working RGB
```

For the MVP, lossless PNG is the preferred interchange format where the provider accepts it.

The node/provider must record the assumed interchange color space and transfer function so that generation behavior is reproducible and output is converted correctly.

## 56. AI Provider Interface
The first AI implementation should not embed an inference runtime.

AI checkpoint nodes call providers through a common interface.

Conceptually:

```rust
trait AiProvider {
    async fn submit(
        &self,
        request: GenerationRequest,
    ) -> Result<JobId>;

    async fn status(
        &self,
        job: JobId,
    ) -> Result<JobStatus>;

    async fn result(
        &self,
        job: JobId,
    ) -> Result<GenerationResult>;

    async fn cancel(
        &self,
        job: JobId,
    ) -> Result<()>;
}
```

The graph does not care whether execution occurs:

```text
on the same machine
on another LAN machine
over VPN
on a remote GPU server
through a commercial online API
```

Provider and workflow are separate concepts:

```text
Provider = where/how execution happens
Workflow = what processing is performed
```

Example:

```text
Provider:
    ComfyUI @ http://192.168.1.50:8188

Workflow:
    FLUX Inpaint

Preset:
    Object Removal
```

A provider job identifier should be persisted during an active job where useful so polling/recovery can survive temporary UI disconnects.

## 57. ComfyUI Provider
ComfyUI should be the first AI backend.

The editor treats an entire ComfyUI workflow as one checkpoint node. It must **not** attempt to reproduce the internal ComfyUI graph as native photo-editor nodes.

Example:

```text
Photo editor:

Develop -> [ ComfyUI: Generative Fill ] -> Grade

                         |
                         v

                 arbitrary ComfyUI workflow
                 Load Image
                     |
                 VAE Encode
                     |
                 Control / Sampler / custom nodes
                     |
                 VAE Decode
                     |
                 output image
```

A workflow integration stores:

```text
workflow definition
input bindings
exposed parameter bindings
output binding
```

Example binding structure:

```json
{
  "workflow": "flux_inpaint.json",
  "bindings": {
    "image": {
      "node": "12",
      "input": "image"
    },
    "prompt": {
      "node": "6",
      "input": "text"
    },
    "seed": {
      "node": "3",
      "input": "seed"
    },
    "denoise": {
      "node": "3",
      "input": "denoise"
    }
  },
  "output": {
    "node": "27"
  }
}
```

The ComfyUI provider should support:

```text
localhost
LAN server
VPN-accessible server
remote hosted instance
```

Initial execution flow:

```text
1. Render checkpoint input snapshot.
2. Render optional mask/reference inputs.
3. Transform into AI interchange color representation.
4. Upload required assets.
5. Patch the workflow with bound values.
6. Submit workflow.
7. Store returned job/prompt identifier.
8. Poll authoritative status.
9. Optionally consume WebSocket/SSE progress/previews.
10. Retrieve final result.
11. Convert result into editor working space.
12. Commit result as a persistent checkpoint artifact.
13. Automatically reevaluate downstream normal nodes.
```

Polling must be the authoritative completion mechanism. Streaming progress is an optional enhancement so reconnects, sleep/resume, and transient network failure are recoverable.

The exact fully expanded workflow submitted for each committed generation should be saved with generation metadata whenever feasible.

## 58. Generic HTTP AI Provider
Online APIs use the same checkpoint/provider abstraction as ComfyUI.

The project should provide a generic declarative HTTP provider for straightforward APIs so that adding a service does not necessarily require new Rust code.

Conceptual manifest:

```yaml
name: Example Img2Img API

request:
  method: POST
  url: https://example.com/v1/img2img

  multipart:
    image: $IMAGE
    mask: $MASK

  json:
    prompt: $PROMPT
    strength: $STRENGTH
    seed: $SEED

job:
  id: $.id

status:
  url: https://example.com/v1/jobs/$JOB_ID
  state: $.status
  completed: completed

result:
  image_url: $.output.image
```

The generic provider should support both:

```text
synchronous request -> immediate image result
```

and:

```text
submit -> job id -> poll -> retrieve result
```

Provider credentials must be stored using platform-appropriate secure secret storage rather than project files.

Provider-specific Rust adapters may be implemented when authentication, streaming, multipart behavior, or API semantics exceed the generic manifest model.

## 59. Checkpoint Chains and Export Policy
Multiple checkpoints may exist in one graph.

Example:

```text
Develop
   |
AI Cleanup        checkpoint A
   |
Grade
   |
AI Upscale        checkpoint B
   |
Output
```

If an input before checkpoint A changes:

```text
A -> locally stale
B -> transitively stale relative to the new upstream graph
```

Checkpoint B's committed artifact remains usable because it is still valid relative to the committed output of A from which it was generated.

The UI may distinguish:

```text
locally stale
transitively stale
```

Provide explicit operations:

```text
Regenerate
Regenerate downstream checkpoints
Regenerate from here
```

Checkpoint regeneration must occur in dependency order.

Export must never silently trigger expensive checkpoint regeneration.

If stale checkpoints exist, interactive export should offer explicit behavior such as:

```text
Export Current Appearance
Regenerate & Export
Cancel
```

Headless export should require a policy:

```text
--checkpoint-policy=committed
--checkpoint-policy=require-current
--checkpoint-policy=regenerate
```

Cloud/API generation that may incur monetary cost must always remain explicit unless the user has deliberately configured an automation policy outside the normal interactive workflow.

## 60. Freeze and Bake Semantics
Checkpoint results may support:

```text
Generate
Freeze
Bake
```

`Generate` creates or updates the committed result while preserving the live checkpoint node.

`Freeze` locks the checkpoint against accidental parameter changes/regeneration while preserving generation metadata.

`Bake` replaces the active checkpoint execution dependency with its committed artifact while preserving sufficient provenance to understand how the artifact was created.

Baking is useful for:

- archival projects;
- sharing projects without external AI dependencies;
- unavailable remote services;
- reducing project execution requirements.

## 61. Project Format

Projects and workflows are lightweight, non-destructive, and graph-native.

A workflow definition stores:

```text
typed graph topology
node/subgraph identities and versions
default parameters
exposed parameter ports
workflow inputs and outputs
logic/control nodes
dependencies
template/package metadata
```

A workflow instance stores:

```text
workflow-definition reference/version
concrete source bindings
per-instance parameter overrides
checkpoint states
committed artifact references
UI/layout state where appropriate
```

A project may embed a workflow definition for portability or reference an externally managed definition with an explicit version/hash.

Source pixels are not embedded by default.

Committed manual-checkpoint artifacts may be persisted because they are project-semantic data rather than disposable cache.


The file-format design should distinguish at least:

```text
.workflow
    reusable graph definition

.session
    current browser/queue/work context

project/artifact container or sidecar directory
    optional committed artifacts and provenance
```

Exact extensions are not mandated yet.

A session may reference a workflow by path/hash or embed a working copy when necessary for recovery.

Batch jobs should store sufficient state to resume/retry without making the reusable workflow file itself stateful.
## 62. Example Project Structure

```json
{
  "version": 1,
  "source": "photo.CR3",
  "working_space": "LinearRec2020",
  "nodes": [
    {
      "id": "raw",
      "type": "raw.develop",
      "version": 1
    },
    {
      "id": "exp",
      "type": "color.exposure",
      "version": 2,
      "parameters": {
        "exposure": 0.72
      }
    }
  ],
  "connections": [
    {
      "from": "raw.image",
      "to": "exp.image"
    }
  ]
}
```

The actual on-disk format may use JSON, CBOR, or another versioned representation.

Human-readable JSON is preferred during early development.

---

## 63. Node Versioning

Every node implementation must have an algorithm version.

Example:

```text
color.exposure
algorithm_version = 2
```

Opening an old project must preserve the historical algorithm when feasible.

New behavior should not silently modify the appearance of existing projects.

This requirement applies to:

- built-in nodes;
- RAW processing;
- tone mapping;
- color transforms;
- plugin adapters;
- serialization.

---

## 64. Undo and Redo

Use command-based history.

Examples:

```text
AddNode
RemoveNode
ConnectNodes
SetParameters
MoveNode
CreateGroup
ModifyMask
```

High-frequency parameter edits should be coalesced.

Example:

```text
slider drag:
0.10
0.11
0.12
...
0.74
```

should generally become one undo entry.

---

## 65. Project Reproducibility

Project files should store enough information to reproduce ordinary edits and preserve the visible result of operations that may not be reproducible later.

Store where relevant:

```text
node implementation versions
plugin identity/version
external provider identity
submitted external workflow
model identifiers and hashes when available
AI generation parameters
seed
input dependency hash
committed artifact hash
color configuration identity
AI interchange color transform
RAW decoder settings
working-space identity
export transform
compatibility overrides
```

For deterministic automatic nodes, preserving inputs and algorithm versions is preferred.

For generative/manual checkpoint nodes, preserving the committed artifact is authoritative for long-term appearance.

When an exact historical plugin, model, provider, or workflow is unavailable, the project should report that clearly while continuing to use the committed artifact when present.

## 66. Headless Operation

The Rust engine should support headless execution independently from the GUI.

Future CLI:

```text
rawgraph render photo.CR3 \
    --project photo.rgraph \
    --output photo.tif
```

Possible operations:

```text
render
batch export
apply preset
set node parameter
inspect graph
list plugins
validate project
generate thumbnail
inspect checkpoint status
explicitly generate checkpoints
```

The GUI should call the same underlying APIs used by headless workflows.

---


Headless tools must not implicitly execute manual checkpoints unless an explicit checkpoint policy requests regeneration.
## 67. Automation API

Node parameters should be schema-driven and addressable.

Conceptual syntax:

```text
node23.exposure = 0.7
node17.saturation = 1.1
```

This should enable future:

```text
Python bindings
AI agents
batch jobs
remote control
scripting
workflow automation
```

No core processing feature should require direct GUI interaction.

---

## 68. Security Model

Third-party plugin execution must be treated as untrusted.

Preferred default:

```text
foreign plugin
-> dedicated host process
```

Potential future sandboxing:

```text
Linux namespaces/seccomp
Windows AppContainer or job restrictions
macOS sandboxing
WASM
```

WASM may later be supported as a safe portable plugin tier.

---



Online AI credentials must not be stored in normal project files. Use OS-appropriate credential/secret storage.

Remote providers must be treated as data-export boundaries. Before sending source-derived images or masks to an online service, the UI should clearly identify the configured provider. Local ComfyUI and remote API execution share the same graph semantics but have different privacy/trust implications.
## 69. Performance Requirements

The design should optimize for perceived interactive performance.

Priorities:

1. user interaction;
2. visible viewport;
3. low-resolution preview;
4. full viewport;
5. background precomputation;
6. full-resolution export.

The engine should avoid:

- whole-image recomputation during every edit;
- unnecessary format conversions;
- repeated GPU upload/download;
- sending large buffers through Tauri JSON IPC;
- blocking the UI thread;
- loading foreign plugins into the main process by default.

---

## 70. Threading Model

The Rust backend should have independent execution domains for:

```text
UI command handling
graph scheduling
cheap control/metadata evaluation
CPU worker pool
GPU submission
plugin host communication
I/O
thumbnail generation
background cache maintenance
export
```

Long-running operations must be cancellable.

Newer graph revisions should be able to supersede older queued renders.

---



Cheap control values should propagate without allocating image tiles. Image/spatial work remains independently schedulable.

Lazy conditional routing must avoid scheduling unselected image branches where possible.
## 71. Initial UI Layout
The application has three closely connected working surfaces:

```text
Browse
Workflow
Batch
```

The default Workflow workspace keeps the graph primary while exposing the queue.

Recommended layout:

```text
+-----------------------------------------------------------------------+
| Browse | Workflow | Batch | Workflow Name | Add Node | Run/Generate  |
+-----------------------------------------------------------------------+
| Queue / Filmstrip |                    NODE CANVAS                     |
|                   |                                                    |
| DSC_1001  ★       | RAW -> Develop -> Logic -> Grade -> Output        |
| DSC_1002  T       |                \-> AI Mask -> Local Grade         |
| DSC_1003          |                                                    |
| ...               |                                                    |
+-------------------+-----------------------------------+----------------+
| Viewer / Compare / Scopes                            | Inspector      |
|                                                      |                |
+------------------------------------------------------+----------------+
```

`T` may visually mark Test Set items.

The Browse workspace emphasizes:

```text
folder navigation
thumbnail grid
EXIF
rating/flags
file operations
Add to Working Queue
```

The Batch workspace emphasizes:

```text
preflight
selected queue subset
output recipe
checkpoint policy
job progress
errors/retry
```

Panels should be resizable/dockable where practical.

Dragging a RAW/image file onto the canvas creates an input node when appropriate.

Node creation should support a fast searchable command palette.

Dragging from an existing port should filter node search results to compatible input types.

## 72. Frontend Node Design
Nodes should remain visually compact and emphasize topology.

Example image node:

```text
+------------------+
| Color Balance    |
|   thumbnail      |
o------------------o
```

Example control node:

```text
+-------------+
| ISO > 3200  |
o-----------o-+
```

Detailed parameters belong primarily in the inspector, while commonly changed or workflow-critical values may be shown inline.

Image-producing nodes should support:

```text
Preview
Pin Preview
Set as Viewer A
Set as Viewer B
Compare
```

Parameter inputs that are exposed as graph ports should be visually distinct from image/mask ports.

The UI should visually distinguish data families such as:

```text
Image/RAW
Mask/Spatial
Value/Control
Metadata/Descriptive
External/Checkpoint
```

without relying solely on color.

## 73. Library and Catalog
A filesystem-based photo browser is part of the core workflow.

A traditional imported catalog is not.

Core operation is:

```text
Browse Filesystem
    |
Select / Rate
    |
Add to Working Queue
    |
Load/Create Workflow
    |
Preview / Tinker
    |
Batch Process
```

An optional catalog/index layer may later accelerate:

```text
global search
albums
virtual collections
duplicate detection
cross-folder tags
saved searches
large-library thumbnail indexes
```

This optional catalog must not replace the filesystem as the source of truth or become necessary for opening and processing files.

SQLite may be used internally for:

```text
thumbnail cache/index
EXIF search index
session history
plugin/provider registry
optional catalog acceleration
```

Portable ratings and common metadata should use XMP or established metadata standards where practical.

## 74. Batch Preflight
Before batch execution, the application performs a preflight pass.

Preflight should detect where practical:

```text
missing source files
unsupported input formats
missing node packs
missing plugins
unavailable external providers
missing ComfyUI workflow dependencies
stale or ungenerated manual checkpoints
invalid workflow connections
unsupported output format/color configuration
output filename collisions
unwritable output directories
insufficient disk-space risk
```

The preflight result should classify issues:

```text
Error       blocks execution
Warning     execution allowed with acknowledgement/policy
Info        advisory only
```

Manual checkpoints require an explicit batch policy.

Examples:

```text
Use committed result
Generate if missing
Regenerate all
Fail if stale
```

Paid or remote generation must never be started for a large batch merely as a side effect of pressing ordinary export without an explicit policy.

A **Dry Run** should be supported:

```text
current preview item
Test Set
first N queue items
explicit selected subset
```

Dry runs use the exact workflow revision and output recipe intended for the final batch.

## 75. Batch Job Manager
Batch processing is an explicit job system.

A batch job captures:

```text
workflow definition + exact revision/hash
selected queue items
per-image overrides
checkpoint policy
output recipe(s)
dependency versions
start time
job state
```

Per-item states include:

```text
Waiting
Running
Completed
Skipped
Failed
Cancelled
```

The job manager should support:

```text
pause where practical
cancel
retry failed
retry selected
skip item
open failed item as current preview
resume interrupted job
```

A process/application crash should not require successful items to be recomputed if outputs and job state can be validated.

Batch scheduling may exploit:

```text
CPU concurrency
GPU scheduling
cache reuse
common workflow structure
provider concurrency limits
```

but must preserve deterministic per-item semantics.

Interactive preview should have higher scheduling priority than background batch work unless the user explicitly chooses otherwise.

## 76. Output Recipes
Export configuration is separated from the processing workflow through **Output Recipes**.

A workflow may feed one or more output recipes.

Example:

```text
Workflow Output
    |
    +--> Full Resolution TIFF
    |
    +--> Web JPEG
    |
    +--> Thumbnail JPEG
```

An output recipe may define:

```text
format
resolution / resize policy
bit depth
output color space
ICC / OCIO transform
metadata policy
output sharpening
quality/compression
destination directory
filename template
overwrite/collision policy
```

Filename templates may use source metadata and workflow values:

```text
{{date}}_{{camera}}_{{original_name}}_edit.jpg
```

Output recipes should be independently reusable and shareable.

Batch processing must record which output recipe version/settings produced each result.

## 77. Workflow Snapshots and Batch Reproducibility
Workflow tinkering should support explicit snapshots/revisions.

Examples:

```text
Portrait v3
Portrait v4-high-ISO
Wedding-final
```

A batch always binds to an exact workflow revision/hash at launch.

Subsequent editing of the workflow must not silently change a running batch job.

Batch provenance should record where applicable:

```text
workflow revision/hash
node-pack versions
plugin versions
external workflow version/hash
model identifiers/hashes
output recipe
per-image overrides
manual checkpoint artifact hashes
```

This enables later debugging and reproducibility.

Autosaved editor history may coexist with named workflow snapshots, but named batch provenance should never depend solely on volatile undo history.

## 78. Session Persistence
A session preserves working context without conflating it with a reusable workflow.

Session data may include:

```text
current folder
browser sort/filter state
working queue
test-set membership
current preview item
selected workflow
unsaved workflow edits
per-image overrides
viewer targets
panel layout
output recipes selected for batch
batch job state/history
```

Users should conceptually understand:

```text
Save Workflow
    saves reusable computation

Save Session
    saves current work context
```

Closing the application should offer recovery of unsaved session/workflow state according to normal desktop application conventions.

## 79. Export
Initial output formats should include:

```text
JPEG
PNG
TIFF
OpenEXR
```

Later:

```text
AVIF
JPEG XL
HEIF
DNG where practical
```

Single-image export and batch processing both use Output Recipes.

Export operations should support:

```text
resolution
bit depth
output color space
ICC/OCIO transform
metadata policy
sharpening
file naming templates
destination rules
collision handling
```

Single-image quick export may use the currently selected/default recipe.

Batch output always records its exact workflow revision and output recipe configuration.

## 80. Repository Structure

Suggested layout:

```text
/
├── apps/
│   └── desktop/
│       ├── src-tauri/
│       └── frontend/
│           └── platform/
│               └── tauri.ts
│
├── crates/
│   ├── core/
│   ├── graph/
│   ├── renderer/
│   ├── raw/
│   ├── color/
│   ├── cache/
│   ├── project/
│   ├── artifact-store/
│   ├── metadata/
│   ├── external-node-api/
│   ├── external-node-host/
│   └── export/
│
├── external/
│   ├── native/
│   ├── ofx/
│   ├── gegl/
│   ├── gimp/
│   ├── cli/
│   ├── comfyui/
│   └── http-ai/
│
├── shaders/
│   └── wgsl/
│
├── plugin-compat-db/
│
├── docs/
│
└── SPEC.MD
```

## 81. Core Rust Interfaces

Illustrative only.

```rust
pub trait Node {
    fn descriptor(&self) -> &NodeDescriptor;

    fn prepare(
        &mut self,
        ctx: &PrepareContext,
    ) -> Result<(), NodeError>;

    fn render(
        &self,
        request: &RenderRequest,
        ctx: &RenderContext,
    ) -> Result<RenderResult, NodeError>;
}
```

The internal Rust trait is not a public binary plugin ABI.

External plugins communicate through stable host protocols or C ABI layers.

---

## 82. Render Request

Illustrative structure:

```rust
pub struct RenderRequest {
    pub graph_revision: u64,
    pub output_node: NodeId,
    pub region: Rect,
    pub mip_level: u32,
    pub quality: RenderQuality,
}
```

Possible quality levels:

```text
Draft
Interactive
Preview
Full
Export
```

---

## 83. Milestone Plan
### Phase 1: Graph-First Core Prototype

Implement:

```text
Tauri application
React/React Flow canvas
Rust authoritative graph
typed ports
node search/add workflow
image input node
value constants
connectable parameters
Exposure
Output
Viewer A/B
workflow save/load
```

Goal:

Prove the graph itself is the primary computational document.

### Phase 2: Browser and Working Queue

Implement:

```text
filesystem browser
thumbnail grid
EXIF display
ratings/flags
basic file operations
multi-select
Working Queue
current preview item
Test Set
session save/recovery
```

Goal:

Establish the Browse -> Queue -> Workflow user journey without requiring a catalog.

### Phase 3: RAW and Color Pipeline

Implement:

```text
RAW decoding
demosaic
camera profile
white balance
scene-linear working space
basic RAW Develop subgraph
basic export
```

Goal:

Produce a usable RAW-developed image entirely through graph nodes.

### Phase 4: Control and Adaptive Workflows

Implement:

```text
metadata node
scalar/control types
Compare
Switch
Select
Map Range
Curve
Expression
lazy conditional routing
parameter input ports
subgraph blueprints
per-image exposed-parameter overrides
```

Goal:

Support metadata-driven and input-adaptive photographic workflows.

### Phase 5: Interactive Renderer

Implement:

```text
tile scheduler
mip pyramid
cache
viewport requests
progressive rendering
revision cancellation
wgpu acceleration
Test Set preview scheduling
```

Goal:

Interactive large-image editing and rapid testing across representative images.

### Phase 6: Spatial and Mask Workflows

Implement:

```text
painted masks
gradients
qualifiers
mask algebra
label maps
mask previews
local adjustments
```

Goal:

Make spatial data a first-class graph domain.

### Phase 7: Batch System

Implement:

```text
preflight
dry run
Output Recipes
batch job manager
job persistence/resume
retry failures
workflow revision binding
filename templates
checkpoint batch policies
```

Goal:

Reliably apply one exact workflow revision across the selected Working Queue.

### Phase 8: Workflow Ecosystem

Implement:

```text
WorkflowDefinition / WorkflowInstance
node packs
workflow templates
dependency metadata
subgraph publishing
workflow versioning
named snapshots
```

Goal:

Make workflows and node packs shareable independently of source images.

### Phase 9: External Node Framework

Implement:

```text
canonical NodeDescriptor
external-node-host protocol
shared-memory image buffers
native plugin host
capability negotiation
crash recovery
```

Goal:

Stable third-party/external-node foundation.

### Phase 10: Compatibility Hosts

Priority:

```text
GEGL
OFX
CLI
GIMP
```

### Phase 11: AI Checkpoints and AI Masks

Implement:

```text
ManualCheckpoint evaluation policy
checkpoint state machine
committed artifact store
AI interchange color transform
AiProvider abstraction
ComfyUI provider
workflow parameter binding
generic HTTP AI provider
polling/recovery
img2img/inpaint
AI segmentation
prompt-guided masks
MaskSet/LabelMap/ConfidenceMap artifacts
stale/current UI
batch checkpoint policies
```

Goal:

Support generative editing and AI-assisted masking without embedding a model runtime.

### Phase 12: ImageSet and Professional Workflows

Implement:

```text
ImageSet
HDR merge
focus stacking
batch graph collection operations
advanced scopes
lens correction
advanced denoise/sharpen
optional global catalog/index
```

Goal:

Extend the same graph model to computational photography and large professional workflows.

## 84. Architectural Rules

The following rules should remain stable throughout development.

### Rule 1

The Rust backend is authoritative for image-processing state.

### Rule 2

The frontend must not process RAW images directly.

### Rule 3

The frontend must not become the authoritative node graph.

### Rule 4

Full-resolution images must not be transported through JSON/Tauri IPC.

### Rule 5

Image processing must be region-, tile-, and resolution-aware where practical.

### Rule 6

Plugin ecosystems must integrate through compatibility hosts rather than modify the core graph engine.

### Rule 7

Untrusted binary plugins should run out-of-process.

### Rule 8

The internal graph protocol is more important than any specific external plugin ABI.

### Rule 9

The project format must preserve historical rendering behavior where practical.

### Rule 10

Browser WebGPU is optional; native wgpu is the canonical GPU backend.

### Rule 11

GIMP may be supported as a compatibility host but must not be the primary image-processing backend.

### Rule 12

Performance decisions should be profiling-driven. Do not introduce native viewport complexity until WebView rendering is shown to be inadequate.

---



### Rule 13

Manual checkpoints never reevaluate merely because upstream dependencies changed.

### Rule 14

A stale checkpoint retains and serves its last committed artifact until explicitly regenerated or replaced.

### Rule 15

Committed generated artifacts are project-semantic data, not disposable render cache.

### Rule 16

The core graph must not depend on ComfyUI, any particular AI vendor, or any particular model runtime. These are providers behind the external-node/checkpoint interfaces.

### Rule 17

Tauri-specific calls must remain behind platform adapters; the core application architecture must remain Tauri-independent.

### Rule 18

External AI execution must use an explicit, recorded image/color interchange boundary rather than passing unbounded scene-linear buffers without transformation.

### Rule 19

Export must not silently trigger manual checkpoint regeneration or paid remote computation.


### Rule 20

The workflow graph is the primary document. A traditional fixed adjustment stack must not become the hidden authoritative processing model.

### Rule 21

Any meaningful node parameter may be exposed as a typed input port.

### Rule 22

Metadata, analysis results, scalar values, and images are all first-class graph data.

### Rule 23

Conditional behavior should normally use lazy `Switch`/`Select` nodes. Runtime graph-topology mutation is not the normal mechanism for conditionals.

### Rule 24

Unselected conditional image branches should not be evaluated unless another downstream consumer requires them.

### Rule 25

AI-generated masks, label maps, confidence maps, and similar spatial outputs must be usable by ordinary downstream nodes without special-case integration.

### Rule 26

AI is a producer of graph data, not a separate editing mode.

### Rule 27

Subgraphs are the primary progressive-disclosure mechanism. High-level photographic nodes may wrap transparent inspectable node networks.

### Rule 28

Catalog/library functionality is optional infrastructure and must remain separate from core workflow semantics.

### Rule 29

Workflow definitions must be reusable independently of any single source image.


### Rule 30

The filesystem is the source of truth for ordinary source images. Core processing must not require import into an application catalog.

### Rule 31

The Working Queue is transient/session processing context, not a hidden catalog.

### Rule 32

Workflow, session, queue, and batch job state are distinct concepts and should be serialized independently where practical.

### Rule 33

A batch job binds to an exact workflow revision/hash at launch. Subsequent workflow edits must not alter the running job.

### Rule 34

Per-image overrides must not duplicate or fork the shared workflow graph.

### Rule 35

Interactive preview work has scheduling priority over background batch work by default.

### Rule 36

Batch execution requires preflight and explicit manual-checkpoint policy where checkpoints may be stale, missing, remote, or paid.

### Rule 37

Output configuration is represented by reusable Output Recipes rather than being embedded unpredictably into batch UI state.

### Rule 38

`core.ImageSet` is a graph data type for algorithms operating on collections; it is not synonymous with the Working Queue.
## 85. Initial Recommended Stack Summary
```text
Desktop shell:
    Tauri 2
    isolated behind project-owned platform adapter

Frontend:
    React
    TypeScript
    Vite
    React Flow
    Zustand
    Tailwind CSS
    Radix UI

Backend:
    Rust

GPU:
    wgpu
    WGSL

RAW:
    LibRaw or RawSpeed

Color:
    OpenColorIO
    LittleCMS where needed

Metadata:
    Exiv2

Database:
    SQLite for optional/indexing services

Core document:
    WorkflowDefinition
    WorkflowInstance

Graph:
    extensible typed ports
    photographic/spatial/control/descriptive data families
    connectable parameter ports
    metadata-driven control
    lazy Switch/Select routing
    automatic nodes
    manual checkpoint nodes
    reusable subgraphs
    ImageSet processing

External-node architecture:
    canonical External Node Host Protocol
    out-of-process compatibility hosts where appropriate

Initial compatibility:
    Native
    GEGL
    OFX
    CLI
    GIMP

AI:
    manual generative checkpoints
    AI segmentation/mask nodes
    ComfyUI provider
    generic HTTP AI provider
    persistent committed artifacts

Project:
    versioned graph-native workflow/project format
    workflow templates and node-pack dependencies
    checkpoint provenance/artifacts
```

## 86. Product Identity
The product is not:

```text
an open-source Lightroom clone
```

and not:

```text
GIMP with nodes
```

and not:

```text
a second ComfyUI frontend
```

It is:

```text
a graph-first visual computing environment for photography
```

The closest conceptual combination is:

```text
ComfyUI-style workflow composition
+
DaVinci Resolve/Fusion-style visual processing
+
professional RAW/photo color management
+
Natron/GEGL/OFX-style extensibility
+
modern Rust GPU/concurrency architecture
```

The defining distinction from Lightroom-style editors is that the graph itself is the reusable computational artifact.

A photographer should be able to build workflows such as:

```text
RAW
 |
 +--> Metadata --> ISO --> Curve --------------------------+
 |                                                          |
 v                                                          v
Develop --> AI Segmentation --> Subject Mask --> Local Grade --> Denoise
                 |
                 +--> Sky Mask --> Sky Grade
                                      |
                                      v
                                    Output
```

or:

```text
ImageSet -> Filter(metadata.rating >= 4) -> Portrait Workflow -> Export
```

Ordinary photographic nodes remain live and automatically updating.

Expensive or generative operations can become explicit manual checkpoints.

AI is integrated as a producer/transformer of normal graph data:

```text
AI Img2Img          -> Image
AI Inpaint          -> Image
AI Segmentation     -> Mask / LabelMap
AI Depth            -> DepthMap
AI Scene Analysis   -> control/descriptive values
```

This preserves a predictable photographic workflow while enabling reusable adaptive visual programs that traditional adjustment-stack editors cannot express.



The intended end-to-end photographer experience is:

```text
Filesystem Browser
      |
Select / Rate / Inspect EXIF
      |
Working Queue
      |
Load or Build Workflow
      |
Choose Representative Test Images
      |
Tinker Graph + Inspect Intermediate Results
      |
Dry Run / Preflight
      |
Batch Process Selected Queue
      |
Review Completed / Failed / Retry
```

This keeps file management simple and familiar while making the graph—not a Lightroom-style adjustment panel—the center of image processing.
