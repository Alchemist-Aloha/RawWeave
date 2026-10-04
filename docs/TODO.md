# TODO

- [x] Implement curve-based displays for compatible nodes: Curve, Curves (gamma), LUT/LUT Tools and Film Curve. Live plots follow local drafts; point edits apply once on Enter/blur. Saved parameter formats and Rust evaluation are unchanged. Direct point dragging remains a possible follow-up, not implemented.
- [ ] Continue replacing opaque value-only controls with appropriate visual representations.
  - [x] Review existing parameter UX and image/geometry helpers; promote curve points out of Advanced and add a bounded gamma slider with a live plot. See `EDITOR_IMPROVEMENT_LOG.md`, iteration 21.
  - [x] Levels: combined black/white/gamma transfer display with clipping boundaries and coupled invalid-value messages. Exact fields and Rust validation remain unchanged; plot updates on commit.
  - [x] Map Range / Clamp: input/output transfer diagrams with labeled endpoints, reversed ranges, extrapolation/clamped tails and equal Clamp bounds. See `EDITOR_IMPROVEMENT_LOG.md`, iteration 22.
  - [ ] Color qualifier: target-color swatch beside RGB fields; label working-space interpretation rather than implying display-color accuracy.
  - [ ] Color Zones / Split Toning: hue-selection visualization, keeping precise degree fields and keyboard controls.
  - Existing crop/resize and gradient drawing helpers should be reused, not replaced.