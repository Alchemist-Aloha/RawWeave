# Vendored gpui-flow

Source: https://github.com/pacifio/gpui-flow
Revision: cfa578758ee251657f15f281d893a08b85aedbff
License: MIT, retained in `LICENSE`.

Vendored because upstream uses moving Zed Git GPUI, while GPUI Kit 0.7.1 requires exactly `gpui-pre = 0.3.8`. This copy uses the same engine as Kit, avoiding incompatible GPUI entity/element types.

Integration changes:
- Cargo dependency pinned to `gpui-pre = 0.3.8`; examples/dev dependencies omitted.
- Native application owns graph/parameter history; removed canvas-only undo/redo key handlers that would lose node parameters.
- Fixed the unused default-renderer callback so dark node labels remain readable.
- Added embedded-canvas origin to handle/edge coordinates and zoom/selection calculations.
- Separated multiple ports on the same node side; painting and hit testing share handle-center calculation.
- Removed duplicate dead handle-center helper; rustfmt applied.

The native app observes flow changes and validates them through RawWeave's Rust graph engine. Upstream connection callbacks are not used as the source of truth. The embedding/port regression is in `tests/embedded.rs`.
