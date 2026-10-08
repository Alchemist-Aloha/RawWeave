use gpui::*;
use std::{cell::Cell, rc::Rc};

use crate::store::FlowState;

const MINIMAP_WIDTH: f32 = 148.0;
const MINIMAP_HEIGHT: f32 = 100.0;
const MINIMAP_PADDING: f32 = 8.0;
/// The footprint never grows past this: a window cut into the plane, not a panel
/// laid over it.
const MINIMAP_MAX_WIDTH: f32 = 148.0;
const MINIMAP_MAX_HEIGHT: f32 = 100.0;

/// Plane colours for the overview, so it follows the same cast as the canvas
/// while this widget stays free of any one shell's token table.
#[derive(Clone, Copy)]
pub struct MinimapPalette {
    pub ground: u32,
    pub frame: u32,
    pub frame_selected: u32,
    pub mask_fill: u32,
    pub mask_line: u32,
    pub border: u32,
}

impl Default for MinimapPalette {
    fn default() -> Self {
        Self {
            ground: 0x101112,
            frame: 0x3b3d40,
            frame_selected: 0xf2efe6,
            mask_fill: 0x3b3d40,
            mask_line: 0x55585c,
            border: 0x3b3d40,
        }
    }
}

/// A minimap component that shows a bird's-eye view of the flow graph.
///
/// Renders a scaled-down view of all nodes and edges, with a rectangle
/// indicating the current viewport. Click or drag on the minimap to pan.
pub struct Minimap {
    state: Entity<FlowState>,
    /// Container bounds captured during rendering (for viewport calculations).
    container_bounds: Option<(f32, f32)>,
    /// Bounds captured at prepaint, so pointer coordinates can be made local
    /// and so pan math uses the box actually painted. Stored as (x, y, w, h).
    painted_bounds: Rc<Cell<(f32, f32, f32, f32)>>,
    /// The plane's own palette, supplied by the shell that owns the tokens.
    palette: MinimapPalette,
    /// Footprint resolved from the plane's aspect ratio.
    footprint_size: Option<(f32, f32)>,
}

impl Minimap {
    pub fn new(state: Entity<FlowState>) -> Self {
        Self {
            state,
            container_bounds: None,
            painted_bounds: Rc::new(Cell::new((0.0, 0.0, MINIMAP_WIDTH, MINIMAP_HEIGHT))),
            palette: MinimapPalette::default(),
            footprint_size: None,
        }
    }

    /// Fit the footprint to the plane's aspect ratio, inside the design's bound.
    pub fn footprint(mut self, plane_width: f32, plane_height: f32) -> Self {
        if plane_width > 0.0 && plane_height > 0.0 {
            let scale = (MINIMAP_MAX_WIDTH / plane_width).min(MINIMAP_MAX_HEIGHT / plane_height);
            self.footprint_size = Some((
                (plane_width * scale).clamp(48.0, MINIMAP_MAX_WIDTH),
                (plane_height * scale).clamp(32.0, MINIMAP_MAX_HEIGHT),
            ));
        }
        self
    }

    /// The plane's palette, so the window follows the same cast as the canvas.
    pub fn palette(mut self, palette: MinimapPalette) -> Self {
        self.palette = palette;
        self
    }

    /// Set the container bounds (the main flow graph's size).
    pub fn container_bounds(mut self, width: f32, height: f32) -> Self {
        self.container_bounds = Some((width, height));
        self
    }

    /// Refresh the container size after a layout change.
    /// Returns whether the value changed, so callers only notify on real resizes.
    pub fn set_container_bounds(&mut self, width: f32, height: f32) -> bool {
        if self.container_bounds == Some((width, height)) {
            return false;
        }
        self.container_bounds = Some((width, height));
        true
    }

    /// Refit the footprint to the plane after a layout change.
    /// Returns whether it changed, so callers only notify on real resizes.
    pub fn set_plane_size(&mut self, plane_width: f32, plane_height: f32) -> bool {
        if plane_width <= 0.0 || plane_height <= 0.0 {
            return false;
        }
        let scale = (MINIMAP_MAX_WIDTH / plane_width).min(MINIMAP_MAX_HEIGHT / plane_height);
        let size = (
            (plane_width * scale).clamp(48.0, MINIMAP_MAX_WIDTH),
            (plane_height * scale).clamp(32.0, MINIMAP_MAX_HEIGHT),
        );
        if self.footprint_size == Some(size) {
            return false;
        }
        self.footprint_size = Some(size);
        true
    }
}

impl Render for Minimap {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state_for_canvas = self.state.clone();
        let state_for_mouse = self.state.clone();
        let entity_id = cx.entity_id();
        let container = self.container_bounds.unwrap_or((900.0, 600.0));
        let (width, height) = self
            .footprint_size
            .unwrap_or((MINIMAP_WIDTH, MINIMAP_HEIGHT));
        let palette = self.palette;
        // Pointer events arrive in window coordinates; painting uses bounds
        // origin. Capturing it here keeps minimap clicks over the node they
        // point at instead of a constant offset away.
        let painted = self.painted_bounds.clone();
        let origin = self.painted_bounds.clone();

        div()
            .id("flow-minimap")
            .w(px(width))
            .h(px(height))
            // Recessed ground, a hairline frame: a window cut into the plane.
            .bg(gpui::rgb(palette.ground))
            .rounded(px(3.0))
            .border_1()
            .border_color(gpui::rgb(palette.border))
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        painted.set((
                            bounds.origin.x.as_f32(),
                            bounds.origin.y.as_f32(),
                            bounds.size.width.as_f32(),
                            bounds.size.height.as_f32(),
                        ));
                    },
                    move |bounds, _: (), window, cx| {
                        let state = state_for_canvas.read(cx);
                        paint_minimap(&bounds, state, container, palette, window);
                    },
                )
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, {
                let state = state_for_mouse.clone();
                let entity_id = entity_id;
                let origin = origin.clone();
                move |event, _window, cx| {
                    let (ox, oy, width, height) = origin.get();
                    pan_to_minimap_point(
                        &state,
                        event.position.x.as_f32() - ox,
                        event.position.y.as_f32() - oy,
                        (width, height),
                        container,
                        cx,
                    );
                    cx.notify(entity_id);
                }
            })
            .on_mouse_move({
                let state = state_for_mouse.clone();
                let entity_id = entity_id;
                let origin = origin.clone();
                move |event, _window, cx| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        let (ox, oy, width, height) = origin.get();
                        pan_to_minimap_point(
                            &state,
                            event.position.x.as_f32() - ox,
                            event.position.y.as_f32() - oy,
                            (width, height),
                            container,
                            cx,
                        );
                        cx.notify(entity_id);
                    }
                }
            })
    }
}

/// Pan the viewport so the center of the visible area aligns with the clicked minimap point.
///
/// `mx`/`my` are relative to the minimap's painted box, and `box_size` is that box,
/// so a border or padding on the widget cannot skew which node a click targets.
fn pan_to_minimap_point(
    state: &Entity<FlowState>,
    mx: f32,
    my: f32,
    box_size: (f32, f32),
    container: (f32, f32),
    cx: &mut App,
) {
    state.update(cx, |state, _| {
        let (graph_bounds, _) = compute_graph_bounds(state);
        if graph_bounds.2 <= 0.0 || graph_bounds.3 <= 0.0 {
            return;
        }

        let inner_w = box_size.0 - MINIMAP_PADDING * 2.0;
        let inner_h = box_size.1 - MINIMAP_PADDING * 2.0;
        let scale_x = inner_w / graph_bounds.2;
        let scale_y = inner_h / graph_bounds.3;
        let scale = scale_x.min(scale_y);

        let offset_x = (inner_w - graph_bounds.2 * scale) / 2.0 + MINIMAP_PADDING;
        let offset_y = (inner_h - graph_bounds.3 * scale) / 2.0 + MINIMAP_PADDING;

        let flow_x = (mx - offset_x) / scale + graph_bounds.0;
        let flow_y = (my - offset_y) / scale + graph_bounds.1;

        // Center the viewport on this flow point.
        state.viewport.x = container.0 / 2.0 - flow_x * state.viewport.zoom;
        state.viewport.y = container.1 / 2.0 - flow_y * state.viewport.zoom;
    });
}

/// Paint the minimap contents.
fn paint_minimap(
    bounds: &Bounds<Pixels>,
    state: &FlowState,
    container: (f32, f32),
    palette: MinimapPalette,
    window: &mut Window,
) {
    let (graph_bounds, has_nodes) = compute_graph_bounds(state);
    if !has_nodes {
        return;
    }

    let bx = bounds.origin.x.as_f32();
    let by = bounds.origin.y.as_f32();

    // Scale graph to fit the painted box with padding
    let inner_w = bounds.size.width.as_f32() - MINIMAP_PADDING * 2.0;
    let inner_h = bounds.size.height.as_f32() - MINIMAP_PADDING * 2.0;
    let scale_x = inner_w / graph_bounds.2;
    let scale_y = inner_h / graph_bounds.3;
    let scale = scale_x.min(scale_y);

    let offset_x = bx + (inner_w - graph_bounds.2 * scale) / 2.0 + MINIMAP_PADDING;
    let offset_y = by + (inner_h - graph_bounds.3 * scale) / 2.0 + MINIMAP_PADDING;

    // Paint nodes as small rectangles
    // Frames are drawn in the plane's own line tone, not as bright blocks.
    let node_color: gpui::Rgba = gpui::rgb(palette.frame).into();
    for node in &state.nodes {
        if node.hidden {
            continue;
        }
        let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(120.0);
        let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(40.0);
        let nx = offset_x + (node.position.x - graph_bounds.0) * scale;
        let ny = offset_y + (node.position.y - graph_bounds.1) * scale;
        let nw = w * scale;
        let nh = h * scale;

        let node_bounds = Bounds::new(
            Point::new(px(nx), px(ny)),
            Size {
                width: px(nw),
                height: px(nh),
            },
        );

        let color: gpui::Rgba = if node.selected {
            gpui::rgb(palette.frame_selected).into()
        } else {
            node_color
        };
        window.paint_quad(fill(node_bounds, color));
    }

    // Paint edges as thin lines
    let edge_color: Background = gpui::rgb(palette.frame).into();
    for edge in &state.edges {
        if edge.hidden {
            continue;
        }
        let source = state.get_node(&edge.source);
        let target = state.get_node(&edge.target);
        if let (Some(src), Some(tgt)) = (source, target) {
            let sw = src.measured_width.map(|p| p.as_f32()).unwrap_or(120.0);
            let sh = src.measured_height.map(|p| p.as_f32()).unwrap_or(40.0);
            let tw = tgt.measured_width.map(|p| p.as_f32()).unwrap_or(120.0);
            let th = tgt.measured_height.map(|p| p.as_f32()).unwrap_or(40.0);

            let sx = offset_x + (src.position.x + sw / 2.0 - graph_bounds.0) * scale;
            let sy = offset_y + (src.position.y + sh / 2.0 - graph_bounds.1) * scale;
            let tx = offset_x + (tgt.position.x + tw / 2.0 - graph_bounds.0) * scale;
            let ty = offset_y + (tgt.position.y + th / 2.0 - graph_bounds.1) * scale;

            let mut builder = PathBuilder::stroke(px(1.0));
            builder.move_to(Point::new(px(sx), px(sy)));
            builder.line_to(Point::new(px(tx), px(ty)));
            if let Ok(path) = builder.build() {
                window.paint_path(path, edge_color.clone());
            }
        }
    }

    // Paint viewport indicator
    let viewport = &state.viewport;
    // The canvas is embedded at an origin, so screen origin is not flow zero.
    let vp_left = (-viewport.x - state.canvas_origin.x) / viewport.zoom;
    let vp_top = (-viewport.y - state.canvas_origin.y) / viewport.zoom;
    let vp_width = container.0 / viewport.zoom;
    let vp_height = container.1 / viewport.zoom;

    let vx = offset_x + (vp_left - graph_bounds.0) * scale;
    let vy = offset_y + (vp_top - graph_bounds.1) * scale;
    let vw = vp_width * scale;
    let vh = vp_height * scale;

    let vp_bounds = Bounds::new(
        Point::new(px(vx), px(vy)),
        Size {
            width: px(vw),
            height: px(vh),
        },
    );
    window.paint_quad(fill(vp_bounds, gpui::rgb(palette.mask_fill).opacity(0.06)));

    // Viewport border
    let border_color: Background = gpui::rgb(palette.mask_line).into();
    let top = Bounds::new(
        Point::new(px(vx), px(vy)),
        Size {
            width: px(vw),
            height: px(1.0),
        },
    );
    window.paint_quad(fill(top, border_color.clone()));
    let bottom = Bounds::new(
        Point::new(px(vx), px(vy + vh)),
        Size {
            width: px(vw),
            height: px(1.0),
        },
    );
    window.paint_quad(fill(bottom, border_color.clone()));
    let left = Bounds::new(
        Point::new(px(vx), px(vy)),
        Size {
            width: px(1.0),
            height: px(vh),
        },
    );
    window.paint_quad(fill(left, border_color.clone()));
    let right = Bounds::new(
        Point::new(px(vx + vw), px(vy)),
        Size {
            width: px(1.0),
            height: px(vh),
        },
    );
    window.paint_quad(fill(right, border_color));
}

/// Compute the bounding box of all nodes in flow coordinates.
/// Returns ((min_x, min_y, width, height), has_nodes).
fn compute_graph_bounds(state: &FlowState) -> ((f32, f32, f32, f32), bool) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut count = 0;

    for node in &state.nodes {
        if node.hidden {
            continue;
        }
        let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(120.0);
        let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(40.0);
        min_x = min_x.min(node.position.x);
        min_y = min_y.min(node.position.y);
        max_x = max_x.max(node.position.x + w);
        max_y = max_y.max(node.position.y + h);
        count += 1;
    }

    if count == 0 {
        return ((0.0, 0.0, 0.0, 0.0), false);
    }

    // Add some padding
    let padding = 50.0;
    min_x -= padding;
    min_y -= padding;
    max_x += padding;
    max_y += padding;

    ((min_x, min_y, max_x - min_x, max_y - min_y), true)
}
