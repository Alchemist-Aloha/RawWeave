use std::collections::HashMap;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::edges;
use crate::store::FlowState;
use crate::types::*;

type NodeRendererFn = Box<dyn Fn(&FlowNode, &mut Window, &mut App) -> AnyElement>;

/// The top-level flow graph component.
///
/// Renders nodes as positioned divs and edges via a canvas paint layer.
pub struct FlowGraph {
    state: Entity<FlowState>,
    focus_handle: FocusHandle,
    node_renderers: HashMap<SharedString, NodeRendererFn>,
    default_renderer: Option<NodeRendererFn>,
    /// Called when a new connection is completed.
    on_connect: Option<Box<dyn Fn(&Connection, &mut FlowState)>>,
    /// Custom validation for connections. Return false to reject.
    is_valid_connection: Option<Box<dyn Fn(&Connection, &FlowState) -> bool>>,
    /// Whether to show the default node wrapper chrome (bg, border, shadow, padding).
    show_node_chrome: bool,
    /// Background color for the canvas (default: 0xf8f8f8).
    bg_color: u32,
    /// Dot grid color (default: 0xd4d4d4).
    grid_color: u32,
    /// Spacing between grid lines in world coordinates.
    grid_gap: f32,
    /// Background pattern style.
    bg_pattern: BackgroundPattern,
    /// Node wrapper background color.
    node_bg_color: u32,
    /// Node wrapper border color.
    node_border_color: u32,
    /// Ring drawn on a selected node.
    selection_color: u32,
    /// Ink for handle labels.
    handle_label_color: u32,
    /// Connection draft line, and the handle a drag would land on.
    target_color: u32,
    /// The handle a drag is currently snapped to.
    target_active_color: u32,
    /// Box-selection marquee.
    marquee_color: u32,
    /// Wire colour when an edge carries no colour of its own.
    edge_color: u32,
    /// Wire colour while an edge is selected.
    edge_selected_color: u32,
    /// Whether we've done the initial measurement pass.
    measured: bool,
}

impl FlowGraph {
    pub fn new(state: Entity<FlowState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus_handle: cx.focus_handle(),
            node_renderers: HashMap::new(),
            default_renderer: None,
            on_connect: None,
            is_valid_connection: None,
            show_node_chrome: true,
            bg_color: 0xf8f8f8,
            grid_color: 0xd4d4d4,
            grid_gap: 20.0,
            bg_pattern: BackgroundPattern::Dots,
            node_bg_color: 0xffffff,
            node_border_color: 0xe2e2e2,
            selection_color: 0x3b82f6,
            handle_label_color: 0xe1e5eb,
            target_color: 0x93c5fd,
            target_active_color: 0x3b82f6,
            marquee_color: 0x3b82f6,
            edge_color: 0xb1b1b7,
            edge_selected_color: 0x3b82f6,
            measured: false,
        }
    }

    /// Register a renderer for a specific node type.
    pub fn node_renderer(
        mut self,
        node_type: impl Into<SharedString>,
        renderer: impl Fn(&FlowNode, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.node_renderers
            .insert(node_type.into(), Box::new(renderer));
        self
    }

    /// Set the default renderer for nodes without a specific type renderer.
    pub fn default_renderer(
        mut self,
        renderer: impl Fn(&FlowNode, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.default_renderer = Some(Box::new(renderer));
        self
    }

    /// Set the on_connect callback.
    pub fn on_connect(mut self, callback: impl Fn(&Connection, &mut FlowState) + 'static) -> Self {
        self.on_connect = Some(Box::new(callback));
        self
    }

    /// Hide the default node wrapper chrome (bg, border, shadow, padding).
    pub fn no_node_chrome(mut self) -> Self {
        self.show_node_chrome = false;
        self
    }

    /// Set the canvas background color.
    pub fn bg_color(mut self, color: u32) -> Self {
        self.bg_color = color;
        self
    }

    /// Set the dot grid color.
    pub fn grid_color(mut self, color: u32) -> Self {
        self.grid_color = color;
        self
    }

    /// Set the grid spacing in world coordinates.
    pub fn grid_gap(mut self, gap: f32) -> Self {
        self.grid_gap = gap.max(2.0);
        self
    }

    /// Set the node wrapper background color.
    pub fn node_bg_color(mut self, color: u32) -> Self {
        self.node_bg_color = color;
        self
    }

    /// Set the node wrapper border color.
    pub fn node_border_color(mut self, color: u32) -> Self {
        self.node_border_color = color;
        self
    }

    /// Set the ring drawn on a selected node.
    pub fn selection_color(mut self, color: u32) -> Self {
        self.selection_color = color;
        self
    }

    /// Set the ink used for handle labels.
    pub fn handle_label_color(mut self, color: u32) -> Self {
        self.handle_label_color = color;
        self
    }

    /// Set the drop-target and draft-line colour.
    pub fn target_color(mut self, color: u32) -> Self {
        self.target_color = color;
        self
    }

    /// Set the colour of the handle a drag is snapped to.
    pub fn target_active_color(mut self, color: u32) -> Self {
        self.target_active_color = color;
        self
    }

    /// Set the box-selection marquee colour.
    pub fn marquee_color(mut self, color: u32) -> Self {
        self.marquee_color = color;
        self
    }

    /// Set the default and selected wire colours.
    pub fn edge_colors(mut self, default: u32, selected: u32) -> Self {
        self.edge_color = default;
        self.edge_selected_color = selected;
        self
    }

    /// Set the background pattern (Dots, Lines, Cross).
    pub fn bg_pattern(mut self, pattern: BackgroundPattern) -> Self {
        self.bg_pattern = pattern;
        self
    }

    /// Set custom connection validation.
    pub fn validate_connection(
        mut self,
        validator: impl Fn(&Connection, &FlowState) -> bool + 'static,
    ) -> Self {
        self.is_valid_connection = Some(Box::new(validator));
        self
    }

    /// Render a single node using the appropriate renderer.
    fn render_node(
        &self,
        node: &FlowNode,
        viewport: &Viewport,
        is_connecting: bool,
        snap_node_id: Option<&NodeId>,
        _entity_id: EntityId,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if node.hidden {
            return div().into_any_element();
        }

        let (screen_x, screen_y) = viewport.flow_to_screen(node.position);

        // Choose renderer
        let content = if let Some(node_type) = &node.node_type {
            if let Some(renderer) = self.node_renderers.get(node_type) {
                renderer(node, window, cx)
            } else {
                self.render_default_node(node, viewport.zoom, window, cx)
            }
        } else {
            self.render_default_node(node, viewport.zoom, window, cx)
        };

        let node_id = node.id.clone();
        let state = self.state.clone();
        let selected = node.selected;
        let dragging = node.dragging;
        let show_chrome = self.show_node_chrome;
        let node_bg = self.node_bg_color;
        let node_border = self.node_border_color;
        let selection_color = self.selection_color;
        let element_id: ElementId = ElementId::Name(node.id.clone());

        // Build handle dot elements (skip if not connecting to reduce overhead)
        let handle_elements = if !node.handles.is_empty() {
            Self::render_handles(
                &node.handles,
                &node.id,
                &state,
                is_connecting,
                snap_node_id,
                node_bg,
                node_border,
                viewport.zoom,
                _entity_id,
                self.handle_label_color,
                self.target_color,
                self.target_active_color,
                &self.focus_handle,
            )
        } else {
            Vec::new()
        };

        // Per-node measurement canvas: captures actual wrapper size.
        // Writes dimensions silently — the graph-level re-render is triggered once.
        let measure_state = self.state.clone();
        let measure_node_id = node.id.clone();
        let prev_w = node.measured_width;
        let prev_h = node.measured_height;
        let zoom = viewport.zoom;
        let measure_canvas = canvas(
            |_bounds, _window, _cx| {},
            move |bounds, _: (), _window, cx| {
                // Edges, hit testing and Fit use world dimensions, not scaled pixels.
                let w = bounds.size.width / zoom;
                let h = bounds.size.height / zoom;
                if prev_w != Some(w) || prev_h != Some(h) {
                    measure_state.update(cx, |state, _| {
                        if let Some(node) = state.get_node_mut(&measure_node_id) {
                            node.measured_width = Some(w);
                            node.measured_height = Some(h);
                        }
                    });
                }
            },
        )
        .size_full()
        .absolute();

        div()
            .id(element_id)
            .absolute()
            .left(px(screen_x))
            .top(px(screen_y))
            // Node box styling on the wrapper so handles align to visual edges
            .when(show_chrome, |el: Stateful<Div>| {
                el.bg(gpui::rgb(node_bg))
                    .border(px(zoom))
                    .border_color(gpui::rgb(node_border))
                    .rounded(px(8.0 * zoom))
                    .shadow_sm()
                    .px(px(16.0 * zoom))
                    .py(px(8.0 * zoom))
            })
            .cursor(if dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .when(selected, |el: Stateful<Div>| {
                el.border(px(2.0 * zoom))
                    .border_color(gpui::rgb(selection_color))
            })
            .on_mouse_down(MouseButton::Left, {
                let node_id = node_id.clone();
                let state = state.clone();
                move |event, _window, cx| {
                    let multi = event.modifiers.platform;
                    let mouse_pos = event.position;

                    state.update(cx, |state, _| {
                        // Don't start node drag if we're connecting
                        if state.connecting.is_some() {
                            return;
                        }

                        // Handle selection
                        if !multi {
                            for n in &mut state.nodes {
                                n.selected = false;
                            }
                        }
                        // Z-index elevation
                        let max_z = state.nodes.iter().map(|n| n.z_index).max().unwrap_or(0);
                        if let Some(n) = state.get_node_mut(&node_id) {
                            n.selected = !n.selected || !multi;
                            if n.selected {
                                n.z_index = max_z + 1;
                            }
                        }

                        // Start drag — collect all selected nodes
                        let mut node_origins = Vec::new();
                        if let Some(n) = state.get_node(&node_id) {
                            if n.selected && n.draggable {
                                for n in &state.nodes {
                                    if n.selected && n.draggable {
                                        node_origins.push((n.id.clone(), n.position));
                                    }
                                }
                            }
                        }

                        if !node_origins.is_empty() {
                            state.push_undo();
                            state.drag_state = Some(DragState {
                                origin_mouse: (mouse_pos.x.as_f32(), mouse_pos.y.as_f32()),
                                node_origins,
                            });
                            for n in &mut state.nodes {
                                if n.selected && n.draggable {
                                    n.dragging = true;
                                }
                            }
                        }
                    });
                }
            })
            .child(content)
            .child(measure_canvas)
            .children(handle_elements)
            .into_any_element()
    }

    /// Render handle dots for a node.
    #[allow(clippy::too_many_arguments)] // Cohesive per-node render inputs; a struct would only rename them.
    fn render_handles(
        handles: &[HandleDef],
        node_id: &NodeId,
        state: &Entity<FlowState>,
        is_connecting: bool,
        snap_node_id: Option<&NodeId>,
        default_bg: u32,
        default_border: u32,
        zoom: f32,
        entity_id: EntityId,
        label_color: u32,
        target_color: u32,
        target_active_color: u32,
        focus: &FocusHandle,
    ) -> Vec<AnyElement> {
        let handle_size = 24.0;
        let half = handle_size / 2.0;
        let is_snapped_node = snap_node_id == Some(node_id);

        handles
            .iter()
            .enumerate()
            .map(|(i, handle)| {
                let node_id = node_id.clone();
                let handle_id = handle.id.clone();
                let handle_type = handle.handle_type;
                let is_connectable = handle.is_connectable;
                let focus = focus.clone();
                let handle_position = handle.position;
                let state = state.clone();

                // Highlight: strongly if this is the snapped target, mildly if potential target
                let is_snap_target = is_snapped_node && handle_type == HandleType::Target;
                let is_potential_target = is_connecting && handle_type == HandleType::Target;

                let (bg_color, border_color, size_mult) = if is_snap_target {
                    // Actively snapped — the mark you are about to drop onto.
                    (
                        gpui::rgb(target_active_color),
                        gpui::rgb(target_active_color),
                        1.4,
                    )
                } else if is_potential_target {
                    // A valid destination, held quieter than the live target.
                    (gpui::rgb(target_color), gpui::rgb(target_color), 1.0)
                } else {
                    (gpui::rgb(default_bg), gpui::rgb(default_border), 1.0)
                };
                let dot_size = 10.0 * size_mult;

                let dot = div()
                    .w(px(dot_size))
                    .h(px(dot_size))
                    .rounded_full()
                    .bg(bg_color)
                    .border_1()
                    .border_color(border_color)
                    .cursor(CursorStyle::Crosshair)
                    .flex_shrink_0();

                let container = div()
                    .id(ElementId::Integer(i as u64))
                    .absolute()
                    .flex()
                    .items_center()
                    .justify_center()
                    // Handle mouse down → start connection
                    .on_mouse_down(MouseButton::Left, {
                        let state = state.clone();
                        let node_id = node_id.clone();
                        let handle_id = handle_id.clone();
                        move |event, window, cx| {
                            if !is_connectable {
                                return;
                            }
                            cx.stop_propagation();
                            window.focus(&focus, cx);
                            let mouse_pos = event.position;
                            state.update(cx, |state, _| {
                                // Find handle center for the from_point
                                let from_point = state
                                    .find_handle_center(&node_id, &handle_id, handle_position)
                                    .unwrap_or((mouse_pos.x.as_f32(), mouse_pos.y.as_f32()));

                                state.connecting = Some(ConnectionDraft {
                                    from_node: node_id.clone(),
                                    from_handle: handle_id.clone(),
                                    from_type: handle_type,
                                    from_position: handle_position,
                                    from_point,
                                    to_point: (mouse_pos.x.as_f32(), mouse_pos.y.as_f32()),
                                    snap_target: None,
                                });
                                // Prevent node drag
                                state.drag_state = None;
                            });
                            cx.notify(entity_id);
                        }
                    });

                let same_side: Vec<_> = handles
                    .iter()
                    .filter(|h| h.position == handle.position)
                    .collect();
                let index = same_side
                    .iter()
                    .position(|h| h.id == handle.id)
                    .unwrap_or(0);
                let fraction = (index + 1) as f32 / (same_side.len() + 1) as f32;
                let container = container.w(px(handle_size)).h(px(handle_size));
                let container = match handle.position {
                    HandlePosition::Left => container
                        .left(px(-half))
                        .top(relative(fraction))
                        .mt(px(-half)),
                    HandlePosition::Right => container
                        .right(px(-half))
                        .top(relative(fraction))
                        .mt(px(-half)),
                    HandlePosition::Top => container
                        .top(px(-half))
                        .left(relative(fraction))
                        .ml(px(-half)),
                    HandlePosition::Bottom => container
                        .bottom(px(-half))
                        .left(relative(fraction))
                        .ml(px(-half)),
                };

                container
                    .when_some(handle.offset, |el, offset| match handle.position {
                        HandlePosition::Left | HandlePosition::Right => el.top(px(offset * zoom)),
                        _ => el.left(px(offset * zoom)),
                    })
                    .child(dot)
                    .when_some(handle.label.clone(), |el, label| {
                        el.child(
                            div()
                                .absolute()
                                .text_size(px(9.0 * zoom))
                                .text_color(gpui::rgb(label_color))
                                .font_family("Martian Mono")
                                .w(px(170.0 * zoom))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .when(handle.position == HandlePosition::Left, |el| {
                                    el.left(px(half + 8.0))
                                })
                                .when(handle.position == HandlePosition::Right, |el| {
                                    el.right(px(half + 8.0)).text_right()
                                })
                                .child(label),
                        )
                    })
                    .into_any_element()
            })
            .collect()
    }

    /// Default node rendering — just the label content (styling is on the wrapper).
    fn render_default_node(
        &self,
        node: &FlowNode,
        zoom: f32,
        _window: &mut Window,
        _cx: &mut App,
    ) -> AnyElement {
        if let Some(renderer) = &self.default_renderer {
            return renderer(node, _window, _cx);
        }
        div()
            .min_w(px(80.0 * zoom))
            .text_size(px(14.0 * zoom))
            .text_color(gpui::rgb(0x1a1a1a))
            .child(if node.label.is_empty() {
                node.id.to_string()
            } else {
                node.label.to_string()
            })
            .into_any_element()
    }

    /// Paint the background dot grid.
    fn paint_grid(
        bounds: &Bounds<Pixels>,
        viewport: &Viewport,
        grid_color: u32,
        grid_gap: f32,
        pattern: BackgroundPattern,
        window: &mut Window,
    ) {
        let color = gpui::rgb(grid_color);
        let spacing = grid_gap * viewport.zoom;

        if spacing < 5.0 {
            return;
        }

        let start_x = viewport.x % spacing;
        let start_y = viewport.y % spacing;
        let bw = bounds.size.width.as_f32();
        let bh = bounds.size.height.as_f32();
        let ox = bounds.origin.x;
        let oy = bounds.origin.y;

        match pattern {
            BackgroundPattern::Dots => {
                let dot_size = px(1.5 * viewport.zoom.min(1.0));
                let mut x = start_x;
                while x < bw {
                    let mut y = start_y;
                    while y < bh {
                        let dot_bounds = Bounds::new(
                            Point::new(ox + px(x) - dot_size / 2.0, oy + px(y) - dot_size / 2.0),
                            Size {
                                width: dot_size,
                                height: dot_size,
                            },
                        );
                        window.paint_quad(gpui::fill(dot_bounds, color));
                        y += spacing;
                    }
                    x += spacing;
                }
            }
            BackgroundPattern::Lines => {
                let line_w = px(0.5);
                let mut x = start_x;
                while x < bw {
                    let line = Bounds::new(
                        Point::new(ox + px(x), oy),
                        Size {
                            width: line_w,
                            height: bounds.size.height,
                        },
                    );
                    window.paint_quad(gpui::fill(line, color));
                    x += spacing;
                }
            }
            BackgroundPattern::Cross => {
                let line_w = px(0.5);
                // Vertical lines
                let mut x = start_x;
                while x < bw {
                    let line = Bounds::new(
                        Point::new(ox + px(x), oy),
                        Size {
                            width: line_w,
                            height: bounds.size.height,
                        },
                    );
                    window.paint_quad(gpui::fill(line, color));
                    x += spacing;
                }
                // Horizontal lines
                let mut y = start_y;
                while y < bh {
                    let line = Bounds::new(
                        Point::new(ox, oy + px(y)),
                        Size {
                            width: bounds.size.width,
                            height: line_w,
                        },
                    );
                    window.paint_quad(gpui::fill(line, color));
                    y += spacing;
                }
            }
        }
    }

    /// Paint a selection box rectangle.
    fn paint_selection_box(sel: &SelectionBox, color: u32, window: &mut Window) {
        let x = sel.start.0.min(sel.current.0);
        let y = sel.start.1.min(sel.current.1);
        let w = (sel.start.0 - sel.current.0).abs();
        let h = (sel.start.1 - sel.current.1).abs();

        if w < 1.0 || h < 1.0 {
            return;
        }

        let bounds = Bounds::new(
            Point::new(px(x), px(y)),
            Size {
                width: px(w),
                height: px(h),
            },
        );

        // A wash of the marquee colour, with a solid hairline edge.
        window.paint_quad(fill(bounds, gpui::rgba((color << 8) | 0x18)));

        let border_color: Background = gpui::rgb(color).into();
        // Top
        let top = Bounds::new(
            Point::new(px(x), px(y)),
            Size {
                width: px(w),
                height: px(1.0),
            },
        );
        window.paint_quad(fill(top, border_color.clone()));
        // Bottom
        let bottom = Bounds::new(
            Point::new(px(x), px(y + h - 1.0)),
            Size {
                width: px(w),
                height: px(1.0),
            },
        );
        window.paint_quad(fill(bottom, border_color.clone()));
        // Left
        let left = Bounds::new(
            Point::new(px(x), px(y)),
            Size {
                width: px(1.0),
                height: px(h),
            },
        );
        window.paint_quad(fill(left, border_color.clone()));
        // Right
        let right = Bounds::new(
            Point::new(px(x + w - 1.0), px(y)),
            Size {
                width: px(1.0),
                height: px(h),
            },
        );
        window.paint_quad(fill(right, border_color));
    }

    /// Paint a draft connection line from handle to mouse cursor.
    fn paint_connection_draft(draft: &ConnectionDraft, window: &mut Window) {
        let color: Background = gpui::rgba(0x3b82f680).into();
        let (sx, sy) = draft.from_point;
        let (tx, ty) = draft.to_point;

        let mut builder = PathBuilder::stroke(px(2.0));
        builder.move_to(Point::new(px(sx), px(sy)));

        // Simple bezier toward cursor
        let dx = (tx - sx).abs() * 0.5;
        let (cx1, cy1, cx2, cy2) = match draft.from_position {
            HandlePosition::Right => (sx + dx, sy, tx - dx, ty),
            HandlePosition::Left => (sx - dx, sy, tx + dx, ty),
            HandlePosition::Bottom => (sx, sy + dx, tx, ty - dx),
            HandlePosition::Top => (sx, sy - dx, tx, ty + dx),
        };

        builder.cubic_bezier_to(
            Point::new(px(tx), px(ty)),
            Point::new(px(cx1), px(cy1)),
            Point::new(px(cx2), px(cy2)),
        );

        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

impl Render for FlowGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // On second render, edges will have correct node measurements
        // from the first render's measurement canvases.
        if !self.measured {
            self.measured = true;
            cx.notify(); // triggers one immediate re-render
        }

        let entity_id = cx.entity_id();
        let window_size = window.viewport_size();
        let win_w = window_size.width.as_f32();
        let win_h = window_size.height.as_f32();
        let cull_margin = 200.0;

        // Read state, extract what we need, then release the borrow
        let (
            viewport,
            is_panning,
            is_connecting,
            snap_node_id,
            connecting_draft,
            selection_box,
            visible_nodes,
            edge_label_elements,
        ) = {
            let state = self.state.read(cx);
            let viewport = state.viewport;
            let is_panning = state.pan_drag.is_some();
            let is_connecting = state.connecting.is_some();
            let snap_node_id = state
                .connecting
                .as_ref()
                .and_then(|d| d.snap_target.as_ref())
                .map(|t| t.node_id.clone());
            let connecting_draft = state.connecting.clone();
            let selection_box = state.selection_box;

            // Build visible node indices
            let mut visible_indices: Vec<usize> = Vec::new();
            for (i, node) in state.nodes.iter().enumerate() {
                if node.hidden {
                    continue;
                }
                let (sx, sy) = viewport.flow_to_screen(node.position);
                let nw = node.measured_width.map(|p| p.as_f32()).unwrap_or(150.0);
                let nh = node.measured_height.map(|p| p.as_f32()).unwrap_or(50.0);
                if sx + nw < -cull_margin
                    || sx > win_w + cull_margin
                    || sy + nh < -cull_margin
                    || sy > win_h + cull_margin
                {
                    continue;
                }
                visible_indices.push(i);
            }
            visible_indices.sort_by_key(|&i| state.nodes[i].z_index);

            let visible_nodes: Vec<FlowNode> = visible_indices
                .iter()
                .map(|&i| state.nodes[i].clone())
                .collect();

            // Edge labels
            let bg = self.bg_color;
            let mut edge_label_elements: Vec<AnyElement> = Vec::new();
            for edge in &state.edges {
                if let Some(ref label) = edge.label {
                    if let Some((lx, ly)) = edges::compute_edge_label_position(state, edge) {
                        let label_color = edge.color.unwrap_or(0xb1b1b7);
                        edge_label_elements.push(
                            div()
                                .absolute()
                                .left(px(lx - state.canvas_origin.x))
                                .top(px(ly - state.canvas_origin.y))
                                .px_2()
                                .py_0p5()
                                .bg(gpui::rgb(bg))
                                .rounded_sm()
                                .text_xs()
                                .text_color(gpui::rgb(label_color))
                                .child(label.to_string())
                                .into_any_element(),
                        );
                    }
                }
            }

            (
                viewport,
                is_panning,
                is_connecting,
                snap_node_id,
                connecting_draft,
                selection_box,
                visible_nodes,
                edge_label_elements,
            )
        }; // state borrow ends here

        // Render only visible nodes
        let mut node_elements: Vec<AnyElement> = Vec::with_capacity(visible_nodes.len());
        for node in &visible_nodes {
            node_elements.push(self.render_node(
                node,
                &viewport,
                is_connecting,
                snap_node_id.as_ref(),
                entity_id,
                window,
                cx,
            ));
        }

        let state_for_canvas = self.state.clone();
        let viewport_for_canvas = viewport;
        let bg_color = self.bg_color;
        let grid_color = self.grid_color;
        let marquee_color = self.marquee_color;
        let edge_color = self.edge_color;
        let edge_selected_color = self.edge_selected_color;
        let bg_pattern = self.bg_pattern;
        let grid_gap = self.grid_gap;
        let state_for_scroll = self.state.clone();
        let state_for_mouse_down = self.state.clone();
        let state_for_mouse_move = self.state.clone();
        let state_for_mouse_up = self.state.clone();
        let state_for_key = self.state.clone();
        let state_for_pinch = self.state.clone();

        div()
            .id("flow-graph")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .relative()
            .bg(gpui::rgb(bg_color))
            .cursor(if is_panning {
                CursorStyle::ClosedHand
            } else if is_connecting {
                CursorStyle::Crosshair
            } else {
                CursorStyle::Arrow
            })
            // Background grid + edge painting layer
            .child(
                canvas(
                    {
                        let state = self.state.clone();
                        move |bounds, _, cx| {
                            state.update(cx, |state, _| {
                                state.canvas_origin = FlowPoint::new(
                                    bounds.origin.x.as_f32(),
                                    bounds.origin.y.as_f32(),
                                )
                            });
                        }
                    },
                    move |bounds, _: (), window, cx| {
                        Self::paint_grid(
                            &bounds,
                            &viewport_for_canvas,
                            grid_color,
                            grid_gap,
                            bg_pattern,
                            window,
                        );
                        let state = state_for_canvas.read(cx);
                        edges::paint_edges(state, edge_color, edge_selected_color, window);

                        // Paint draft connection line
                        if let Some(ref draft) = connecting_draft {
                            Self::paint_connection_draft(draft, window);
                        }

                        // Paint selection box
                        if let Some(ref sel) = selection_box {
                            Self::paint_selection_box(sel, marquee_color, window);
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            // Node layer
            .children(node_elements)
            // Edge labels
            .children(edge_label_elements)
            // Mouse down on empty space → start panning, deselect, or edge selection
            .on_mouse_down(MouseButton::Left, {
                let entity_id = entity_id;
                let focus = self.focus_handle.clone();
                move |event, window, cx| {
                    window.focus(&focus, cx);
                    let mouse_pos = event.position;
                    state_for_mouse_down.update(cx, |state, _| {
                        // If a node drag or connection was already started, skip
                        if state.drag_state.is_some() || state.connecting.is_some() {
                            return;
                        }

                        // Try edge hit testing
                        let mx = mouse_pos.x.as_f32();
                        let my = mouse_pos.y.as_f32();
                        if let Some(edge_id) = edges::hit_test_edges(state, mx, my, 5.0) {
                            if !event.modifiers.platform {
                                for n in &mut state.nodes {
                                    n.selected = false;
                                }
                                for e in &mut state.edges {
                                    e.selected = false;
                                }
                            }
                            if let Some(edge) = state.edges.iter_mut().find(|e| e.id == edge_id) {
                                edge.selected = true;
                            }
                            return;
                        }

                        // Shift+drag → start box selection
                        if event.modifiers.shift {
                            if !event.modifiers.platform {
                                for n in &mut state.nodes {
                                    n.selected = false;
                                }
                                for e in &mut state.edges {
                                    e.selected = false;
                                }
                            }
                            state.selection_box = Some(SelectionBox {
                                start: (mx, my),
                                current: (mx, my),
                            });
                            return;
                        }

                        // Deselect all nodes and edges
                        if !event.modifiers.platform {
                            for n in &mut state.nodes {
                                n.selected = false;
                            }
                            for e in &mut state.edges {
                                e.selected = false;
                            }
                        }

                        // Start pan drag
                        state.pan_drag = Some(PanDragState {
                            start_mouse: (mx, my),
                            start_viewport: (state.viewport.x, state.viewport.y),
                        });
                    });
                    cx.notify(entity_id);
                }
            })
            // Global mouse move → handle dragging, panning, or connecting
            .on_mouse_move({
                let entity_id = entity_id;
                move |event, _window, cx| {
                    let mouse_pos = event.position;
                    let mx = mouse_pos.x.as_f32();
                    let my = mouse_pos.y.as_f32();

                    let mut changed = false;

                    state_for_mouse_move.update(cx, |state, _| {
                        // Box selection
                        if let Some(ref mut sel) = state.selection_box {
                            sel.current = (mx, my);
                            // Select nodes whose screen bounds intersect the box
                            let (sx, sy, ex, ey) = (
                                sel.start.0.min(sel.current.0),
                                sel.start.1.min(sel.current.1),
                                sel.start.0.max(sel.current.0),
                                sel.start.1.max(sel.current.1),
                            );
                            let viewport = state.viewport;
                            for node in &mut state.nodes {
                                if node.hidden {
                                    continue;
                                }
                                let (nx, ny) = viewport.flow_to_screen(node.position);
                                let (nx, ny) =
                                    (nx + state.canvas_origin.x, ny + state.canvas_origin.y);
                                let nw = node.measured_width.map(|p| p.as_f32()).unwrap_or(150.0)
                                    * viewport.zoom;
                                let nh = node.measured_height.map(|p| p.as_f32()).unwrap_or(40.0)
                                    * viewport.zoom;
                                // AABB intersection
                                let intersects = nx < ex && nx + nw > sx && ny < ey && ny + nh > sy;
                                node.selected = intersects;
                            }
                            changed = true;
                        }
                        // Connection dragging with snap-to-handle
                        else if state.connecting.is_some() {
                            // Clone draft to avoid borrow conflict with find_snap_target
                            let mut draft = state.connecting.clone().unwrap();
                            let snap = state.find_snap_target(&draft, mx, my);
                            if let Some(ref target) = snap {
                                draft.to_point = target.point;
                            } else {
                                draft.to_point = (mx, my);
                            }
                            draft.snap_target = snap;
                            state.connecting = Some(draft);
                            changed = true;
                        }
                        // Node dragging
                        else if let Some(ref drag) = state.drag_state {
                            let dx = (mx - drag.origin_mouse.0) / state.viewport.zoom;
                            let dy = (my - drag.origin_mouse.1) / state.viewport.zoom;

                            let origins = drag.node_origins.clone();
                            let snap = state.snap_to_grid;
                            let snap_grid = state.snap_grid;
                            for (node_id, origin) in &origins {
                                let mut new_x = origin.x + dx;
                                let mut new_y = origin.y + dy;

                                if snap {
                                    let (gx, gy) = snap_grid;
                                    new_x = (new_x / gx).round() * gx;
                                    new_y = (new_y / gy).round() * gy;
                                }

                                if let Some(node) = state.get_node_mut(node_id) {
                                    node.position = FlowPoint::new(new_x, new_y);
                                }
                            }
                            changed = true;
                        }
                        // Canvas panning
                        else if let Some(pan) = state.pan_drag {
                            state.viewport.x = pan.start_viewport.0 + (mx - pan.start_mouse.0);
                            state.viewport.y = pan.start_viewport.1 + (my - pan.start_mouse.1);
                            changed = true;
                        }
                    });

                    if changed {
                        cx.notify(entity_id);
                    }
                }
            })
            // Global mouse up → end dragging, panning, or cancel connection
            .on_mouse_up(MouseButton::Left, {
                let entity_id = entity_id;
                move |event, _window, cx| {
                    let mut changed = false;

                    state_for_mouse_up.update(cx, |state, _| {
                        if state.selection_box.is_some() {
                            state.selection_box = None;
                            changed = true;
                        }
                        if let Some(draft) = state.connecting.take() {
                            // Resolve at release, not the last move (which may be coalesced).
                            if let Some(snap) = state.find_snap_target(
                                &draft,
                                event.position.x.as_f32(),
                                event.position.y.as_f32(),
                            ) {
                                // Complete the connection
                                let (source, target, source_handle, target_handle) =
                                    if draft.from_type == HandleType::Source {
                                        (
                                            draft.from_node.clone(),
                                            snap.node_id.clone(),
                                            draft.from_handle.clone(),
                                            snap.handle_id.clone(),
                                        )
                                    } else {
                                        (
                                            snap.node_id.clone(),
                                            draft.from_node.clone(),
                                            snap.handle_id.clone(),
                                            draft.from_handle.clone(),
                                        )
                                    };

                                let connection = Connection {
                                    source,
                                    target,
                                    source_handle,
                                    target_handle,
                                };

                                if state.is_valid_connection(&connection) {
                                    state.push_undo();
                                    let edge_id: SharedString = format!(
                                        "e{}:{:?}-{}:{:?}",
                                        connection.source,
                                        connection.source_handle,
                                        connection.target,
                                        connection.target_handle
                                    )
                                    .into();
                                    state.add_edge_from_connection(&connection, edge_id);
                                }
                            }
                            // else: connection cancelled (no snap target)
                            changed = true;
                        }
                        if state.drag_state.is_some() {
                            state.drag_state = None;
                            for n in &mut state.nodes {
                                n.dragging = false;
                            }
                            changed = true;
                        }
                        if state.pan_drag.is_some() {
                            state.pan_drag = None;
                            changed = true;
                        }
                    });

                    if changed {
                        cx.notify(entity_id);
                    }
                }
            })
            // Keyboard shortcuts (delete, undo/redo)
            .on_key_down({
                let entity_id = entity_id;
                let focus = self.focus_handle.clone();
                move |event: &KeyDownEvent, window, cx| {
                    let key: &str = event.keystroke.key.as_ref();

                    // Only handle destructive keys (delete/backspace) when the
                    // flow graph itself is focused — not when a child element
                    // (like an Input inside a node) has focus.
                    let graph_focused = focus.is_focused(window);

                    // RawWeave owns graph/parameter history; canvas-only undo
                    // would restore nodes without their persisted parameters.
                    // Select all: Cmd+A
                    if key == "a" && event.keystroke.modifiers.platform && graph_focused {
                        state_for_key.update(cx, |state, _| {
                            state.select_all();
                        });
                        cx.notify(entity_id);
                        return;
                    }

                    if (key == "backspace" || key == "delete") && graph_focused {
                        state_for_key.update(cx, |state, _| {
                            state.push_undo();
                            // Collect IDs of selected deletable nodes
                            let node_ids_to_remove: Vec<NodeId> = state
                                .nodes
                                .iter()
                                .filter(|n| n.selected && n.deletable)
                                .map(|n| n.id.clone())
                                .collect();

                            // Remove edges connected to deleted nodes + selected edges
                            state.edges.retain(|e| {
                                let connected_to_removed = node_ids_to_remove.contains(&e.source)
                                    || node_ids_to_remove.contains(&e.target);
                                let selected_and_deletable = e.selected && e.deletable;
                                !connected_to_removed && !selected_and_deletable
                            });

                            // Remove selected nodes
                            state.nodes.retain(|n| !node_ids_to_remove.contains(&n.id));
                            if !node_ids_to_remove.is_empty() {
                                state.rebuild_lookup();
                            }
                        });
                        cx.notify(entity_id);
                    }
                }
            })
            // Wheel zooms at the cursor; Shift+wheel retains trackpad/sideways pan.
            .on_scroll_wheel({
                move |event, _window, cx| {
                    let delta = event.delta.pixel_delta(px(20.0));
                    let mouse_pos = event.position;

                    if !event.modifiers.shift || event.modifiers.platform || event.modifiers.control
                    {
                        // Positive GPUI wheel delta is wheel-up. Multiplicative steps
                        // keep wheel-down reciprocal and useful at every zoom level.
                        let factor = (delta.y.as_f32() * 0.002).exp();
                        state_for_scroll.update(cx, |state, _| {
                            let old_zoom = state.viewport.zoom;
                            let new_zoom =
                                (old_zoom * factor).clamp(state.min_zoom, state.max_zoom);
                            let mx = mouse_pos.x.as_f32() - state.canvas_origin.x;
                            let my = mouse_pos.y.as_f32() - state.canvas_origin.y;
                            state.viewport.x = mx - (mx - state.viewport.x) * (new_zoom / old_zoom);
                            state.viewport.y = my - (my - state.viewport.y) * (new_zoom / old_zoom);
                            state.viewport.zoom = new_zoom;
                        });
                    } else {
                        // Pan
                        state_for_scroll.update(cx, |state, _| {
                            state.viewport.x += delta.x.as_f32();
                            state.viewport.y += delta.y.as_f32();
                        });
                    }

                    cx.notify(entity_id);
                }
            })
            // Pinch to zoom (macOS trackpad / Linux Wayland)
            .on_pinch({
                move |event, _window, cx| {
                    let mouse_pos = event.position;
                    let zoom_delta = event.delta;
                    state_for_pinch.update(cx, |state, _| {
                        let old_zoom = state.viewport.zoom;
                        let new_zoom =
                            (old_zoom * (1.0 + zoom_delta)).clamp(state.min_zoom, state.max_zoom);
                        let mx = mouse_pos.x.as_f32() - state.canvas_origin.x;
                        let my = mouse_pos.y.as_f32() - state.canvas_origin.y;
                        state.viewport.x = mx - (mx - state.viewport.x) * (new_zoom / old_zoom);
                        state.viewport.y = my - (my - state.viewport.y) * (new_zoom / old_zoom);
                        state.viewport.zoom = new_zoom;
                    });
                    cx.notify(entity_id);
                }
            })
    }
}
