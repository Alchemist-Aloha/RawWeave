//! Presentation of shared Rust display analysis. No image processing lives in these widgets.
use gpui_kit::component::{
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
    *,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rawweave_gpui::{PreviewFrame, Session};
use rawweave_image::Dimensions;
use rawweave_rendering::{
    CancellationToken,
    scopes::{AnalysisOptions, Raster, ScopeAnalysis, ScopeKind, analyze_bgra},
};
use std::sync::Arc;

pub struct AnalyzedFrame {
    pub frame: PreviewFrame,
    pub analysis: Arc<ScopeAnalysis>,
    pub clipping: Option<Raster>,
}
pub fn render_frame(
    session: &Session,
    mip: u8,
    cancellation: &CancellationToken,
    clipping: bool,
) -> Result<AnalyzedFrame, String> {
    let frame = session.preview(mip, cancellation)?;
    let analysis = analyze_bgra(
        frame.dimensions,
        &frame.bgra,
        AnalysisOptions::default(),
        cancellation,
    )
    .map_err(|e| e.to_string())?;
    let clipping = if clipping {
        Some(analysis.clipping_overlay().map_err(|e| e.to_string())?)
    } else {
        None
    };
    if cancellation.is_cancelled() {
        return Err("Scope analysis cancelled".into());
    }
    Ok(AnalyzedFrame {
        frame,
        analysis: Arc::new(analysis),
        clipping,
    })
}
pub fn upload_raster(raster: Raster) -> Result<Arc<RenderImage>, String> {
    let buffer = image::RgbaImage::from_raw(
        raster.dimensions.width,
        raster.dimensions.height,
        raster.bgra,
    )
    .ok_or("Invalid native scope upload")?;
    Ok(Arc::new(RenderImage::new(smallvec::smallvec![
        image::Frame::new(buffer)
    ])))
}
#[cfg(test)]
mod tests {
    use super::render_frame;
    use rawweave_gpui::{Session, Source};
    use rawweave_image::{Dimensions, Image};
    use rawweave_rendering::CancellationToken;

    #[test]
    fn native_preview_analysis_and_overlay_share_sampled_coordinates_and_cancellation() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels(4, 2, vec![[1.0, 0.0, 0.0, 1.0]; 8]).unwrap(),
            ))
            .unwrap();
        let result = render_frame(&session, 1, &CancellationToken::new(), true).unwrap();
        assert_eq!(result.frame.dimensions, Dimensions::new(2, 1));
        assert_eq!(result.frame.full_dimensions, Dimensions::new(4, 2));
        assert_eq!(result.analysis.dimensions(), result.frame.dimensions);
        assert_eq!(result.analysis.highlight_count, 2);
        assert_eq!(result.clipping.unwrap().dimensions, result.frame.dimensions);
        assert!(
            render_frame(&session, 1, &CancellationToken::new(), false)
                .unwrap()
                .clipping
                .is_none()
        );
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(render_frame(&session, 1, &cancelled, true).is_err());
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct RasterKey {
    generation: u64,
    kind: ScopeKind,
    dimensions: Dimensions,
}
pub struct Scopes {
    analysis: Option<Arc<ScopeAnalysis>>,
    label: String,
    kind: ScopeKind,
    image: Option<Arc<RenderImage>>,
    dimensions: Dimensions,
    generation: u64,
    key: Option<RasterKey>,
    running: bool,
    enabled: bool,
    error: Option<String>,
}
impl Scopes {
    pub fn new() -> Self {
        Self {
            analysis: None,
            label: String::new(),
            kind: ScopeKind::Histogram,
            image: None,
            dimensions: Dimensions::new(256, 160),
            generation: 0,
            key: None,
            running: false,
            enabled: true,
            error: None,
        }
    }
    fn clear(&mut self, window: &mut Window) {
        if let Some(image) = self.image.take() {
            let _ = window.drop_image(image);
        }
        self.key = None;
        self.error = None;
    }
    pub fn set_analysis(
        &mut self,
        analysis: Option<Arc<ScopeAnalysis>>,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same = match (&analysis, &self.analysis) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same && self.label == label {
            return;
        }
        self.analysis = analysis;
        self.label = label;
        let Some(generation) = self.generation.checked_add(1) else {
            self.error = Some("Scope counter exhausted".into());
            return;
        };
        self.generation = generation;
        self.clear(window);
        self.refresh(window, cx);
        cx.notify();
    }
    pub fn set_enabled(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.enabled = enabled;
        if !enabled {
            self.clear(window);
        } else {
            self.refresh(window, cx);
        }
        cx.notify();
    }
    fn current_key(&self) -> Option<RasterKey> {
        if !self.enabled || self.analysis.is_none() || self.kind == ScopeKind::PixelInspector {
            return None;
        }
        Some(RasterKey {
            generation: self.generation,
            kind: self.kind,
            dimensions: self.dimensions,
        })
    }
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.current_key() else {
            return;
        };
        if self.running || self.key == Some(key) {
            return;
        }
        let Some(analysis) = self.analysis.clone() else {
            return;
        };
        self.running = true;
        let task = cx.background_executor().spawn(async move {
            analysis
                .render(key.kind, key.dimensions)
                .map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.running = false;
                if this.current_key() == Some(key) {
                    this.clear(window);
                    this.key = Some(key);
                    match result.and_then(upload_raster) {
                        Ok(image) => this.image = Some(image),
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                } else {
                    this.refresh(window, cx);
                }
            });
        })
        .detach();
    }
    fn resize(
        &mut self,
        width: f32,
        height: f32,
        ratio: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ![width, height, ratio]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
        {
            return;
        }
        let dimensions = Dimensions::new(
            (width * ratio).round().clamp(1.0, 1024.0) as u32,
            (height * ratio).round().clamp(1.0, 1024.0) as u32,
        );
        if dimensions == self.dimensions {
            return;
        }
        self.dimensions = dimensions;
        self.refresh(window, cx);
    }
}
impl Render for Scopes {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let kind = self.kind;
        let menu =
            Button::new("scope-view")
                .label(kind.label())
                .dropdown_menu(move |mut menu, _, _| {
                    for kind in ScopeKind::ALL {
                        let weak = weak.clone();
                        menu = menu.item(PopupMenuItem::new(kind.label()).on_click(
                            move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.kind = kind;
                                    this.clear(window);
                                    this.refresh(window, cx);
                                    cx.notify();
                                });
                            },
                        ));
                    }
                    menu
                });
        let weak = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.resize(
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                        window.scale_factor(),
                        window,
                        cx,
                    )
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let mut body = div()
            .relative()
            .h(px(160.0))
            .w_full()
            .overflow_hidden()
            .bg(rgb(0x141414))
            .child(measure);
        if self.kind == ScopeKind::PixelInspector {
            let sample = self.analysis.as_ref().and_then(|a| {
                let dimensions = a.dimensions();
                a.sample_pixel(dimensions.width / 2, dimensions.height / 2)
            });
            body = body.child(div().v_flex().gap_1().p_2().text_sm().when_some(
                sample,
                |view, sample| {
                    view.child(format!(
                        "RGB {}, {}, {} · {}",
                        sample.rgba[0],
                        sample.rgba[1],
                        sample.rgba[2],
                        sample.hex()
                    ))
                    .child(format!(
                        "Alpha {} · display luma {:.2}",
                        sample.rgba[3], sample.luma
                    ))
                    .child(format!(
                        "Preview pixel {}, {} (center; sampled inspector)",
                        sample.x, sample.y
                    ))
                },
            ));
        } else if let Some(image) = &self.image {
            body = body.child(img(image.clone()).absolute().size_full());
        } else {
            body = body.child(
                div()
                    .p_2()
                    .text_sm()
                    .child(self.error.clone().unwrap_or_else(|| {
                        if self.analysis.is_some() {
                            "Drawing scope…".into()
                        } else {
                            "No preview to analyze".into()
                        }
                    })),
            );
        }
        let note = match self.kind {
            ScopeKind::GamutWarning => {
                "Display saturation heuristic, not a color-managed gamut test."
            }
            ScopeKind::FalseColor => "False color of display-code luma, not scene-linear exposure.",
            ScopeKind::Zebra => "Display highlights ≥98% / shadows ≤2%; not RAW clipping.",
            ScopeKind::PixelInspector => "8-bit display preview codes, not source RAW samples.",
            _ => "8-bit display preview values; statistics use at most 100,000 samples.",
        };
        div()
            .id("scopes-panel")
            .test_support()
            .v_flex()
            .w_full()
            .flex_shrink_0()
            .border_t_1()
            .border_color(rgb(0x343b45))
            .p_2()
            .gap_1()
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(div().child("Scopes"))
                    .child(menu),
            )
            .child(div().text_xs().child(self.label.clone()))
            .child(body)
            .when_some(self.analysis.as_ref(), |view, analysis| {
                view.child(div().text_xs().child(format!(
                    "{} samples · {} highlights · {} shadows",
                    analysis.sample_count, analysis.highlight_count, analysis.shadow_count
                )))
            })
            .child(div().text_xs().child(note))
    }
}
