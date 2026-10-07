//! Native desktop session. Graph execution stays in the existing Rust engine.
use rawweave_color::{DisplayTransform, SrgbDisplayTransform};
use rawweave_image::{Dimensions, Image};
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;
use rawweave_raw::{RawDecodeLimits, RawloaderDecoder};
use rawweave_rendering::CancellationToken;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

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
}
impl Default for Session {
    fn default() -> Self {
        Self {
            editor: EditorCore::new(),
            source: None,
            target: ("output".into(), "image".into()),
            awaiting_source: false,
        }
    }
}
impl Session {
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
        Ok(())
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
        let context = match source {
            Source::Ordinary(image) if proxy => {
                EvaluationContext::with_source_image(image.clone()).with_source_image_mip(mip)
            }
            Source::Ordinary(image) => EvaluationContext::with_source_image(image.clone()),
            Source::Raw { bytes, path, .. } => EvaluationContext::default()
                .with_source_bytes(Arc::clone(bytes))
                .with_source_path(path),
        }
        .with_mip_level(mip);
        let value = self
            .editor
            .evaluate(&self.target.0, &self.target.1, context)
            .map_err(|e| e.to_string())?;
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
                PreviewFrame::from_pixels(
                    selected.dimensions(),
                    full,
                    selected.pixels().iter().copied(),
                )
            }
            Value::SceneLinearRGB(scene) => {
                let display = SrgbDisplayTransform
                    .transform(&scene)
                    .map_err(|e| e.to_string())?;
                display_frame(display, mip)
            }
            Value::DisplayRGB(display) => display_frame(display, mip),
            _ => Err("selected output is not a displayable image".into()),
        }
    }
}
pub fn preview_type(kind: &str) -> bool {
    matches!(
        kind,
        "core.Image" | "color.SceneLinearRGB" | "color.DisplayRGB"
    )
}

pub struct PreviewFrame {
    pub dimensions: Dimensions,
    pub full_dimensions: Dimensions,
    /// GPUI's native image atlas expects BGRA, not RGBA.
    pub bgra: Vec<u8>,
}
impl PreviewFrame {
    fn from_pixels(
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
    fn bgra_upload_and_sampled_bounds_are_exact() {
        let mut session = Session::default();
        session
            .attach(Source::Ordinary(
                Image::from_pixels(3, 1, vec![[1.0, 0.0, 0.25, 0.5]; 3]).unwrap(),
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
