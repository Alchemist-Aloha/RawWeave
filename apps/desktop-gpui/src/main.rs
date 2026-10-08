extern crate gpui_kit as gpui;
mod native_nodes;
mod native_scopes;
mod native_theme;
mod native_viewer;
#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests;
use gpui_flow::{FlowEdge, FlowGraph, FlowNode, FlowState, HandleDef, HandlePosition};
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{AnyInputState, Input, InputEvent, InputState, Textarea, TextareaState},
    *,
};
use gpui_kit::component::{
    checkbox::Checkbox,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use native_theme as t;
use native_viewer::Viewers;
use rawweave_batch::{Compression, OutputSharpening};
use rawweave_core::NodeId as CoreNodeId;
use rawweave_gpui::export::ExportSettings;
use rawweave_gpui::library::{drop_position, library_groups, node_category};
use rawweave_gpui::parameters::{
    ParameterDraft, add_curve_point, curve_points, parameter_ux, remove_curve_point,
};
use rawweave_gpui::workspace::{Workspace, preferences_path};
use rawweave_gpui::{
    MoveHistory, Session, Shortcut, Source, bounded_read, save_workflow_atomic, shortcut,
};
use rawweave_node_api::{ParameterType, ParameterValue};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
#[derive(Clone)]
struct LibraryDrag {
    kind: String,
    label: String,
}
impl Render for LibraryDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_2()
            .bg(rgb(t::ROOM_RAISE))
            .text_color(rgb(t::ROOM_INK))
            .font_family(t::Face::Control.family())
            .font_weight(t::Face::Control.weight())
            .text_size(t::Face::Control.size())
            .border_1()
            .border_color(rgb(t::ROOM_LINE_STRONG))
            .rounded(t::RADIUS)
            .child(self.label.clone())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Image,
    Workflow,
    Blueprint,
}

struct Field {
    draft: ParameterDraft,
    state: AnyInputState,
    slider: Option<Entity<SliderState>>,
    exposed: bool,
    error: Option<String>,
    stepping: bool,
    curve_index: usize,
    curve_bounds: Option<Bounds<Pixels>>,
}
impl Field {
    fn set_widget_value(&self, text: String, window: &mut Window, cx: &mut App) {
        match &self.state {
            AnyInputState::Input(state) => {
                state.update(cx, |input, cx| input.set_value(text, window, cx))
            }
            AnyInputState::Textarea(state) => {
                state.update(cx, |input, cx| input.set_value(text, window, cx))
            }
            _ => {}
        }
    }
}
struct Editor {
    session: Session,
    flow_state: Entity<FlowState>,
    flow: Entity<FlowGraph>,
    _flow_subscription: Subscription,
    fields: Vec<Field>,
    field_subscriptions: Vec<Subscription>,
    selected: Option<String>,
    export_busy: bool,
    export_status: String,
    export_settings: ExportSettings,
    export_quality: Entity<InputState>,
    export_long_edge: Entity<InputState>,
    show_export_settings: bool,
    show_shortcuts: bool,
    show_advanced: bool,
    compatible_only: bool,
    collapsed_categories: BTreeSet<String>,
    search: Entity<InputState>,
    _search_subscription: Subscription,
    viewers: Entity<Viewers>,
    source_generation: u64,
    source_path: Option<PathBuf>,
    status: String,
    gpu_label: String,
    undo: Vec<String>,
    redo: Vec<String>,
    move_history: MoveHistory,
    layout: BTreeMap<String, (f32, f32)>,
    focus: FocusHandle,
    workspace: Workspace,
    preferences_file: Option<PathBuf>,
    workspace_epoch: u64,
    canvas_size: (f32, f32),
    pending_canvas_fit: bool,
    pending_canvas_focus: Option<String>,
    minimap: Entity<gpui_flow::Minimap>,
    controls_open: bool,
    curve_gesture: Option<(usize, [f32; 4])>,
    geometry: Entity<native_nodes::GeometryHelper>,
}
impl Editor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session = Session::default();
        let status = "Open an image or add nodes to start weaving".into();
        let workspace = preferences_path()
            .and_then(|path| {
                Workspace::load(&path)
                    .map_err(|error| eprintln!("Workspace preferences: {error}"))
                    .ok()
            })
            .unwrap_or_default();
        let flow_state = cx.new(|_| FlowState::new(vec![], vec![]));
        let flow_state_for_minimap = flow_state.clone();
        let renderer_state = flow_state.clone();
        let owner = cx.entity().downgrade();
        let renderer_owner = owner.clone();
        let flow = cx.new(|cx| {
            FlowGraph::new(flow_state.clone(), cx)
                .default_renderer(move |node, window, cx| {
                    let zoom = renderer_state.read(cx).viewport.zoom;
                    renderer_owner
                        .update(cx, |this, cx| {
                            this.render_graph_node(node, zoom, window, cx)
                        })
                        .unwrap_or_else(|_| div().into_any_element())
                })
                // The plane, in the bench cast: dark by default, ruled every 24px.
                .bg_color(native_theme::BENCH_GROUND)
                .grid_color(native_theme::BENCH_GRID)
                .bg_pattern(gpui_flow::BackgroundPattern::Cross)
                .grid_gap(24.0)
                .node_bg_color(native_theme::BENCH_RAISE)
                .node_border_color(native_theme::BENCH_LINE_STRONG)
                // Selection is wax white; amber is the mark you are about to
                // drop onto; a wire is coloured by the data it carries.
                .selection_color(native_theme::WAX_WHITE)
                .target_color(native_theme::WAX_AMBER)
                .target_active_color(native_theme::WAX_AMBER)
                .edge_colors(native_theme::BENCH_LINE, native_theme::WAX_WHITE)
                .marquee_color(native_theme::WAX_WHITE)
                .handle_label_color(native_theme::BENCH_INK_BODY)
        });
        let subscription = cx.observe_in(&flow, window, |this, _, window, cx| {
            this.sync_flow(window, cx)
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes"));
        let search_subscription = cx.subscribe_in(
            &search,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. })
                    && let Some(kind) = this
                        .library_nodes(cx)
                        .first()
                        .and_then(|(_, nodes)| nodes.first())
                        .map(|node| node.type_id.clone())
                {
                    this.add_library_node(&kind, None, window, cx);
                }
                cx.notify();
            },
        );
        let gpu_label = match window.gpu_specs() {
            Some(specs) => {
                eprintln!(
                    "Native compositor: {} · {} · software={}",
                    specs.device_name, specs.driver_name, specs.is_software_emulated
                );
                if specs.is_software_emulated {
                    format!("Software renderer · {}", specs.device_name)
                } else {
                    format!("Native GPU · {}", specs.device_name)
                }
            }
            None => "Native compositor · adapter unreported".into(),
        };
        let mut this = Self {
            session,
            flow_state,
            flow,
            _flow_subscription: subscription,
            fields: vec![],
            field_subscriptions: vec![],
            selected: None,
            export_busy: false,
            export_status: String::new(),
            export_settings: ExportSettings::default(),
            export_quality: cx.new(|cx| InputState::new(window, cx).default_value("92")),
            export_long_edge: cx.new(|cx| InputState::new(window, cx).placeholder("Original")),
            show_export_settings: false,
            show_shortcuts: false,
            show_advanced: false,
            compatible_only: true,
            collapsed_categories: BTreeSet::new(),
            search,
            _search_subscription: search_subscription,
            viewers: cx.new(Viewers::new),
            source_generation: 0,
            source_path: None,
            status,
            gpu_label,
            undo: vec![],
            redo: vec![],
            move_history: MoveHistory::default(),
            layout: BTreeMap::new(),
            focus: cx.focus_handle(),
            workspace,
            preferences_file: preferences_path(),
            workspace_epoch: 0,
            canvas_size: (600.0, 500.0),
            pending_canvas_fit: true,
            pending_canvas_focus: None,
            minimap: cx.new(|_| {
                gpui_flow::Minimap::new(flow_state_for_minimap).palette(
                    gpui_flow::minimap::MinimapPalette {
                        ground: native_theme::BENCH_SUNK,
                        frame: native_theme::BENCH_LINE,
                        frame_selected: native_theme::WAX_WHITE,
                        mask_fill: native_theme::BENCH_LINE_STRONG,
                        mask_line: native_theme::BENCH_LINE_STRONG,
                        border: native_theme::BENCH_LINE,
                    },
                )
            }),
            controls_open: true,
            curve_gesture: None,
            geometry: cx.new(|_| native_nodes::GeometryHelper::new(owner.clone())),
        };
        this.rebuild_flow(cx);
        let enabled = this.workspace.viewer_open;
        this.viewers
            .update(cx, |viewers, cx| viewers.set_enabled(enabled, window, cx));
        this.focus.focus(window, cx);
        this
    }
    fn persist_workspace(&mut self) {
        if let Some(path) = &self.preferences_file {
            // ponytail: tiny atomic write on completed gestures; serialize background saves if profiling shows UI delay.
            if let Err(error) = self.workspace.save(path) {
                self.status = format!("Could not save layout: {error}");
            }
        }
    }
    fn toggle_panel(&mut self, panel: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_canvas_move(window, cx);
        // Never leave keyboard focus in a dock that is about to leave the tree.
        window.focus(&self.focus, cx);
        match panel {
            0 => self.workspace.library_open = !self.workspace.library_open,
            1 => {
                self.workspace.viewer_open = !self.workspace.viewer_open;
                let enabled = self.workspace.viewer_open;
                self.viewers
                    .update(cx, |viewers, cx| viewers.set_enabled(enabled, window, cx));
            }
            _ => {
                window.focus(&self.focus, cx);
                self.workspace.inspector_open = !self.workspace.inspector_open;
            }
        }
        self.persist_workspace();
        cx.notify();
    }
    fn resize_workspace(
        &mut self,
        panel: usize,
        delta: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_canvas_move(window, cx);
        match panel {
            0 => {
                self.workspace.library_width =
                    (self.workspace.library_width + delta).clamp(180.0, 400.0)
            }
            1 => {
                self.workspace.viewer_width =
                    (self.workspace.viewer_width + delta).clamp(280.0, 720.0)
            }
            _ => {
                self.workspace.inspector_height =
                    (self.workspace.inspector_height + delta).clamp(100.0, 600.0)
            }
        }
        self.workspace_epoch = self.workspace_epoch.wrapping_add(1);
        self.persist_workspace();
        cx.notify();
    }
    fn canvas_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        let minimap = self.minimap.clone();
        let measure = canvas(
            move |bounds, window, cx| {
                let pending = weak
                    .update(cx, |this, _| {
                        this.canvas_size =
                            (bounds.size.width.as_f32(), bounds.size.height.as_f32());
                        (
                            std::mem::take(&mut this.pending_canvas_fit),
                            this.pending_canvas_focus.take(),
                        )
                    })
                    .unwrap_or_default();
                // The overview's viewport mask needs the plane's real size.
                minimap.update(cx, |minimap, cx| {
                    let (width, height) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
                    let resized = minimap.set_container_bounds(width, height);
                    // The overview follows the plane's aspect ratio, so it is a
                    // window into this plane rather than a fixed rectangle.
                    let refitted = minimap.set_plane_size(width, height);
                    if resized || refitted {
                        cx.notify();
                    }
                });
                if pending.0 || pending.1.is_some() {
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(id) = pending.1.as_ref() {
                                this.reveal_canvas_node(id, cx);
                            } else {
                                this.canvas_navigation(0, cx);
                            }
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        // Copy what the chrome needs, then drop the flow-state borrow: building the
        // controls below needs `cx` mutably.
        let (nodes, links, zoom, empty, connections) = {
            let state = self.flow_state.read(cx);
            (
                state.nodes.len(),
                state.edges.len(),
                state.viewport.zoom,
                state.nodes.is_empty(),
                state
                    .edges
                    .iter()
                    .filter(|edge| edge.selected && edge.deletable)
                    .count(),
            )
        };
        div()
            .v_flex()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_baseline()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .flex_shrink_0()
                    .text_color(rgb(t::BENCH_INK))
                    .border_b_1()
                    .border_color(rgb(t::BENCH_LINE))
                    // The scope header names the document; the status side counts
                    // what is in it. Two jobs, two ends of one bar. The edge code
                    // is a caption beside the title, never a kicker above it.
                    .child(
                        div()
                            .h_flex()
                            .items_baseline()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .font_family(t::Face::Display.family())
                                    .font_weight(t::Face::Display.weight())
                                    .text_size(t::Face::Display.size())
                                    .child("Workflow"),
                            )
                            .child(
                                div()
                                    .font_family(t::Face::EdgeCode.family())
                                    .font_weight(t::Face::EdgeCode.weight())
                                    .text_size(t::Face::EdgeCode.size())
                                    .text_color(rgb(t::BENCH_INK_DIM))
                                    .child(t::code("workflow scope")),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .font_family(t::Face::EdgeCode.family())
                            .font_weight(t::Face::EdgeCode.weight())
                            .text_size(t::Face::EdgeCode.size())
                            .text_color(rgb(t::BENCH_INK_DIM))
                            .child(format!(
                                "{} {} · {} {}",
                                nodes,
                                if nodes == 1 { "node" } else { "nodes" },
                                links,
                                if links == 1 { "link" } else { "links" }
                            ))
                            .child(div().child(format!("{:.0}%", zoom * 100.0))),
                    ),
            )
            .when(connections > 0, |view| view.child(div().h_flex().gap_2().px_2().py_1().flex_shrink_0()
                .child(format!("{connections} connection{} selected", if connections == 1 { "" } else { "s" }))
                .child(Button::new("disconnect-selection").small().label("Disconnect").tooltip("Remove selected connections; keep their nodes").on_click(cx.listener(|this, _, window, cx| this.disconnect_selection(window, cx))))))
            .child(
                div()
                    .id("workflow-canvas")
                    .test_support()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.finish_canvas_move(window, cx)),
                    )
                    .on_drop(cx.listener(|this, drag: &LibraryDrag, window, cx| {
                        this.drop_library_node(drag, window, cx)
                    }))
                    .child(self.flow.clone())
                    .child(measure)
                    // Canvas controls live in the corner of the plane they act on,
                    // as a window cut into it, not in the panel's command bar.
                    .child(
                        div()
                            .id("workflow-controls")
                            .test_support()
                            .absolute()
                            .left(px(12.0))
                            .bottom(px(12.0))
                            .v_flex()
                            .overflow_hidden()
                            .rounded(t::RADIUS)
                            .border_1()
                            .border_color(rgb(t::BENCH_LINE_STRONG))
                            .child(self.canvas_control("workflow-zoom-in", IconName::Plus, "Zoom in", 1, cx))
                            .child(self.canvas_control("workflow-zoom-out", IconName::Minus, "Zoom out", -1, cx))
                            // `Maximize` is the nearest bundled fit-view mark; the
                            // component icon subset has no four-corner expand.
                            .child(self.canvas_control("fit-workflow", IconName::Maximize, "Fit the whole workflow", 0, cx)),
                    )
                    // Bird's-eye view, as a window cut into the plane's corner.
                    .child(
                        div()
                            .id("workflow-overview")
                            .test_support()
                            .absolute()
                            .right(px(12.0))
                            .bottom(px(12.0))
                            .child(self.minimap.clone()),
                    )
                    .when(empty, |view| {
                        view.child(
                            // A plane-sized, input-transparent centring layer: the
                            // plane stays draggable behind the arrival block.
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .w(px(self.canvas_size.0))
                                .h(px(self.canvas_size.1))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .id("canvas-empty")
                                        .test_support()
                                        .w(px(300.0))
                                        .border_t_1()
                                        .border_b_1()
                                        .border_color(rgb(t::BENCH_LINE))
                                        .py_3()
                                        .child(
                                            div()
                                                .mb_2()
                                                .size(px(26.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(px(3.0))
                                                .border_1()
                                                .border_color(rgb(t::ROOM_LINE_STRONG))
                                                .child(Icon::new(IconName::Plus).size(px(14.0))),
                                        )
                                        .child(
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .text_size(px(15.0))
                                                .child("Start weaving"),
                                        )
                                        .child(
                                            div()
                                                .mt_1()
                                                .text_sm()
                                                .text_color(rgb(t::ROOM_INK_FAINT))
                                                .child("Drag a node from the library into the workflow, or click one to add it."),
                                        ),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(t::ROW_LOOSE)
                    .flex_shrink_0()
                    .text_color(rgb(t::BENCH_INK))
                    .border_t_1()
                    .border_color(rgb(t::BENCH_LINE))
                    .child(div().size(px(6.0)).rounded_full().bg(rgb(t::BENCH_INK)))
                    .child(
                        div()
                            .font_family(t::Face::Body.family())
                            .text_size(t::Face::Body.size())
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .font_family(t::Face::EdgeCode.family())
                            .font_weight(t::Face::EdgeCode.weight())
                            .text_size(t::Face::EdgeCode.size())
                            .text_color(rgb(t::BENCH_INK_DIM))
                            .child(t::code("drag output to input")),
                    ),
            )
            .into_any_element()
    }

    /// One square canvas-control button carrying the plane's own line tone.
    fn canvas_control(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        action: i8,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(id)
            .small()
            .icon(icon)
            .ghost()
            .tooltip(label)
            .accessibility_label(label)
            .on_click(cx.listener(move |this, _, _, cx| this.canvas_navigation(action, cx)))
            .into_any_element()
    }
    fn reveal_canvas_node(&mut self, id: &str, cx: &mut Context<Self>) {
        let (width, height) = self.canvas_size;
        self.flow_state.update(cx, |state, _| {
            let bounds = state
                .get_node(&SharedString::from(id.to_owned()))
                .map(|node| {
                    (
                        node.position.x,
                        node.position.y,
                        node.measured_width.unwrap_or(px(190.0)).as_f32(),
                        node.measured_height.unwrap_or(px(70.0)).as_f32(),
                    )
                });
            if let Some((x, y, w, h)) = bounds {
                state.viewport.zoom = state.viewport.zoom.max(0.8).min(
                    ((width - 16.0) / w)
                        .min((height - 16.0) / h)
                        .max(state.min_zoom),
                );
                state.set_center(x + w / 2.0, y + h / 2.0, width, height);
            }
        });
        self.flow.update(cx, |_, cx| cx.notify());
    }
    fn canvas_navigation(&mut self, action: i8, cx: &mut Context<Self>) {
        let (width, height) = self.canvas_size;
        self.flow_state.update(cx, |state, _| match action {
            -1 => state.zoom_out(width, height),
            1 => state.zoom_in(width, height),
            _ => state.fit_view(32.0, width, height),
        });
        self.flow.update(cx, |_, cx| cx.notify());
    }
    fn rebuild_flow(&mut self, cx: &mut Context<Self>) {
        self.pending_canvas_focus = None;
        self.move_history.reset();
        let nodes =
            self.session
                .editor
                .graph()
                .nodes()
                .values()
                .enumerate()
                .map(|(i, node)| {
                    let (x, y) = self
                        .layout
                        .get(node.id.as_str())
                        .copied()
                        .unwrap_or((40.0 + (i % 2) as f32 * 440.0, 40.0 + (i / 2) as f32 * 240.0));
                    let inputs = node
                        .descriptor
                        .inputs
                        .iter()
                        .map(|port| {
                            (
                                port.id.clone(),
                                format!(
                                    "{} in · {}",
                                    port.name,
                                    port.data_type.rsplit('.').next().unwrap_or(&port.data_type)
                                ),
                            )
                        })
                        .chain(
                            node.exposed_parameters
                                .iter()
                                .filter(|id| {
                                    !node.descriptor.inputs.iter().any(|port| port.id == **id)
                                })
                                .map(|id| {
                                    (
                                        id.clone(),
                                        node.descriptor
                                            .parameter(id)
                                            .map(|parameter| {
                                                format!("{} in · parameter", parameter.name)
                                            })
                                            .unwrap_or_else(|| id.clone()),
                                    )
                                }),
                        );
                    let handles =
                        inputs
                            .enumerate()
                            .map(|(index, (id, label))| {
                                HandleDef::target(HandlePosition::Left)
                                    .id(id)
                                    .label(label)
                                    .offset(64.0 + index as f32 * 26.0)
                            })
                            .chain(node.descriptor.outputs.iter().enumerate().map(
                                |(index, port)| {
                                    HandleDef::source(HandlePosition::Right)
                                        .id(port.id.clone())
                                        .label(format!(
                                            "{} out · {}",
                                            port.name,
                                            port.data_type
                                                .rsplit('.')
                                                .next()
                                                .unwrap_or(&port.data_type)
                                        ))
                                        .offset(64.0 + index as f32 * 26.0)
                                },
                            ))
                            .collect();
                    let mut view = FlowNode::new(node.id.as_str().to_owned(), x, y)
                        .label(self.session.node_label(node.id.as_str()))
                        .node_type(node.type_id.clone())
                        .handles(handles)
                        .size(332.0, 112.0);
                    view.selected = self.selected.as_deref() == Some(node.id.as_str());
                    view
                })
                .collect();
        let edges = self
            .session
            .editor
            .graph()
            .edges()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                FlowEdge::new(
                    format!("edge-{i}"),
                    e.from_node.as_str().to_owned(),
                    e.to_node.as_str().to_owned(),
                )
                .source_handle(e.from_port.clone())
                .target_handle(e.to_port.clone())
                // A wire's colour says which kind of data it carries.
                .color(t::data_type_color(
                    self.session
                        .editor
                        .graph()
                        .node(&CoreNodeId::from(e.from_node.as_str()))
                        .and_then(|node| node.descriptor.output(&e.from_port))
                        .map(|port| port.data_type.as_str()),
                ))
            })
            .collect();
        let graph = self.session.editor.graph().clone();
        self.flow_state.update(cx, |state, _| {
            state.connection_validator = Some(std::rc::Rc::new(move |connection| {
                let Some(output) = graph
                    .node(&CoreNodeId::from(connection.source.as_ref()))
                    .and_then(|node| {
                        connection
                            .source_handle
                            .as_deref()
                            .and_then(|port| node.descriptor.output(port))
                    })
                else {
                    return false;
                };
                let Some(target) = graph.node(&CoreNodeId::from(connection.target.as_ref())) else {
                    return false;
                };
                let Some(port) = connection.target_handle.as_deref() else {
                    return false;
                };
                let expected = target
                    .descriptor
                    .input(port)
                    .map(|input| input.data_type.as_str())
                    .or_else(|| {
                        target
                            .exposed_parameters
                            .contains(port)
                            .then(|| target.descriptor.parameter(port))
                            .flatten()
                            .map(|parameter| match parameter.parameter_type {
                                ParameterType::Float => "value.Float",
                                ParameterType::Integer => "value.Integer",
                                ParameterType::Boolean => "value.Boolean",
                                ParameterType::String => "value.String",
                            })
                    });
                expected.is_some_and(|expected| {
                    rawweave_project::types_compatible(expected, &output.data_type)
                })
            }));
            state.set_nodes(nodes);
            state.set_edges(edges);
        });
        self.flow.update(cx, |_, cx| cx.notify());
    }
    fn document(&self) -> Result<String, String> {
        let graph = self
            .session
            .editor
            .save_workflow()
            .map_err(|e| e.to_string())?;
        let positions: BTreeMap<_, _> = self
            .layout
            .iter()
            .map(|(id, (x, y))| (id, serde_json::json!({"x": x, "y": y})))
            .collect();
        serde_json::to_string_pretty(
            &serde_json::json!({"version": 1, "graph": graph, "positions": positions}),
        )
        .map_err(|e| e.to_string())
    }
    fn remember(&mut self, old: String) {
        self.undo.push(old);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn read_document(&mut self, text: &str, clear_source: bool) -> Result<(), String> {
        let doc: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let graph = match doc.get("graph") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(value) => value.to_string(),
            None => text.to_owned(),
        };
        if clear_source {
            self.session.load_workflow(&graph)?;
        } else {
            self.session
                .editor
                .load_workflow(&graph)
                .map_err(|e| e.to_string())?;
        }
        self.layout.clear();
        if let Some(positions) = doc.get("positions").and_then(|v| v.as_object()) {
            for (id, pos) in positions {
                if let (Some(x), Some(y)) = (pos["x"].as_f64(), pos["y"].as_f64())
                    && x.is_finite()
                    && y.is_finite()
                    && x.abs() <= 1e6
                    && y.abs() <= 1e6
                {
                    self.layout.insert(id.clone(), (x as f32, y as f32));
                }
            }
        }
        Ok(())
    }
    fn finish_canvas_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flow_state.read(cx).drag_state.is_none() {
            return;
        }
        self.flow_state.update(cx, |state, _| {
            state.drag_state = None;
            for node in &mut state.nodes {
                node.dragging = false;
            }
        });
        self.sync_flow(window, cx);
        self.flow.update(cx, |_, cx| cx.notify());
    }
    fn sync_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let nodes = self.flow_state.read(cx).nodes.clone();
        let edges = self.flow_state.read(cx).edges.clone();
        let old = self.document();
        let mut candidate = self.session.editor.clone();
        let result = (|| -> Result<(), String> {
            let removed: Vec<_> = candidate
                .graph()
                .nodes()
                .keys()
                .filter(|id| !nodes.iter().any(|n| n.id.as_ref() == id.as_str()))
                .cloned()
                .collect();
            for id in removed {
                candidate
                    .remove_node(id.as_str())
                    .map_err(|e| e.to_string())?;
            }
            for node in &nodes {
                if candidate
                    .graph()
                    .node(&CoreNodeId::from(node.id.as_ref()))
                    .is_none()
                {
                    candidate
                        .add_node(
                            node.id.as_ref(),
                            node.node_type
                                .as_ref()
                                .ok_or("node type is missing")?
                                .as_ref(),
                        )
                        .map_err(|e| e.to_string())?;
                }
            }
            let disconnected: Vec<_> = candidate
                .graph()
                .edges()
                .iter()
                .filter(|e| {
                    !edges.iter().any(|f| {
                        f.source.as_ref() == e.from_node.as_str()
                            && f.target.as_ref() == e.to_node.as_str()
                            && f.source_handle.as_deref() == Some(e.from_port.as_str())
                            && f.target_handle.as_deref() == Some(e.to_port.as_str())
                    })
                })
                .cloned()
                .collect();
            for e in disconnected {
                candidate
                    .disconnect(
                        e.from_node.as_str(),
                        &e.from_port,
                        e.to_node.as_str(),
                        &e.to_port,
                    )
                    .map_err(|e| e.to_string())?;
            }
            for e in &edges {
                let from = e.source_handle.as_deref().ok_or("source port is missing")?;
                let to = e.target_handle.as_deref().ok_or("target port is missing")?;
                if !candidate.graph().edges().iter().any(|f| {
                    f.from_node.as_str() == e.source.as_ref()
                        && f.to_node.as_str() == e.target.as_ref()
                        && f.from_port == from
                        && f.to_port == to
                }) {
                    candidate
                        .connect(e.source.as_ref(), from, e.target.as_ref(), to)
                        .map_err(|error| {
                            format!(
                                "Cannot connect {} · {from} to {} · {to}: {error}",
                                self.session.node_label(e.source.as_ref()),
                                self.session.node_label(e.target.as_ref())
                            )
                        })?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.status = error;
            self.rebuild_flow(cx);
            cx.notify();
            return;
        }
        let changed = candidate.graph().revision() != self.session.editor.graph().revision();
        if changed {
            self.move_history.reset();
            if let Ok(old) = &old {
                self.remember(old.clone());
            }
            self.session.editor = candidate;
        }
        self.layout = nodes
            .iter()
            .map(|n| (n.id.to_string(), (n.position.x, n.position.y)))
            .collect();
        if !changed && let Ok(before) = old {
            let dragging = self.flow_state.read(cx).drag_state.is_some();
            let after = if dragging {
                Ok(String::new())
            } else {
                self.document()
            };
            if let Ok(after) = after
                && let Some(before) = self.move_history.observe(dragging, before, &after)
            {
                self.remember(before);
            }
        }
        let selected = nodes.iter().find(|n| n.selected).map(|n| n.id.to_string());
        if selected != self.selected {
            self.selected = selected;
            self.rebuild_fields(window, cx);
            self.refresh_geometry(window, cx);
            self.flow.update(cx, |_, cx| cx.notify());
        }
        if changed {
            self.session.reconcile_target();
            self.request_preview(window, cx);
        }
        cx.notify();
    }
    fn rebuild_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fields.clear();
        self.field_subscriptions.clear();
        self.controls_open = true;
        self.curve_gesture = None;
        let Some(node) = self
            .selected
            .as_ref()
            .and_then(|id| {
                self.session
                    .editor
                    .graph()
                    .node(&CoreNodeId::from(id.as_str()))
            })
            .cloned()
        else {
            return;
        };
        for descriptor in node.descriptor.parameters {
            let value = node
                .parameters
                .get(&descriptor.id)
                .unwrap_or(&descriptor.default)
                .clone();
            let ux = parameter_ux(&node.type_id, &descriptor);
            let exposed = node.exposed_parameters.contains(&descriptor.id);
            let draft = ParameterDraft::new(descriptor, ux, value);
            let state: AnyInputState = if draft.ux.multiline {
                cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .default_value(draft.text().to_owned())
                        .auto_grow(2, 6)
                        .submit_on_enter(true)
                })
                .into()
            } else {
                cx.new(|cx| InputState::new(window, cx).default_value(draft.text().to_owned()))
                    .into()
            };
            let slider = if draft.numeric() {
                draft.ux.range().and_then(|(min, max)| {
                    let (min, max, step) = (
                        min * draft.ux.factor,
                        max * draft.ux.factor,
                        draft.ux.step * draft.ux.factor,
                    );
                    if min.abs() > f64::from(f32::MAX)
                        || max.abs() > f64::from(f32::MAX)
                        || step > f64::from(f32::MAX)
                    {
                        return None;
                    }
                    Some(cx.new(|_| {
                        SliderState::new()
                            .min(min as f32)
                            .max(max as f32)
                            .step(step as f32)
                            .default_value(
                                draft.shown_number().unwrap_or(min).clamp(min, max) as f32
                            )
                    }))
                })
            } else {
                None
            };
            let index = self.fields.len();
            let node_id = node.id.as_str().to_owned();
            let subscription = match &state {
                AnyInputState::Input(state) => cx.subscribe_in(
                    state,
                    window,
                    move |this, input, event: &InputEvent, window, cx| {
                        this.field_input_event(
                            &node_id,
                            index,
                            event,
                            input.read(cx).value().to_string(),
                            window,
                            cx,
                        )
                    },
                ),
                AnyInputState::Textarea(state) => cx.subscribe_in(
                    state,
                    window,
                    move |this, input, event: &InputEvent, window, cx| {
                        this.field_input_event(
                            &node_id,
                            index,
                            event,
                            input.read(cx).value().to_string(),
                            window,
                            cx,
                        )
                    },
                ),
                _ => continue,
            };
            self.field_subscriptions.push(subscription);
            if let Some(slider) = &slider {
                let node_id = node.id.as_str().to_owned();
                self.field_subscriptions.push(cx.subscribe_in(
                    slider,
                    window,
                    move |this, _, event: &SliderEvent, window, cx| {
                        if this.selected.as_deref() != Some(node_id.as_str()) {
                            return;
                        }
                        let Some(field) = this.fields.get_mut(index) else {
                            return;
                        };
                        let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                        field.error = field.draft.set_slider(value.start()).err();
                        let text = field.draft.text().to_owned();
                        field.set_widget_value(text, window, cx);
                        if matches!(event, SliderEvent::Release(_)) {
                            this.commit_field(index, window, cx);
                        }
                        cx.notify();
                    },
                ));
            }
            self.fields.push(Field {
                draft,
                state,
                slider,
                exposed,
                error: None,
                stepping: false,
                curve_index: 0,
                curve_bounds: None,
            });
        }
    }
    fn field_input_event(
        &mut self,
        node_id: &str,
        index: usize,
        event: &InputEvent,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected.as_deref() != Some(node_id) {
            return;
        }
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        if matches!(event, InputEvent::Change) {
            field.draft.set_text(text);
            field.error = field.draft.parsed().err();
            if field.draft.ux.point_curve
                && let Ok(points) = curve_points(field.draft.text(), field.draft.ux.scalar_curve)
            {
                field.curve_index = field.curve_index.min(points.len().saturating_sub(1));
            }
        }
        if matches!(
            event,
            InputEvent::PressEnter { shift: false, .. } | InputEvent::Blur
        ) {
            self.commit_field(index, window, cx);
        }
        cx.notify();
    }
    fn commit_field(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(node_id) = self.selected.clone() else {
            return;
        };
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        field.stepping = false;
        let old_value = field.draft.committed().clone();
        let value = match field.draft.take_commit() {
            Ok(Some(value)) => value,
            Ok(None) => return,
            Err(error) => {
                field.error = Some(error);
                cx.notify();
                return;
            }
        };
        let id = field.draft.descriptor.id.clone();
        let old = self.document();
        match self.session.editor.set_node_parameter(&node_id, &id, value) {
            Ok(()) => {
                if let Ok(old) = old {
                    self.remember(old);
                }
                self.fields[index].error = None;
                self.sync_field_slider(index, window, cx);
                self.request_preview(window, cx);
            }
            Err(error) => {
                self.fields[index].draft.set_committed(old_value);
                self.fields[index].error = Some(error.to_string());
            }
        }
        cx.notify();
    }
    fn select_curve_point(&mut self, index: usize, next: bool, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get_mut(index)
            && let Ok(points) = curve_points(field.draft.text(), field.draft.ux.scalar_curve)
        {
            field.curve_index = if next {
                field
                    .curve_index
                    .saturating_add(1)
                    .min(points.len().saturating_sub(1))
            } else {
                field.curve_index.saturating_sub(1)
            };
        }
        cx.notify();
    }
    fn edit_curve_points(
        &mut self,
        index: usize,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        let result = if remove {
            remove_curve_point(
                field.draft.text(),
                field.draft.ux.scalar_curve,
                field.curve_index,
            )
            .map(|text| (text, field.curve_index.saturating_sub(1)))
        } else {
            add_curve_point(field.draft.text(), field.draft.ux.scalar_curve)
        };
        match result {
            Ok((text, selected)) => {
                field.curve_index = selected;
                field.draft.set_text(text.clone());
                field.set_widget_value(text, window, cx);
                self.commit_field(index, window, cx);
                self.focus.focus(window, cx);
            }
            Err(error) => field.error = Some(error),
        }
        cx.notify();
    }
    fn choose_field(
        &mut self,
        index: usize,
        value: ParameterValue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        let text = match value {
            ParameterValue::Boolean(v) => v.to_string(),
            ParameterValue::String(v) => v,
            _ => return,
        };
        field.draft.set_text(text);
        self.commit_field(index, window, cx);
    }
    fn sync_field_slider(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get(index)
            && let Some(slider) = &field.slider
            && let Some(value) = field.draft.shown_number()
        {
            slider.update(cx, |state, cx| {
                state.set_value(
                    value.clamp(f64::from(state.min_value()), f64::from(state.max_value())) as f32,
                    window,
                    cx,
                )
            });
        }
    }
    fn cancel_field(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get_mut(index) {
            field.draft.cancel();
            field.error = None;
            field.stepping = false;
            if field.draft.ux.point_curve
                && let Ok(points) = curve_points(field.draft.text(), field.draft.ux.scalar_curve)
            {
                field.curve_index = field.curve_index.min(points.len().saturating_sub(1));
            }
            let text = field.draft.text().to_owned();
            field.set_widget_value(text, window, cx);
        }
        self.sync_field_slider(index, window, cx);
        cx.notify();
    }
    fn reset_field(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(node_id) = self.selected.clone() else {
            return;
        };
        let Some(field) = self.fields.get(index) else {
            return;
        };
        let (id, value) = (
            field.draft.descriptor.id.clone(),
            field.draft.descriptor.default.clone(),
        );
        if field.draft.committed() == &value {
            self.cancel_field(index, window, cx);
            return;
        }
        let old = self.document();
        match self
            .session
            .editor
            .set_node_parameter(&node_id, &id, value.clone())
        {
            Ok(()) => {
                if let Ok(old) = old {
                    self.remember(old);
                }
                self.fields[index].draft.set_committed(value);
                self.cancel_field(index, window, cx);
                self.request_preview(window, cx);
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
    }
    fn toggle_parameter_port(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(node_id) = self.selected.clone() else {
            return;
        };
        let Some(field) = self.fields.get(index) else {
            return;
        };
        let (id, exposed) = (field.draft.descriptor.id.clone(), field.exposed);
        let old = self.document();
        let result = if exposed {
            self.session.editor.unexpose_parameter(&node_id, &id)
        } else {
            self.session.editor.expose_parameter(&node_id, &id)
        };
        match result {
            Ok(()) => {
                if let Ok(old) = old {
                    self.remember(old);
                }
                self.fields[index].exposed = !exposed;
                self.rebuild_flow(cx);
                self.request_preview(window, cx);
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
    }
    fn request_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_geometry(window, cx);
        self.flow.update(cx, |_, cx| cx.notify());
        let snapshot = self.session.clone();
        self.viewers.update(cx, |viewers, cx| {
            viewers.set_session(snapshot, false, window, cx)
        });
    }
    fn reset_viewers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_geometry(window, cx);
        let snapshot = self.session.clone();
        self.viewers.update(cx, |viewers, cx| {
            viewers.set_session(snapshot, true, window, cx)
        });
    }
    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.viewers.update(cx, |viewers, _| viewers.cancel_all());
        self.source_generation += 1;
        let generation = self.source_generation;
        self.status = format!("Opening {}…", path.display());
        let source_path = path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { Source::open(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if generation != this.source_generation {
                    return;
                }
                match result.and_then(|source| this.session.attach(source)) {
                    Ok(()) => {
                        this.source_path = Some(source_path);
                        this.undo.clear();
                        this.redo.clear();
                        this.selected = None;
                        this.fields.clear();
                        this.field_subscriptions.clear();
                        this.pending_canvas_fit = true;
                        this.rebuild_flow(cx);
                        this.reset_viewers(window, cx);
                        this.status = "Source image loaded".into();
                        cx.notify();
                    }
                    Err(error) => {
                        this.status = error;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn choose_file(&mut self, kind: FileKind, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                match kind {
                    FileKind::Image => "Open image",
                    FileKind::Workflow => "Load workflow",
                    FileKind::Blueprint => "Instantiate blueprint (replace workflow)",
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await
                && let Some(path) = paths.into_iter().next()
            {
                if kind == FileKind::Image {
                    let _ = this.update_in(cx, |this, window, cx| this.open_path(path, window, cx));
                    return;
                }
                let task = cx.background_executor().spawn(async move {
                    bounded_read(&path, 16 * 1024 * 1024)
                        .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
                });
                let result = task.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    match result.and_then(|text| {
                        if kind == FileKind::Blueprint {
                            this.session.import_blueprint(&text)?;
                            this.layout.clear();
                            Ok(())
                        } else {
                            this.read_document(&text, true)
                        }
                    }) {
                        Ok(()) => {
                            this.viewers.update(cx, |viewers, _| viewers.cancel_all());
                            this.source_generation += 1;
                            this.source_path = None;
                            this.undo.clear();
                            this.redo.clear();
                            this.selected = None;
                            this.fields.clear();
                            this.field_subscriptions.clear();
                            this.pending_canvas_fit = true;
                            this.reset_viewers(window, cx);
                            this.rebuild_flow(cx);
                            this.status = if kind == FileKind::Blueprint {
                                "Blueprint instantiated · select a compatible source"
                            } else {
                                "Workflow loaded · select a compatible source"
                            }
                            .into();
                        }
                        Err(e) => this.status = e,
                    };
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let document = match self.document() {
            Ok(d) => d,
            Err(e) => {
                self.status = e;
                cx.notify();
                return;
            }
        };
        let prompt = cx.prompt_for_new_path(std::path::Path::new("."), Some("workflow.json"));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = prompt.await {
                let task = cx
                    .background_executor()
                    .spawn(async move { save_workflow_atomic(&path, &document) });
                let result = task.await;
                let _ = this.update_in(cx, |this, _, cx| {
                    this.status = match result {
                        Ok(()) => "Workflow saved".into(),
                        Err(e) => e,
                    };
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn export_selection_blueprint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_canvas_move(window, cx);
        let selection: Vec<String> = self
            .flow_state
            .read(cx)
            .nodes
            .iter()
            .filter(|node| node.selected)
            .map(|node| node.id.to_string())
            .collect();
        if selection.is_empty() {
            return;
        }
        let snapshot = self.session.clone();
        let prompt =
            cx.prompt_for_new_path(std::path::Path::new("."), Some("selection-blueprint.json"));
        cx.spawn_in(window, async move |this, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(path))) => {
                    let task = cx.background_executor().spawn(async move {
                        let name = path
                            .file_stem()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Selection");
                        let ids: Vec<&str> = selection.iter().map(String::as_str).collect();
                        let text = snapshot.export_blueprint(&ids, name, name)?;
                        save_workflow_atomic(&path, &text)
                    });
                    task.await.map(|()| "Selection blueprint saved".to_owned())
                }
                Ok(Ok(None)) => Ok("Blueprint export cancelled".into()),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |this, _, cx| {
                this.status =
                    result.unwrap_or_else(|error| format!("Blueprint export failed: {error}"));
                cx.notify();
            });
        })
        .detach();
    }
    fn workflow_menus(&self, cx: &mut Context<Self>) -> (AnyElement, AnyElement) {
        let load = cx.entity().downgrade();
        let save = load.clone();
        let selection = load.clone();
        let instantiate = load.clone();
        let selected = self
            .flow_state
            .read(cx)
            .nodes
            .iter()
            .any(|node| node.selected);
        let file = Button::new("workflow-file-menu")
            .label("Workflow files")
            .dropdown_menu(move |menu, _, _| {
                let load = load.clone();
                let save = save.clone();
                let selection = selection.clone();
                let instantiate = instantiate.clone();
                menu.item(PopupMenuItem::new("Open Workflow · Ctrl/Cmd+O").on_click(
                    move |_, window, cx| {
                        let _ = load.update(cx, |this, cx| {
                            this.choose_file(FileKind::Workflow, window, cx)
                        });
                    },
                ))
                .item(PopupMenuItem::new("Save Workflow · Ctrl/Cmd+S").on_click(
                    move |_, window, cx| {
                        let _ = save.update(cx, |this, cx| this.save(window, cx));
                    },
                ))
                .separator()
                .item(
                    PopupMenuItem::new("Export selection blueprint")
                        .disabled(!selected)
                        .on_click(move |_, window, cx| {
                            let _ = selection
                                .update(cx, |this, cx| this.export_selection_blueprint(window, cx));
                        }),
                )
                .item(
                    PopupMenuItem::new("Instantiate blueprint (replace workflow)").on_click(
                        move |_, window, cx| {
                            let _ = instantiate.update(cx, |this, cx| {
                                this.choose_file(FileKind::Blueprint, window, cx)
                            });
                        },
                    ),
                )
            })
            .into_any_element();
        let weak = cx.entity().downgrade();
        let prefs = self.workspace.clone();
        let export_settings = self.show_export_settings;
        let shortcuts = self.show_shortcuts;
        let view = Button::new("workflow-view-menu")
            .label("View")
            .dropdown_menu(move |mut menu, _, _| {
                for (panel, name, open) in [
                    (0, "Nodes", prefs.library_open),
                    (1, "Viewer and scopes", prefs.viewer_open),
                    (2, "Node help", prefs.inspector_open),
                ] {
                    let weak = weak.clone();
                    menu = menu.item(PopupMenuItem::new(name).checked(open).on_click(
                        move |_, window, cx| {
                            let _ =
                                weak.update(cx, |this, cx| this.toggle_panel(panel, window, cx));
                        },
                    ));
                }
                menu = menu.separator();
                for (panel, delta, label) in [
                    (0, -24.0, "Nodes: narrower"),
                    (0, 24.0, "Nodes: wider"),
                    (1, -24.0, "Viewer: narrower"),
                    (1, 24.0, "Viewer: wider"),
                ] {
                    let weak = weak.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.resize_workspace(panel, delta, window, cx)
                        });
                    }));
                }
                let settings = weak.clone();
                let help = weak.clone();
                let reset = weak.clone();
                menu.separator()
                    .item(
                        PopupMenuItem::new("Export settings")
                            .checked(export_settings)
                            .on_click(move |_, _, cx| {
                                let _ = settings.update(cx, |this, cx| {
                                    this.show_export_settings = !this.show_export_settings;
                                    cx.notify();
                                });
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Keyboard shortcuts · ?")
                            .checked(shortcuts)
                            .on_click(move |_, _, cx| {
                                let _ = help.update(cx, |this, cx| {
                                    this.show_shortcuts = !this.show_shortcuts;
                                    cx.notify();
                                });
                            }),
                    )
                    .separator()
                    .item(PopupMenuItem::new("Reset workspace layout").on_click(
                        move |_, window, cx| {
                            let _ = reset.update(cx, |this, cx| {
                                this.finish_canvas_move(window, cx);
                                this.workspace = Workspace::default();
                                this.workspace_epoch = this.workspace_epoch.wrapping_add(1);
                                this.viewers.update(cx, |viewers, cx| {
                                    viewers.set_enabled(true, window, cx)
                                });
                                this.persist_workspace();
                                cx.notify();
                            });
                        },
                    ))
            })
            .into_any_element();
        (file, view)
    }
    fn export_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let compression = self.export_settings.compression;
        let weak = cx.entity().downgrade();
        let compression_menu = Button::new("export-compression")
            .label(match compression {
                Compression::Fast => "PNG: Fast",
                Compression::Best => "PNG: Best",
                _ => "PNG: Default",
            })
            .dropdown_menu(move |mut menu, _, _| {
                for (value, label) in [
                    (Compression::Default, "Default"),
                    (Compression::Fast, "Fast"),
                    (Compression::Best, "Best"),
                ] {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(value == compression)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.export_settings.compression = value;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        let sharpening = self.export_settings.sharpening;
        let weak = cx.entity().downgrade();
        let sharpening_menu = Button::new("export-sharpening")
            .label(if sharpening == OutputSharpening::None {
                "Sharpening: Off"
            } else {
                "Sharpening: Low"
            })
            .dropdown_menu(move |mut menu, _, _| {
                for (value, label) in [
                    (OutputSharpening::None, "Off"),
                    (
                        OutputSharpening::UnsharpMask {
                            radius: 1,
                            amount: 0.3,
                            threshold: 0.01,
                        },
                        "Low — radius 1, amount 0.3, threshold 0.01",
                    ),
                ] {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(value == sharpening)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.export_settings.sharpening = value;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        div().v_flex().gap_2().p_2().border_b_1().border_color(rgb(t::ROOM_LINE))
            .child(div().h_flex().flex_wrap().gap_2()
                .child(div().v_flex().gap_1().child("JPEG quality (1–100)").child(Input::new(&self.export_quality).w(px(120.0))))
                .child(div().v_flex().gap_1().child("Long edge (1–8192 px)").child(Input::new(&self.export_long_edge).w(px(180.0))))
                .child(Checkbox::new("export-png-depth").label("16-bit PNG").checked(self.export_settings.png_sixteen)
                    .on_click(cx.listener(|this, checked, _, cx| { this.export_settings.png_sixteen = *checked; cx.notify(); })))
                .child(compression_menu).child(sharpening_menu))
            .child(div().text_sm().child("Filename selects PNG / JPEG / TIFF / EXR. Blank long edge keeps original dimensions. TIFF: 16-bit sRGB; EXR: float32 linear sRGB; PNG/JPEG: sRGB. Metadata is stripped. Settings are captured when Export Image is clicked."))
            .into_any_element()
    }
    fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_busy || self.session.source.is_none() {
            return;
        }
        let fields = match ExportSettings::from_fields(
            self.export_quality.read(cx).value().as_ref(),
            self.export_long_edge.read(cx).value().as_ref(),
        ) {
            Ok(fields) => fields,
            Err(error) => {
                self.export_status = error;
                self.show_export_settings = true;
                cx.notify();
                return;
            }
        };
        let settings = ExportSettings {
            quality: fields.quality,
            long_edge: fields.long_edge,
            ..self.export_settings.clone()
        };
        let snapshot = self.viewers.read(cx).export_session();
        let prompt = cx.prompt_for_new_path(std::path::Path::new("."), Some("image.png"));
        self.export_busy = true;
        self.export_status = "Export: choose .png / .jpg / .tif / .exr".into();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(path))) => {
                    let label = path.display().to_string();
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.export_status =
                            "Evaluating full-resolution output and applying export recipe…".into();
                        cx.notify();
                    });
                    cx.background_executor()
                        .spawn(async move { snapshot.export_with_settings(&path, &settings) })
                        .await
                        .map(|()| format!("Exported {label}"))
                }
                Ok(Ok(None)) => Ok("Export cancelled".into()),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |this, _, cx| {
                this.export_busy = false;
                this.export_status =
                    result.unwrap_or_else(|error| format!("Export failed: {error}"));
                cx.notify();
            });
        })
        .detach();
    }
    fn disconnect_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_canvas_move(window, cx);
        self.flow_state.update(cx, |state, _| {
            state
                .edges
                .retain(|edge| !(edge.selected && edge.deletable))
        });
        self.sync_flow(window, cx);
        self.flow.update(cx, |_, cx| cx.notify());
    }
    fn delete_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flow_state.update(cx, |state, _| {
            let removed: Vec<_> = state
                .nodes
                .iter()
                .filter(|n| n.selected && n.deletable)
                .map(|n| n.id.clone())
                .collect();
            let nodes = state
                .nodes
                .iter()
                .filter(|n| !removed.contains(&n.id))
                .cloned()
                .collect();
            let edges = state
                .edges
                .iter()
                .filter(|e| {
                    !removed.contains(&e.source)
                        && !removed.contains(&e.target)
                        && !(e.selected && e.deletable)
                })
                .cloned()
                .collect();
            state.set_nodes(nodes);
            state.set_edges(edges);
        });
        self.sync_flow(window, cx);
        self.flow.update(cx, |_, cx| cx.notify());
    }
    fn selected_output_types(&self) -> Vec<String> {
        self.selected
            .as_ref()
            .and_then(|id| {
                self.session
                    .editor
                    .graph()
                    .node(&CoreNodeId::from(id.as_str()))
            })
            .map(|node| {
                node.descriptor
                    .outputs
                    .iter()
                    .map(|port| port.data_type.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
    fn library_nodes(&self, cx: &App) -> Vec<(String, Vec<rawweave_node_api::NodeDescriptor>)> {
        let compatible = if self.compatible_only {
            self.selected_output_types()
        } else {
            vec![]
        };
        library_groups(
            self.session.editor.node_descriptors(),
            self.search.read(cx).value().as_ref(),
            &compatible,
        )
    }
    fn add_library_node(
        &mut self,
        kind: &str,
        position: Option<(f32, f32)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_canvas_move(window, cx);
        self.pending_canvas_fit = false;
        let manually_placed = position.is_some();
        let old = self.document();
        let mut i = self.session.editor.graph().nodes().len();
        while self
            .session
            .editor
            .graph()
            .node(&CoreNodeId::from(format!("node-{i}")))
            .is_some()
        {
            i += 1;
        }
        let id = format!("node-{i}");
        match self.session.editor.add_node(&id, kind) {
            Ok(()) => {
                if let Ok(old) = old {
                    self.remember(old);
                }
                let position = position.unwrap_or_else(|| {
                    (
                        40.0,
                        (self
                            .layout
                            .values()
                            .map(|(_, y)| *y)
                            .max_by(f32::total_cmp)
                            .unwrap_or(40.0)
                            + 160.0)
                            .min(1e6),
                    )
                });
                self.layout.insert(id.clone(), position);
                self.selected = Some(id.clone());
                self.rebuild_fields(window, cx);
                self.rebuild_flow(cx);
                if !manually_placed {
                    self.pending_canvas_focus = Some(id);
                }
                self.request_preview(window, cx);
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
    }
    fn drop_library_node(
        &mut self,
        drag: &LibraryDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pointer = window.mouse_position();
        let state = self.flow_state.read(cx);
        let position = drop_position(
            (pointer.x.as_f32(), pointer.y.as_f32()),
            (state.canvas_origin.x, state.canvas_origin.y),
            (state.viewport.x, state.viewport.y),
            state.viewport.zoom,
        );
        match position {
            Ok(position) => {
                self.focus.focus(window, cx);
                self.add_library_node(&drag.kind, Some(position), window, cx);
            }
            Err(error) => {
                self.status = error;
                cx.notify();
            }
        }
    }
    fn history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_canvas_move(window, cx);
        let target = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        let Some(target) = target else {
            return;
        };
        let current = self.document();
        match self.read_document(&target, false) {
            Ok(()) => {
                self.session.reconcile_target();
                if let Ok(current) = current {
                    if redo {
                        self.undo.push(current);
                    } else {
                        self.redo.push(current);
                    }
                }
                self.selected = self.selected.take().filter(|id| {
                    self.session
                        .editor
                        .graph()
                        .node(&CoreNodeId::from(id.as_str()))
                        .is_some()
                });
                self.rebuild_fields(window, cx);
                self.rebuild_flow(cx);
                self.request_preview(window, cx);
            }
            Err(e) => self.status = e,
        };
        cx.notify();
    }
}
impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let export_controls = self.export_controls(cx);
        let (file_menu, view_menu) = self.workflow_menus(cx);
        let has_parameters = false; // Editing is in the node; the optional dock is a short help rail.
        let inspector_height = 48.0;
        let inspector = div()
            .id("node-help")
            .size_full()
            .h_flex()
            .items_center()
            .gap_2()
            .px_3()
            .bg(rgb(t::ROOM_PANEL))
            .border_t_1()
            .border_color(rgb(t::ROOM_LINE))
            .child(
                div()
                    .font_family(t::Face::EdgeCode.family())
                    .font_weight(t::Face::EdgeCode.weight())
                    .text_size(t::Face::EdgeCode.size())
                    .text_color(rgb(t::ROOM_INK_DIM))
                    .child(t::code("parameters in frame")),
            )
            .child(
                Button::new("hide-inspector")
                    .small()
                    .label("Hide help")
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_panel(2, window, cx))),
            )
            .into_any_element();
        let canvas_panel = self.canvas_panel(cx);
        let searching = !self.search.read(cx).value().trim().is_empty();
        let groups = self.library_nodes(cx);
        let library_count = groups.iter().map(|(_, nodes)| nodes.len()).sum::<usize>();
        let library = groups
            .into_iter()
            .map(|(category, nodes)| {
                let open = searching || !self.collapsed_categories.contains(&category);
                let toggle = category.clone();
                div()
                    .v_flex()
                    .child(
                        // A ruled index: a full-width hairline separates rows, and
                        // no container is drawn around the list.
                        div().h(px(1.0)).w_full().bg(rgb(t::ROOM_LINE)),
                    )
                    .child(
                        Button::new(SharedString::from(format!("category-{category}")))
                            .small()
                            .justify_start()
                            .label(t::code(&category))
                            .icon(if open {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .ghost()
                            .toggled(open)
                            .tooltip(format!(
                                "{} {category}",
                                if open { "Collapse" } else { "Expand" }
                            ))
                            // Push the count to the trailing edge so category rows
                            // stay scannable next to uneven node names.
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(t::ROOM_INK_DIM))
                                    .child(nodes.len().to_string()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.search.read(cx).value().trim().is_empty() {
                                    return;
                                }
                                if !this.collapsed_categories.remove(&toggle) {
                                    this.collapsed_categories.insert(toggle.clone());
                                }
                                cx.notify();
                            })),
                    )
                    .when(open, |view| {
                        view.children(nodes.into_iter().map(|descriptor| {
                            let kind = descriptor.type_id.clone();
                            let tooltip = format!("{} · {}", descriptor.name, descriptor.type_id);
                            let drag = LibraryDrag {
                                kind: kind.clone(),
                                label: descriptor.name.clone(),
                            };
                            div()
                                .id(SharedString::from(format!("drag-{kind}")))
                                .border_b_1()
                                .border_color(rgb(t::ROOM_LINE))
                                .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                                .child(
                                    Button::new(SharedString::from(kind.clone()))
                                        .accessibility_label(tooltip.clone())
                                        .justify_start()
                                        .icon(IconName::Plus)
                                        .ghost()
                                        .h_auto()
                                        .rounded(t::RADIUS)
                                        .py_1()
                                        // Indent rows under their category header.
                                        .pl_3()
                                        .child(
                                            div()
                                                .id(SharedString::from(format!("node-name-{kind}")))
                                                .test_support()
                                                // Growing the label pins the icon to the left
                                                // edge; a centred group would drift right
                                                // on short names.
                                                .flex_1()
                                                .min_w_0()
                                                .v_flex()
                                                .items_start()
                                                .child(
                                                    div()
                                                        .font_family(t::Face::Title.family())
                                                        .font_weight(t::Face::Title.weight())
                                                        .text_size(t::Face::Title.size())
                                                        .child(descriptor.name),
                                                )
                                                .child(
                                                    div()
                                                        .font_family(t::Face::EdgeCode.family())
                                                        .font_weight(t::Face::EdgeCode.weight())
                                                        .text_size(t::Face::EdgeCode.size())
                                                        .text_color(rgb(t::ROOM_INK_FAINT))
                                                        .child(t::code(&descriptor.type_id)),
                                                ),
                                        )
                                        .tooltip(tooltip)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.add_library_node(&kind, None, window, cx)
                                        })),
                                )
                        }))
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        div()
            .id("rawweave")
            .track_focus(&self.focus)
            .size_full()
            .v_flex()
            .bg(rgb(t::ROOM_GROUND))
            .text_color(rgb(t::ROOM_INK))
            .font_family(t::Face::Body.family())
            .text_size(t::Face::Body.size())
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let editing = this.fields.iter().any(|f| f.state.focus_handle(cx).is_focused(window))
                    || this.search.read(cx).focus_handle(cx).is_focused(window)
                    || this.export_quality.read(cx).focus_handle(cx).is_focused(window)
                    || this.export_long_edge.read(cx).focus_handle(cx).is_focused(window);
                if event.keystroke.key == "escape" {
                    if let Some((index,_)) = this.curve_gesture.take() { this.cancel_field(index,window,cx); }
                    this.geometry.update(cx, |helper,cx| helper.cancel(cx));
                    this.flow_state.update(cx, |state,_| state.connecting = None);
                    this.flow.update(cx, |_,cx| cx.notify());
                }
                let modifiers = event.keystroke.modifiers;
                if !editing && event.keystroke.key == "?" {
                    this.show_shortcuts = !this.show_shortcuts;
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                if let Some(action) = shortcut(&event.keystroke.key, modifiers.control || modifiers.platform, modifiers.shift, modifiers.alt, editing) {
                    match action {
                        Shortcut::OpenImage => this.choose_file(FileKind::Image, window, cx),
                        Shortcut::OpenWorkflow => this.choose_file(FileKind::Workflow, window, cx),
                        Shortcut::SaveWorkflow => this.save(window, cx),
                        Shortcut::Search => {
                            this.workspace.library_open = true;
                            this.persist_workspace();
                            this.search.read(cx).focus_handle(cx).focus(window, cx);
                            cx.notify();
                        },
                        Shortcut::Undo => this.history(false, window, cx),
                        Shortcut::Redo => this.history(true, window, cx),
                        Shortcut::Delete => this.delete_selection(window, cx),
                        Shortcut::SelectAll => {
                            this.flow_state.update(cx, |state, _| state.select_all());
                            this.flow.update(cx, |_, cx| cx.notify());
                        },
                    }
                    cx.stop_propagation();
                }
            }))
            .child(
                div().h_flex().flex_wrap().gap_2().p_2().flex_shrink_0().border_b_1().border_color(rgb(t::ROOM_LINE))
                    .child(div().font_weight(FontWeight::BOLD).mr_2().child("RawWeave"))
                    .child(Button::new("open").label("Open Image").primary().on_click(
                        cx.listener(|this, _, window, cx| this.choose_file(FileKind::Image, window, cx))))
                    .child(file_menu)
                    .child(Button::new("export").label(if self.export_busy { "Exporting…" } else { "Export Image" })
                        .disabled(self.export_busy || self.session.source.is_none())
                        .on_click(cx.listener(|this, _, window, cx| this.export(window, cx))))
                    .child(view_menu)
                    .child(Button::new("undo").label("Undo").tooltip("Ctrl/Cmd+Z").disabled(self.undo.is_empty())
                        .on_click(cx.listener(|this, _, window, cx| this.history(false, window, cx))))
                    .child(Button::new("redo").label("Redo").tooltip("Ctrl/Cmd+Shift+Z").disabled(self.redo.is_empty())
                        .on_click(cx.listener(|this, _, window, cx| this.history(true, window, cx))))
            )
            .when(self.show_export_settings, |view| view.child(export_controls))
            .when(self.show_shortcuts, |view| view.child(div().p_2().text_sm()
                .child("Ctrl / Cmd: O load workflow · Shift+O open image · S save · K search · A select all (outside text fields) · Z undo · Shift+Z / Y redo. Delete removes selection; ? toggles this help. Text fields retain native Undo / Redo.")))
            .child(
                div().id("workbench-scroll").flex_1().min_h_0().overflow_x_scroll()
                    .child(div().h_full().w_full().min_w(px(
                        280.0 + if self.workspace.library_open { self.workspace.library_width } else { 0.0 }
                            + if self.workspace.viewer_open { self.workspace.viewer_width } else { 0.0 }
                    )).child(
                        // Base groups scale old sizes proportionally. A new window size
                        // must instead restore our absolute dock preferences (including WM tiling).
                        h_resizable(SharedString::from(format!("workbench-{}-{}-{}-{}", self.workspace_epoch, self.workspace.library_open, self.workspace.viewer_open, window.viewport_size().width.as_f32())))
                            .on_resize({
                                let weak = cx.entity().downgrade();
                                let library_open = self.workspace.library_open;
                                let viewer_open = self.workspace.viewer_open;
                                move |state, _, cx| {
                                    let sizes = state.read(cx).sizes().clone();
                                    let _ = weak.update(cx, |this, _| {
                                        if library_open && let Some(size) = sizes.first() { this.workspace.library_width = size.as_f32().clamp(180.0, 400.0); }
                                        if viewer_open && let Some(size) = sizes.last() { this.workspace.viewer_width = size.as_f32().clamp(280.0, 720.0); }
                                        this.persist_workspace();
                                    });
                                }
                            })
                            .when(self.workspace.library_open, |group| group.child(
                                resizable_panel().size(px(self.workspace.library_width)).size_range(px(180.0)..px(400.0)).flex_none().child(
                                    div().id("library").test_support().size_full().overflow_y_scroll().v_flex().p_2().bg(rgb(t::ROOM_PANEL))
                                        .child(div().h_flex().flex_wrap().gap_1().child(div().font_weight(FontWeight::BOLD).child("Nodes"))
                                            .child(Button::new("hide-library").small().label("Hide").on_click(cx.listener(|this, _, window, cx| this.toggle_panel(0, window, cx)))))
                                        .child(Input::new(&self.search))
                                        .child(div().h_flex().flex_wrap().gap_1()
                                            .child(Button::new("collapse-node-groups").small().label("Collapse all").on_click(cx.listener(|this, _, _, cx| {
                                                this.collapsed_categories = this.session.editor.node_descriptors().iter().map(|node| node_category(&node.type_id)).collect(); cx.notify();
                                            })))
                                            .child(Button::new("expand-node-groups").small().label("Expand all").on_click(cx.listener(|this, _, _, cx| { this.collapsed_categories.clear(); cx.notify(); }))))
                                        .child(
                                            div()
                                                .py_1()
                                                .font_family(t::Face::EdgeCode.family())
                                                .font_weight(t::Face::EdgeCode.weight())
                                                .text_size(t::Face::EdgeCode.size())
                                                .text_color(rgb(t::ROOM_INK_FAINT))
                                                .child(t::code("drag in · click or enter to add")),
                                        )
                                        .when(!self.selected_output_types().is_empty(), |view| view.child(Checkbox::new("compatible-nodes").label("Compatible inputs only").checked(self.compatible_only).on_click(cx.listener(|this, checked, _, cx| { this.compatible_only = *checked; cx.notify(); }))))
                                        .child(
                                            div()
                                                .pt_2()
                                                .font_family(t::Face::EdgeCode.family())
                                                .font_weight(t::Face::EdgeCode.weight())
                                                .text_size(t::Face::EdgeCode.size())
                                                .text_color(rgb(t::ROOM_INK_FAINT))
                                                .child(t::code(&format!(
                                                    "{library_count} matching {}",
                                                    if library_count == 1 { "node" } else { "nodes" }
                                                ))),
                                        )
                                        .children(library)
                                        .when(library_count == 0, |view| view.child(div().text_sm().child("No matching nodes. Clear search or turn off compatible filtering.")))
                                )
                            ))
                            .child(resizable_panel().size_range(px(280.0)..px(10000.0)).child(
                                v_resizable(SharedString::from(format!("workflow-panels-{}-{}-{}-{:?}-{}-{}", self.workspace_epoch, self.workspace.inspector_open, has_parameters, window.viewport_size(), self.show_export_settings, self.show_shortcuts)))
                                    .on_resize({
                                        let weak = cx.entity().downgrade();
                                        let inspector_open = self.workspace.inspector_open && has_parameters;
                                        move |state, _, cx| {
                                            if inspector_open && let Some(size) = state.read(cx).sizes().last().copied() {
                                                let _ = weak.update(cx, |this, _| { this.workspace.inspector_height = size.as_f32().clamp(100.0, 600.0); this.persist_workspace(); });
                                            }
                                        }
                                    })
                                    .child(resizable_panel().size_range(px(120.0)..px(10000.0)).child(canvas_panel))
                                    .when(self.workspace.inspector_open, |group| group.child(resizable_panel().size(px(inspector_height)).size_range(if has_parameters { px(100.0)..px(600.0) } else { px(38.0)..px(64.0) }).flex_none().child(inspector)))
                            ))
                            .when(self.workspace.viewer_open, |group| group.child(
                                resizable_panel().size(px(self.workspace.viewer_width)).size_range(px(280.0)..px(720.0)).flex_none().child(self.viewers.clone())
                            ))
                    ))
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .flex_shrink_0()
                    .bg(rgb(t::ROOM_STRIP))
                    .font_family(t::Face::Readout.family())
                    .text_size(t::Face::Readout.size())
                    .text_color(rgb(t::ROOM_INK_DIM))
                    .border_t_1()
                    .border_color(rgb(t::ROOM_LINE))
                    .child(format!("{}{} · {}", self.source_path.as_ref().map(|path| format!("{} · ", path.file_name().unwrap_or(path.as_os_str()).to_string_lossy())).unwrap_or_default(), self.status, self.gpu_label))
                    .when(!self.export_status.is_empty(), |view| view.child(div().child(self.export_status.clone()))),
            )
    }
}
/// Map the design's casts onto the component theme once, so shared controls
/// (buttons, inputs, menus, resizable handles) are in the world too.
fn configure_theme(cx: &mut App) {
    use native_theme as t;
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = t::Face::Body.family();
        theme.font_size = t::Face::Body.size();
        theme.mono_font_family = t::Face::Readout.family();
        theme.mono_font_size = t::Face::Readout.size();
        theme.radius = t::RADIUS;
        theme.radius_lg = t::RADIUS;
        // Room ground is warm near-black; the panel steps read by lightness alone.
        theme.background = rgb(t::ROOM_GROUND).into();
        theme.foreground = rgb(t::ROOM_INK).into();
        theme.border = rgb(t::ROOM_LINE).into();
        theme.input = rgb(t::ROOM_SUNK).into();
        theme.muted = rgb(t::ROOM_RAISE).into();
        theme.muted_foreground = rgb(t::ROOM_INK_DIM).into();
        theme.primary = rgb(t::WAX_WHITE).into();
        theme.primary_foreground = rgb(t::WAX_WHITE_INK).into();
        theme.primary_hover = rgb(t::WAX_WHITE_BRIGHT).into();
        theme.primary_active = rgb(t::ROOM_INK_BODY).into();
        theme.button_primary = theme.primary;
        theme.button_primary_foreground = theme.primary_foreground;
        theme.button_primary_hover = theme.primary_hover;
        theme.button_primary_active = theme.primary_active;
        theme.ring = rgb(t::WAX_AMBER).into();
        theme.danger = rgb(t::WAX_RED).into();
        theme.danger_foreground = rgb(t::WAX_RED_INK).into();
        // The plane and the judging station are their own casts, not chrome.
        theme.colors.primary = rgb(t::WAX_WHITE).into();
        theme.colors.foreground = rgb(t::ROOM_INK).into();
        theme.colors.background = rgb(t::ROOM_GROUND).into();
        theme.colors.border = rgb(t::ROOM_LINE).into();
        theme.colors.input = rgb(t::ROOM_SUNK).into();
        theme.colors.ring = rgb(t::WAX_AMBER).into();
    });
}
fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Err(error) = native_theme::install_fonts(cx.text_system(), cx) {
                eprintln!("RawWeave design faces: {error}");
            }
            configure_theme(cx);
            let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
            let result = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_decorations: Some(WindowDecorations::Server),
                    titlebar: Some(TitlebarOptions {
                        title: Some("RawWeave — Native".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    cx.new(|cx| {
                        let mut view = Editor::new(window, cx);
                        if let Some(path) = path {
                            view.open_path(path, window, cx);
                        }
                        view
                    })
                },
            );
            if let Err(error) = result {
                eprintln!("Could not open native GPU window: {error}");
                cx.quit();
            } else {
                cx.activate(true);
            }
        });
}
