//! Controls live in the node graph; all edits use the authoritative editor/history paths.
use super::*;

impl Editor {
    fn parameter_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
                                    .id("parameters")
                                    .test_support()
                                    .w_full()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_x_hidden()
                                    .overflow_y_scroll()
                                    .v_flex()
                                    .p_3()
                                    .gap_2()
                                    .child(div().h_flex().flex_wrap().gap_2()
                                        .child(div().font_weight(FontWeight::BOLD).child("Parameters"))
                                        .child(Button::new("collapse-node-controls").small().label("Collapse controls")
                                            .on_click(cx.listener(|this, _, window, cx| { window.focus(&this.focus, cx); this.controls_open = false; this.flow.update(cx, |_, cx| cx.notify()); cx.notify(); }))))
                                    .when(self.fields.iter().any(|field|field.draft.ux.advanced),|view|view.child(Button::new("advanced-parameters").label(if self.show_advanced {"Hide Advanced"} else {"Show Advanced"}).on_click(cx.listener(|this,_,window,cx|{window.focus(&this.focus,cx);this.show_advanced = !this.show_advanced;cx.notify();}))))
                                    .children(self.fields.iter().enumerate().filter(|(_,field)| !field.draft.ux.advanced || self.show_advanced).map(|(index,field)| {
                                        let ux=&field.draft.ux;
                                        let points=if ux.point_curve {curve_points(field.draft.text(),ux.scalar_curve).ok()} else {None};
                                        let control=if field.draft.descriptor.parameter_type==ParameterType::Boolean {
                                            Checkbox::new(("boolean",index)).label(ux.name.clone()).checked(matches!(field.draft.committed(),ParameterValue::Boolean(true)))
                                                .on_click(cx.listener(move |this,value,window,cx|this.choose_field(index,ParameterValue::Boolean(*value),window,cx))).into_any_element()
                                        } else if !ux.options.is_empty() {
                                            let options=ux.options.clone();let current=field.draft.text().to_owned();let weak=cx.entity().downgrade();
                                            let label=options.iter().find(|(value,_)|value==&current).map(|(_,label)|label.clone()).unwrap_or_else(||current.clone());
                                            Button::new(("choice",index)).label(label).dropdown_menu(move |mut menu,_,_| {
                                                for (value,label) in &options {let value=value.clone();let weak=weak.clone();menu=menu.item(PopupMenuItem::new(label.clone()).checked(value==current).on_click(move |_,window,cx|{let _=weak.update(cx,|this,cx|this.choose_field(index,ParameterValue::String(value.clone()),window,cx));}));}menu
                                            }).into_any_element()
                                        } else {match &field.state {AnyInputState::Input(state)=>Input::new(state).into_any_element(),AnyInputState::Textarea(state)=>Textarea::new(state).into_any_element(),_=>div().child("Unsupported parameter field").into_any_element()}};
                                        div().v_flex().gap_1().border_b_1().border_color(rgb(0x343b45)).pb_2()
                                            .capture_key_down(cx.listener(move |this,event:&KeyDownEvent,window,cx| {
                                                let text_focused=this.fields.get(index).is_some_and(|field|field.state.focus_handle(cx).is_focused(window));
                                                if !text_focused {return;}
                                                if event.keystroke.key=="escape" {this.cancel_field(index,window,cx);cx.stop_propagation();}
                                                else if matches!(event.keystroke.key.as_str(),"up"|"down") && !event.keystroke.modifiers.control && !event.keystroke.modifiers.platform
                                                    && let Some(field)=this.fields.get_mut(index) && field.draft.numeric() {
                                                        field.stepping = true;
                                                        let multiplier=if event.keystroke.key=="up" {1.0} else {-1.0};
                                                        field.error=field.draft.step(multiplier).err();let text=field.draft.text().to_owned();
                                                        if let Some(input)=field.state.as_input() {input.update(cx,|input,cx|input.replace_all(text,window,cx));}cx.stop_propagation();cx.notify();
                                                }
                                            }))
                                            .on_key_up(cx.listener(move |this,event:&KeyUpEvent,window,cx|{if matches!(event.keystroke.key.as_str(),"up"|"down") && this.fields.get(index).is_some_and(|field|field.stepping) {this.commit_field(index,window,cx);}}))
                                            .child(div().h_flex().flex_wrap().gap_2().child(format!("{}{}",ux.name,ux.unit.as_ref().map(|unit|format!(" ({unit})")).unwrap_or_default()))
                                                .child(div().capture_any_mouse_down(cx.listener(move |this,_,window,cx|this.cancel_field(index,window,cx))).child(Button::new(("reset",index)).label("Reset").disabled(!field.draft.modified()).on_click(cx.listener(move |this,_,window,cx|this.reset_field(index,window,cx)))))
                                                .child(Button::new(("port",index)).label(if field.exposed {"Hide input"} else {"As input"}).on_click(cx.listener(move |this,_,window,cx|this.toggle_parameter_port(index,window,cx)))))
                                            .when_some(points.as_ref(),|view,points| {
                                                let selected=field.curve_index.min(points.len().saturating_sub(1));
                                                view.child(div().v_flex().gap_1()
                                                    .when_some(points.get(selected),|view,(x,y)|view.child(div().text_sm().child(format!("Point {} of {}: input {x}, output {y}",selected+1,points.len()))))
                                                    .child(div().h_flex().flex_wrap().gap_1()
                                                        .child(Button::new(("curve-previous",index)).label("Previous point").disabled(selected==0).on_click(cx.listener(move |this,_,_,cx|this.select_curve_point(index,false,cx))))
                                                        .child(Button::new(("curve-next",index)).label("Next point").disabled(selected+1>=points.len()).on_click(cx.listener(move |this,_,_,cx|this.select_curve_point(index,true,cx))))
                                                        .child(Button::new(("curve-add",index)).label("Add point").disabled(points.len()>=4096).on_click(cx.listener(move |this,_,window,cx|this.edit_curve_points(index,false,window,cx))))
                                                        .child(Button::new(("curve-remove",index)).label("Delete point").disabled(selected==0||selected+1>=points.len()).on_click(cx.listener(move |this,_,window,cx|this.edit_curve_points(index,true,window,cx)))))
                                                    .child(div().text_xs().text_color(rgb(0xa6adb8)).child("Add inserts the midpoint of the widest input interval. Endpoints cannot be deleted with these controls; edit exact x,y pairs below.")))
                                            })
                                            .when(ux.point_curve || (self.selected.as_ref().is_some_and(|id| self.session.editor.graph().node(&CoreNodeId::from(id.as_str())).is_some_and(|node| node.type_id == "core.curves")) && field.draft.descriptor.id == "gamma"), |view| view.child(self.curve_plot(index, cx)))
                                            .child(control)
                                            .when(ux.multiline,|view|view.child(div().text_xs().text_color(rgb(0xa6adb8)).child("Shift+Enter inserts a line; Enter applies; Escape cancels.")))
                                            .when_some(field.slider.as_ref(),|view,slider|view.child(Slider::new(slider).w_full()))
                                            .child(div().text_xs().text_color(rgb(0xa6adb8)).child(ux.description.clone()))
                                            .when(field.draft.outside_recommended(),|view|view.child(div().text_xs().child("Outside recommended slider range; exact value is preserved.")))
                                            .when_some(self.selected.as_ref().and_then(|id| self.session.editor.graph().edges().iter().find(|edge| edge.to_node.as_str()==id && edge.to_port==field.draft.descriptor.id)), |view, edge| view.child(div().text_xs().child(format!("Driven by {} · {}",self.session.node_label(edge.from_node.as_str()),edge.from_port))))
                                            .when_some(field.error.as_ref(),|view,error|view.child(div().text_xs().text_color(rgb(0xffa480)).child(error.clone())))
                                    })).into_any_element()
    }
}

impl Editor {
    pub(super) fn render_graph_node(
        &mut self,
        node: &FlowNode,
        zoom: f32,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ports = node
            .handles
            .iter()
            .filter_map(|handle| handle.offset)
            .fold(64.0, f32::max);
        let selected = self.selected.as_deref() == Some(node.id.as_ref());
        let focus_id = node.id.to_string();
        let source = self
            .session
            .image_input_target(node.id.as_ref())
            .map(|(id, _)| self.session.node_label(&id));
        let image_node = self
            .session
            .editor
            .graph()
            .node(&CoreNodeId::from(node.id.as_ref()))
            .is_some_and(|node| node.descriptor.inputs.iter().any(|port| port.id == "image"));
        self.geometry.update(cx, |helper, _| helper.zoom = zoom);
        div()
            .id(SharedString::from(format!("node-content-{}", node.id)))
            .test_support()
            .w(px(300.0 * zoom))
            .v_flex()
            .gap(px(4.0 * zoom))
            .text_size(px(12.0 * zoom))
            .text_color(rgb(0xe1e5eb))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(14.0 * zoom))
                    .child(node.label.clone()),
            )
            .child(
                div()
                    .text_size(px(10.5 * zoom))
                    .text_color(rgb(0xa6adb8))
                    .child(node.node_type.clone().unwrap_or_default()),
            )
            .child(div().h(px((ports - 36.0) * zoom)).flex_shrink_0())
            .when(selected, |view| {
                view.child(
                    Button::new("focus-node-controls")
                        .small()
                        .label("Focus node")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.reveal_canvas_node(&focus_id, cx)
                        })),
                )
            })
            .when(image_node, |view| {
                view.child(
                    div().text_size(px(10.5 * zoom)).child(
                        source
                            .map(|name| format!("Input image from {name}"))
                            .unwrap_or_else(|| {
                                "Input image: connect an image output socket".into()
                            }),
                    ),
                )
            })
            .when(selected && !self.controls_open, |view| {
                view.child(
                    Button::new("open-node-controls")
                        .small()
                        .label("Edit controls")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.controls_open = true;
                            this.flow.update(cx, |_, cx| cx.notify());
                            cx.notify();
                        })),
                )
            })
            .when(selected && self.controls_open, |view| {
                view.child(
                    div()
                        .id("inline-node-controls")
                        .test_support()
                        .w_full()
                        .max_h(px(
                            (self.canvas_size.1 - (ports + 96.0) * zoom).clamp(90.0, 420.0 * zoom)
                        ))
                        .overflow_y_scroll()
                        // Fields and helper gestures must never start a node drag or canvas zoom.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .child(self.parameter_references(node.id.as_ref(), cx))
                        .when(image_node, |view| view.child(self.geometry.clone()))
                        .when(!self.fields.is_empty(), |view| {
                            view.child(self.parameter_controls(cx))
                        }),
                )
            })
            .into_any_element()
    }

    pub(super) fn refresh_geometry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let node = self.selected.clone();
        let generation = self.source_generation;
        self.geometry.update(cx, |helper, cx| {
            helper.configure(session, node, generation, window, cx)
        });
    }

    fn apply_node_values(
        &mut self,
        id: &str,
        values: BTreeMap<String, ParameterValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old = self.document();
        let mut candidate = self.session.editor.clone();
        for (key, value) in values {
            if let Err(error) = candidate.set_node_parameter(id, &key, value) {
                self.status = format!("Could not edit {}: {error}", self.session.node_label(id));
                cx.notify();
                return;
            }
        }
        if candidate.graph().revision() == self.session.editor.graph().revision() {
            return;
        }
        if let Ok(old) = old {
            self.remember(old);
        }
        self.session.editor = candidate;
        self.rebuild_fields(window, cx);
        self.request_preview(window, cx);
        cx.notify();
    }
}

pub(super) struct GeometryHelper {
    owner: WeakEntity<Editor>,
    session: Session,
    key: Option<(String, u64, u64)>,
    node: Option<String>,
    kind: String,
    image: Option<std::sync::Arc<RenderImage>>,
    dimensions: Option<rawweave_image::Dimensions>,
    origin: (u32, u32),
    bounds: Bounds<Pixels>,
    start: Option<Point<Pixels>>,
    end: Option<Point<Pixels>>,
    token: rawweave_rendering::CancellationToken,
    status: String,
    zoom: f32,
}
impl GeometryHelper {
    pub(super) fn new(owner: WeakEntity<Editor>) -> Self {
        Self {
            owner,
            session: Session::default(),
            key: None,
            node: None,
            kind: String::new(),
            image: None,
            dimensions: None,
            origin: (0, 0),
            bounds: Bounds::default(),
            start: None,
            end: None,
            token: rawweave_rendering::CancellationToken::new(),
            status: "Connect an image input to use these controls".into(),
            zoom: 1.0,
        }
    }
    fn configure(
        &mut self,
        session: Session,
        node: Option<String>,
        source_generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = node.as_ref().map(|node| {
            (
                node.clone(),
                session.editor.graph().revision(),
                source_generation,
            )
        });
        if key == self.key {
            return;
        }
        self.token.cancel();
        self.token = rawweave_rendering::CancellationToken::new();
        self.start = None;
        self.end = None;
        self.dimensions = None;
        if let Some(image) = self.image.take() {
            let _ = window.drop_image(image);
        }
        self.kind = node
            .as_ref()
            .and_then(|id| session.editor.graph().node(&CoreNodeId::from(id.as_str())))
            .map(|node| node.type_id.clone())
            .unwrap_or_default();
        self.node = node.clone();
        self.key = key.clone();
        self.session = session.clone();
        self.status = "Connect an image input to use these controls".into();
        let Some(node) = node.filter(|id| session.image_input_target(id).is_some()) else {
            cx.notify();
            return;
        };
        self.status = "Reading input image…".into();
        let token = self.token.clone();
        let task = cx
            .background_executor()
            .spawn(async move { session.image_input_preview(&node, &token) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.key != key || this.token.is_cancelled() {
                    return;
                }
                match result.and_then(|preview| {
                    let dims = preview.frame.full_dimensions;
                    let image =
                        crate::native_scopes::upload_raster(rawweave_rendering::scopes::Raster {
                            dimensions: preview.frame.dimensions,
                            bgra: preview.frame.bgra,
                        })?;
                    Ok((image, dims, preview.origin))
                }) {
                    Ok((image, dims, origin)) => {
                        this.image = Some(image);
                        this.dimensions = Some(dims);
                        this.origin = origin;
                        this.status = format!("Input: {} × {} px", dims.width, dims.height);
                    }
                    Err(error) => this.status = format!("Input preview unavailable: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn local(&self, pointer: Point<Pixels>) -> Option<(f64, f64)> {
        let size = self.dimensions?;
        if self.bounds.size.width <= px(0.0) || self.bounds.size.height <= px(0.0) {
            return None;
        }
        Some((
            f64::from(
                (pointer.x - self.bounds.origin.x).as_f32() / self.bounds.size.width.as_f32(),
            ) * f64::from(size.width),
            f64::from(
                (pointer.y - self.bounds.origin.y).as_f32() / self.bounds.size.height.as_f32(),
            ) * f64::from(size.height),
        ))
    }
    fn submit(
        &mut self,
        values: BTreeMap<String, ParameterValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.node.clone() else {
            return;
        };
        let owner = self.owner.clone();
        // Apply after this helper's update completes; parent refresh may update this same entity.
        window.defer(cx, move |window, cx| {
            let _ = owner.update(cx, |this, cx| {
                this.apply_node_values(&node, values, window, cx)
            });
        });
    }
    fn finish(&mut self, pointer: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let values = self
            .start
            .and_then(|start| self.local(start))
            .zip(self.local(pointer))
            .zip(self.dimensions)
            .and_then(|((start, end), size)| {
                rawweave_gpui::geometry::drawn_parameters(&self.kind, start, end, size, self.origin)
            });
        self.start = None;
        self.end = None;
        if let Some(values) = values {
            self.submit(values, window, cx);
        }
        cx.notify();
    }
    fn preset(&mut self, ratio: Option<f64>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(values) = self
            .dimensions
            .and_then(|size| rawweave_gpui::geometry::crop_preset(size, ratio))
        {
            self.submit(values, window, cx);
        }
    }
    fn resize_to(&mut self, scale: f64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(size) = self.dimensions {
            self.submit(
                [
                    (
                        "width".into(),
                        ParameterValue::Float(
                            (f64::from(size.width) * scale).round().max(1.0) as f32
                        ),
                    ),
                    (
                        "height".into(),
                        ParameterValue::Float(
                            (f64::from(size.height) * scale).round().max(1.0) as f32
                        ),
                    ),
                ]
                .into(),
                window,
                cx,
            );
        }
    }
    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.start = None;
        self.end = None;
        cx.notify();
    }
    fn driven(&self) -> bool {
        self.node
            .as_ref()
            .and_then(|id| {
                self.session
                    .editor
                    .graph()
                    .node(&CoreNodeId::from(id.as_str()))
            })
            .is_some_and(|node| {
                node.exposed_parameters.iter().any(|id| {
                    matches!(
                        id.as_str(),
                        "x" | "y"
                            | "width"
                            | "height"
                            | "start_x"
                            | "start_y"
                            | "end_x"
                            | "end_y"
                            | "center_x"
                            | "center_y"
                            | "radius"
                    )
                })
            })
    }
}
impl Render for GeometryHelper {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drawable = matches!(
            self.kind.as_str(),
            "core.crop" | "core.mask-linear-gradient" | "core.mask-radial-gradient"
        );
        let ready = self.dimensions.is_some() && !self.driven();
        let weak = cx.entity().downgrade();
        let dimensions = self.dimensions;
        let scale = dimensions.map_or(1.0, |size| {
            (300.0 * self.zoom / size.width as f32).min(180.0 * self.zoom / size.height as f32)
        });
        let thumbnail =
            dimensions.map(|size| (size.width as f32 * scale, size.height as f32 * scale));
        let start = self.start;
        let end = self.end;
        let kind = self.kind.clone();
        let overlay = canvas(
            move |bounds, _, cx| {
                let actual = dimensions.map_or(bounds, |size| {
                    let scale = (bounds.size.width.as_f32() / size.width as f32)
                        .min(bounds.size.height.as_f32() / size.height as f32);
                    let extent = gpui_kit::size(
                        px(size.width as f32 * scale),
                        px(size.height as f32 * scale),
                    );
                    Bounds::new(
                        bounds.origin
                            + point(
                                (bounds.size.width - extent.width) / 2.0,
                                (bounds.size.height - extent.height) / 2.0,
                            ),
                        extent,
                    )
                });
                let _ = weak.update(cx, |this, _| this.bounds = actual);
                actual
            },
            move |_, actual, window, _| {
                if let Some((a, b)) = start.zip(end) {
                    let clamp = |p: Point<Pixels>| {
                        point(
                            p.x.clamp(actual.left(), actual.right()),
                            p.y.clamp(actual.top(), actual.bottom()),
                        )
                    };
                    let (a, b) = (clamp(a), clamp(b));
                    let mut path = PathBuilder::stroke(px(2.0));
                    if kind == "core.crop" {
                        path.move_to(a);
                        path.line_to(point(b.x, a.y));
                        path.line_to(b);
                        path.line_to(point(a.x, b.y));
                        path.close();
                    } else if kind == "core.mask-radial-gradient" {
                        let radius = ((b.x - a.x).as_f32().hypot((b.y - a.y).as_f32())).max(1.0);
                        for i in 0..=64 {
                            let angle = i as f32 / 64.0 * std::f32::consts::TAU;
                            let p = a + point(px(angle.cos() * radius), px(angle.sin() * radius));
                            if i == 0 {
                                path.move_to(p);
                            } else {
                                path.line_to(p);
                            }
                        }
                    } else {
                        path.move_to(a);
                        path.line_to(b);
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(0xe8a33d));
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        div().v_flex().gap_1().w_full().text_xs()
            .child(self.status.clone())
            .when(self.driven(), |view| view.child("Geometry is driven by parameter ports. Hide those ports to draw or use presets."))
            .when_some(self.image.clone(), |view,image| view.child(div().id("node-image-helper").test_support().relative().w_full().h(px(180.0*self.zoom)).overflow_hidden().bg(rgb(0x141414))
                .when_some(thumbnail, |view,(width,height)| view.child(div().id("node-input-thumbnail").test_support().absolute()
                    .left(px((300.0*self.zoom-width)/2.0)).top(px((180.0*self.zoom-height)/2.0)).w(px(width)).h(px(height))
                    .child(img(image).absolute().w(px(width)).h(px(height)))))
                .child(overlay)
                .when(drawable && ready, |view| view.cursor(CursorStyle::Crosshair)
                    .on_mouse_down(MouseButton::Left,cx.listener(|this,event: &MouseDownEvent,window,cx| { cx.stop_propagation(); if let Some(owner)=this.owner.upgrade() {let focus=owner.read(cx).focus.clone(); window.focus(&focus,cx);} this.start=Some(event.position); this.end=Some(event.position); cx.notify(); }))
                    .on_mouse_move(cx.listener(|this,event: &MouseMoveEvent,_,cx| { if this.start.is_some() { this.end=Some(event.position); cx.notify(); } }))
                    .on_mouse_up(MouseButton::Left,cx.listener(|this,event: &MouseUpEvent,window,cx| this.finish(event.position,window,cx)))
                    .on_mouse_up_out(MouseButton::Left,cx.listener(|this,event: &MouseUpEvent,window,cx| this.finish(event.position,window,cx)))
                    .on_key_down(cx.listener(|this,event: &KeyDownEvent,_,cx| { if event.keystroke.key=="escape" {this.start=None;this.end=None;cx.notify();} }))
                )))
            .when(drawable, |view| view.child(if self.kind == "core.crop" { "Drag a crop rectangle; release to apply. Escape cancels." } else { "Drag a gradient; release to apply. Escape cancels." }))
            .when(self.kind=="core.crop", |view| view.child(div().h_flex().flex_wrap().gap_1()
                .children([("Full image",None),("Square",Some(1.0)),("3:2",Some(1.5)),("4:3",Some(4.0/3.0)),("16:9",Some(16.0/9.0))].into_iter().enumerate().map(|(i,(label,ratio))|
                    Button::new(("crop-preset",i)).small().label(label).disabled(!ready || self.dimensions.and_then(|dims| rawweave_gpui::geometry::crop_preset(dims,ratio)).is_none())
                        .on_click(cx.listener(move |this,_,window,cx| this.preset(ratio,window,cx)))))))
            .when(self.kind=="core.resize", |view| view.child(div().h_flex().flex_wrap().gap_1()
                .children([("Original",1.0),("Half",0.5),("Quarter",0.25),("Three-quarter",0.75)].into_iter().enumerate().map(|(i,(label,scale))|
                    Button::new(("resize-preset",i)).small().label(label).disabled(!ready).on_click(cx.listener(move |this,_,window,cx| this.resize_to(scale,window,cx)))))))
    }
}

impl Editor {
    fn curve_plot(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let field = &self.fields[index];
        let scalar = field.draft.ux.scalar_curve;
        let editable = field.draft.ux.point_curve;
        let points = if editable {
            match curve_points(field.draft.text(), scalar) {
                Ok(points) => points,
                Err(error) => return div().text_xs().child(error).into_any_element(),
            }
        } else {
            let gamma = field.draft.shown_number().unwrap_or(0.0);
            if !gamma.is_finite() || gamma <= 0.0 {
                return div()
                    .child("Enter a gamma greater than zero to show its curve.")
                    .into_any_element();
            }
            (0..=64)
                .map(|i| {
                    let x = i as f32 / 64.0;
                    (x, x.powf(1.0 / gamma as f32))
                })
                .collect()
        };
        let mut domain = [0.0f32, 1.0, 0.0, 1.0];
        for &(x, y) in &points {
            if scalar {
                domain[0] = domain[0].min(x);
                domain[1] = domain[1].max(x);
            }
            domain[2] = domain[2].min(y);
            domain[3] = domain[3].max(y);
        }
        if let Some((active, bounds)) = self.curve_gesture
            && active == index
        {
            domain = bounds;
        }
        let chosen = field.curve_index;
        let weak = cx.entity().downgrade();
        let plot = canvas(
            move |bounds, _, cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(field) = this.fields.get_mut(index) {
                        field.curve_bounds = Some(bounds);
                    }
                });
            },
            move |bounds, _, window, _| {
                let map = |(x, y): (f32, f32)| {
                    point(
                        bounds.left()
                            + px(curve_fraction(x, domain[0], domain[1])
                                * bounds.size.width.as_f32()),
                        bounds.bottom()
                            - px(curve_fraction(y, domain[2], domain[3])
                                * bounds.size.height.as_f32()),
                    )
                };
                let mut grid = PathBuilder::stroke(px(1.0));
                for i in 0..=4 {
                    let fraction = i as f32 / 4.0;
                    let x = bounds.left() + bounds.size.width * fraction;
                    grid.move_to(point(x, bounds.top()));
                    grid.line_to(point(x, bounds.bottom()));
                    let y = bounds.top() + bounds.size.height * fraction;
                    grid.move_to(point(bounds.left(), y));
                    grid.line_to(point(bounds.right(), y));
                }
                if let Ok(grid) = grid.build() {
                    window.paint_path(grid, rgb(0x434951));
                }
                let mut path = PathBuilder::stroke(px(2.0));
                for (i, &p) in points.iter().enumerate() {
                    if i == 0 {
                        path.move_to(map(p));
                    } else {
                        path.line_to(map(p));
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(0xf2efe6));
                }
                if editable {
                    for (i, &p) in points.iter().enumerate() {
                        window.paint_quad(fill(
                            Bounds::new(map(p) - point(px(3.0), px(3.0)), size(px(6.0), px(6.0))),
                            rgb(if i == chosen { 0xe8a33d } else { 0xf2efe6 }),
                        ));
                    }
                }
            },
        )
        .size_full();
        div()
            .v_flex()
            .gap_1()
            .w_full()
            .child(format!("Output {}…{}", domain[2], domain[3]))
            .child(
                div()
                    .id(("curve-plot", index))
                    .test_support()
                    .w_full()
                    .h(px(120.0))
                    .bg(rgb(0x141414))
                    .child(plot)
                    .when(editable, |view| {
                        view.cursor(CursorStyle::Crosshair)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    window.focus(&this.focus, cx);
                                    let Some(field) = this.fields.get_mut(index) else {
                                        return;
                                    };
                                    let Some(bounds) = field.curve_bounds else {
                                        return;
                                    };
                                    let Ok(points) = curve_points(
                                        field.draft.text(),
                                        field.draft.ux.scalar_curve,
                                    ) else {
                                        return;
                                    };
                                    let nearest =
                                        points.iter().enumerate().min_by(|(_, a), (_, b)| {
                                            let distance = |(x, y): (f32, f32)| {
                                                let x = bounds.left().as_f32()
                                                    + curve_fraction(x, domain[0], domain[1])
                                                        * bounds.size.width.as_f32();
                                                let y = bounds.bottom().as_f32()
                                                    - curve_fraction(y, domain[2], domain[3])
                                                        * bounds.size.height.as_f32();
                                                (x - event.position.x.as_f32())
                                                    .hypot(y - event.position.y.as_f32())
                                            };
                                            distance(**a).total_cmp(&distance(**b))
                                        });
                                    if let Some((selected, _)) = nearest {
                                        field.curve_index = selected;
                                        this.curve_gesture = Some((index, domain));
                                    }
                                    cx.notify();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |this, event: &MouseMoveEvent, window, cx| {
                                    this.drag_curve_point(index, event.position, window, cx)
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                                    this.drag_curve_point(index, event.position, window, cx);
                                    this.finish_curve_gesture(index, window, cx);
                                }),
                            )
                            .on_mouse_up_out(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                                    this.drag_curve_point(index, event.position, window, cx);
                                    this.finish_curve_gesture(index, window, cx);
                                }),
                            )
                    }),
            )
            .child(format!(
                "Input {}…{}{}",
                domain[0],
                domain[1],
                if editable {
                    " · Drag points; release to apply"
                } else {
                    ""
                }
            ))
            .into_any_element()
    }
    fn drag_curve_point(
        &mut self,
        index: usize,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((active, domain)) = self.curve_gesture else {
            return;
        };
        if active != index {
            return;
        }
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        let Some(bounds) = field.curve_bounds else {
            return;
        };
        let x = (f64::from(domain[0])
            + f64::from(
                ((pointer.x - bounds.left()).as_f32() / bounds.size.width.as_f32()).clamp(0.0, 1.0),
            ) * (f64::from(domain[1]) - f64::from(domain[0]))) as f32;
        let y = (f64::from(domain[2])
            + f64::from(
                ((bounds.bottom() - pointer.y).as_f32() / bounds.size.height.as_f32())
                    .clamp(0.0, 1.0),
            ) * (f64::from(domain[3]) - f64::from(domain[2]))) as f32;
        if let Ok(text) = rawweave_gpui::parameters::move_curve_point(
            field.draft.text(),
            field.draft.ux.scalar_curve,
            field.curve_index,
            x,
            y,
        ) {
            field.draft.set_text(text.clone());
            field.set_widget_value(text, window, cx);
            cx.notify();
        }
    }
    fn finish_curve_gesture(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .curve_gesture
            .is_some_and(|(active, _)| active == index)
        {
            self.curve_gesture = None;
            self.commit_field(index, window, cx);
            cx.notify();
        }
    }
}

impl Editor {
    fn parameter_references(&self, id: &str, _cx: &mut Context<Self>) -> AnyElement {
        let Some(node) = self.session.editor.graph().node(&CoreNodeId::from(id)) else {
            return div().into_any_element();
        };
        let mut values: BTreeMap<String, ParameterValue> = node
            .descriptor
            .parameters
            .iter()
            .map(|parameter| (parameter.id.clone(), parameter.default.clone()))
            .collect();
        values.extend(node.parameters.clone());
        let number = |key: &str, default: f32| match values.get(key) {
            Some(ParameterValue::Float(value)) => *value,
            Some(ParameterValue::Integer(value)) => *value as f32,
            None => default,
            _ => f32::NAN,
        };
        let mut view = div().v_flex().w_full().gap_1().text_xs();
        if matches!(
            node.type_id.as_str(),
            "core.levels" | "core.map-range" | "core.clamp"
        ) {
            view = view.child("Parameter transfer · input to output");
            view = match rawweave_gpui::parameters::transfer_points(&node.type_id, &values) {
                Ok(points) => view.child(reference_diagram(points)),
                Err(error) => view.child(div().text_color(rgb(0xffa480)).child(error)),
            };
        } else if node.type_id == "core.mask-color-qualifier" {
            let channels = [
                number("target_r", 1.0),
                number("target_g", 1.0),
                number("target_b", 1.0),
            ];
            if node.parameters.contains_key("color") {
                view = view.child("Legacy color overrides RGB; no swatch is shown.");
            } else if channels
                .iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
            {
                view = view.child(
                    div()
                        .h_flex()
                        .gap_2()
                        .child(div().w(px(32.0)).h(px(24.0)).bg(Rgba {
                            r: channels[0],
                            g: channels[1],
                            b: channels[2],
                            a: 1.0,
                        }))
                        .child(format!(
                            "RGB {}, {}, {}",
                            channels[0], channels[1], channels[2]
                        )),
                );
            } else {
                view = view.child("Reference requires finite RGB channels from 0 to 1.");
            }
            view = view.child("Reference sRGB swatch only, not a working-space transform.");
        } else if node.type_id == "pro.color-zones" {
            view = view.child(hue_reference(
                "Target hue",
                number("hue", 0.0),
                Some(number("width", 0.2)),
                None,
            ));
        } else if node.type_id == "pro.split-toning" {
            view = view
                .child(hue_reference(
                    "Shadow hue",
                    number("shadow_hue", 0.6),
                    None,
                    Some(number("shadow_saturation", 0.0)),
                ))
                .child(hue_reference(
                    "Highlight hue",
                    number("highlight_hue", 0.1),
                    None,
                    Some(number("highlight_saturation", 0.0)),
                ));
        } else {
            return div().into_any_element();
        }
        view.child("Applied parameter reference; exposed inputs may override these values.")
            .into_any_element()
    }
}
fn curve_fraction(value: f32, low: f32, high: f32) -> f32 {
    ((f64::from(value) - f64::from(low)) / (f64::from(high) - f64::from(low))) as f32
}
fn reference_diagram(points: Vec<(f32, f32)>) -> AnyElement {
    let mut domain = [0.0f64, 1.0, 0.0, 1.0];
    for &(x, y) in &points {
        domain[0] = domain[0].min(f64::from(x));
        domain[1] = domain[1].max(f64::from(x));
        domain[2] = domain[2].min(f64::from(y));
        domain[3] = domain[3].max(f64::from(y));
    }
    div()
        .w_full()
        .h(px(100.0))
        .bg(rgb(0x141414))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let map = |(x, y): (f32, f32)| {
                        point(
                            bounds.left()
                                + bounds.size.width
                                    * ((f64::from(x) - domain[0]) / (domain[1] - domain[0])) as f32,
                            bounds.bottom()
                                - bounds.size.height
                                    * ((f64::from(y) - domain[2]) / (domain[3] - domain[2])) as f32,
                        )
                    };
                    let mut grid = PathBuilder::stroke(px(1.0));
                    for i in 0..=4 {
                        let fraction = i as f32 / 4.0;
                        let x = bounds.left() + bounds.size.width * fraction;
                        grid.move_to(point(x, bounds.top()));
                        grid.line_to(point(x, bounds.bottom()));
                        let y = bounds.top() + bounds.size.height * fraction;
                        grid.move_to(point(bounds.left(), y));
                        grid.line_to(point(bounds.right(), y));
                    }
                    if let Ok(path) = grid.build() {
                        window.paint_path(path, rgb(0x434951));
                    }
                    let mut path = PathBuilder::stroke(px(2.0));
                    for (i, &p) in points.iter().enumerate() {
                        if i == 0 {
                            path.move_to(map(p));
                        } else {
                            path.line_to(map(p));
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(0xf2efe6));
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}
fn hue_reference(
    label: &str,
    hue: f32,
    radius: Option<f32>,
    saturation: Option<f32>,
) -> AnyElement {
    if !hue.is_finite()
        || !(0.0..=1.0).contains(&hue)
        || radius.is_some_and(|radius| !radius.is_finite() || !(0.001..=0.5).contains(&radius))
        || saturation
            .is_some_and(|saturation| !saturation.is_finite() || !(0.0..=1.0).contains(&saturation))
    {
        return div()
            .child("Enter finite hue (0–360°), radius (0.36–180°), and saturation (0–100%).")
            .into_any_element();
    }
    div()
        .v_flex()
        .gap_1()
        .w_full()
        .child(format!(
            "{label}: {:.1}°{}",
            hue * 360.0,
            radius
                .map(|radius| format!(" · radius {:.1}°", radius * 360.0))
                .or_else(|| saturation
                    .map(|saturation| format!(" · saturation {:.1}%", saturation * 100.0)))
                .unwrap_or_default()
        ))
        .child(
            div().w_full().h(px(24.0)).child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for i in 0..72 {
                            let left = bounds.left() + bounds.size.width * (i as f32 / 72.0);
                            let right = bounds.left() + bounds.size.width * ((i + 1) as f32 / 72.0);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(left, bounds.top()),
                                    size(right - left, px(14.0)),
                                ),
                                hsla(i as f32 / 72.0, 1.0, 0.5, 1.0),
                            ));
                            let center = (i as f32 + 0.5) / 72.0;
                            let distance = (center - hue % 1.0).abs();
                            let distance = distance.min(1.0 - distance);
                            if radius.is_some_and(|radius| distance <= radius) {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(left, bounds.top() + px(18.0)),
                                        size(right - left, px(4.0)),
                                    ),
                                    rgb(0xf2efe6),
                                ));
                            }
                        }
                        let marker = bounds.left() + bounds.size.width * (hue % 1.0);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(marker - px(1.0), bounds.top()),
                                size(px(2.0), px(16.0)),
                            ),
                            rgb(0x141414),
                        ));
                    },
                )
                .size_full(),
            ),
        )
        .child("0° · 180° · 360° · saturated hue reference, not a color-managed result.")
        .into_any_element()
}
