//! Native viewer state and display-space comparison, independent of GPUI.
use crate::{PreviewFrame, Session, Source, preview_type};
use rawweave_image::Dimensions;

pub type Target = (String, String);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Comparison {
    #[default]
    Single,
    Horizontal,
    Vertical,
    Wipe,
    Blink,
    Difference,
}
impl Comparison {
    pub const ALL: [Self; 6] = [
        Self::Single,
        Self::Horizontal,
        Self::Vertical,
        Self::Wipe,
        Self::Blink,
        Self::Difference,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Horizontal => "A/B horizontal",
            Self::Vertical => "A/B stacked",
            Self::Wipe => "Wipe",
            Self::Blink => "Blink",
            Self::Difference => "Difference",
        }
    }
    pub fn overlay(self) -> bool {
        matches!(self, Self::Wipe | Self::Blink | Self::Difference)
    }
}

#[derive(Default)]
pub struct ViewerModel {
    pub mode: Comparison,
    pub targets: [Option<Target>; 2],
    explicit: [bool; 2],
}
impl ViewerModel {
    pub fn select(&mut self, pane: usize, target: Target, session: &Session) -> Result<(), String> {
        let output = self.targets.get_mut(pane).ok_or("Invalid viewer")?;
        if !available(session, &target) {
            return Err("Select an available image output".into());
        }
        *output = Some(target);
        if let Some(explicit) = self.explicit.get_mut(pane) {
            *explicit = true;
        }
        Ok(())
    }
    pub fn set_mode(&mut self, mode: Comparison, session: &Session) {
        self.mode = mode;
        self.reconcile(session);
    }
    pub fn reconcile(&mut self, session: &Session) {
        if session.source.is_none() {
            self.targets = [None, None];
            return;
        }
        let raw = matches!(session.source, Some(Source::Raw { .. }));
        let preferred = |kind: &str| {
            session
                .editor
                .graph()
                .nodes()
                .values()
                .find(|node| node.type_id == kind)
                .and_then(|node| {
                    node.descriptor
                        .outputs
                        .iter()
                        .find(|port| preview_type(&port.data_type))
                        .map(|port| (node.id.as_str().to_owned(), port.id.clone()))
                })
        };
        let output = preferred(if raw {
            "raw.display-transform"
        } else {
            "core.output"
        })
        .or_else(|| {
            session.editor.graph().nodes().values().find_map(|node| {
                node.descriptor
                    .outputs
                    .iter()
                    .find(|port| preview_type(&port.data_type))
                    .map(|port| (node.id.as_str().to_owned(), port.id.clone()))
            })
        });
        let input = preferred(if raw {
            "raw.demosaic"
        } else {
            "core.image-input"
        })
        .or_else(|| output.clone());
        let defaults = if self.mode == Comparison::Single {
            [output, None]
        } else {
            [input, output]
        };
        for ((target, explicit), default) in self
            .targets
            .iter_mut()
            .zip(&mut self.explicit)
            .zip(defaults)
        {
            if *explicit
                && target
                    .as_ref()
                    .is_some_and(|target| available(session, target))
            {
                continue;
            }
            *explicit = false;
            *target = default;
        }
    }
}
fn available(session: &Session, target: &Target) -> bool {
    session
        .editor
        .graph()
        .node(&rawweave_core::NodeId::from(target.0.as_str()))
        .is_some_and(|node| {
            node.descriptor
                .outputs
                .iter()
                .any(|port| port.id == target.1 && preview_type(&port.data_type))
        })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceTransform {
    pub zoom: f32,
    pub pan: [f32; 2],
}

/// Rasterize absolute difference of the two displayed surfaces, not scene-linear image processing.
/// Both sources keep independent zoom/pan/full dimensions and are composited over the viewer ground.
pub fn display_difference(
    a: &PreviewFrame,
    b: &PreviewFrame,
    viewport: Dimensions,
    ta: SurfaceTransform,
    tb: SurfaceTransform,
) -> Result<PreviewFrame, String> {
    validate(a)?;
    validate(b)?;
    let count = viewport.pixel_count().map_err(|e| e.to_string())?;
    let capacity = count
        .checked_mul(4)
        .filter(|n| *n <= 128 * 1024 * 1024)
        .ok_or("Comparison exceeds upload budget")?;
    if viewport.width == 0
        || viewport.height == 0
        || viewport.width > 8192
        || viewport.height > 8192
        || [ta, tb]
            .iter()
            .any(|t| !t.zoom.is_finite() || t.zoom <= 0.0 || t.pan.iter().any(|v| !v.is_finite()))
    {
        return Err("Invalid comparison surface".into());
    }
    let mut bgra = Vec::with_capacity(capacity);
    for y in 0..viewport.height {
        for x in 0..viewport.width {
            let first = surface_pixel(a, viewport, ta, x, y);
            let second = surface_pixel(b, viewport, tb, x, y);
            bgra.extend(
                first
                    .into_iter()
                    .zip(second)
                    .map(|(a, b)| (a - b).abs().round().clamp(0.0, 255.0) as u8),
            );
            bgra.push(255);
        }
    }
    Ok(PreviewFrame {
        dimensions: viewport,
        full_dimensions: viewport,
        bgra,
    })
}
fn validate(frame: &PreviewFrame) -> Result<(), String> {
    let expected = frame
        .dimensions
        .pixel_count()
        .map_err(|e| e.to_string())?
        .checked_mul(4)
        .ok_or("Invalid comparison buffer")?;
    if expected != frame.bgra.len()
        || expected == 0
        || expected > 128 * 1024 * 1024
        || frame.full_dimensions.width == 0
        || frame.full_dimensions.height == 0
    {
        return Err("Invalid comparison buffer".into());
    }
    Ok(())
}
fn surface_pixel(
    frame: &PreviewFrame,
    viewport: Dimensions,
    transform: SurfaceTransform,
    x: u32,
    y: u32,
) -> [f32; 3] {
    let width = frame.full_dimensions.width as f32 * transform.zoom;
    let height = frame.full_dimensions.height as f32 * transform.zoom;
    let nx = (x as f32 + 0.5 - (viewport.width as f32 - width) / 2.0 - transform.pan[0]) / width;
    let ny = (y as f32 + 0.5 - (viewport.height as f32 - height) / 2.0 - transform.pan[1]) / height;
    // BGRA-order neutral judging-station ground, matching the native surface.
    let ground = [21.0, 18.0, 16.0];
    if !(0.0..1.0).contains(&nx) || !(0.0..1.0).contains(&ny) {
        return ground;
    }
    let px = (nx * frame.dimensions.width as f32 - 0.5)
        .clamp(0.0, frame.dimensions.width.saturating_sub(1) as f32);
    let py = (ny * frame.dimensions.height as f32 - 0.5)
        .clamp(0.0, frame.dimensions.height.saturating_sub(1) as f32);
    let x0 = px.floor() as u32;
    let y0 = py.floor() as u32;
    let x1 = x0
        .saturating_add(1)
        .min(frame.dimensions.width.saturating_sub(1));
    let y1 = y0
        .saturating_add(1)
        .min(frame.dimensions.height.saturating_sub(1));
    let weights = [
        (x0, y0, (1.0 - px.fract()) * (1.0 - py.fract())),
        (x1, y0, px.fract() * (1.0 - py.fract())),
        (x0, y1, (1.0 - px.fract()) * py.fract()),
        (x1, y1, px.fract() * py.fract()),
    ];
    let mut sample = [0.0_f32; 4];
    for (x, y, weight) in weights {
        let index = u64::from(y)
            .checked_mul(u64::from(frame.dimensions.width))
            .and_then(|n| n.checked_add(u64::from(x)))
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| usize::try_from(n).ok());
        if let Some(pixel) = index.and_then(|n| frame.bgra.get(n..n.saturating_add(4))) {
            for (channel, byte) in sample.iter_mut().zip(pixel) {
                *channel += f32::from(*byte) * weight;
            }
        }
    }
    let alpha = sample[3] / 255.0;
    [
        sample[0] * alpha + ground[0] * (1.0 - alpha),
        sample[1] * alpha + ground[1] * (1.0 - alpha),
        sample[2] * alpha + ground[2] * (1.0 - alpha),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Session, Source};
    use rawweave_image::{Dimensions, Image};

    fn ordinary() -> Session {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(Image::new(4, 2).unwrap()))
            .unwrap();
        session
    }

    #[test]
    fn comparison_defaults_and_explicit_targets_match_tauri() {
        let session = ordinary();
        let mut viewer = ViewerModel::default();
        viewer.reconcile(&session);
        assert_eq!(viewer.targets[0], Some(("output".into(), "image".into())));
        assert_eq!(viewer.targets[1], None);
        viewer.set_mode(Comparison::Horizontal, &session);
        assert_eq!(viewer.targets[0], Some(("input".into(), "image".into())));
        assert_eq!(viewer.targets[1], Some(("output".into(), "image".into())));
        viewer
            .select(0, ("output".into(), "image".into()), &session)
            .unwrap();
        viewer.set_mode(Comparison::Wipe, &session);
        assert_eq!(viewer.targets[0], Some(("output".into(), "image".into())));
        viewer.set_mode(Comparison::Single, &session);
        assert_eq!(viewer.targets[1], None);
        assert!(
            viewer
                .select(2, ("input".into(), "image".into()), &session)
                .is_err()
        );
        assert!(
            viewer
                .select(0, ("input".into(), "missing".into()), &session)
                .is_err()
        );
    }

    #[test]
    fn deleted_targets_and_source_clears_release_hidden_panes() {
        let mut session = ordinary();
        let mut viewer = ViewerModel::default();
        viewer.set_mode(Comparison::Vertical, &session);
        viewer
            .select(1, ("input".into(), "image".into()), &session)
            .unwrap();
        viewer.set_mode(Comparison::Single, &session);
        assert_eq!(viewer.targets[1], Some(("input".into(), "image".into())));
        session.editor.remove_node("input").unwrap();
        viewer.reconcile(&session);
        assert_eq!(viewer.targets[1], None);
        session.source = None;
        viewer.reconcile(&session);
        assert_eq!(viewer.targets, [None, None]);
    }

    #[test]
    fn raw_comparison_uses_demosaic_and_display_not_bytes() {
        let mut session = Session::default();
        session.editor.reset_raw_image_graph().unwrap();
        session.source = Some(Source::Raw {
            bytes: Default::default(),
            path: "raw.nef".into(),
            dimensions: Dimensions::new(4, 4),
        });
        let mut viewer = ViewerModel::default();
        viewer.set_mode(Comparison::Horizontal, &session);
        assert_eq!(viewer.targets[0], Some(("demosaic".into(), "scene".into())));
        assert_eq!(
            viewer.targets[1],
            Some(("display-transform".into(), "display".into()))
        );
    }

    #[test]
    fn difference_respects_independent_pan_transparency_and_letterboxing() {
        let opaque = crate::PreviewFrame {
            dimensions: Dimensions::new(1, 1),
            full_dimensions: Dimensions::new(1, 1),
            bgra: vec![255, 255, 255, 255],
        };
        let transparent = crate::PreviewFrame {
            dimensions: Dimensions::new(1, 1),
            full_dimensions: Dimensions::new(1, 1),
            bgra: vec![0, 0, 0, 0],
        };
        let transform = SurfaceTransform {
            zoom: 1.0,
            pan: [0.0; 2],
        };
        let result = display_difference(
            &opaque,
            &transparent,
            Dimensions::new(3, 1),
            transform,
            transform,
        )
        .unwrap();
        assert_eq!(
            result.bgra,
            [0, 0, 0, 255, 234, 237, 239, 255, 0, 0, 0, 255]
        );
        let shifted = display_difference(
            &opaque,
            &opaque,
            Dimensions::new(3, 1),
            transform,
            SurfaceTransform {
                zoom: 1.0,
                pan: [1.0, 0.0],
            },
        )
        .unwrap();
        assert_eq!(
            shifted.bgra,
            [0, 0, 0, 255, 234, 237, 239, 255, 234, 237, 239, 255]
        );
    }
    #[test]
    fn difference_compares_display_space_and_rejects_invalid_buffers() {
        let a = crate::PreviewFrame {
            dimensions: Dimensions::new(2, 1),
            full_dimensions: Dimensions::new(2, 1),
            bgra: vec![10, 20, 30, 255, 50, 80, 100, 255],
        };
        let b = crate::PreviewFrame {
            dimensions: Dimensions::new(1, 1),
            full_dimensions: Dimensions::new(2, 1),
            bgra: vec![5, 40, 60, 255],
        };
        let transform = SurfaceTransform {
            zoom: 1.0,
            pan: [0.0, 0.0],
        };
        let result =
            display_difference(&a, &b, Dimensions::new(2, 1), transform, transform).unwrap();
        assert_eq!(result.bgra, [5, 20, 30, 255, 45, 40, 40, 255]);
        let identical =
            display_difference(&a, &a, Dimensions::new(2, 1), transform, transform).unwrap();
        assert_eq!(identical.bgra, [0, 0, 0, 255, 0, 0, 0, 255]);
        let invalid = crate::PreviewFrame {
            dimensions: Dimensions::new(1, 1),
            full_dimensions: Dimensions::new(1, 1),
            bgra: vec![],
        };
        assert!(
            display_difference(&invalid, &b, Dimensions::new(2, 1), transform, transform).is_err()
        );
        assert!(
            display_difference(&a, &b, Dimensions::new(8193, 1), transform, transform).is_err()
        );
        assert!(
            display_difference(
                &a,
                &b,
                Dimensions::new(2, 1),
                SurfaceTransform {
                    zoom: f32::NAN,
                    pan: [0.0; 2]
                },
                transform
            )
            .is_err()
        );
    }
}
