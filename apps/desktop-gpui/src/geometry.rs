//! Presentation-only image gestures; parameter application still runs Rust graph nodes.
use rawweave_image::Dimensions;
use rawweave_node_api::ParameterValue;
use std::collections::BTreeMap;

pub fn crop_preset(
    size: Dimensions,
    ratio: Option<f64>,
) -> Option<BTreeMap<String, ParameterValue>> {
    if size.width == 0 || size.height == 0 || ratio.is_some_and(|r| !r.is_finite() || r <= 0.0) {
        return None;
    }
    let (width, height) = ratio.map_or((size.width, size.height), |ratio| {
        (
            size.width
                .min((f64::from(size.height) * ratio).floor() as u32),
            size.height
                .min((f64::from(size.width) / ratio).floor() as u32),
        )
    });
    if width == 0 || height == 0 {
        return None;
    }
    Some(
        [
            ("x", (size.width - width) / 2),
            ("y", (size.height - height) / 2),
            ("width", width),
            ("height", height),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), ParameterValue::Float(value as f32)))
        .collect(),
    )
}

pub fn drawn_parameters(
    kind: &str,
    start: (f64, f64),
    end: (f64, f64),
    size: Dimensions,
    origin: (u32, u32),
) -> Option<BTreeMap<String, ParameterValue>> {
    if size.width == 0
        || size.height == 0
        || [start.0, start.1, end.0, end.1]
            .iter()
            .any(|v| !v.is_finite())
    {
        return None;
    }
    let clamp = |(x, y): (f64, f64)| {
        (
            x.clamp(0.0, f64::from(size.width)).round(),
            y.clamp(0.0, f64::from(size.height)).round(),
        )
    };
    let (a, b) = (clamp(start), clamp(end));
    if a == b {
        return None;
    }
    if kind == "core.crop" {
        if a.0 == b.0 || a.1 == b.1 {
            return None;
        }
        return Some(
            [
                ("x", a.0.min(b.0)),
                ("y", a.1.min(b.1)),
                ("width", (b.0 - a.0).abs()),
                ("height", (b.1 - a.1).abs()),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), ParameterValue::Float(value as f32)))
            .collect(),
        );
    }
    let values = match kind {
        "core.mask-linear-gradient" => vec![
            ("start_x", a.0 + f64::from(origin.0)),
            ("start_y", a.1 + f64::from(origin.1)),
            ("end_x", b.0 + f64::from(origin.0)),
            ("end_y", b.1 + f64::from(origin.1)),
        ],
        "core.mask-radial-gradient" => vec![
            ("center_x", a.0 + f64::from(origin.0)),
            ("center_y", a.1 + f64::from(origin.1)),
            ("radius", (b.0 - a.0).hypot(b.1 - a.1)),
        ],
        _ => return None,
    };
    Some(
        values
            .into_iter()
            .map(|(key, value)| (key.into(), ParameterValue::Float(value as f32)))
            .collect(),
    )
}

pub struct InputPreview {
    pub frame: crate::PreviewFrame,
    pub origin: (u32, u32),
}

impl crate::Session {
    /// A name for people, not the persisted graph identifier. Duplicate names get a local ordinal.
    pub fn node_label(&self, id: &str) -> String {
        let Some(node) = self.editor.graph().node(&rawweave_core::NodeId::from(id)) else {
            return "Unavailable node".into();
        };
        let peers: Vec<_> = self
            .editor
            .graph()
            .nodes()
            .values()
            .filter(|peer| peer.descriptor.name == node.descriptor.name)
            .collect();
        if peers.len() < 2 {
            return node.descriptor.name.clone();
        }
        let ordinal = peers
            .iter()
            .position(|peer| peer.id == node.id)
            .unwrap_or(0)
            + 1;
        format!("{} ({ordinal})", node.descriptor.name)
    }
    pub fn readable_error(&self, error: &str) -> String {
        let mut readable = error.to_owned();
        for node in self.editor.graph().nodes().values() {
            readable = readable.replace(
                &format!("'{}'", node.id),
                &format!("'{}'", self.node_label(node.id.as_str())),
            );
        }
        readable
    }
    pub fn image_input_target(&self, id: &str) -> Option<(String, String)> {
        self.editor
            .graph()
            .edges()
            .iter()
            .find(|edge| {
                edge.to_node.as_str() == id && matches!(edge.to_port.as_str(), "image" | "scene")
            })
            .map(|edge| (edge.from_node.as_str().to_owned(), edge.from_port.clone()))
    }
    /// Read the connected input at mip 0 for accurate dimensions/origin, then upload only a bounded thumbnail.
    pub fn image_input_preview(
        &self,
        id: &str,
        cancellation: &rawweave_rendering::CancellationToken,
    ) -> Result<InputPreview, String> {
        if cancellation.is_cancelled() {
            return Err("Input preview cancelled".into());
        }
        let mut input = self.clone();
        input.target = self
            .image_input_target(id)
            .ok_or("Connect an image input to use these controls")?;
        let value = input
            .evaluate(0)
            .map_err(|error| self.readable_error(&error))?;
        if cancellation.is_cancelled() {
            return Err("Input preview cancelled".into());
        }
        let (dimensions, origin) = match &value {
            rawweave_node_api::Value::Image(image) => (image.dimensions(), image.origin()),
            rawweave_node_api::Value::SceneLinearRGB(image) => (image.dimensions(), (0, 0)),
            rawweave_node_api::Value::DisplayRGB(image) => (image.dimensions(), (0, 0)),
            _ => return Err("Connected input is not an image".into()),
        };
        let mip = (0..=6)
            .find(|mip| dimensions.width.max(dimensions.height).div_ceil(1 << mip) <= 512)
            .unwrap_or(6);
        let frame = match value {
            rawweave_node_api::Value::Image(image) => {
                let selected = image.sample_mip(mip).map_err(|e| e.to_string())?;
                crate::PreviewFrame::from_pixels(
                    selected.dimensions(),
                    dimensions,
                    selected.pixels().iter().copied(),
                )?
            }
            rawweave_node_api::Value::SceneLinearRGB(scene) => {
                use rawweave_color::DisplayTransform;
                crate::display_frame(
                    rawweave_color::SrgbDisplayTransform
                        .transform(&scene)
                        .map_err(|e| e.to_string())?,
                    mip,
                )?
            }
            rawweave_node_api::Value::DisplayRGB(display) => crate::display_frame(display, mip)?,
            _ => return Err("Connected input is not an image".into()),
        };
        Ok(InputPreview { frame, origin })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_helpers_follow_scene_wires_through_pro_nodes() {
        use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
        let mut session = crate::Session {
            editor: rawweave_project::EditorCore::new_with_raw_decoder(DeterministicDecoder::new(
                DeterministicCorpus::bayer_12_bit(),
            )),
            ..Default::default()
        };
        session
            .attach(crate::Source::Raw {
                bytes: std::sync::Arc::new(vec![0]),
                path: "synthetic.nef".into(),
                dimensions: Dimensions::new(4, 2),
            })
            .unwrap();
        session
            .editor
            .add_node("detail", "pro.detail-separation")
            .unwrap();
        session
            .editor
            .add_node("gradient", "core.mask-linear-gradient")
            .unwrap();
        session
            .editor
            .connect("camera-transform", "scene", "detail", "scene")
            .unwrap();
        session
            .editor
            .connect("detail", "base_scene", "gradient", "scene")
            .unwrap();
        assert_eq!(
            session.image_input_target("gradient"),
            Some(("detail".into(), "base_scene".into()))
        );
        let preview = session
            .image_input_preview("gradient", &rawweave_rendering::CancellationToken::new())
            .unwrap();
        assert_eq!(preview.origin, (0, 0));
        assert_eq!(preview.frame.full_dimensions, Dimensions::new(4, 2));
        assert!(preview.frame.bgra.iter().any(|byte| *byte > 0));
        session.select_target("detail", "detail_scene").unwrap();
        assert_eq!(
            session
                .preview(1, &rawweave_rendering::CancellationToken::new())
                .unwrap()
                .full_dimensions,
            Dimensions::new(4, 2)
        );
    }

    #[test]
    fn input_helpers_read_upstream_dimensions_and_origins_not_the_source_or_crop_output() {
        let mut session = crate::Session::default();
        session
            .attach(crate::Source::Ordinary(
                rawweave_image::Image::from_pixels(13, 21, vec![[0.5, 0.5, 0.5, 1.0]; 273])
                    .unwrap(),
            ))
            .unwrap();
        session.editor.add_node("node-2", "core.crop").unwrap();
        session
            .editor
            .connect("input", "image", "node-2", "image")
            .unwrap();
        for (key, value) in [("x", 1), ("y", 2), ("width", 9), ("height", 15)] {
            session
                .editor
                .set_node_parameter("node-2", key, ParameterValue::Float(value as f32))
                .unwrap();
        }
        session.editor.add_node("node-3", "core.crop").unwrap();
        session
            .editor
            .connect("node-2", "image", "node-3", "image")
            .unwrap();
        let preview = session
            .image_input_preview("node-3", &rawweave_rendering::CancellationToken::new())
            .unwrap();
        assert_eq!(preview.frame.full_dimensions, Dimensions::new(9, 15));
        assert_eq!(preview.origin, (1, 2));
        assert_eq!(session.node_label("node-2"), "Crop (1)");
        assert_eq!(session.node_label("node-3"), "Crop (2)");
        let token = rawweave_rendering::CancellationToken::new();
        token.cancel();
        assert!(session.image_input_preview("node-3", &token).is_err());
    }
    #[test]
    fn crop_presets_and_drawing_stay_inside_input_and_masks_keep_global_origin() {
        let size = Dimensions::new(13, 21);
        let square = crop_preset(size, Some(1.0)).unwrap();
        assert_eq!(square["x"], ParameterValue::Float(0.0));
        assert_eq!(square["y"], ParameterValue::Float(4.0));
        assert_eq!(square["width"], ParameterValue::Float(13.0));
        assert_eq!(
            crop_preset(size, None).unwrap()["height"],
            ParameterValue::Float(21.0)
        );
        assert!(crop_preset(Dimensions::new(1, 1), Some(16.0 / 9.0)).is_none());
        let crop =
            drawn_parameters("core.crop", (18.0, 20.0), (-1.0, 3.0), size, (100, 200)).unwrap();
        assert_eq!(crop["x"], ParameterValue::Float(0.0));
        assert_eq!(crop["width"], ParameterValue::Float(13.0));
        let gradient = drawn_parameters(
            "core.mask-linear-gradient",
            (1.0, 2.0),
            (5.0, 6.0),
            size,
            (100, 200),
        )
        .unwrap();
        assert_eq!(gradient["start_x"], ParameterValue::Float(101.0));
        assert_eq!(gradient["end_y"], ParameterValue::Float(206.0));
        assert!(drawn_parameters("core.crop", (1.0, 2.0), (1.0, 4.0), size, (0, 0)).is_none());
        assert!(drawn_parameters("core.crop", (f64::NAN, 2.0), (1.0, 4.0), size, (0, 0)).is_none());
    }
}
