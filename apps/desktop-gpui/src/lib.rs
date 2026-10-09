//! Native desktop session. Graph execution stays in the existing Rust engine.
pub mod batchqueue;
pub mod browse;
pub mod export;
pub mod geometry;
pub mod library;
pub mod parameters;
pub mod spatial;
pub mod viewer;
pub mod workspace;
use rawweave_color::{DisplayTransform, SrgbDisplayTransform};
use rawweave_image::{ColorDomain, Dimensions, Image, PixelFormat};
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;
use rawweave_raw::{RawDecodeLimits, RawloaderDecoder};
use rawweave_rendering::CancellationToken;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Capture one document before a pointer gesture, not one entry per movement.
#[derive(Default)]
pub struct MoveHistory {
    before: Option<String>,
}
impl MoveHistory {
    pub fn observe(&mut self, dragging: bool, before: String, after: &str) -> Option<String> {
        if dragging {
            self.before.get_or_insert(before);
            None
        } else {
            self.before.take().filter(|value| value != after)
        }
    }
    pub fn reset(&mut self) {
        self.before = None;
    }
}

#[cfg(test)]
mod move_history_tests {
    #[test]
    fn moves_group_until_release_and_ignore_selection_or_return_to_origin() {
        let mut history = super::MoveHistory::default();
        assert_eq!(history.observe(false, "initial".into(), "initial"), None);
        assert_eq!(history.observe(true, "initial".into(), "midway"), None);
        assert_eq!(history.observe(true, "midway".into(), "final"), None);
        assert_eq!(
            history.observe(false, "final".into(), "final"),
            Some("initial".into())
        );
        assert_eq!(history.observe(false, "final".into(), "final"), None);
        history.observe(true, "initial".into(), "midway");
        assert_eq!(history.observe(false, "midway".into(), "initial"), None);
        history.observe(true, "initial".into(), "midway");
        history.reset();
        assert_eq!(history.observe(false, "loaded".into(), "loaded"), None);
    }
}

#[derive(Clone)]
pub enum Source {
    Ordinary(Image),
    Raw {
        bytes: Arc<Vec<u8>>,
        path: PathBuf,
        dimensions: Dimensions,
    },
}
impl Source {
    pub fn dimensions(&self) -> Dimensions {
        match self {
            Self::Ordinary(image) => image.dimensions(),
            Self::Raw { dimensions, .. } => *dimensions,
        }
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        let raw = path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                [
                    "3fr", "arw", "cr2", "cr3", "crw", "dng", "erf", "fff", "iiq", "kdc", "mef",
                    "mos", "mrw", "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "srw",
                    "x3f",
                ]
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
            });
        if !raw {
            return rawweave_batch::decode_ordinary_file(path)
                .map(Self::Ordinary)
                .map_err(|e| e.to_string());
        }
        let bytes = bounded_read(path, RawDecodeLimits::default().max_input_bytes)?;
        let frame = RawloaderDecoder::default()
            .decode(&bytes)
            .map_err(|e| e.to_string())?;
        Ok(Self::Raw {
            bytes: Arc::new(bytes),
            path: path.to_owned(),
            dimensions: frame.sensor_dimensions(),
        })
    }
}

pub fn save_workflow_atomic(path: &Path, document: &str) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(document.as_bytes())
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let count = limit
        .checked_add(1)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("invalid file limit")?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(count)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("file exceeds resource limit".into());
    }
    Ok(bytes)
}

#[derive(Clone)]
pub struct Session {
    pub editor: EditorCore,
    pub source: Option<Source>,
    pub target: (String, String),
    pub awaiting_source: bool,
    pub mask_display: spatial::MaskDisplay,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            editor: EditorCore::new(),
            source: None,
            target: ("output".into(), "image".into()),
            awaiting_source: false,
            mask_display: spatial::MaskDisplay::default(),
        }
    }
}
impl Session {
    pub fn select_target(&mut self, node: &str, port: &str) -> Result<(), String> {
        let valid = self
            .editor
            .graph()
            .node(&rawweave_core::NodeId::from(node))
            .is_some_and(|node| {
                node.descriptor
                    .outputs
                    .iter()
                    .any(|output| output.id == port && preview_type(&output.data_type))
            });
        if !valid {
            return Err("Select an available image output".into());
        }
        self.target = (node.to_owned(), port.to_owned());
        Ok(())
    }
    /// Export the selected graph output at mip 0, never the viewer's proxy texture.
    pub fn export(&self, path: &Path) -> Result<(), String> {
        self.export_with_settings(path, &export::ExportSettings::default())
    }
    pub fn export_with_settings(
        &self,
        path: &Path,
        settings: &export::ExportSettings,
    ) -> Result<(), String> {
        use rawweave_batch::BatchItem;
        let mut recipe = settings.recipe(path)?;
        let image = match self.evaluate(0)? {
            Value::Image(image) => image,
            Value::SceneLinearRGB(scene) => {
                rgb_image(scene.dimensions(), scene.pixels(), ColorDomain::LinearSrgb)?
            }
            Value::DisplayRGB(display) => {
                rgb_image(display.dimensions(), display.pixels(), ColorDomain::Srgb)?
            }
            _ => return Err("Selected output is not an image".into()),
        };
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        // Stage encoding separately: the batch writer's overwrite policy removes the old file.
        // Installing a NamedTempFile instead preserves the destination if encoding/install fails.
        let staging = tempfile::tempdir_in(parent).map_err(|e| e.to_string())?;
        recipe.destination = staging.path().to_owned();
        let encoded = rawweave_batch::write_output(
            &image,
            &recipe,
            &BatchItem::new("export", "source", "source"),
            0,
            None,
        )
        .map_err(|e| e.to_string())?;
        let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        std::io::copy(
            &mut std::fs::File::open(encoded).map_err(|e| e.to_string())?,
            &mut output,
        )
        .map_err(|e| e.to_string())?;
        output.as_file().sync_all().map_err(|e| e.to_string())?;
        output.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn evaluate(&self, mip: u8) -> Result<Value, String> {
        let source = self.source.as_ref().ok_or("Select a source image")?;
        let context = match source {
            Source::Ordinary(image) if self.editor.graph().supports_source_mip(&self.target.0) => {
                EvaluationContext::with_source_image(image.clone()).with_source_image_mip(mip)
            }
            Source::Ordinary(image) => EvaluationContext::with_source_image(image.clone()),
            Source::Raw { bytes, path, .. } => EvaluationContext::default()
                .with_source_bytes(Arc::clone(bytes))
                .with_source_path(path),
        }
        .with_mip_level(mip);
        self.editor
            .evaluate(&self.target.0, &self.target.1, context)
            .map_err(|e| e.to_string())
    }
    pub fn attach(&mut self, source: Source) -> Result<(), String> {
        let raw = matches!(source, Source::Raw { .. });
        if self.awaiting_source {
            let expects_raw = self
                .editor
                .graph()
                .nodes()
                .values()
                .any(|node| node.type_id.starts_with("raw."));
            if raw != expects_raw {
                return Err("source kind does not match the loaded workflow".into());
            }
        } else if raw {
            self.editor
                .reset_raw_image_graph()
                .map_err(|e| e.to_string())?;
            self.target = ("display-transform".into(), "display".into());
        } else {
            self.editor
                .reset_ordinary_image_graph()
                .map_err(|e| e.to_string())?;
            self.target = ("output".into(), "image".into());
        }
        self.source = Some(source);
        self.awaiting_source = false;
        Ok(())
    }
    pub fn load_workflow(&mut self, json: &str) -> Result<(), String> {
        self.editor.load_workflow(json).map_err(|e| e.to_string())?;
        self.source = None;
        self.awaiting_source = true;
        self.target = (String::new(), String::new());
        self.reconcile_target();
        Ok(())
    }
    pub fn export_blueprint(
        &self,
        selection: &[&str],
        id: &str,
        name: &str,
    ) -> Result<String, String> {
        let mut definition = self
            .editor
            .create_subgraph_from_selection(selection, id, "1.0.0", Default::default())
            .map_err(|e| e.to_string())?;
        definition.metadata.name = name.to_owned();
        let bytes = self
            .editor
            .save_blueprint(&definition)
            .map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    pub fn import_blueprint(&mut self, text: &str) -> Result<(), String> {
        if text.len() > 16 * 1024 * 1024 {
            return Err("Blueprint exceeds resource limit".into());
        }
        let definition = self
            .editor
            .load_blueprint(text.as_bytes())
            .map_err(|e| e.to_string())?;
        self.editor
            .instantiate_blueprint(&definition)
            .map_err(|e| e.to_string())?;
        self.source = None;
        self.awaiting_source = true;
        self.target = (String::new(), String::new());
        self.reconcile_target();
        Ok(())
    }
    /// Keep viewer state valid after deletion or history restoration.
    pub fn reconcile_target(&mut self) -> bool {
        let (node, port) = self.target.clone();
        if self.select_target(&node, &port).is_ok() {
            return false;
        }
        self.target = [("output", "image"), ("display-transform", "display")]
            .into_iter()
            .find(|(id, port)| {
                self.editor
                    .graph()
                    .node(&rawweave_core::NodeId::from(*id))
                    .is_some_and(|n| n.descriptor.outputs.iter().any(|p| p.id == *port))
            })
            .map(|(id, port)| (id.to_owned(), port.to_owned()))
            .or_else(|| {
                self.editor
                    .graph()
                    .nodes()
                    .values()
                    .flat_map(|node| {
                        node.descriptor
                            .outputs
                            .iter()
                            .filter(|p| preview_type(&p.data_type))
                            .map(|port| (node.id.as_str().to_owned(), port.id.clone()))
                    })
                    .next()
            })
            .unwrap_or_default();
        true
    }
    pub fn preview(
        &self,
        mip: u8,
        cancellation: &CancellationToken,
    ) -> Result<PreviewFrame, String> {
        if mip > 6 || cancellation.is_cancelled() {
            return Err("preview cancelled or invalid mip".into());
        }
        let source = self.source.as_ref().ok_or("select a source image")?;
        let proxy = matches!(source, Source::Ordinary(_))
            && self.editor.graph().supports_source_mip(&self.target.0);
        let value = self.evaluate(mip)?;
        if cancellation.is_cancelled() {
            return Err("preview cancelled".into());
        }
        match value {
            Value::Image(image) => {
                let full = if proxy {
                    source.dimensions()
                } else {
                    image.dimensions()
                };
                let selected = if proxy {
                    image
                } else {
                    image.sample_mip(mip).map_err(|e| e.to_string())?
                };
                image_display_frame(&selected, full)
            }
            Value::SceneLinearRGB(scene) => {
                let display = SrgbDisplayTransform
                    .transform(&scene)
                    .map_err(|e| e.to_string())?;
                display_frame(display, mip)
            }
            Value::DisplayRGB(display) => display_frame(display, mip),
            value => spatial::spatial_frame(&value, mip, self.mask_display),
        }
    }
}
/// The bounded BGRA display raster for an ordinary or linear-sRGB image.
pub fn image_display_frame(image: &Image, full: Dimensions) -> Result<PreviewFrame, String> {
    if image.color_metadata().domain != ColorDomain::LinearSrgb {
        return PreviewFrame::from_pixels(image.dimensions(), full, image.pixels().iter().copied());
    }
    let scene = rawweave_color::SceneLinearRGB::new(
        image.dimensions(),
        image
            .pixels()
            .iter()
            .map(|[r, g, b, _]| [*r, *g, *b])
            .collect(),
        rawweave_color::WorkingSpace::Srgb,
    )
    .map_err(|error| error.to_string())?;
    let display = SrgbDisplayTransform
        .transform(&scene)
        .map_err(|error| error.to_string())?;
    PreviewFrame::from_pixels(
        image.dimensions(),
        full,
        display
            .pixels()
            .iter()
            .zip(image.pixels())
            .map(|([r, g, b], source)| [*r, *g, *b, source[3]]),
    )
}

fn rgb_image(
    dimensions: Dimensions,
    pixels: &[[f32; 3]],
    domain: ColorDomain,
) -> Result<Image, String> {
    Image::from_pixels_with_metadata(
        dimensions.width,
        dimensions.height,
        pixels.iter().map(|[r, g, b]| [*r, *g, *b, 1.0]).collect(),
        PixelFormat::Rgba32Float,
        domain,
    )
    .map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    OpenImage,
    OpenWorkflow,
    SaveWorkflow,
    Search,
    Undo,
    Redo,
    Delete,
    SelectAll,
}

pub fn shortcut(
    key: &str,
    modifier: bool,
    shift: bool,
    alt: bool,
    editing: bool,
) -> Option<Shortcut> {
    use Shortcut::*;
    if alt {
        return None;
    }
    if modifier {
        match key {
            "k" if !shift => Some(Search),
            "a" if !editing && !shift => Some(SelectAll),
            "o" => Some(if shift { OpenImage } else { OpenWorkflow }),
            "s" if !shift => Some(SaveWorkflow),
            "z" if !editing => Some(if shift { Redo } else { Undo }),
            "y" if !editing && !shift => Some(Redo),
            _ => None,
        }
    } else if !editing && matches!(key, "backspace" | "delete") {
        Some(Delete)
    } else {
        None
    }
}

pub fn preview_type(kind: &str) -> bool {
    matches!(
        kind,
        "core.Image"
            | "color.SceneLinearRGB"
            | "color.DisplayRGB"
            | "core.Mask"
            | "core.MaskSet"
            | "core.LabelMap"
            | "core.ConfidenceMap"
            | "core.DepthMap"
            | "core.RegionSet"
    )
}

pub struct PreviewFrame {
    pub dimensions: Dimensions,
    pub full_dimensions: Dimensions,
    /// GPUI's native image atlas expects BGRA, not RGBA.
    pub bgra: Vec<u8>,
}
impl PreviewFrame {
    /// Bounded BGRA display raster for a native upload.
    pub fn from_pixels(
        dimensions: Dimensions,
        full_dimensions: Dimensions,
        pixels: impl Iterator<Item = [f32; 4]>,
    ) -> Result<Self, String> {
        if dimensions.width == 0
            || dimensions.height == 0
            || dimensions.width > 8192
            || dimensions.height > 8192
        {
            return Err("preview dimensions exceed native upload limits".into());
        }
        let count = dimensions.pixel_count().map_err(|e| e.to_string())?;
        let capacity = count
            .checked_mul(4)
            .filter(|n| *n <= 128 * 1024 * 1024)
            .ok_or("preview exceeds upload budget")?;
        let mut bgra = Vec::with_capacity(capacity);
        for [r, g, b, a] in pixels {
            if ![r, g, b, a].into_iter().all(f32::is_finite) {
                return Err("nonfinite display pixel".into());
            }
            bgra.extend([b, g, r, a].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
        if bgra.len() != capacity {
            return Err("invalid preview pixel count".into());
        }
        Ok(Self {
            dimensions,
            full_dimensions,
            bgra,
        })
    }
}
fn display_frame(display: rawweave_color::DisplayRGB, mip: u8) -> Result<PreviewFrame, String> {
    let physical = display.dimensions();
    let full = display.sampling().map_or(physical, |s| s.full_dimensions);
    let scale = if display.sampling().is_some() {
        1
    } else {
        1_u32 << mip
    };
    let dimensions = Dimensions::new(
        physical.width.div_ceil(scale),
        physical.height.div_ceil(scale),
    );
    let mut pixels = Vec::with_capacity(dimensions.pixel_count().map_err(|e| e.to_string())?);
    for y in (0..physical.height).step_by(usize::try_from(scale).map_err(|e| e.to_string())?) {
        for x in (0..physical.width).step_by(usize::try_from(scale).map_err(|e| e.to_string())?) {
            let [r, g, b] = display.pixel(x, y).ok_or("display pixel outside buffer")?;
            pixels.push([r, g, b, 1.0]);
        }
    }
    PreviewFrame::from_pixels(dimensions, full, pixels.into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_preview_targets_validate_without_mutating_on_failure() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(Image::new(3, 2).unwrap()))
            .unwrap();
        session.select_target("input", "image").unwrap();
        assert_eq!(session.target, ("input".into(), "image".into()));
        assert!(session.select_target("input", "missing").is_err());
        assert!(session.select_target("missing", "image").is_err());
        session
            .editor
            .add_node("number", "core.constant-float")
            .unwrap();
        assert!(session.select_target("number", "value").is_err());
        assert_eq!(session.target, ("input".into(), "image".into()));
    }
    #[test]
    fn blueprint_selection_roundtrips_boundaries_and_parameters_without_runtime_sources() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(Image::new(3, 2).unwrap()))
            .unwrap();
        session
            .editor
            .add_node("exposure", "core.exposure")
            .unwrap();
        session
            .editor
            .connect("input", "image", "exposure", "image")
            .unwrap();
        session
            .editor
            .disconnect("input", "image", "output", "image")
            .unwrap();
        session
            .editor
            .connect("exposure", "image", "output", "image")
            .unwrap();
        session
            .editor
            .expose_parameter("exposure", "exposure")
            .unwrap();
        session
            .editor
            .set_node_parameter(
                "exposure",
                "exposure",
                rawweave_node_api::ParameterValue::Float(1.5),
            )
            .unwrap();
        let before = session.editor.save_workflow().unwrap();
        let text = session
            .export_blueprint(&["exposure"], "my-exposure", "My Exposure")
            .unwrap();
        let definition = session.editor.load_blueprint(text.as_bytes()).unwrap();
        assert_eq!(definition.metadata.name, "My Exposure");
        assert_eq!(definition.identity.id, "my-exposure");
        assert_eq!(definition.inputs.len(), 1);
        assert_eq!(definition.outputs.len(), 1);
        assert_eq!(definition.parameters.len(), 1);
        assert_eq!(session.editor.save_workflow().unwrap(), before);
        assert!(session.export_blueprint(&[], "empty", "Empty").is_err());
        assert!(
            session
                .export_blueprint(&["missing"], "bad", "Bad")
                .is_err()
        );
        assert!(session.import_blueprint("invalid").is_err());
        assert!(session.source.is_some());
        assert_eq!(session.editor.save_workflow().unwrap(), before);
        session.import_blueprint(&text).unwrap();
        assert!(session.source.is_none());
        assert!(session.awaiting_source);
        assert_eq!(session.editor.graph().nodes().len(), 1);
        assert_eq!(session.target, ("exposure".into(), "image".into()));
        session
            .attach(Source::Ordinary(Image::new(3, 2).unwrap()))
            .unwrap();
        assert_eq!(session.editor.graph().nodes().len(), 1);
    }
    #[test]
    fn graph_mask_outputs_are_selectable_and_previewable() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(Image::new(3, 2).unwrap()))
            .unwrap();
        session
            .editor
            .add_node("mask", "core.mask-linear-gradient")
            .unwrap();
        session
            .editor
            .connect("input", "image", "mask", "image")
            .unwrap();
        session.select_target("mask", "mask").unwrap();
        let before = session.editor.save_workflow().unwrap();
        let gray = session.preview(0, &CancellationToken::new()).unwrap();
        session.mask_display = spatial::MaskDisplay::Overlay;
        let overlay = session.preview(0, &CancellationToken::new()).unwrap();
        assert_eq!(gray.full_dimensions, Dimensions::new(3, 2));
        assert_ne!(gray.bgra, overlay.bgra);
        assert_eq!(session.editor.save_workflow().unwrap(), before);
        assert!(
            session
                .export(&tempfile::tempdir().unwrap().path().join("mask.png"))
                .is_err()
        );
        for kind in [
            "core.Mask",
            "core.MaskSet",
            "core.LabelMap",
            "core.ConfidenceMap",
            "core.DepthMap",
            "core.RegionSet",
        ] {
            assert!(preview_type(kind));
        }
        assert!(!preview_type("core.Float"));
    }
    #[test]
    fn removed_viewer_output_falls_back_to_an_available_target() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(Image::new(1, 1).unwrap()))
            .unwrap();
        session.select_target("output", "image").unwrap();
        session.editor.remove_node("output").unwrap();
        assert!(session.reconcile_target());
        assert_eq!(session.target, ("input".into(), "image".into()));
        assert!(!session.reconcile_target());
        session.editor.remove_node("input").unwrap();
        assert!(session.reconcile_target());
        assert_eq!(session.target, (String::new(), String::new()));
    }
    #[test]
    fn exports_evaluate_the_selected_output_and_keep_color_domains() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels_with_metadata(
                    1,
                    1,
                    vec![[0.25, 0.5, 0.75, 1.0]],
                    PixelFormat::Rgba32Float,
                    ColorDomain::Srgb,
                )
                .unwrap(),
            ))
            .unwrap();
        session.editor.add_node("bright", "core.exposure").unwrap();
        session
            .editor
            .set_node_parameter(
                "bright",
                "exposure",
                rawweave_node_api::ParameterValue::Float(1.0),
            )
            .unwrap();
        session
            .editor
            .connect("input", "image", "bright", "image")
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("selected.PNG");
        session.select_target("input", "image").unwrap();
        session.export(&path).unwrap();
        let original = rawweave_batch::decode_ordinary_file(&path).unwrap();
        assert!((original.pixels()[0][0] - 0.25).abs() < 0.005);
        session.select_target("bright", "image").unwrap();
        session.export(&path).unwrap();
        let bright = rawweave_batch::decode_ordinary_file(&path).unwrap();
        assert!(bright.pixels()[0][0] > original.pixels()[0][0]);
        assert_eq!(
            rgb_image(
                Dimensions::new(1, 1),
                &[[2.0, 0.5, 0.25]],
                ColorDomain::LinearSrgb
            )
            .unwrap()
            .pixels()[0][0],
            2.0
        );
    }
    #[cfg(feature = "native")]
    #[test]
    fn recipe_export_resizes_encodes_sixteen_bits_and_is_atomic_on_invalid_settings() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels(8, 4, vec![[0.5, 0.25, 0.75, 1.0]; 32]).unwrap(),
            ))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resized.png");
        let settings = export::ExportSettings {
            long_edge: Some(4),
            png_sixteen: true,
            compression: rawweave_batch::Compression::Best,
            ..Default::default()
        };
        session.export_with_settings(&path, &settings).unwrap();
        let image = image::open(&path).unwrap();
        assert_eq!((image.width(), image.height()), (4, 2));
        assert_eq!(image.color(), image::ColorType::Rgba16);
        let bytes = std::fs::read(&path).unwrap();
        let invalid = export::ExportSettings {
            long_edge: Some(u32::MAX),
            ..Default::default()
        };
        assert!(session.export_with_settings(&path, &invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    #[cfg(feature = "native")]
    #[test]
    fn exr_export_preserves_scene_linear_highlights() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels_with_metadata(
                    1,
                    1,
                    vec![[2.5, 0.5, 0.25, 1.0]],
                    PixelFormat::Rgba32Float,
                    ColorDomain::LinearSrgb,
                )
                .unwrap(),
            ))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hdr.exr");
        session.export(&path).unwrap();
        let decoded = image::open(&path).unwrap().into_rgba32f();
        assert_eq!(decoded.get_pixel(0, 0).0, [2.5, 0.5, 0.25, 1.0]);
    }
    #[test]
    fn export_uses_full_resolution_and_preserves_existing_files_on_failure() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels(3, 2, vec![[0.25, 0.5, 0.75, 1.0]; 6]).unwrap(),
            ))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        for extension in ["png", "jpg", "tif", "exr"] {
            let path = directory.path().join(format!("result.{extension}"));
            std::fs::write(&path, b"old").unwrap();
            session.export(&path).unwrap();
            let decoded = rawweave_batch::decode_ordinary_file(&path);
            if extension != "exr" {
                assert_eq!(decoded.unwrap().dimensions(), Dimensions::new(3, 2));
            } else {
                assert!(std::fs::metadata(&path).unwrap().len() > 3);
            }
        }
        let path = directory.path().join("result.png");
        let before = std::fs::read(&path).unwrap();
        session.source = None;
        assert!(session.export(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(
            session
                .export(&directory.path().join("result.unknown"))
                .is_err()
        );
    }
    #[test]
    fn shortcuts_match_tauri_and_leave_text_editing_native() {
        use Shortcut::*;
        assert_eq!(shortcut("o", true, false, false, false), Some(OpenWorkflow));
        assert_eq!(shortcut("o", true, true, false, false), Some(OpenImage));
        assert_eq!(shortcut("s", true, false, false, false), Some(SaveWorkflow));
        assert_eq!(shortcut("z", true, false, false, false), Some(Undo));
        assert_eq!(shortcut("z", true, true, false, false), Some(Redo));
        assert_eq!(shortcut("y", true, false, false, false), Some(Redo));
        assert_eq!(shortcut("k", true, false, false, true), Some(Search));
        assert_eq!(shortcut("a", true, false, false, false), Some(SelectAll));
        assert_eq!(shortcut("a", true, false, false, true), None);
        assert_eq!(shortcut("a", true, true, false, false), None);
        assert_eq!(
            shortcut("backspace", false, false, false, false),
            Some(Delete)
        );
        for key in ["z", "y", "backspace", "delete"] {
            assert_eq!(
                shortcut(key, key == "z" || key == "y", false, false, true),
                None
            );
        }
        assert_eq!(shortcut("o", true, false, true, false), None);
    }
    #[test]
    fn bgra_upload_and_sampled_bounds_are_exact() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels_with_metadata(
                    3,
                    1,
                    vec![[1.0, 0.0, 0.25, 0.5]; 3],
                    PixelFormat::Rgba32Float,
                    ColorDomain::Srgb,
                )
                .unwrap(),
            ))
            .unwrap();
        let frame = session.preview(1, &CancellationToken::new()).unwrap();
        assert_eq!(frame.dimensions, Dimensions::new(2, 1));
        assert_eq!(frame.full_dimensions, Dimensions::new(3, 1));
        assert_eq!(frame.bgra, [64, 0, 255, 128, 64, 0, 255, 128]);
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(session.preview(0, &cancelled).is_err());
        assert!(session.preview(13, &CancellationToken::new()).is_err());
    }
    #[test]
    fn linear_image_preview_encodes_rgb_once_and_keeps_alpha() {
        let source = Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 0.5]]).unwrap();
        let frame = image_display_frame(&source, source.dimensions()).unwrap();
        assert_eq!(frame.bgra, [225, 188, 137, 128]);
        assert_eq!(source.pixels(), &[[0.25, 0.5, 0.75, 0.5]]);
    }

    #[test]
    fn workflow_load_clears_runtime_source_and_reattaches_without_rebuilding() {
        let mut session = Session::default();
        let source =
            Source::Ordinary(Image::from_pixels(2, 1, vec![[0.5, 0.0, 0.0, 1.0]; 2]).unwrap());
        session.attach(source.clone()).unwrap();
        session.editor.add_node("saved", "core.exposure").unwrap();
        let json = session.editor.save_workflow().unwrap();
        session.load_workflow(&json).unwrap();
        assert!(session.source.is_none());
        session.attach(source).unwrap();
        assert!(
            session
                .editor
                .graph()
                .nodes()
                .contains_key(&rawweave_core::NodeId::from("saved"))
        );
        assert!(
            session
                .editor
                .save_workflow()
                .unwrap()
                .find("pixels")
                .is_none()
        );
        assert_eq!(session.target, ("output".into(), "image".into()));
        assert!(session.preview(0, &CancellationToken::new()).is_ok());
    }
    #[test]
    fn licensed_raw_preview_preserves_sampled_full_size_bounds() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/images/raw/nikon-d70s-12bit-lossy.nef");
        let mut session = Session::default();
        session.attach(Source::open(&path).unwrap()).unwrap();
        let frame = session.preview(2, &CancellationToken::new()).unwrap();
        assert_eq!(frame.full_dimensions, Dimensions::new(3040, 2014));
        assert_eq!(frame.dimensions, Dimensions::new(760, 504));
        assert_eq!(frame.bgra.len(), 760 * 504 * 4);
        let workflow = session.editor.save_workflow().unwrap();
        session.load_workflow(&workflow).unwrap();
        assert!(
            session
                .attach(Source::Ordinary(Image::new(1, 1).unwrap()))
                .is_err()
        );
        assert!(session.source.is_none());
    }
    #[test]
    fn native_raw_scene_nodes_and_image_conversion_preserve_preview_bounds() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/images/raw/nikon-d70s-12bit-lossy.nef");
        let mut session = Session::default();
        session.attach(Source::open(&path).unwrap()).unwrap();
        session.editor.add_node("bright", "core.exposure").unwrap();
        session
            .editor
            .set_node_parameter("bright", "exposure", 1.0_f32.into())
            .unwrap();
        session
            .editor
            .connect("camera-transform", "scene", "bright", "scene")
            .unwrap();
        session.select_target("bright", "scene").unwrap();
        let scene = session.preview(2, &CancellationToken::new()).unwrap();
        assert_eq!(scene.full_dimensions, Dimensions::new(3040, 2014));
        assert_eq!(scene.dimensions, Dimensions::new(760, 504));
        session
            .editor
            .add_node("convert", "core.scene-linear-to-image")
            .unwrap();
        session
            .editor
            .connect("bright", "scene", "convert", "scene")
            .unwrap();
        session.select_target("convert", "image").unwrap();
        let converted = session.preview(2, &CancellationToken::new()).unwrap();
        assert_eq!(converted.full_dimensions, scene.full_dimensions);
        assert_eq!(converted.dimensions, scene.dimensions);
        // The Image preview uses mip filtering; RAW scene previews sample the sensor grid.
        // Compare full-resolution displays, not these different preview reduction policies.
        let image_full = session.preview(0, &CancellationToken::new()).unwrap();
        session.select_target("bright", "scene").unwrap();
        let scene_full = session.preview(0, &CancellationToken::new()).unwrap();
        assert!(
            image_full
                .bgra
                .iter()
                .zip(&scene_full.bgra)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
    }

    #[test]
    fn licensed_raw_export_matches_display_without_double_encoding() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/images/raw/nikon-d70s-12bit-lossy.nef");
        let mut session = Session::default();
        session.attach(Source::open(&source).unwrap()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("raw.png");
        session.export(&path).unwrap();
        let exported = rawweave_batch::decode_ordinary_file(&path).unwrap();
        let display = session.preview(0, &CancellationToken::new()).unwrap();
        assert_eq!(exported.dimensions(), display.dimensions);
        for index in [0, exported.pixels().len() / 2, exported.pixels().len() - 1] {
            let [r, g, b, _] = exported.pixels()[index];
            let bgra = &display.bgra[index * 4..index * 4 + 4];
            for (channel, byte) in [b, g, r].into_iter().zip(bgra) {
                assert!((channel * 255.0 - f32::from(*byte)).abs() < 1.1);
            }
        }
    }
    #[test]
    fn invalid_uploads_and_failed_saves_preserve_resources() {
        assert!(
            PreviewFrame::from_pixels(
                Dimensions::new(0, 1),
                Dimensions::new(0, 1),
                std::iter::empty()
            )
            .is_err()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workflow.json");
        save_workflow_atomic(&path, "original").unwrap();
        assert!(save_workflow_atomic(&path.join("invalid-child"), "replacement").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        save_workflow_atomic(&path, "updated").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "updated");
    }
    #[test]
    fn malformed_workflow_does_not_discard_the_current_source() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels(1, 1, vec![[1.0; 4]]).unwrap(),
            ))
            .unwrap();
        assert!(session.load_workflow("not json").is_err());
        assert!(session.source.is_some());
    }
}
