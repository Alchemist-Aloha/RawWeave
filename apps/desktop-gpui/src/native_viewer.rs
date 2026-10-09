//! Native display surfaces. Graph processing remains in Session/Rust nodes.
use crate::native_scopes::{AnalyzedFrame, Scopes, render_frame, upload_raster};
use crate::native_theme as t;
use gpui_kit::Size;
use gpui_kit::component::{
    button::Button,
    checkbox::Checkbox,
    menu::{DropdownMenu, PopupMenuItem},
    slider::{Slider, SliderEvent, SliderState},
    *,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rawweave_gpui::viewer::{
    Comparison, SurfaceTransform, Target, ViewerModel, display_difference,
};
use rawweave_gpui::{PreviewFrame, Session, Source, preview_type, spatial::MaskDisplay};
use rawweave_image::Dimensions;
use rawweave_rendering::{CancellationToken, scopes::ScopeAnalysis};
use std::{sync::Arc, time::Duration};

struct Pane {
    image: Option<Arc<RenderImage>>,
    frame: Option<Arc<PreviewFrame>>,
    analysis: Option<Arc<ScopeAnalysis>>,
    clip_image: Option<Arc<RenderImage>>,
    image_target: Option<Target>,
    viewport: Size<Pixels>,
    ratio: f32,
    zoom: Option<f32>,
    pan: Point<Pixels>,
    drag: Option<Point<Pixels>>,
    token: CancellationToken,
    generation: u64,
    frame_id: u64,
    loaded_mip: Option<u8>,
    requested_mip: Option<u8>,
    status: String,
    loading: bool,
    mask_display: MaskDisplay,
}
impl Default for Pane {
    fn default() -> Self {
        Self {
            image: None,
            frame: None,
            analysis: None,
            clip_image: None,
            image_target: None,
            viewport: size(px(600.0), px(600.0)),
            ratio: 1.0,
            zoom: None,
            pan: point(px(0.0), px(0.0)),
            drag: None,
            token: CancellationToken::new(),
            generation: 0,
            frame_id: 0,
            loaded_mip: None,
            requested_mip: None,
            status: "Select a source image".into(),
            loading: false,
            mask_display: MaskDisplay::default(),
        }
    }
}
impl Pane {
    fn zoom(&self, source: Option<Dimensions>) -> f32 {
        self.zoom.unwrap_or_else(|| {
            self.frame
                .as_ref()
                .map(|f| f.full_dimensions)
                .or(source)
                .map_or(1.0, |dims| {
                    (f32::from(self.viewport.width) / dims.width as f32)
                        .min(f32::from(self.viewport.height) / dims.height as f32)
                })
        })
    }
    fn mip(&self, source: Option<Dimensions>) -> u8 {
        ((1.0 / (self.zoom(source) * self.ratio).max(0.0001))
            .log2()
            .floor()
            .max(0.0) as u8)
            .min(6)
    }
    fn clear(&mut self, window: &mut Window) {
        self.token.cancel();
        if let Some(image) = self.image.take() {
            let _ = window.drop_image(image);
        }
        self.frame = None;
        self.analysis = None;
        if let Some(image) = self.clip_image.take() {
            let _ = window.drop_image(image);
        }
        self.image_target = None;
        self.loaded_mip = None;
        self.requested_mip = None;
        self.loading = false;
    }
}

#[derive(Clone, Copy, PartialEq)]
struct DifferenceKey {
    frames: [u64; 2],
    dimensions: Dimensions,
    transforms: [SurfaceTransform; 2],
    clipping: bool,
}

pub struct Viewers {
    session: Session,
    model: ViewerModel,
    panes: [Pane; 2],
    active: usize,
    scopes: Entity<Scopes>,
    scopes_visible: bool,
    clipping: bool,
    wipe: f32,
    wipe_slider: Entity<SliderState>,
    _wipe_subscription: Subscription,
    wipe_drag: bool,
    /// The wipe surface's rectangle, so a dragged edge can become a fraction of it.
    stage_bounds: Option<Bounds<Pixels>>,
    blink_b: bool,
    blink_epoch: u64,
    difference: Option<Arc<RenderImage>>,
    difference_key: Option<DifferenceKey>,
    difference_running: bool,
    difference_error: Option<String>,
    enabled: bool,
}
impl Viewers {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let wipe_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(0.5)
        });
        let subscription = cx.subscribe(&wipe_slider, |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            let fraction = value.start().clamp(0.0, 1.0);
            if (fraction - this.wipe).abs() > f32::EPSILON {
                this.wipe = fraction;
            }
            cx.notify();
        });
        Self {
            session: Session::default(),
            model: ViewerModel::default(),
            panes: [Pane::default(), Pane::default()],
            active: 0,
            scopes: cx.new(|_| Scopes::new()),
            scopes_visible: true,
            clipping: false,
            wipe: 0.5,
            wipe_slider,
            _wipe_subscription: subscription,
            wipe_drag: false,
            stage_bounds: None,
            blink_b: false,
            blink_epoch: 0,
            difference: None,
            difference_key: None,
            difference_running: false,
            difference_error: None,
            enabled: true,
        }
    }
    pub fn set_session(
        &mut self,
        session: Session,
        reset: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session = session;
        if reset {
            for pane in &mut self.panes {
                pane.clear(window);
                pane.pan = point(px(0.0), px(0.0));
                pane.zoom = None;
            }
            let mode = self.model.mode;
            self.model = ViewerModel::default();
            self.model.mode = mode;
            self.clear_difference(window);
            // A new source under an overlay starts registered, not as two fits.
            self.link_view(self.active, window, cx);
        }
        self.model.reconcile(&self.session);
        self.request_all(window, cx);
    }
    pub fn set_enabled(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.blink_epoch = self.blink_epoch.saturating_add(1);
        if enabled {
            self.request_all(window, cx);
            if self.model.mode == Comparison::Blink {
                self.start_blink(window, cx);
            }
        } else {
            for pane in &mut self.panes {
                pane.clear(window);
            }
            self.clear_difference(window);
        }
        self.scopes.update(cx, |scopes, cx| {
            scopes.set_enabled(enabled && self.scopes_visible, window, cx)
        });
        cx.notify();
    }
    pub fn export_session(&self) -> Session {
        let mut session = self.session.clone();
        if let Some(target) = self.model.targets.get(self.active).and_then(Option::as_ref) {
            session.target = target.clone();
        }
        session
    }
    pub fn cancel_all(&mut self) {
        for pane in &mut self.panes {
            pane.token.cancel();
            pane.loading = false;
            pane.requested_mip = None;
        }
    }
    fn clear_difference(&mut self, window: &mut Window) {
        if let Some(image) = self.difference.take() {
            let _ = window.drop_image(image);
        }
        self.difference_key = None;
        self.difference_error = None;
    }
    fn visible(&self, pane: usize) -> bool {
        pane == 0 || self.model.mode != Comparison::Single
    }
    fn request_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        for pane in 0..2 {
            if self.visible(pane) {
                self.request(pane, window, cx);
            } else if let Some(pane) = self.panes.get_mut(pane) {
                pane.clear(window);
            }
        }
        self.sync_scopes(window, cx);
        cx.notify();
    }
    fn sync_scopes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let analysis = self
            .panes
            .get(self.active)
            .and_then(|pane| pane.analysis.clone());
        let label = self
            .model
            .targets
            .get(self.active)
            .and_then(Option::as_ref)
            .map(|(node, port)| {
                format!(
                    "Viewer {} · {node} · {port}",
                    if self.active == 0 { "A" } else { "B" }
                )
            })
            .unwrap_or_default();
        self.scopes.update(cx, |scopes, cx| {
            scopes.set_analysis(analysis, label, window, cx)
        });
    }
    fn request(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.model.targets.get(index).cloned().flatten() else {
            if let Some(pane) = self.panes.get_mut(index) {
                pane.clear(window);
                pane.status = "Select a source image".into();
            }
            self.clear_difference(window);
            cx.notify();
            return;
        };
        let dimensions = self.session.source.as_ref().map(Source::dimensions);
        let Some(pane) = self.panes.get_mut(index) else {
            return;
        };
        pane.token.cancel();
        pane.token = CancellationToken::new();
        let Some(generation) = pane.generation.checked_add(1) else {
            pane.status = "Preview counter exhausted".into();
            return;
        };
        pane.generation = generation;
        if pane.image_target.as_ref() != Some(&target) {
            pane.clear(window);
            pane.token = CancellationToken::new();
        }
        let mip = pane.mip(dimensions);
        let coarse =
            if dimensions.is_some_and(|d| u64::from(d.width) * u64::from(d.height) >= 1_000_000) {
                (mip + 1).min(6)
            } else {
                mip
            };
        pane.loading = true;
        pane.requested_mip = Some(mip);
        pane.status = "Rendering…".into();
        let token = pane.token.clone();
        let mut snapshot = self.session.clone();
        snapshot.target = target.clone();
        snapshot.mask_display = pane.mask_display;
        let first = snapshot.clone();
        let first_token = token.clone();
        let clipping = self.clipping;
        let task = cx
            .background_executor()
            .spawn(async move { render_frame(&first, coarse, &first_token, clipping) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let accepted = this
                .update_in(cx, |this, window, cx| {
                    this.accept(index, generation, coarse, result, window, cx)
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
            let task = cx
                .background_executor()
                .spawn(async move { render_frame(&snapshot, mip, &token, clipping) });
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.accept(index, generation, mip, result, window, cx);
            });
        })
        .detach();
        if self.panes.get(index).is_some_and(|p| p.frame.is_none()) {
            self.clear_difference(window);
        }
        self.sync_scopes(window, cx);
        cx.notify();
    }
    fn accept(
        &mut self,
        index: usize,
        generation: u64,
        mip: u8,
        result: Result<AnalyzedFrame, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes.get_mut(index) else {
            return false;
        };
        if pane.generation != generation || pane.token.is_cancelled() {
            return false;
        }
        match result.and_then(|result| {
            let image = upload(&result.frame)?;
            let clip = result.clipping.map(upload_raster).transpose()?;
            Ok((result.frame, result.analysis, image, clip))
        }) {
            Ok((frame, analysis, image, clip)) => {
                if let Some(previous) = pane.image.replace(image) {
                    let _ = window.drop_image(previous);
                }
                pane.status = format!(
                    "{} × {} · mip {}",
                    frame.full_dimensions.width, frame.full_dimensions.height, mip
                );
                pane.frame = Some(Arc::new(frame));
                pane.analysis = Some(analysis);
                if let Some(old) = pane.clip_image.take() {
                    let _ = window.drop_image(old);
                }
                pane.clip_image = clip;
                pane.image_target = self.model.targets.get(index).cloned().flatten();
                pane.loaded_mip = Some(mip);
                pane.loading = pane.requested_mip != Some(mip);
                if !pane.loading {
                    pane.requested_mip = None;
                }
                pane.frame_id = pane.frame_id.saturating_add(1);
                self.refresh_difference(window, cx);
                self.sync_scopes(window, cx);
                cx.notify();
                true
            }
            Err(error) => {
                pane.status = format!("Preview failed: {error}");
                pane.loading = false;
                pane.requested_mip = None;
                cx.notify();
                false
            }
        }
    }
    fn resize(
        &mut self,
        index: usize,
        viewport: Size<Pixels>,
        ratio: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self.session.source.as_ref().map(Source::dimensions);
        let Some(pane) = self.panes.get_mut(index) else {
            return;
        };
        if viewport.width <= px(0.0) || viewport.height <= px(0.0) {
            return;
        }
        if (f32::from(pane.viewport.width - viewport.width)).abs() <= 1.0
            && (f32::from(pane.viewport.height - viewport.height)).abs() <= 1.0
            && pane.ratio == ratio
        {
            return;
        }
        pane.viewport = viewport;
        pane.ratio = ratio;
        let mip = pane.mip(source);
        if pane.requested_mip != Some(mip) && pane.loaded_mip != Some(mip) {
            self.request(index, window, cx);
        }
        self.refresh_difference(window, cx);
        cx.notify();
    }
    fn set_mode(&mut self, mode: Comparison, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.mode == mode {
            return;
        }
        self.model.set_mode(mode, &self.session);
        self.blink_b = false;
        self.blink_epoch = self.blink_epoch.saturating_add(1);
        self.clear_difference(window);
        if mode == Comparison::Single {
            self.active = 0;
        }
        // Entering an overlay registers both sources on the active pane's view.
        self.link_view(self.active, window, cx);
        self.request_all(window, cx);
        if mode == Comparison::Blink && self.enabled {
            self.start_blink(window, cx);
        }
    }
    fn start_blink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let epoch = self.blink_epoch;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(450))
                    .await;
                let keep = this
                    .update_in(cx, |this, _, cx| {
                        if !this.enabled
                            || this.model.mode != Comparison::Blink
                            || this.blink_epoch != epoch
                        {
                            return false;
                        }
                        this.blink_b = !this.blink_b;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }
    fn difference_plan(&self) -> Option<(DifferenceKey, [Arc<PreviewFrame>; 2])> {
        if self.model.mode != Comparison::Difference {
            return None;
        }
        let [a, b] = &self.panes;
        let frames = [a.frame.clone()?, b.frame.clone()?];
        let source = self.session.source.as_ref().map(Source::dimensions);
        let dimensions = Dimensions::new(
            (f32::from(a.viewport.width) * a.ratio).round().max(1.0) as u32,
            (f32::from(a.viewport.height) * a.ratio).round().max(1.0) as u32,
        );
        let transforms = [a, b].map(|pane| SurfaceTransform {
            zoom: pane.zoom(source) * a.ratio,
            pan: [
                f32::from(pane.pan.x) * a.ratio,
                f32::from(pane.pan.y) * a.ratio,
            ],
        });
        Some((
            DifferenceKey {
                frames: [a.frame_id, b.frame_id],
                dimensions,
                transforms,
                clipping: self.clipping,
            },
            frames,
        ))
    }
    fn refresh_difference(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((key, frames)) = self.difference_plan() else {
            return;
        };
        if self.difference_running || self.difference_key == Some(key) {
            return;
        }
        self.difference_running = true;
        let analyses = self.panes.each_ref().map(|pane| pane.analysis.clone());
        let task = cx.background_executor().spawn(async move {
            let [a, b] = frames;
            let [aa, ba] = analyses;
            let frames = [
                comparison_frame(a, aa, key.clipping)?,
                comparison_frame(b, ba, key.clipping)?,
            ];
            display_difference(
                &frames[0],
                &frames[1],
                key.dimensions,
                key.transforms[0],
                key.transforms[1],
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.difference_running = false;
                if this
                    .difference_plan()
                    .is_some_and(|(current, _)| current == key)
                {
                    match result.and_then(|frame| upload(&frame)) {
                        Ok(image) => {
                            this.clear_difference(window);
                            this.difference = Some(image);
                            this.difference_key = Some(key);
                        }
                        Err(error) => {
                            this.difference_error = Some(error);
                            this.difference_key = Some(key);
                        }
                    }
                    cx.notify();
                } else {
                    this.refresh_difference(window, cx);
                }
            });
        })
        .detach();
    }
    fn controls(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let label = if index == 0 { "A" } else { "B" };
        let targets: Vec<_> = self
            .session
            .editor
            .graph()
            .nodes()
            .values()
            .flat_map(|node| {
                node.descriptor
                    .outputs
                    .iter()
                    .filter(|port| preview_type(&port.data_type))
                    .map(|port| {
                        (
                            (node.id.as_str().to_owned(), port.id.clone()),
                            format!(
                                "{} · {}",
                                self.session.node_label(node.id.as_str()),
                                port.name
                            ),
                        )
                    })
            })
            .collect();
        let selected = self.model.targets.get(index).and_then(Option::as_ref);
        let title = targets
            .iter()
            .find(|(target, _)| Some(target) == selected)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Select image output".into());
        let weak = cx.entity().downgrade();
        let menu = Button::new(SharedString::from(format!("viewer-{label}-target")))
            .w_full()
            .label(title)
            .dropdown_menu(move |mut menu, _, _| {
                for (target, title) in &targets {
                    let weak = weak.clone();
                    let target = target.clone();
                    menu = menu.item(PopupMenuItem::new(title.clone()).on_click(
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if this
                                    .model
                                    .select(index, target.clone(), &this.session)
                                    .is_ok()
                                {
                                    this.active = index;
                                    this.request(index, window, cx);
                                }
                            });
                        },
                    ));
                }
                menu
            });
        let mask_target = selected.is_some_and(|(id, port)| {
            self.session
                .editor
                .graph()
                .node(&rawweave_core::NodeId::from(id.as_str()))
                .is_some_and(|node| {
                    node.descriptor.outputs.iter().any(|output| {
                        output.id == *port
                            && matches!(output.data_type.as_str(), "core.Mask" | "core.MaskSet")
                    })
                })
        });
        let spatial_note = selected.and_then(|(id, port)| {
            self.session.editor.graph().node(&rawweave_core::NodeId::from(id.as_str()))
                .and_then(|node| node.descriptor.outputs.iter().find(|output| output.id == *port))
                .and_then(|output| match output.data_type.as_str() {
                    "core.Mask" | "core.MaskSet" => Some("Mask coverage; colored mode uses coverage as alpha over the viewer ground."),
                    "core.LabelMap" => Some("Categorical label colors, not measured image RGB."),
                    "core.ConfidenceMap" => Some("Confidence: 0–1 grayscale, not image brightness."),
                    "core.DepthMap" => Some("Depth normalized to this output's minimum/maximum; constant depth is midgray."),
                    "core.RegionSet" => Some("Regions: opaque borders and translucent interiors."),
                    _ => None,
                })
        });
        let mask_display = self
            .panes
            .get(index)
            .map_or(MaskDisplay::default(), |pane| pane.mask_display);
        let loading = self.panes.get(index).is_some_and(|p| p.loading);
        div()
            .id(SharedString::from(format!("viewer-controls-{label}")))
            .test_support()
            .v_flex()
            .min_w_0()
            .gap_1()
            .p_2()
            .flex_shrink_0()
            .child(
                Button::new(SharedString::from(format!("active-{label}")))
                    .label(format!(
                        "Viewer {label}{}",
                        if self.active == index {
                            " · active"
                        } else {
                            ""
                        }
                    ))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.active = index;
                        this.sync_scopes(window, cx);
                        cx.notify();
                    })),
            )
            .child(menu)
            .when_some(spatial_note, |view, note| {
                view.child(div().text_sm().child(note))
            })
            .when(mask_target, |view| {
                view.child(
                    Checkbox::new(SharedString::from(format!("mask-overlay-{label}")))
                        .label("Colored mask overlay")
                        .checked(mask_display == MaskDisplay::Overlay)
                        .on_click(cx.listener(move |this, checked, window, cx| {
                            if let Some(pane) = this.panes.get_mut(index) {
                                pane.mask_display = if *checked {
                                    MaskDisplay::Overlay
                                } else {
                                    MaskDisplay::Grayscale
                                };
                                // A mode change must not display an old, differently colored texture.
                                pane.clear(window);
                            }
                            this.request(index, window, cx);
                        })),
                )
            })
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        Button::new(SharedString::from(format!("fit-{label}")))
                            .label("Fit")
                            .on_click(
                                cx.listener(move |this, _, window, cx| this.fit(index, window, cx)),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("actual-{label}")))
                            .label("100%")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.zoom(index, Some(1.0), window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("out-{label}")))
                            .label("−")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.adjust_zoom(index, 1.0 / 1.1, window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("in-{label}")))
                            .label("+")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.adjust_zoom(index, 1.1, window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("retry-{label}")))
                            .label(if loading { "Cancel" } else { "Refresh" })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(pane) = this.panes.get_mut(index)
                                    && pane.loading
                                {
                                    pane.token.cancel();
                                    pane.loading = false;
                                    pane.requested_mip = None;
                                    pane.status = "Cancelled".into();
                                    cx.notify();
                                    return;
                                }
                                this.request(index, window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
    /// An overlay draws both sources into one viewport, so they must share one
    /// view: otherwise a wipe cuts two differently framed images and a blink
    /// jumps. The moved pane is the source of truth, and its implicit fit zoom
    /// becomes explicit for both.
    fn link_view(&mut self, from: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.mode.overlay() {
            return;
        }
        let source = self.session.source.as_ref().map(Source::dimensions);
        let Some(moved) = self.panes.get(from) else {
            return;
        };
        let (zoom, pan) = (Some(moved.zoom(source)), moved.pan);
        let mut request = None;
        for index in 0..2 {
            if index == from {
                continue;
            }
            let Some(pane) = self.panes.get_mut(index) else {
                continue;
            };
            pane.zoom = zoom;
            pane.pan = pan;
            let mip = pane.mip(source);
            if pane.loaded_mip != Some(mip) && pane.requested_mip != Some(mip) {
                request = Some(index);
            }
        }
        if let Some(index) = request {
            self.request(index, window, cx);
        }
    }
    fn fit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get_mut(index) {
            pane.pan = point(px(0.0), px(0.0));
        }
        self.zoom(index, None, window, cx);
    }
    fn zoom(
        &mut self,
        index: usize,
        zoom: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self.session.source.as_ref().map(Source::dimensions);
        if let Some(pane) = self.panes.get_mut(index) {
            if zoom.is_some_and(|z| !z.is_finite()) {
                return;
            }
            let fit = pane
                .frame
                .as_ref()
                .map(|f| f.full_dimensions)
                .or(source)
                .map_or(1.0, |dims| {
                    (f32::from(pane.viewport.width) / dims.width as f32)
                        .min(f32::from(pane.viewport.height) / dims.height as f32)
                });
            pane.zoom = zoom.map(|z| z.clamp((fit / 4.0).clamp(0.000001, 0.1), 8.0));
            let mip = pane.mip(source);
            if pane.loaded_mip != Some(mip) && pane.requested_mip != Some(mip) {
                self.request(index, window, cx);
            }
        }
        self.link_view(index, window, cx);
        self.refresh_difference(window, cx);
        cx.notify();
    }
    fn adjust_zoom(
        &mut self,
        index: usize,
        factor: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.panes.get(index) {
            let zoom = pane.zoom(self.session.source.as_ref().map(Source::dimensions));
            self.zoom(index, Some(zoom * factor), window, cx);
        }
    }
    fn image(&self, index: usize) -> AnyElement {
        let Some(pane) = self.panes.get(index) else {
            return div().into_any_element();
        };
        let mut stage = div()
            .absolute()
            .size_full()
            .overflow_hidden()
            .bg(rgb(t::JUDGE_SUNK));
        if let (Some(image), Some(frame)) = (&pane.image, &pane.frame) {
            let zoom = pane.zoom(self.session.source.as_ref().map(Source::dimensions));
            let width = frame.full_dimensions.width as f32 * zoom;
            let height = frame.full_dimensions.height as f32 * zoom;
            stage = stage.child(
                img(image.clone())
                    .absolute()
                    .w(px(width))
                    .h(px(height))
                    .left((pane.viewport.width - px(width)) / 2.0 + pane.pan.x)
                    .top((pane.viewport.height - px(height)) / 2.0 + pane.pan.y),
            );
            if self.clipping
                && let Some(overlay) = &pane.clip_image
            {
                stage = stage.child(
                    img(overlay.clone())
                        .absolute()
                        .w(px(width))
                        .h(px(height))
                        .left((pane.viewport.width - px(width)) / 2.0 + pane.pan.x)
                        .top((pane.viewport.height - px(height)) / 2.0 + pane.pan.y),
                );
            }
        } else {
            stage = stage.child(div().p_3().text_sm().child(pane.status.clone()));
        }
        stage.into_any_element()
    }
    fn interaction(&self, index: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id(SharedString::from(format!("viewer-{index}-interaction")))
            .relative()
            .size_full()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let index = if this.model.mode.overlay() {
                        this.active
                    } else {
                        index
                    };
                    this.active = index;
                    if this.grab_wipe_edge(event.position) {
                        this.sync_scopes(window, cx);
                        cx.notify();
                        return;
                    }
                    if let Some(pane) = this.panes.get_mut(index) {
                        pane.drag = Some(event.position);
                    }
                    this.sync_scopes(window, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                    if this.drag_wipe_edge(event.position, window, cx) {
                        return;
                    }
                    let index = if this.model.mode.overlay() {
                        this.active
                    } else {
                        index
                    };
                    if let Some(pane) = this.panes.get_mut(index)
                        && let Some(previous) = pane.drag
                    {
                        pane.pan += event.position - previous;
                        pane.drag = Some(event.position);
                        this.link_view(index, window, cx);
                        this.refresh_difference(window, cx);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    for pane in &mut this.panes {
                        pane.drag = None;
                    }
                    this.wipe_drag = false;
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    for pane in &mut this.panes {
                        pane.drag = None;
                    }
                    this.wipe_drag = false;
                }),
            )
            .on_scroll_wheel(
                cx.listener(move |this, event: &ScrollWheelEvent, window, cx| {
                    let index = if this.model.mode.overlay() {
                        this.active
                    } else {
                        index
                    };
                    this.adjust_zoom(
                        index,
                        (f32::from(event.delta.pixel_delta(px(20.0)).y) * 0.002).exp(),
                        window,
                        cx,
                    );
                }),
            )
    }
    /// How close to the wipe edge a press counts as grabbing it, in pixels.
    const WIPE_GRAB: f32 = 12.0;

    /// Whether this press starts a wipe-edge drag instead of a pan.
    fn grab_wipe_edge(&mut self, position: Point<Pixels>) -> bool {
        if self.model.mode != Comparison::Wipe {
            return false;
        }
        let Some(bounds) = self.stage_bounds else {
            return false;
        };
        let edge = bounds.left() + bounds.size.width * self.wipe;
        if bounds.size.width <= px(0.0)
            || (position.x - edge).abs() > px(Self::WIPE_GRAB)
            || position.y < bounds.top()
            || position.y > bounds.bottom()
        {
            return false;
        }
        self.wipe_drag = true;
        true
    }

    /// Move the wipe edge while it is held. Returns whether the event was consumed.
    fn drag_wipe_edge(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.wipe_drag {
            return false;
        }
        let Some(bounds) = self.stage_bounds else {
            return false;
        };
        if bounds.size.width <= px(0.0) {
            return false;
        }
        let fraction =
            (f32::from(position.x - bounds.left()) / f32::from(bounds.size.width)).clamp(0.0, 1.0);
        self.set_wipe(fraction, window, cx);
        true
    }

    /// One write path for the wipe, so the edge and the slider never disagree.
    fn set_wipe(&mut self, fraction: f32, window: &mut Window, cx: &mut Context<Self>) {
        let fraction = fraction.clamp(0.0, 1.0);
        if (fraction - self.wipe).abs() <= f32::EPSILON {
            return;
        }
        self.wipe = fraction;
        self.wipe_slider
            .update(cx, |state, cx| state.set_value(fraction, window, cx));
        cx.notify();
    }
    fn stage(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.resize(index, bounds.size, window.scale_factor(), window, cx)
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        self.interaction(index, cx)
            .test_support()
            .child(measure)
            .child(self.image(index))
            .into_any_element()
    }
    fn pane(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .m_1()
            .rounded(px(8.0))
            .bg(rgb(t::JUDGE_RAISE))
            .border_1()
            .border_color(rgb(t::JUDGE_LINE))
            .overflow_hidden()
            .child(self.controls(index, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.stage(index, cx)),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .flex_shrink_0()
                    .font_family(t::Face::Readout.family())
                    .text_size(t::Face::Readout.size())
                    .text_color(rgb(t::JUDGE_INK_DIM))
                    .child(
                        self.panes
                            .get(index)
                            .map(|p| p.status.clone())
                            .unwrap_or_default(),
                    ),
            )
            .into_any_element()
    }
    fn overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let weak_for_bounds = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, window, cx| {
                let _ = weak_for_bounds.update(cx, |this, cx| {
                    this.stage_bounds = Some(bounds);
                    for index in 0..2 {
                        this.resize(index, bounds.size, window.scale_factor(), window, cx);
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let mut surface = self
            .interaction(self.active, cx)
            .test_support()
            .child(measure)
            .bg(rgb(t::JUDGE_SUNK));
        match self.model.mode {
            Comparison::Wipe => {
                let viewport = self.panes[0].viewport;
                surface = surface
                    .child(self.image(1))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(relative(self.wipe))
                            .overflow_hidden()
                            .child(
                                div()
                                    .relative()
                                    .w(viewport.width)
                                    .h(viewport.height)
                                    .child(self.image(0)),
                            ),
                    )
                    // One edge for both sides: the same 1px mark is the grab handle,
                    // widened only for the pointer, never for the eye.
                    .child(
                        div()
                            .id("wipe-edge")
                            .test_support()
                            .absolute()
                            .left(relative(self.wipe))
                            .top_0()
                            .h_full()
                            .w(px(Self::WIPE_GRAB))
                            .ml(px(-Self::WIPE_GRAB / 2.0))
                            .cursor(gpui_kit::CursorStyle::ResizeColumn)
                            .child(
                                div()
                                    .absolute()
                                    .left(px(Self::WIPE_GRAB / 2.0))
                                    .top_0()
                                    .h_full()
                                    .w(px(1.0))
                                    .bg(rgb(t::JUDGE_INK)),
                            ),
                    );
            }
            Comparison::Blink => {
                surface = surface.child(self.image(usize::from(self.blink_b)));
            }
            Comparison::Difference => {
                if let Some(image) = &self.difference {
                    surface = surface.child(img(image.clone()).absolute().size_full());
                } else {
                    surface = surface.child(
                        div().p_3().child(
                            self.difference_error
                                .clone()
                                .unwrap_or_else(|| "Waiting for A and B…".into()),
                        ),
                    );
                }
            }
            _ => {}
        }
        let source_label = match self.model.mode {
            Comparison::Blink => {
                if self.blink_b {
                    "B"
                } else {
                    "A"
                }
            }
            Comparison::Wipe => "A left · B right",
            _ => "Absolute display difference A / B",
        };
        surface = surface.child(
            div()
                .absolute()
                .top(px(8.0))
                .left(px(8.0))
                .px_2()
                .py_1()
                .text_xs()
                .bg(rgb(t::JUDGE_GROUND))
                .child(source_label),
        );
        div()
            .v_flex()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .h_flex()
                    .min_w_0()
                    .child(div().flex_1().min_w_0().child(self.controls(0, cx)))
                    .child(div().flex_1().min_w_0().child(self.controls(1, cx))),
            )
            .when(self.model.mode == Comparison::Wipe, |view| {
                view.child(
                    div()
                        .h_flex()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .child(div().text_xs().child(format!(
                            "Wipe A {:.0}% / B {:.0}%",
                            self.wipe * 100.0,
                            (1.0 - self.wipe) * 100.0
                        )))
                        .child(Slider::new(&self.wipe_slider).w_full()),
                )
            })
            .child(div().relative().flex_1().min_h_0().child(surface))
            .child(div().px_2().py_1().text_xs().child(format!(
                "A: {} · B: {}",
                self.panes[0].status, self.panes[1].status
            )))
            .into_any_element()
    }
}
impl Render for Viewers {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.model.mode;
        let weak = cx.entity().downgrade();
        let menu = Button::new("comparison-mode")
            .label(mode.label())
            .dropdown_menu(move |mut menu, _, _| {
                for comparison in Comparison::ALL {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(comparison.label())
                            .checked(mode == comparison)
                            .on_click(move |_, window, cx| {
                                let _ = weak
                                    .update(cx, |this, cx| this.set_mode(comparison, window, cx));
                            }),
                    );
                }
                menu
            });
        let body = if mode.overlay() {
            self.overlay(cx)
        } else if mode == Comparison::Single {
            self.pane(0, cx)
        } else {
            div()
                .flex()
                .when(mode == Comparison::Vertical, |v| v.flex_col())
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(self.pane(0, cx))
                .child(self.pane(1, cx))
                .into_any_element()
        };
        let min_height = match mode {
            Comparison::Single => 600.0,
            Comparison::Vertical => 860.0,
            _ => 700.0,
        } - if self.scopes_visible { 0.0 } else { 240.0 };
        div()
            .id("viewer-dock")
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .v_flex()
                    .min_w_0()
                    .h_full()
                    .min_h(px(min_height))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .flex_wrap()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(rgb(t::JUDGE_LINE))
                            .child(
                                div()
                                    .font_family(t::Face::EdgeCode.family())
                                    .font_weight(t::Face::EdgeCode.weight())
                                    .text_size(t::Face::EdgeCode.size())
                                    .text_color(rgb(t::JUDGE_INK_DIM))
                                    .child(t::code("comparison")),
                            )
                            .child(menu)
                            .child(
                                Button::new("scopes-visible")
                                    .label(if self.scopes_visible {
                                        "Hide Scopes"
                                    } else {
                                        "Show Scopes"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.scopes_visible = !this.scopes_visible;
                                        this.scopes.update(cx, |scopes, cx| {
                                            scopes.set_enabled(this.scopes_visible, window, cx)
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Checkbox::new("clipping")
                                    .label("Clipping")
                                    .checked(self.clipping)
                                    .on_click(cx.listener(|this, enabled, window, cx| {
                                        this.clipping = *enabled;
                                        this.clear_difference(window);
                                        if !this.clipping {
                                            for pane in &mut this.panes {
                                                if let Some(image) = pane.clip_image.take() {
                                                    let _ = window.drop_image(image);
                                                }
                                            }
                                        }
                                        this.request_all(window, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(body)
                    .when(self.scopes_visible, |view| view.child(self.scopes.clone())),
            )
    }
}
fn comparison_frame(
    frame: Arc<PreviewFrame>,
    analysis: Option<Arc<ScopeAnalysis>>,
    clipping: bool,
) -> Result<Arc<PreviewFrame>, String> {
    if !clipping {
        return Ok(frame);
    }
    let analysis = analysis.ok_or("No clipping analysis for comparison")?;
    let raster = analysis
        .overlay_clipping(&frame.bgra)
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(PreviewFrame {
        dimensions: frame.dimensions,
        full_dimensions: frame.full_dimensions,
        bgra: raster.bgra,
    }))
}
/// Upload one bounded BGRA frame as a native texture.
pub(crate) fn upload(frame: &PreviewFrame) -> Result<Arc<RenderImage>, String> {
    let image = image::RgbaImage::from_raw(
        frame.dimensions.width,
        frame.dimensions.height,
        frame.bgra.clone(),
    )
    .ok_or("Invalid native image upload")?;
    Ok(Arc::new(RenderImage::new(smallvec::smallvec![
        image::Frame::new(image)
    ])))
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use super::{Comparison, Viewers};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    fn viewer_window(
        cx: &mut TestAppContext,
    ) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Viewers>) {
        cx.update(gpui_kit::init);
        cx.update(crate::configure_theme);
        cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(380.0), px(340.0)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(Viewers::new),
            )
            .unwrap()
        })
    }

    #[gpui_kit::test]
    fn compact_comparison_modes_keep_images_controls_and_scopes_separate(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(crate::configure_theme);
        let (handle, viewers) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(380.0), px(340.0)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(Viewers::new),
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            for mode in Comparison::ALL {
                viewers.update(cx, |viewers, cx| viewers.set_mode(mode, window, cx));
                window.render_frame(cx);
                window.render_frame(cx);
                let controls = window.find("viewer-controls-A").bounds();
                let image = window.find("viewer-0-interaction").bounds();
                let scopes = window.find("scopes-panel").bounds();
                assert!(
                    image.size.height >= px(80.0),
                    "{mode:?} collapsed: {image:?}"
                );
                assert!(
                    controls.bottom() <= image.top() + px(1.0),
                    "{mode:?} controls overlap image"
                );
                assert!(
                    image.bottom() <= scopes.top(),
                    "{mode:?} scopes overlap image"
                );
            }
            viewers.update(cx, |viewers, cx| {
                viewers.set_enabled(false, window, cx);
                assert!(viewers.panes.iter().all(|pane| pane.token.is_cancelled()
                    && pane.image.is_none()
                    && !pane.loading));
                let generations = viewers.panes.each_ref().map(|pane| pane.generation);
                viewers.request_all(window, cx);
                assert_eq!(
                    viewers.panes.each_ref().map(|pane| pane.generation),
                    generations
                );
                assert!(viewers.difference.is_none());
            });
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn an_overlay_compares_one_shared_view_and_side_by_side_keeps_two(cx: &mut TestAppContext) {
        let (handle, viewers) = viewer_window(cx);
        let transforms = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                viewers
                    .read(cx)
                    .panes
                    .each_ref()
                    .map(|pane| (pane.zoom, pane.pan))
            })
        };
        cx.update_window(handle, |_, window, cx| {
            viewers.update(cx, |viewers, cx| {
                viewers.set_mode(Comparison::Wipe, window, cx)
            });
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .unwrap();
        let before = transforms(cx);
        cx.update_window(handle, |_, window, cx| {
            viewers.update(cx, |viewers, cx| viewers.adjust_zoom(0, 2.0, window, cx));
        })
        .unwrap();
        let [moved, linked] = transforms(cx);
        assert_eq!(moved.0, linked.0, "zooming one side must zoom the other");
        assert_eq!(moved.1, linked.1, "the wipe shares one view");
        assert_ne!(moved.0, before[0].0);

        // Dragging the surface pans both sides of the wipe.
        let stage = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.find("viewer-0-interaction").bounds()
            })
            .unwrap();
        let start = stage.center() + point(px(80.0), px(0.0));
        cx.update_window(handle, |_, window, cx| {
            window.drag(start, start + point(px(-32.0), px(18.0)), cx);
            window.render_frame(cx);
        })
        .unwrap();
        let [moved, linked] = transforms(cx);
        assert_eq!(
            moved, linked,
            "a wipe compares one registration, not two framings"
        );
        assert_ne!(linked.1, before[0].1, "the drag must move that shared view");

        // Blink alternates sources in one viewport, so it shares the view too.
        cx.update_window(handle, |_, window, cx| {
            viewers.update(cx, |viewers, cx| {
                viewers.set_mode(Comparison::Blink, window, cx);
                viewers.adjust_zoom(1, 1.5, window, cx);
            });
        })
        .unwrap();
        let [a, b] = transforms(cx);
        assert_eq!(a, b, "blink must not jump between two framings");

        // Side-by-side A/B keeps an independent view per pane on purpose.
        cx.update_window(handle, |_, window, cx| {
            viewers.update(cx, |viewers, cx| {
                viewers.set_mode(Comparison::Horizontal, window, cx);
                viewers.adjust_zoom(0, 2.0, window, cx);
            });
        })
        .unwrap();
        let [a, b] = transforms(cx);
        assert_ne!(a.0, b.0, "stacked panes stay independently framed");
    }

    #[gpui_kit::test]
    fn the_wipe_edge_is_draggable_and_agrees_with_its_slider(cx: &mut TestAppContext) {
        let (handle, viewers) = viewer_window(cx);
        cx.update_window(handle, |_, window, cx| {
            viewers.update(cx, |viewers, cx| {
                viewers.set_mode(Comparison::Wipe, window, cx)
            });
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(
                window.try_find("wipe-edge").is_some(),
                "the edge is a handle"
            );
        })
        .unwrap();
        let stage = cx
            .update_window(handle, |_, window, _| {
                window.find("viewer-0-interaction").bounds()
            })
            .unwrap();
        assert_eq!(cx.update(|cx| viewers.read(cx).wipe), 0.5);
        let edge = stage.left() + stage.size.width * 0.5;
        cx.update_window(handle, |_, window, cx| {
            window.drag(
                point(edge, stage.center().y),
                point(stage.left() + stage.size.width * 0.25, stage.center().y),
                cx,
            );
            window.render_frame(cx);
        })
        .unwrap();
        let (wiped, slider) = cx.update(|cx| {
            let viewers = viewers.read(cx);
            (viewers.wipe, viewers.wipe_slider.read(cx).value().start())
        });
        assert!(
            (wiped - 0.25).abs() < 0.05,
            "the dragged edge landed at {wiped}"
        );
        assert_eq!(wiped, slider, "the edge and the slider agree");
        // Dragging the image itself stays a pan; only the edge moves the wipe.
        let pan_before = cx.update(|cx| viewers.read(cx).panes[0].pan);
        cx.update_window(handle, |_, window, cx| {
            let away = point(stage.left() + stage.size.width * 0.8, stage.center().y);
            window.drag(away, away + point(px(10.0), px(0.0)), cx);
        })
        .unwrap();
        assert_eq!(cx.update(|cx| viewers.read(cx).wipe), wiped);
        assert_ne!(cx.update(|cx| viewers.read(cx).panes[0].pan), pan_before);
    }
}
