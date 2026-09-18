# Step 11 — AI Provider Layer

## Objective

Add generative/image-editing AI by calling existing services rather than embedding inference into the editor.

## Provider Interface

Implement a provider abstraction equivalent to:

```text
submit
status
result
cancel
capabilities
```

Provider and workflow must remain separate concepts.

## ComfyUI Provider

Support:

- localhost;
- LAN;
- VPN;
- remote server.

Workflow integration should store:

```text
workflow definition
image input binding
mask input binding
prompt binding
parameter bindings
output binding
```

The editor treats an entire ComfyUI workflow as one AI checkpoint node.

Execution:

```text
render checkpoint input
transform to AI interchange
upload assets
patch workflow
submit
poll
optional stream progress
retrieve result
convert result to editor working space
commit artifact
```

Polling is authoritative. Streaming is optional enhancement.

## Generic HTTP Provider

Support manifest-driven APIs when possible:

```text
request endpoint
auth reference
multipart fields
JSON fields
job-id extraction
status endpoint
completion mapping
result extraction
```

Support synchronous and asynchronous APIs.

## Credentials

Store credentials in platform-appropriate secure storage.

Never serialize secrets into workflow/project files.

## AI Color Boundary

Define explicit model-facing image representation.

Initial interchange can use lossless PNG with declared color assumptions.

## Initial AI Nodes

- Img2Img;
- Inpaint;
- Generative Fill;
- Upscale.

All use ManualCheckpoint by default.

## Acceptance Criteria

- same AI checkpoint can switch between compatible provider configurations;
- ComfyUI can run locally or on LAN;
- result survives provider disappearance because committed artifact is stored;
- job can be cancelled where supported;
- stale semantics work exactly like Step 10;
- no model runtime is linked into the core app.

## Exit Deliverable

Useful generative AI editing with minimal editor-side ML complexity.
