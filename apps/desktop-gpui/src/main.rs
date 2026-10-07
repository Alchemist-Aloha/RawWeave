extern crate gpui_kit as gpui;
use gpui_flow::{FlowEdge, FlowGraph, FlowNode, FlowState, HandleDef, HandlePosition};
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
    *,
};
use gpui_kit::*;
use rawweave_core::NodeId as CoreNodeId;
use rawweave_gpui::{
    PreviewFrame, Session, Source, bounded_read, preview_type, save_workflow_atomic,
};
use rawweave_node_api::{ParameterType, ParameterValue};
use rawweave_rendering::CancellationToken;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

struct Field {
    label: String,
    state: Entity<InputState>,
}
struct Editor {
    session: Session,
    flow_state: Entity<FlowState>,
    flow: Entity<FlowGraph>,
    _flow_subscription: Subscription,
    fields: Vec<Field>,
    field_subscriptions: Vec<Subscription>,
    selected: Option<String>,
    search: Entity<InputState>,
    _search_subscription: Subscription,
    image: Option<Arc<RenderImage>>,
    full_dimensions: Option<rawweave_image::Dimensions>,
    viewport: gpui_kit::Size<Pixels>,
    zoom: Option<f32>,
    pan: Point<Pixels>,
    drag: Option<Point<Pixels>>,
    cancellation: CancellationToken,
    generation: u64,
    source_generation: u64,
    status: String,
    gpu_label: String,
    pixel_ratio: f32,
    undo: Vec<String>,
    redo: Vec<String>,
    layout: BTreeMap<String, (f32, f32)>,
    focus: FocusHandle,
}
impl Editor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut session = Session::default();
        let status = match session.editor.reset_ordinary_image_graph() {
            Ok(()) => "Open an image to begin".into(),
            Err(e) => e.to_string(),
        };
        let flow_state = cx.new(|_| FlowState::new(vec![], vec![]));
        let flow = cx.new(|cx| {
            FlowGraph::new(flow_state.clone(), cx)
                .default_renderer(|node, _, _| {
                    div()
                        .w(px(160.0))
                        .v_flex()
                        .gap_1()
                        .text_color(rgb(0xe1e5eb))
                        .child(node.label.to_string())
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0xa6adb8))
                                .child(node.id.to_string()),
                        )
                        .into_any_element()
                })
                .bg_color(0x17191c)
                .grid_color(0x303439)
                .node_bg_color(0x23272d)
                .node_border_color(0x434951)
        });
        let subscription = cx.observe_in(&flow, window, |this, _, window, cx| {
            this.sync_flow(window, cx)
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes"));
        let search_subscription = cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify());
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
            search,
            _search_subscription: search_subscription,
            image: None,
            full_dimensions: None,
            viewport: size(px(800.0), px(600.0)),
            zoom: None,
            pan: point(px(0.0), px(0.0)),
            drag: None,
            cancellation: CancellationToken::new(),
            generation: 0,
            source_generation: 0,
            status,
            gpu_label,
            pixel_ratio: window.scale_factor(),
            undo: vec![],
            redo: vec![],
            layout: BTreeMap::new(),
            focus: cx.focus_handle(),
        };
        this.rebuild_flow(cx);
        this
    }
    fn rebuild_flow(&mut self, cx: &mut Context<Self>) {
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
                        .unwrap_or((40.0 + (i % 2) as f32 * 240.0, 40.0 + (i / 2) as f32 * 160.0));
                    let handles =
                        node.descriptor
                            .inputs
                            .iter()
                            .map(|p| HandleDef::target(HandlePosition::Left).id(p.id.clone()))
                            .chain(
                                node.descriptor.outputs.iter().map(|p| {
                                    HandleDef::source(HandlePosition::Right).id(p.id.clone())
                                }),
                            )
                            .collect();
                    let mut view = FlowNode::new(node.id.as_str().to_owned(), x, y)
                        .label(node.descriptor.name.clone())
                        .node_type(node.type_id.clone())
                        .handles(handles)
                        .size(190.0, 70.0);
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
                .color(0xb5bdcb)
            })
            .collect();
        self.flow_state.update(cx, |state, _| {
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
                        .map_err(|e| e.to_string())?;
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
            if let Ok(old) = old {
                self.remember(old);
            }
            self.session.editor = candidate;
        }
        self.layout = nodes
            .iter()
            .map(|n| (n.id.to_string(), (n.position.x, n.position.y)))
            .collect();
        let selected = nodes.iter().find(|n| n.selected).map(|n| n.id.to_string());
        let mut target_changed = false;
        if selected != self.selected {
            self.selected = selected;
            self.rebuild_fields(window, cx);
            if let Some(node) = self.selected.as_ref().and_then(|id| {
                self.session
                    .editor
                    .graph()
                    .node(&CoreNodeId::from(id.as_str()))
            }) && let Some(port) = node
                .descriptor
                .outputs
                .iter()
                .find(|p| preview_type(&p.data_type))
            {
                let target = (node.id.as_str().to_owned(), port.id.clone());
                target_changed = self.session.target != target;
                self.session.target = target;
            }
        }
        if changed || target_changed {
            self.request_preview(window, cx);
        }
        cx.notify();
    }
    fn rebuild_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fields.clear();
        self.field_subscriptions.clear();
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
                .unwrap_or(&descriptor.default);
            let value = match value {
                ParameterValue::Float(v) => v.to_string(),
                ParameterValue::Integer(v) => v.to_string(),
                ParameterValue::Boolean(v) => v.to_string(),
                ParameterValue::String(v) => v.clone(),
            };
            let state = cx.new(|cx| InputState::new(window, cx).default_value(value));
            let node_id = node.id.as_str().to_owned();
            let id = descriptor.id;
            let kind = descriptor.parameter_type;
            let subscription = cx.subscribe_in(
                &state,
                window,
                move |this, field, event: &InputEvent, window, cx| {
                    if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        return;
                    }
                    let text = field.read(cx).value().to_string();
                    let parsed = match kind {
                        ParameterType::Float => text
                            .parse::<f32>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .map(ParameterValue::Float),
                        ParameterType::Integer => text.parse().ok().map(ParameterValue::Integer),
                        ParameterType::Boolean => text.parse().ok().map(ParameterValue::Boolean),
                        ParameterType::String => Some(ParameterValue::String(text)),
                    };
                    let Some(value) = parsed else {
                        this.status = "Invalid parameter value".into();
                        cx.notify();
                        return;
                    };
                    if this
                        .session
                        .editor
                        .graph()
                        .node(&CoreNodeId::from(node_id.as_str()))
                        .and_then(|n| n.parameters.get(&id))
                        == Some(&value)
                    {
                        return;
                    }
                    let old = this.document();
                    match this.session.editor.set_node_parameter(&node_id, &id, value) {
                        Ok(()) => {
                            if let Ok(old) = old {
                                this.remember(old);
                            }
                            this.request_preview(window, cx);
                        }
                        Err(e) => this.status = e.to_string(),
                    }
                    cx.notify();
                },
            );
            self.field_subscriptions.push(subscription);
            self.fields.push(Field {
                label: descriptor.name,
                state,
            });
        }
    }
    fn image_zoom(&self) -> f32 {
        self.zoom.unwrap_or_else(|| {
            self.full_dimensions
                .or_else(|| self.session.source.as_ref().map(Source::dimensions))
                .map_or(1.0, |dims| {
                    (f32::from(self.viewport.width) / dims.width as f32)
                        .min(f32::from(self.viewport.height) / dims.height as f32)
                })
        })
    }
    fn desired_mip(&self) -> u8 {
        let zoom = self.image_zoom();
        ((1.0 / (zoom * self.pixel_ratio).max(0.0001))
            .log2()
            .floor()
            .max(0.0) as u8)
            .min(6)
    }
    fn request_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancellation.cancel();
        self.cancellation = CancellationToken::new();
        let Some(generation) = self.generation.checked_add(1) else {
            self.status = "preview counter exhausted".into();
            return;
        };
        self.generation = generation;
        if self.session.source.is_none() {
            self.status = "Select a compatible source image".into();
            cx.notify();
            return;
        }
        let snapshot = self.session.clone();
        let token = self.cancellation.clone();
        let mip = self.desired_mip();
        let coarse = (mip + 1).min(6);
        self.status = "Rendering coarse preview…".into();
        let first_snapshot = snapshot.clone();
        let first_token = token.clone();
        let task = cx
            .background_executor()
            .spawn(async move { first_snapshot.preview(coarse, &first_token) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let accepted = this
                .update_in(cx, |this, window, cx| {
                    this.accept_frame(generation, result, window, cx)
                })
                .unwrap_or(false);
            if !accepted || token.is_cancelled() || coarse == mip {
                return;
            }
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            if token.is_cancelled() {
                return;
            }
            let mip = this
                .update_in(cx, |this, _, _| this.desired_mip())
                .unwrap_or(mip);
            let task = cx
                .background_executor()
                .spawn(async move { snapshot.preview(mip, &token) });
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.accept_frame(generation, result, window, cx) {
                    this.status = format!("Ready · {}", this.gpu_label);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn accept_frame(
        &mut self,
        generation: u64,
        result: Result<PreviewFrame, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.generation || self.cancellation.is_cancelled() {
            return false;
        }
        match result {
            Ok(frame) => {
                let Some(buffer) = image::RgbaImage::from_raw(
                    frame.dimensions.width,
                    frame.dimensions.height,
                    frame.bgra,
                ) else {
                    self.status = "Invalid image upload".into();
                    cx.notify();
                    return false;
                };
                if let Some(previous) = self.image.take()
                    && let Err(error) = window.drop_image(previous)
                {
                    eprintln!("release preview texture: {error}");
                }
                self.image = Some(Arc::new(RenderImage::new(smallvec::smallvec![
                    image::Frame::new(buffer)
                ])));
                self.full_dimensions = Some(frame.full_dimensions);
                self.status = format!(
                    "{} × {} · preview {} × {}",
                    frame.full_dimensions.width,
                    frame.full_dimensions.height,
                    frame.dimensions.width,
                    frame.dimensions.height
                );
                cx.notify();
                true
            }
            Err(error) => {
                self.status = error;
                cx.notify();
                false
            }
        }
    }
    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.cancellation.cancel();
        self.source_generation += 1;
        let generation = self.source_generation;
        self.status = format!("Opening {}…", path.display());
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
                        if let Some(old) = this.image.take() {
                            let _ = window.drop_image(old);
                        }
                        this.full_dimensions = this.session.source.as_ref().map(Source::dimensions);
                        this.undo.clear();
                        this.redo.clear();
                        this.selected = None;
                        this.fields.clear();
                        this.field_subscriptions.clear();
                        this.rebuild_flow(cx);
                        this.request_preview(window, cx);
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
    fn choose_file(&mut self, workflow: bool, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if workflow {
                    "Load workflow"
                } else {
                    "Open image"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await
                && let Some(path) = paths.into_iter().next()
            {
                if !workflow {
                    let _ = this.update_in(cx, |this, window, cx| this.open_path(path, window, cx));
                    return;
                }
                let task = cx.background_executor().spawn(async move {
                    bounded_read(&path, 16 * 1024 * 1024)
                        .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
                });
                let result = task.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    match result.and_then(|text| this.read_document(&text, true)) {
                        Ok(()) => {
                            this.cancellation.cancel();
                            this.source_generation += 1;
                            this.generation += 1;
                            this.undo.clear();
                            this.redo.clear();
                            this.selected = None;
                            this.fields.clear();
                            this.field_subscriptions.clear();
                            this.full_dimensions = None;
                            if let Some(old) = this.image.take() {
                                let _ = window.drop_image(old);
                            }
                            this.rebuild_flow(cx);
                            this.status = "Workflow loaded · select a compatible source".into();
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
    fn history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
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
                if let Ok(current) = current {
                    if redo {
                        self.undo.push(current);
                    } else {
                        self.redo.push(current);
                    }
                }
                self.selected = None;
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let search = self.search.read(cx).value().to_lowercase();
        let library = self
            .session
            .editor
            .node_descriptors()
            .into_iter()
            .filter(|d| d.name.to_lowercase().contains(&search) || d.type_id.contains(&search))
            .map(|descriptor| {
                let kind = descriptor.type_id.clone();
                Button::new(SharedString::from(descriptor.type_id))
                    .label(descriptor.name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let old = this.document();
                        let mut i = this.session.editor.graph().nodes().len();
                        while this
                            .session
                            .editor
                            .graph()
                            .node(&CoreNodeId::from(format!("node-{i}")))
                            .is_some()
                        {
                            i += 1;
                        }
                        let id = format!("node-{i}");
                        match this.session.editor.add_node(&id, &kind) {
                            Ok(()) => {
                                if let Ok(old) = old {
                                    this.remember(old);
                                }
                                let y = this
                                    .layout
                                    .values()
                                    .map(|(_, y)| *y)
                                    .max_by(f32::total_cmp)
                                    .unwrap_or(40.0)
                                    + 160.0;
                                this.layout.insert(id.clone(), (40.0, y));
                                this.selected = Some(id);
                                this.rebuild_fields(window, cx);
                                this.rebuild_flow(cx);
                            }
                            Err(e) => this.status = e.to_string(),
                        };
                        cx.notify();
                    }))
            });
        let weak = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if (f32::from(this.viewport.width - bounds.size.width)).abs() > 1.0
                        || (f32::from(this.viewport.height - bounds.size.height)).abs() > 1.0
                        || this.pixel_ratio != window.scale_factor()
                    {
                        this.viewport = bounds.size;
                        this.pixel_ratio = window.scale_factor();
                        this.request_preview(window, cx);
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let mut viewer = div()
            .id("viewer")
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .bg(rgb(0x101215))
            .child(measure);
        if let (Some(image), Some(dimensions)) = (&self.image, self.full_dimensions) {
            let zoom = self.image_zoom();
            let width = dimensions.width as f32 * zoom;
            let height = dimensions.height as f32 * zoom;
            viewer = viewer.child(
                img(image.clone())
                    .absolute()
                    .w(px(width))
                    .h(px(height))
                    .left((self.viewport.width - px(width)) / 2.0 + self.pan.x)
                    .top((self.viewport.height - px(height)) / 2.0 + self.pan.y),
            );
        } else {
            viewer = viewer.child(
                div()
                    .absolute()
                    .left(px(24.0))
                    .top(px(24.0))
                    .text_color(rgb(0xa6adb8))
                    .child("Open JPEG / PNG / RAW · or load a workflow"),
            );
        }
        viewer = viewer
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, _| this.drag = Some(event.position)),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(previous) = this.drag {
                    this.pan += event.position - previous;
                    this.drag = Some(event.position);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = None),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let delta = event.delta.pixel_delta(px(20.0));
                let zoom = this.image_zoom();
                this.zoom = Some((zoom * (f32::from(delta.y) * 0.002).exp()).clamp(0.01, 8.0));
                this.request_preview(window, cx);
            }));
        div()
            .id("rawweave")
            .track_focus(&self.focus)
            .size_full()
            .v_flex()
            .bg(rgb(0x1c2026))
            .text_color(rgb(0xe1e5eb))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.modifiers.control
                    && event.keystroke.key == "z"
                    && !this
                        .fields
                        .iter()
                        .any(|f| f.state.read(cx).focus_handle(cx).is_focused(window))
                    && !this.search.read(cx).focus_handle(cx).is_focused(window)
                {
                    this.history(event.keystroke.modifiers.shift, window, cx);
                }
            }))
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .p_2()
                    .border_b_1()
                    .border_color(rgb(0x343b45))
                    .child(div().font_weight(FontWeight::BOLD).mr_4().child("RawWeave"))
                    .child(Button::new("open").label("Open Image").on_click(
                        cx.listener(|this, _, window, cx| this.choose_file(false, window, cx)),
                    ))
                    .child(Button::new("load").label("Load Workflow").on_click(
                        cx.listener(|this, _, window, cx| this.choose_file(true, window, cx)),
                    ))
                    .child(
                        Button::new("save")
                            .label("Save Workflow")
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(Button::new("undo").label("Undo").on_click(
                        cx.listener(|this, _, window, cx| this.history(false, window, cx)),
                    ))
                    .child(Button::new("redo").label("Redo").on_click(
                        cx.listener(|this, _, window, cx| this.history(true, window, cx)),
                    ))
                    .child(Button::new("fit").label("Fit").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.zoom = None;
                            this.pan = point(px(0.0), px(0.0));
                            this.request_preview(window, cx);
                        },
                    )))
                    .child(Button::new("actual").label("100%").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.zoom = Some(1.0);
                            this.pan = point(px(0.0), px(0.0));
                            this.request_preview(window, cx);
                        },
                    ))),
            )
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("library")
                            .w(px(200.0))
                            .h_full()
                            .overflow_y_scroll()
                            .v_flex()
                            .gap_1()
                            .p_2()
                            .child(Input::new(&self.search))
                            .children(library),
                    )
                    .child(
                        div()
                            .v_flex()
                            .w(px(510.0))
                            .h_full()
                            .child(div().flex_1().min_h_0().child(self.flow.clone()))
                            .child(
                                div()
                                    .id("parameters")
                                    .max_h(px(240.0))
                                    .overflow_y_scroll()
                                    .v_flex()
                                    .p_3()
                                    .gap_2()
                                    .child(self.selected.clone().unwrap_or_else(|| {
                                        "Select a node to edit parameters".into()
                                    }))
                                    .children(self.fields.iter().map(|field| {
                                        div()
                                            .h_flex()
                                            .gap_2()
                                            .child(div().w(px(180.0)).child(field.label.clone()))
                                            .child(Input::new(&field.state))
                                    })),
                            ),
                    )
                    .child(viewer),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .border_t_1()
                    .border_color(rgb(0x343b45))
                    .child(self.status.clone()),
            )
    }
}
fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
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
